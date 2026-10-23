//! cool-raft：基于 openraft 的分布式能力
//!
//! - **集群选主**：多节点 Raft 选举，Leader 节点执行定时任务（避免重复调度）
//! - **持久化存储**：RocksDB 保存 Raft 日志 / vote / applied-state，进程重启可恢复
//! - **成员管理**：add_learner / change_membership
//! - **状态查询**：节点角色、Leader、term
//!
//! 单节点时自动成为 Leader；配置 peers 即组成集群。
//! 节点地址由本模块自行维护（TypeConfig 的 Node 类型为 ()）。

pub mod network;
pub mod store;

use openraft::Config as RaftConfig;
use openraft::ServerState;
use openraft::async_runtime::watch::WatchReceiver;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type NodeId = u64;
/// TypeConfig：Node=()，地址通过 PeerMap 维护
pub use openraft_memstore::TypeConfig;

pub type LogStore = store::RocksLogStore<TypeConfig>;
pub type StateMachine = store::RocksStateMachine;
pub type Raft = openraft::Raft<TypeConfig, StateMachine>;

/// 节点地址表
pub type PeerMap = Arc<Mutex<BTreeMap<NodeId, String>>>;

/// 集群句柄
#[derive(Clone)]
pub struct Cluster {
    pub node_id: NodeId,
    pub raft: Raft,
    pub peers: PeerMap,
}

impl Cluster {
    /// 是否 Leader（仅角色判断，不保证有 quorum）
    pub async fn is_leader(&self) -> bool {
        self.raft.current_leader().await == Some(self.node_id)
    }

    /// 验证「有效 Leader」：角色为 Leader **且** quorum 可达
    ///
    /// 少数派/网络分区时 Leader 角色可能滞留，此处用 linearizable 读屏障
    /// （无写放大）确认多数派可达，超时/失败即视为失去领导力。
    pub async fn verify_quorum_leadership(&self) -> bool {
        if !self.is_leader().await {
            return false;
        }
        let barrier = self
            .raft
            .ensure_linearizable(openraft::raft::ReadPolicy::ReadIndex);
        matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), barrier).await,
            Ok(Ok(_))
        )
    }

    /// 当前 Leader（可能未知）
    pub async fn leader(&self) -> Option<NodeId> {
        self.raft.current_leader().await
    }

    /// 节点角色
    pub fn role(&self) -> &'static str {
        match self.raft.metrics().borrow_watched().state {
            ServerState::Leader => "Leader",
            ServerState::Candidate => "Candidate",
            ServerState::Follower => "Follower",
            ServerState::Learner => "Learner",
            ServerState::Shutdown => "Shutdown",
        }
    }

    /// 集群状态摘要
    pub async fn status(&self) -> serde_json::Value {
        let m = self.raft.metrics().borrow_watched().clone();
        let peers = self.peers.lock().await.clone();
        serde_json::json!({
            "nodeId": self.node_id,
            "role": self.role(),
            "leader": self.leader().await,
            "isLeader": self.is_leader().await,
            "term": m.current_term,
            "lastLogIndex": m.last_log_index,
            "lastApplied": m.last_applied,
            "state": format!("{:?}", m.state),
            "peers": peers,
        })
    }

    /// 登记节点地址
    pub async fn add_peer_addr(&self, id: NodeId, addr: String) {
        self.peers.lock().await.insert(id, addr);
    }

    /// 添加学习者节点（addr 为该节点 API 地址，用于 Raft RPC）
    pub async fn add_learner(&self, id: NodeId, addr: String) -> Result<(), String> {
        self.add_peer_addr(id, addr).await;
        self.raft
            .add_learner(id, (), true)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// 变更集群成员（投票权）
    pub async fn change_membership(&self, members: Vec<NodeId>) -> Result<(), String> {
        let m: std::collections::BTreeSet<NodeId> = members.into_iter().collect();
        self.raft
            .change_membership(m, false)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// 启动 Raft 节点（RocksDB 持久化）
///
/// - `data_dir`：Raft 日志与 applied-state 落盘目录（每节点独立）
/// - `peers` 为空 → 单节点，立即成为 Leader
/// - `peers` 非空 → 登记地址；最小节点 ID 负责 bootstrap（已初始化则跳过）
pub async fn start(
    node_id: NodeId,
    self_addr: String,
    peers: Vec<(NodeId, String)>,
    data_dir: PathBuf,
    config: Option<RaftConfig>,
) -> anyhow::Result<Cluster> {
    let cfg = config.unwrap_or_else(|| RaftConfig {
        heartbeat_interval: 500,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        enable_pre_vote: Some(true),
        ..Default::default()
    });
    let cfg = Arc::new(cfg.validate().map_err(|e| anyhow::anyhow!("{e}"))?);

    let peer_map: PeerMap = Arc::new(Mutex::new(BTreeMap::new()));
    for (id, addr) in &peers {
        peer_map.lock().await.insert(*id, addr.clone());
    }
    peer_map.lock().await.insert(node_id, self_addr.clone());

    let log_dir = data_dir.join("log");
    let sm_dir = data_dir.join("sm");
    tracing::info!(node_id, data_dir = %data_dir.display(), "Raft 使用 RocksDB 持久化存储");
    let log_store =
        LogStore::open(&log_dir).map_err(|e| anyhow::anyhow!("打开 Raft 日志库失败: {e}"))?;
    let state_machine =
        StateMachine::open(&sm_dir).map_err(|e| anyhow::anyhow!("打开 Raft 状态机失败: {e}"))?;

    let network = network::HttpNetworkFactory::new(peer_map.clone());
    let raft = Raft::new(node_id, cfg, network, log_store, state_machine).await?;

    // 集群成员集合
    let mut members: std::collections::BTreeSet<NodeId> = peers.iter().map(|(id, _)| *id).collect();
    if members.is_empty() {
        members.insert(node_id);
    }

    // Bootstrap：最小节点 ID 的节点负责初始化全成员集群（一次性）
    // 其余节点等待选举结果；已初始化（重启）则跳过
    let bootstrap_id = *members.iter().min().unwrap_or(&node_id);
    let already = raft.is_initialized().await.unwrap_or(false);
    if already {
        tracing::info!(node_id, "Raft 已初始化，跳过 bootstrap（从持久化状态恢复）");
    } else if node_id == bootstrap_id {
        for id in &members {
            if *id != node_id {
                let _ = raft.add_learner(*id, (), false).await;
            }
        }
        tracing::info!(node_id, members = ?members, "Raft 节点 bootstrap（初始化全成员集群）");
        raft.initialize(members)
            .await
            .map_err(|e| anyhow::anyhow!("初始化集群失败: {e}"))?;
    } else {
        tracing::info!(
            node_id,
            bootstrap_id,
            "Raft 节点启动（等待 bootstrap 节点选主）"
        );
    }

    Ok(Cluster {
        node_id,
        raft,
        peers: peer_map,
    })
}
