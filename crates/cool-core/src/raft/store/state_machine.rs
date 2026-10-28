//! RocksDB-backed applied-state（只持久化 last_log_id + membership，不做业务 KV）

use std::io;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use futures::TryStreamExt;
use openraft::EntryPayload;
use openraft::OptionalSend;
use openraft::alias::LogIdOf;
use openraft::alias::SnapshotMetaOf;
use openraft::alias::SnapshotOf;
use openraft::alias::StoredMembershipOf;
use openraft::storage::EntryResponder;
use openraft::storage::RaftSnapshotBuilder;
use openraft::storage::RaftStateMachine;
use openraft_memstore::{ClientResponse, TypeConfig};
use rocksdb::{ColumnFamilyDescriptor, DB, Options, WriteOptions};
use serde::{Deserialize, Serialize};

/// 与 network 层一致的快照载荷（JSON 序列化）
pub type SnapshotData = Cursor<Vec<u8>>;

/// 持久化 applied-state 的状态机
///
/// `sm_meta` 列族保存 applied 位点。应用层不写 Raft KV；Normal 条目仅推进 last_log_id。
pub struct RocksStateMachine {
    db: Arc<DB>,
    current_snapshot: Option<SnapshotOf<TypeConfig, SnapshotData>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AppliedMeta {
    last_applied_log: Option<LogIdOf<TypeConfig>>,
    last_membership: StoredMembershipOf<TypeConfig>,
}

impl RocksStateMachine {
    /// 打开（或创建）指定路径下的 RocksDB 状态机
    pub fn open<P>(db_path: P) -> Result<Self, io::Error>
    where
        P: AsRef<Path>,
    {
        let mut options = Options::default();
        options.create_missing_column_families(true);
        options.create_if_missing(true);
        let descriptors = [ColumnFamilyDescriptor::new("sm_meta", Options::default())];
        let db =
            DB::open_cf_descriptors(&options, db_path, descriptors).map_err(io::Error::other)?;
        Ok(Self {
            db: Arc::new(db),
            current_snapshot: None,
        })
    }

    fn cf_meta(&self) -> &rocksdb::ColumnFamily {
        self.db
            .cf_handle("sm_meta")
            .expect("column family `sm_meta` not found")
    }

    fn read_meta(&self) -> io::Result<AppliedMeta> {
        match self
            .db
            .get_cf(self.cf_meta(), b"applied")
            .map_err(io::Error::other)?
        {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            None => Ok(AppliedMeta::default()),
        }
    }

    fn write_meta(&self, meta: &AppliedMeta) -> io::Result<()> {
        let bytes =
            serde_json::to_vec(meta).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut opts = WriteOptions::default();
        opts.set_sync(true);
        self.db
            .put_cf_opt(self.cf_meta(), b"applied", bytes, &opts)
            .map_err(io::Error::other)
    }

    fn make_snapshot(&self) -> io::Result<SnapshotOf<TypeConfig, SnapshotData>> {
        let meta = self.read_meta()?;
        let snapshot_meta = SnapshotMetaOf::<TypeConfig> {
            last_log_id: meta.last_applied_log,
            last_membership: meta.last_membership.clone(),
        };
        let data =
            serde_json::to_vec(&meta).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(SnapshotOf::<TypeConfig, SnapshotData> {
            meta: snapshot_meta,
            snapshot: Cursor::new(data),
        })
    }
}

impl RaftSnapshotBuilder<TypeConfig> for RocksStateMachine {
    type SnapshotData = SnapshotData;

    async fn build_snapshot(&mut self) -> Result<SnapshotOf<TypeConfig, SnapshotData>, io::Error> {
        let snap = self.make_snapshot()?;
        self.current_snapshot = Some(snap.clone());
        Ok(snap)
    }
}

impl RaftStateMachine<TypeConfig> for RocksStateMachine {
    type SnapshotData = SnapshotData;

    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogIdOf<TypeConfig>>, StoredMembershipOf<TypeConfig>), io::Error> {
        let meta = self.read_meta()?;
        Ok((meta.last_applied_log, meta.last_membership))
    }

