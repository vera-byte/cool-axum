//! 应用装配：配置 → 数据源 → sa-token → 路由 → 启动 HTTP 服务

use anyhow::Context;
use axum::Json;
use axum::routing::get;
use cool_auth::AuthState as SaTokenState;
use cool_cache::CacheManager;
use cool_config::AppConfig;
use cool_core::CrudState;
use cool_core::response::{BaseRes, ok};
use cool_db::DbManager;
use cool_file::FileDriver;
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
    let sa = cool_auth::init_sa_token(&config.modules.base.auth, redis_url.as_deref()).await;

    // 缓存管理器
    let cache = match &redis_url {
        Some(url) => CacheManager::new_redis(url).await,
        None => {
            tracing::info!("CacheManager 使用内存模式");
            CacheManager::new_memory()
        }
    };
    // 模块启动钩子（缓存注入、参数预热、种子管理员等）
    modules::boot_all(&db, cache.clone()).await;

    // 文件上传驱动
    let file = match cool_file::create_driver(&config.cool.file) {
        Ok(d) => {
            tracing::info!(mode = d.mode_name(), "文件上传驱动就绪");
            Arc::new(d)
        }
        Err(e) => {
            tracing::error!(error = %e, "文件上传驱动初始化失败，回退 local");
            Arc::new(cool_file::FileDriver::Local(cool_file::LocalDriver::new(
                &config.cool.file,
            )))
        }
    };

    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        sa,
        scheduler,
        file,
        cache,
    };

    // 框架级路由（健康检查/服务信息）
    let extra_open = axum::Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/base/health", get(health));

    // 业务路由由 modules 注册中心自动装配
    let app = modules::build_router(state.clone(), state.sa.clone(), extra_open);

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

/// autoMigrate：按配置创建全部模块表（IF NOT EXISTS）
async fn auto_migrate(db: &DbManager) {
    let Ok(pool) = db.default_pool() else {
        tracing::warn!("autoMigrate 跳过：默认数据源未就绪");
        return;
    };
    let kind = pool.kind();

    let mut tables = Vec::new();
    tables.push(cool_db::base_sys_init_def());
    tables.extend(modules::all_tables());

    let mut ok = 0;
    let mut fail = 0;
    for def in &tables {
        let sql = cool_db::build_create_table_sql(kind, def);
        match pool.execute(&sql).await {
            Ok(_) => ok += 1,
            Err(e) => {
                fail += 1;
                tracing::error!(table = %def.name, error = %e, "autoMigrate 建表失败");
            }
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
