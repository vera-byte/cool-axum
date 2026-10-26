//! mod-base：对齐 cool-admin-go modules/base
//!
//! 目录结构与 Go 侧一致：
//! ```text
//! base/
//! ├── api/          接口请求类型
//! ├── config/       模块配置
//! ├── controller/   控制器（admin / app）
//! ├── funcs/        任务函数
//! ├── middleware/   中间件（authority / log）
//! ├── model/        数据表定义 + initjson
//! ├── service/      业务服务
//! └── utils/        工具函数
//! ```

pub mod api;
pub mod config;
pub mod controller;
pub mod funcs;
pub mod middleware;
pub mod model;
pub mod service;
pub mod utils;

// ── 兼容 re-export（cool-axum 使用的入口） ──────────────────────────
pub use controller::admin::base_comm as comm;
pub use controller::routes as routes_all;
pub use middleware::authority;
pub use model as tables;
pub use model::initjson;
pub use service::base_sys_login as login;
pub use service::base_sys_param as param_svc;
pub use service::base_sys_perms as perms;
pub use service::base_sys_user as user_svc;
pub use service::captcha;

use axum::Router;
use controller::admin::base_comm::FileState;
use cool_core::CrudState;

/// 开放路由（免登录）：captcha / login / eps
pub fn open_routes<S: CrudState + FileState>() -> Router<S> {
    controller::admin::base_open::routes::<S>()
}

/// comm 路由（仅需登录，handler 自解析 token；对齐 AuthComm）
pub fn comm_routes<S: CrudState + FileState>() -> Router<S> {
    controller::admin::base_comm::routes::<S>().merge(controller::app::routes::<S>())
}

/// 业务路由（登录 + 权限点校验）
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    controller::admin::biz_routes::<S>()
}

// ==================== 模块初始化（对齐 base/base.go init） ====================

/// 模块注册声明：业务表（autoMigrate）
pub fn spec() -> cool_core::ModuleSpec {
    cool_core::ModuleSpec::new("base").tables(model::all_tables())
}

/// 模块初始化（对齐 base/base.go init()）
///
/// 逐表 FillInitData + 模块专属启动
pub async fn init(db: &cool_db::DbManager, cache: cool_cache::CacheManager) {
    tracing::debug!("module base init start ...");

    // FillInitData：initjson 种子数据（base_sys_init 去重，仅首次）
    for (table, raw) in [
        ("base_sys_menu", initjson::MENU),
        ("base_sys_user", initjson::USER),
        ("base_sys_user_role", initjson::USER_ROLE),
        ("base_sys_role", initjson::ROLE),
        ("base_sys_role_menu", initjson::ROLE_MENU),
        ("base_sys_department", initjson::DEPARTMENT),
        ("base_sys_role_department", initjson::ROLE_DEPARTMENT),
        ("base_sys_param", initjson::PARAM),
        ("base_sys_conf", initjson::CONF),
    ] {
        match cool_db::fill_init_data(db, "default", table, raw).await {
            Ok(n) => tracing::debug!(table, inserted = n, "FillInitData"),
            Err(e) => tracing::error!(table, error = %e, "FillInitData 失败"),
        }
    }

    // 模块专属启动：缓存注入、参数预热、种子管理员
    login::init_cache(cache.clone());
    captcha::init_cache(cache);
    middleware::log::init(db.clone());
    login::refresh_param_cache(db).await;
    login::seed_admin_if_empty(db).await;

    tracing::debug!("module base init finished ...");
}

/// 路由分组（open / comm / biz），由注册中心自动装配
pub fn route_groups<S>() -> cool_core::RouteGroups<S>
where
    S: CrudState + FileState + Clone + Send + Sync + 'static,
{
    cool_core::RouteGroups::new()
        .open(open_routes::<S>())
        .comm(comm_routes::<S>())
        .biz(routes::<S>())
}
