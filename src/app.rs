//! 应用装配：配置 → 数据源 → sa-token → 路由 → 启动 HTTP 服务

use anyhow::Context;
use axum::Json;
use axum::routing::get;
use cool_core::CrudState;
use cool_core::auth::AuthState as SaTokenState;
use cool_core::cache::CacheManager;
use cool_core::config::AppConfig;
use cool_core::db::DbManager;
use cool_core::file::FileDriver;
use cool_core::response::{BaseRes, ok};
use mod_task::TaskState;
use mod_task::scheduler::Scheduler;
use std::sync::Arc;

/// 全局应用状态
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: DbManager,
    pub sa: SaTokenState,
    pub scheduler: Arc<Scheduler>,
    pub file: Arc<FileDriver>,
    pub cache: CacheManager,
    pub raft: Option<cool_core::raft::Cluster>,
}

impl CrudState for AppState {
    fn db(&self) -> &DbManager {
        &self.db
    }
}

impl TaskState for AppState {
    fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }
}

impl mod_base::comm::FileState for AppState {
    fn file_driver(&self) -> &FileDriver {
        &self.file
    }
}

pub async fn serve(config: AppConfig) -> anyhow::Result<()> {
    // 链路节点身份：X-Trace-Id 前缀 / X-Node-Label
    mod_base::middleware::init_node(
        config.raft.node_id,
        &normalize_addr(&config.server.address),
        config.server.node_label.as_deref(),
    );

    // 初始化数据源
    let db = DbManager::init(&config.database)
        .await
        .context("初始化数据库失败")?;

    // autoMigrate（各模块 spec 声明业务表）
    if config.cool.auto_migrate {
        auto_migrate(&db).await;
    }

    // 任务调度器
    let scheduler = Arc::new(Scheduler::new(db.clone()));
    scheduler.boot().await;

    // 健康检查放后台，不阻塞启动
    {
        let db = db.clone();
        tokio::spawn(async move {
            if let Err(e) = db.ping_all().await {
                tracing::warn!(error = %e, "数据库健康检查失败（启动继续，等待连接恢复）");
            }
        });
    }

    // sa-token：有 redis.cool 配置则用 Redis 会话（多节点共享），否则内存
    let redis_url = config
        .cool_redis()
        .map(|r| match r.password.as_deref().unwrap_or("") {
            "" => format!("redis://{}/{}", r.address, r.db),
            pass => format!("redis://:{}@{}/{}", pass, r.address, r.db),
        });
    let sa = cool_core::auth::init_sa_token(&config.modules.base.auth, redis_url.as_deref()).await;

    // 缓存管理器
    let cache = match &redis_url {
        Some(url) => CacheManager::new_redis(url).await,
        None => {
            tracing::info!("CacheManager 使用内存模式");
            CacheManager::new_memory()
        }
    };

    // 分布式全局事件总线（Redis Pub/Sub；无 Redis 降级进程内）
    let event_bus =
        cool_core::EventBus::new(cool_core::EventBusConfig::new(redis_url.clone(), "cool")).await;
    // 设为全局，供模块 init 注册处理器、notify_soft_delete 等无句柄调用点 emit
    cool_core::init_global(event_bus.clone());

    // 模块启动钩子（缓存注入、参数预热、种子管理员等；可在此注册事件处理器）
    modules::boot_all(&db, cache.clone()).await;
    // 处理器注册完成后再启动 Redis 订阅
    event_bus.start_listener();
    // 启动生命周期事件（对齐 cool-admin-midway configuration 的 emit('onReady'/'onServerReady')）
    cool_core::event
        .emit(cool_core::Event::Ready, serde_json::json!({}))
        .await;
    cool_core::event
        .emit(cool_core::Event::ServerReady, serde_json::json!({}))
        .await;

    // 文件上传驱动
    let file = match cool_core::file::create_driver(&config.cool.file) {
        Ok(d) => {
            tracing::info!(mode = d.mode_name(), "文件上传驱动就绪");
            Arc::new(d)
        }
        Err(e) => {
            tracing::error!(error = %e, "文件上传驱动初始化失败，回退 local");
            Arc::new(cool_core::file::FileDriver::Local(
                cool_core::file::LocalDriver::new(&config.cool.file),
            ))
        }
    };

    // Raft 集群（可选）：启用后用于选主，Leader 执行定时任务
    // dataDir 按节点隔离（dataDir/n{nodeId}），避免多节点抢 RocksDB 锁
    let raft = if config.raft.enable {
        if let Err(e) = validate_raft_config(&config) {
            tracing::error!(error = e, "Raft 配置校验失败，继续单机模式");
            None
        } else {
            let peers: Vec<(cool_core::raft::NodeId, String)> = config
                .raft
                .peers
                .iter()
                .filter_map(|p| {
                    let mut it = p.splitn(2, ':');
                    let id: u64 = it.next()?.parse().ok()?;
                    let addr = it.next()?.to_string();
                    Some((id, addr))
                })
                .collect();
            let data_dir = std::path::PathBuf::from(&config.raft.data_dir)
                .join(format!("n{}", config.raft.node_id));
            tracing::info!(node_id = config.raft.node_id, data_dir = %data_dir.display(), "Raft 节点数据目录");
            match cool_core::raft::start(
                config.raft.node_id,
                config.raft.self_addr.clone(),
                peers,
                data_dir,
                None,
            )
            .await
            {
                Ok(c) => {
                    tracing::info!(node_id = c.node_id, "Raft 集群已启动");
                    // Leader 门控：初始置 false，后台轮询更新（避免 Follower 启动窗口误执行）
                    scheduler.set_leader_ok(false);
                    {
                        let c = c.clone();
                        let flag = scheduler.leader_flag();
                        tokio::spawn(async move {
                            loop {
                                flag.store(
                                    c.verify_quorum_leadership().await,
                                    std::sync::atomic::Ordering::Relaxed,
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            }
                        });
                    }
                    Some(c)
                }
                Err(e) => {
                    tracing::error!(error = %e, "Raft 启动失败，继续单机模式");
                    None
                }
            }
        }
    } else {
        tracing::info!("Raft 未启用（单机模式，任务本地执行）");
        None
    };

    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        sa,
        scheduler,
        file,
        cache,
        raft,
    };

    // 框架级路由（健康检查/服务信息/OpenAPI）
    let openapi_path = normalize_route_path(&config.server.openapi_path);
    let swagger_path = normalize_route_path(&config.server.swagger_path);
    let openapi_path_for_html = openapi_path.clone();

    let mut extra_open = axum::Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/base/health", get(health))
        .route(
            openapi_path.as_str(),
            get(|| async { axum::Json(crate::openapi::openapi_json()) }),
        )
        .route(
            swagger_path.as_str(),
            get(move || {
                let api = openapi_path_for_html.clone();
                async move { axum::response::Html(crate::openapi::swagger_html(&api)) }
            }),
        )
        .route("/raft/status", get(raft_status))
        .route("/raft/add-learner", axum::routing::post(raft_add_learner))
        .route(
            "/raft/change-membership",
            axum::routing::post(raft_membership),
        );

    // Raft RPC 入站路由（/raft/vote|append|snapshot）必须始终挂载在开放组：
    // 未启用/启动失败时返回明确错误，避免对端收到 401 无法选举
    extra_open = extra_open.merge(cool_core::raft::network::raft_routes(
        cool_core::raft::network::RaftRpcState {
            raft: state.raft.as_ref().map(|c| c.raft.clone()),
        },
    ));

    // 业务路由由 modules 注册中心自动装配
    let tenant_policy = cool_core::tenant::TenantPolicy {
        enable: state.config.cool.tenant.enable,
        urls: state.config.cool.tenant.urls.clone(),
    };
    let app = modules::build_router(state.clone(), state.sa.clone(), extra_open, tenant_policy);

    let addr = normalize_addr(&config.server.address);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("监听地址失败: {addr}"))?;

    tracing::info!("cool-axum 已启动，监听 {}", addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .context("HTTP 服务异常退出")?;
    Ok(())
}

