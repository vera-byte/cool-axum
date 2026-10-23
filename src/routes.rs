//! 路由装配

mod auth;

use crate::app::AppState;
use axum::routing::get;
use axum::{Json, Router};
use cool_core::response::{ok, BaseRes};

pub fn router(state: AppState) -> Router {
    // 开放路由（免登录）
    let public = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/base/health", get(health))
        .route("/base/login", axum::routing::post(auth::login))
        .route("/base/logout", axum::routing::post(auth::logout))
        .merge(mod_base::open_routes())
        .merge(mod_dict::open_routes());

    // 静态文件
    let static_files = tower_http::services::ServeDir::new("./public");
    let public = public.nest_service("/public", static_files);

    // comm 路由（仅需登录；handler 自解析 token）
    let comm = Router::new()
        .merge(mod_base::comm_routes())
        .route("/base/userinfo", get(auth::userinfo));

    // 业务路由（登录 + 权限点校验；超管 userId=1 / perms=* 放行）
    let protected = Router::new()
        .merge(mod_base::routes())
        .merge(mod_dict::routes())
        .merge(mod_demo::routes())
        .merge(mod_task::routes())
        .merge(mod_space::routes())
        .layer(axum::middleware::from_fn(
            mod_base::authority::authority_middleware,
        ))
        .layer(cool_auth::check_login_layer());

    public
        .merge(comm)
        .merge(protected)
        .layer(cool_auth::token_layer(state.sa.clone()))
        .layer(axum::middleware::from_fn(
            mod_base::middleware::normalize_token_header,
        ))
        .layer(axum::middleware::from_fn(
            mod_base::middleware::request_log,
        ))
        .with_state(state)
}

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
