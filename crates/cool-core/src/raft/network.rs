//! Raft 网络层：HTTP 传输
//!
//! - `HttpNetworkFactory`：出站 RPC（reqwest）
//! - `raft_routes`：入站 RPC（axum）

use crate::raft::{NodeId, PeerMap, TypeConfig};
use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use openraft::alias::{SnapshotOf, VoteOf};
use openraft::anyerror::AnyError;
use openraft::error::{NetworkError, RPCError, ReplicationClosed, StreamingError};
use openraft::network::{RPCOption, RaftNetworkFactory, RaftNetworkV2};
use openraft::raft::SnapshotResponse;
use openraft::raft::{AppendEntriesRequest, AppendEntriesResponse, VoteRequest, VoteResponse};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

pub type SnapshotData = Cursor<Vec<u8>>;

/// 出站网络工厂
pub struct HttpNetworkFactory {
    peers: PeerMap,
    client: reqwest::Client,
}

impl HttpNetworkFactory {
    pub fn new(peers: PeerMap) -> Self {
        Self {
            peers,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl RaftNetworkFactory<TypeConfig> for HttpNetworkFactory {
    type Network = HttpConnection;

    async fn new_client(&mut self, target: NodeId, _node: &()) -> Self::Network {
        let addr = self
            .peers
            .lock()
            .await
            .get(&target)
            .cloned()
            .unwrap_or_default();
        HttpConnection {
            target_addr: addr,
            client: self.client.clone(),
        }
    }
}

/// 出站连接
pub struct HttpConnection {
    target_addr: String,
    client: reqwest::Client,
}

impl HttpConnection {
    async fn send<Req, Resp>(&self, path: &str, req: &Req) -> Result<Resp, NetworkError<TypeConfig>>
    where
        Req: Serialize,
        Resp: for<'de> Deserialize<'de>,
    {
        if self.target_addr.is_empty() {
            return Err(NetworkError::new(&AnyError::error("节点地址未登记")));
        }
        let url = format!("{}/raft/{}", self.target_addr.trim_end_matches('/'), path);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| NetworkError::new(&e))?;
        if !resp.status().is_success() {
            return Err(NetworkError::new(&AnyError::error(format!(
                "HTTP {}",
                resp.status()
            ))));
        }
        // 入站 handler 返回 Result<Resp, String>
        let wrapped: Result<Resp, String> = resp.json().await.map_err(|e| NetworkError::new(&e))?;
        wrapped.map_err(|e| NetworkError::new(&AnyError::error(e)))
    }
}

impl RaftNetworkV2<TypeConfig> for HttpConnection {
    type SnapshotData = SnapshotData;

    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<TypeConfig>,
        option: RPCOption,
    ) -> Result<AppendEntriesResponse<TypeConfig>, RPCError<TypeConfig>> {
        let _ = option;
        self.send("append", &rpc).await.map_err(RPCError::Network)
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<TypeConfig>,
        option: RPCOption,
    ) -> Result<VoteResponse<TypeConfig>, RPCError<TypeConfig>> {
        let _ = option;
        self.send("vote", &rpc).await.map_err(RPCError::Network)
    }

    async fn full_snapshot(
        &mut self,
        _vote: VoteOf<TypeConfig>,
        _snapshot: SnapshotOf<TypeConfig, Self::SnapshotData>,
        _cancel: impl Future<Output = ReplicationClosed> + openraft::OptionalSend + 'static,
        _option: RPCOption,
    ) -> Result<SnapshotResponse<TypeConfig>, StreamingError<TypeConfig>> {
        // 内存状态机规模极小，快照传输暂不实现
        Err(StreamingError::Closed(ReplicationClosed::new(
            "snapshot not implemented",
        )))
    }
}

// ── 入站 RPC ──────────────────────────────────────────────

#[derive(Clone)]
pub struct RaftRpcState {
    /// None = Raft 未启用或启动失败（仍须挂载 RPC 路由，避免对端收到 401）
    pub raft: Option<crate::raft::Raft>,
}

/// RPC 路由：挂载 /raft/*（必须在开放路由组，免鉴权）
pub fn raft_routes<S>(rpc: RaftRpcState) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/raft/vote", post(vote))
        .route("/raft/append", post(append))
        .route("/raft/snapshot", post(snapshot))
        .with_state(rpc)
}

async fn vote(
    State(st): State<RaftRpcState>,
    Json(req): Json<VoteRequest<TypeConfig>>,
) -> Json<Result<VoteResponse<TypeConfig>, String>> {
    let Some(raft) = &st.raft else {
        return Json(Err("Raft 未启用或启动失败".into()));
    };
    match raft.vote(req).await {
        Ok(r) => Json(Ok(r)),
        Err(e) => Json(Err(e.to_string())),
    }
}

async fn append(
    State(st): State<RaftRpcState>,
    Json(req): Json<AppendEntriesRequest<TypeConfig>>,
) -> Json<Result<AppendEntriesResponse<TypeConfig>, String>> {
    let Some(raft) = &st.raft else {
        return Json(Err("Raft 未启用或启动失败".into()));
    };
    match raft.append_entries(req).await {
        Ok(r) => Json(Ok(r)),
        Err(e) => Json(Err(e.to_string())),
    }
}

async fn snapshot() -> axum::http::StatusCode {
    axum::http::StatusCode::NOT_IMPLEMENTED
}

#[allow(dead_code)]
fn _unused(_: TypeConfig) {}