/// ":8001" → "0.0.0.0:8001"
fn normalize_addr(addr: &str) -> String {
    if addr.starts_with(':') {
        format!("0.0.0.0{addr}")
    } else {
        addr.to_string()
    }
}

/// Raft 配置自检：避免多节点误用同一 nodeId / 端口错配
fn validate_raft_config(config: &AppConfig) -> Result<(), String> {
    let nid = config.raft.node_id;
    let self_addr = config.raft.self_addr.trim().trim_end_matches('/');
    if self_addr.is_empty() {
        return Err("raft.selfAddr 不能为空".into());
    }

    // selfAddr 端口须与 server.address 一致（否则对端连不上本节点）
    let listen = config.server.address.trim_start_matches(':');
    let self_port = self_addr.rsplit(':').next().unwrap_or("");
    if !listen.is_empty() && !self_port.is_empty() && listen != self_port {
        return Err(format!(
            "raft.selfAddr 端口 ({self_port}) 与 server.address 端口 ({listen}) 不一致，             多节点请为每个进程单独配置 nodeId/selfAddr/server.address"
        ));
    }

    // peers 中必须有本节点，且 id 与 nodeId 一致
    let mut peer_ids = std::collections::HashSet::new();
    let mut self_listed = false;
    for p in &config.raft.peers {
        let mut it = p.splitn(2, ':');
        let id: u64 = it
            .next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| format!("peers 条目格式应为 id:addr，收到 {p}"))?;
        let addr = it.next().unwrap_or("").trim_end_matches('/');
        if !peer_ids.insert(id) {
            return Err(format!("peers 中 nodeId={id} 重复"));
        }
        if id == nid {
            self_listed = true;
            if addr != self_addr {
                return Err(format!(
                    "peers 中 nodeId={nid} 的地址 ({addr}) 与 selfAddr ({self_addr}) 不一致"
                ));
            }
        }
    }
    if !config.raft.peers.is_empty() && !self_listed {
        return Err(format!(
            "peers 中缺少本节点 nodeId={nid}（当前 selfAddr={self_addr}）"
        ));
    }
    Ok(())
}

