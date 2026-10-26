//! 模块注册：对齐 cool-admin-go `modules/modules.go`
//!
//! Go 通过 blank import 触发各模块 `init()` 中 `cool.RegisterController` 自动注册路由；
//! Rust 无副作用 init，此处显式注册模块，**路由由各模块 `route_groups()` 自动带入**。
//!
//! 新增模块：在 `registered()` 加一行，并在 Cargo.toml 引入依赖——
//! 表 / 种子 / 路由 / 启动钩子即自动接入。

use axum::Router;
use cool_core::RouteGroups;
use cool_db::{DbManager, TableDef};

/// 已注册模块清单（路由工厂）
///
/// 新增模块在此追加一行即可自动注册路由。
type RouteFactory<S> = fn() -> RouteGroups<S>;

fn registered<S>() -> Vec<(&'static str, RouteFactory<S>)>
where
    S: cool_core::CrudState
        + mod_base::comm::FileState
        + mod_task::TaskState
        + Clone
        + Send
        + Sync
        + 'static,
{
    vec![
        ("base", mod_base::route_groups::<S>),
        ("dict", mod_dict::route_groups::<S>),
        ("demo", mod_demo::route_groups::<S>),
        ("task", mod_task::route_groups::<S>),
        ("space", mod_space::route_groups::<S>),
    ]
}

/// 全部业务表（autoMigrate 用）
pub fn all_tables() -> Vec<TableDef> {
    [
        mod_base::tables::all_tables(),
        mod_dict::all_tables(),
        mod_demo::all_tables(),
        mod_task::all_tables(),
        mod_space::all_tables(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// 模块启动钩子（对齐各模块 init()）
pub async fn boot_all(db: &DbManager, cache: cool_cache::CacheManager) {
    mod_base::init(db, cache.clone()).await;
    mod_dict::init(db, cache.clone()).await;
    mod_demo::init(db, cache.clone()).await;
    mod_task::init(db, cache.clone()).await;
    mod_space::init(db, cache).await;
}

/// 组装路由：各模块 route_groups 自动注册 + 框架中间件
///
/// 对齐 cool-admin-go 中间件分层：
/// - open：免登录（/admin/*/open/*）
/// - comm：仅登录（/admin/*/comm/*）
/// - biz：登录 + 权限点
pub fn build_router<S>(
    state: S,
    sa: sa_token_plugin_axum::SaTokenState,
    extra_open: Router<S>,
    tenant_policy: cool_core::tenant::TenantPolicy,
) -> Router
where
    S: cool_core::CrudState
        + mod_base::comm::FileState
        + mod_task::TaskState
        + Clone
        + Send
        + Sync
        + 'static,
{
    let mut open = extra_open;
    let mut comm = Router::new();
    let mut biz = Router::new();

    for (name, f) in registered::<S>() {
        let g = f();
        open = open.merge(g.open);
        comm = comm.merge(g.comm);
        biz = biz.merge(g.biz);
        tracing::debug!(module = name, "路由已注册");
    }

    // 静态文件
    let static_files = tower_http::services::ServeDir::new("./public");
    let open = open.nest_service("/public", static_files);

    // biz：登录 + 权限点 + 多租户隔离
    let biz = biz
        .layer(axum::middleware::from_fn_with_state(
            mod_base::middleware::TenantLayerState {
                app: state.clone(),
                policy: tenant_policy,
            },
            mod_base::middleware::tenant_middleware,
        ))
        .layer(axum::middleware::from_fn(
            mod_base::authority::authority_middleware,
        ))
        .layer(cool_auth::check_login_layer());

    open.merge(comm)
        .merge(biz)
        .layer(cool_auth::token_layer(sa))
        .layer(axum::middleware::from_fn(
            mod_base::middleware::normalize_token_header,
        ))
        .layer(axum::middleware::from_fn(mod_base::middleware::request_log))
        // 链路追踪最外层：生成 X-Trace-Id（含节点）并写响应头
        .layer(axum::middleware::from_fn(
            mod_base::middleware::trace_id_middleware,
        ))
        .with_state(state)
}