    async fn apply<Strm>(&mut self, mut entries: Strm) -> Result<(), io::Error>
    where
        Strm: futures::Stream<Item = Result<EntryResponder<TypeConfig>, io::Error>>
            + Unpin
            + OptionalSend,
    {
        let mut meta = self.read_meta()?;
        let mut last_log_id = meta.last_applied_log;
        let mut last_membership = meta.last_membership.clone();
        let mut responses = Vec::new();

        while let Some((entry, responder)) = entries.try_next().await? {
            tracing::debug!(%entry.log_id, "replicate to sm");
            last_log_id = Some(entry.log_id);

            match entry.payload {
                EntryPayload::Blank | EntryPayload::Normal(_) => {}
                EntryPayload::Membership(ref mem) => {
                    last_membership =
                        StoredMembershipOf::<TypeConfig>::new(Some(entry.log_id), mem.clone());
                }
            }

            if let Some(responder) = responder {
                responses.push(responder);
            }
        }

        meta.last_applied_log = last_log_id;
        meta.last_membership = last_membership;
        self.write_meta(&meta)?;

        for responder in responses {
            responder.send(ClientResponse(None));
        }

        Ok(())
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        Self {
            db: self.db.clone(),
            current_snapshot: self.current_snapshot.clone(),
        }
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMetaOf<TypeConfig>,
        snapshot: Self::SnapshotData,
    ) -> Result<(), io::Error> {
        let data = snapshot.get_ref().to_vec();
        let applied: AppliedMeta = serde_json::from_slice(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.write_meta(&applied)?;

        self.current_snapshot = Some(SnapshotOf::<TypeConfig, SnapshotData> {
            meta: meta.clone(),
            snapshot: Cursor::new(data),
        });
        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<SnapshotOf<TypeConfig, Self::SnapshotData>>, io::Error> {
        if self.current_snapshot.is_none() {
            self.current_snapshot = Some(self.make_snapshot()?);
        }
        Ok(self.current_snapshot.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openraft::storage::RaftStateMachine;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cool-raft-sm-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn applied_state_roundtrip() {
        let dir = tmp_dir("applied");
        {
            let mut sm = RocksStateMachine::open(&dir).unwrap();

            let (log_id, _mem) = sm.applied_state().await.unwrap();
            assert!(log_id.is_none());

            let snap = sm.get_current_snapshot().await.unwrap().unwrap();
            assert!(snap.meta.last_log_id.is_none());

            sm.install_snapshot(&snap.meta, snap.snapshot)
                .await
                .unwrap();
            let (log_id2, _) = sm.applied_state().await.unwrap();
            assert!(log_id2.is_none());
        }

        // 重开后 applied-state 从磁盘恢复（须先 drop 前一个句柄释放 RocksDB 锁）
        let mut sm2 = RocksStateMachine::open(&dir).unwrap();
        let (log_id3, _) = sm2.applied_state().await.unwrap();
        assert!(log_id3.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn snapshot_reopen_persists() {
        let dir = tmp_dir("reopen");
        {
            let mut sm = RocksStateMachine::open(&dir).unwrap();
            let mut snap = sm.get_current_snapshot().await.unwrap().unwrap();
            snap.meta = SnapshotMetaOf::<TypeConfig> {
                last_log_id: None,
                last_membership: StoredMembershipOf::<TypeConfig>::default(),
            };
            sm.install_snapshot(&snap.meta, snap.snapshot)
                .await
                .unwrap();
        }
        let mut sm = RocksStateMachine::open(&dir).unwrap();
        let current = sm.get_current_snapshot().await.unwrap().unwrap();
        assert!(current.meta.last_log_id.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