/// autoMigrate：按配置创建全部模块表（IF NOT EXISTS）
async fn auto_migrate(db: &DbManager) {
    let Ok(pool) = db.default_pool() else {
        tracing::warn!("autoMigrate 跳过：默认数据源未就绪");
        return;
    };
    let kind = pool.kind();

    let mut tables = Vec::new();
    tables.push(cool_core::db::base_sys_init_def());
    tables.extend(modules::all_tables());

    let mut ok = 0;
    let mut fail = 0;
    for def in &tables {
        let sql = cool_core::db::build_create_table_sql(kind, def);
        match pool.execute(&sql).await {
            Ok(_) => ok += 1,
            Err(e) => {
                fail += 1;
                tracing::error!(table = %def.name, error = %e, "autoMigrate 建表失败");
            }
        }
        // 旧表补列（如新增 tenantId）
        if let Err(e) = cool_core::db::ensure_columns(pool, kind, def).await {
            tracing::warn!(table = %def.name, error = %e, "autoMigrate 同步列失败");
        }
    }
    tracing::info!(ok, fail, "autoMigrate 完成");
}

/// 服务信息
async fn index(axum::extract::State(state): axum::extract::State<AppState>) -> Json<BaseRes> {
    Json(ok(serde_json::json!({
        "name": "cool-axum",
        "version": env!("CARGO_PKG_VERSION"),
        "runMode": if state.config.cool.eps { "eps" } else { "normal" },
        "autoMigrate": state.config.cool.auto_migrate,
        "docs": "/docs"
    })))
}

/// 健康检查：DB + 缓存
async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> Json<BaseRes> {
    let mut db_ok = true;
    let mut detail = serde_json::Map::new();

    match state.db.ping_all().await {
        Ok(()) => {
            detail.insert("database".into(), serde_json::json!("up"));
        }
        Err(e) => {
            db_ok = false;
            detail.insert("database".into(), serde_json::json!(format!("down: {e}")));
        }
    }
    detail.insert("pools".into(), serde_json::json!(state.db.pool_count()));
    detail.insert(
        "cache".into(),
        serde_json::json!(if state.cache.is_redis_mode() {
            "redis"
        } else {
            "memory"
        }),
    );

    let code = if db_ok {
        cool_core::response::CODE_OK
    } else {
        cool_core::response::CODE_SERVER
    };
    Json(BaseRes::new(
        code,
        if db_ok { "success" } else { "degraded" },
        Some(serde_json::Value::Object(detail)),
    ))
}

/// GET /raft/status —— 集群状态
async fn raft_status(axum::extract::State(state): axum::extract::State<AppState>) -> Json<BaseRes> {
    match &state.raft {
        Some(c) => Json(ok(c.status().await)),
        None => Json(ok(serde_json::json!({
            "enabled": false,
            "mode": "standalone",
        }))),
    }
}

#[derive(serde::Deserialize)]
struct LearnerReq {
    id: u64,
    addr: String,
}

/// POST /raft/add-learner —— 添加学习者（非 Leader 自动转发到 Leader）
async fn raft_add_learner(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
) -> axum::response::Response {
    forward_or_run_raft_admin(state, req, |c, body: LearnerReq| async move {
        c.add_learner(body.id, body.addr).await
    })
    .await
}

#[derive(serde::Deserialize)]
struct MembershipReq {
    members: Vec<u64>,
}

/// POST /raft/change-membership —— 变更成员（非 Leader 自动转发到 Leader）
async fn raft_membership(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
) -> axum::response::Response {
    forward_or_run_raft_admin(state, req, |c, body: MembershipReq| async move {
        c.change_membership(body.members).await
    })
    .await
}

/// Raft 管理操作：本节点是 Leader 则本地执行，否则 HTTP 转发到 Leader
///
/// - 转发时透传 `X-Trace-Id` 等链路头
/// - 响应头强制写回**客户端原始** `X-Trace-Id`，并标注 `X-Served-Node`
async fn forward_or_run_raft_admin<F, Fut, T>(
    state: AppState,
    req: axum::extract::Request,
    op: F,
) -> axum::response::Response
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(cool_core::raft::Cluster, T) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    use axum::body::Body;
    use axum::http::{HeaderValue, StatusCode};
    use axum::response::{IntoResponse, Response};

    let (parts, body) = req.into_parts();
    let orig_trace = mod_base::middleware::trace_from_headers(&parts.headers).or_else(|| {
        parts
            .extensions
            .get::<mod_base::middleware::TraceId>()
            .map(|t| t.0.clone())
    });
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().map(|s| s.to_string());
    let method = parts.method.clone();

    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(cool_core::response::fail(format!("读取请求体失败: {e}"))),
            )
                .into_response();
        }
    };

    let Some(cluster) = state.raft.clone() else {
        return (
            StatusCode::OK,
            Json(cool_core::response::fail("Raft 未启用")),
        )
            .into_response();
    };

    // 本节点是 Leader：本地执行
    if cluster.is_leader_node().await {
        let body: T = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(cool_core::response::fail(format!("参数错误: {e}"))),
                )
                    .into_response();
            }
        };
        tracing::info!(
            trace = orig_trace.as_deref().unwrap_or(""),
            path = %path,
            "Raft 管理请求本节点(Leader)执行"
        );
        let res = match op(cluster, body).await {
            Ok(()) => Json(ok(serde_json::json!({ "ok": true }))),
            Err(e) => Json(cool_core::response::fail(e)),
        };
        return attach_orig_trace(res.into_response(), orig_trace.as_deref(), "leader-local");
    }

    // 非 Leader：转发到 Leader，并回写原始 trace id
    let Some(leader_addr) = cluster.leader_addr().await else {
        return (
            StatusCode::OK,
            Json(cool_core::response::fail("未知 Leader，无法转发")),
        )
            .into_response();
    };

    match cool_core::raft::forward_to_leader(
        &leader_addr,
        method.as_str(),
        &path,
        query.as_deref(),
        &parts.headers,
        bytes.to_vec(),
    )
    .await
    {
        Ok((status, hdrs, body_vec)) => {
            tracing::info!(
                trace = orig_trace.as_deref().unwrap_or(""),
                leader = %leader_addr,
                path = %path,
                "Raft 管理请求已转发到 Leader"
            );
            let mut resp = Response::new(Body::from(body_vec));
            *resp.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            let h = resp.headers_mut();
            for (k, v) in hdrs.iter() {
                if k.as_str().eq_ignore_ascii_case("x-trace-id") {
                    continue; // 稍后用原始 ID 覆盖
                }
                h.insert(k, v.clone());
            }
            if let Some(t) = &orig_trace
                && let Ok(v) = HeaderValue::from_str(t)
            {
                h.insert("x-trace-id", v);
            }
            if let Ok(v) = HeaderValue::from_str(&leader_addr) {
                h.insert("x-served-node", v);
            }
            resp
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(cool_core::response::fail(e))).into_response(),
    }
}

/// 响应写回原始 trace id
fn attach_orig_trace(
    mut resp: axum::response::Response,
    orig_trace: Option<&str>,
    served: &str,
) -> axum::response::Response {
    use axum::http::HeaderValue;
    if let Some(t) = orig_trace
        && let Ok(v) = HeaderValue::from_str(t)
    {
        resp.headers_mut().insert("x-trace-id", v);
    }
    if let Ok(v) = HeaderValue::from_str(served) {
        resp.headers_mut().insert("x-served-node", v);
    }
    resp
}

/// 规范化路由路径（保证以 / 开头）
fn normalize_route_path(p: &str) -> String {
    let p = p.trim();
    if p.is_empty() {
        return "/api.json".into();
    }
    if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    }
}
