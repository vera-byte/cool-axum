//! mod-dict：对齐 cool-admin-go modules/dict（数据字典）
//!
//! 目录结构与 Go 侧一致：
//! ```text
//! dict/
//! ├── controller/admin/   # dict_info / dict_type 控制器
//! ├── model/              # dict_info / dict_type 表定义 + initjson
//! └── service/            # dict_info.Data 等业务服务
//! ```

pub mod controller;
pub mod model;
pub mod service;

use axum::Router;
use cool_core::CrudState;
use cool_core::db::TableDef;

/// 全部表定义
pub fn all_tables() -> Vec<TableDef> {
    vec![model::dict_type(), model::dict_info()]
}

/// 需登录路由（dict_type / dict_info CRUD）
pub fn routes<S: CrudState>() -> Router<S> {
    controller::routes::<S>()
}

/// 开放路由（匿名可访问）：POST/GET /admin/dict/info/data
pub fn open_routes<S: CrudState>() -> Router<S> {
    controller::open_routes::<S>()
}

// 兼容旧路径
pub use model::initjson;

/// 模块注册声明：业务表
pub fn spec() -> cool_core::ModuleSpec {
    cool_core::ModuleSpec::new("dict").tables(all_tables())
}

/// 模块初始化（对齐 dict/dict.go init()）
pub async fn init(db: &cool_core::db::DbManager, _cache: cool_core::cache::CacheManager) {
    tracing::debug!("module dict init start ...");

    for (table, raw) in [
        ("dict_type", initjson::DICT_TYPE),
        ("dict_info", initjson::DICT_INFO),
    ] {
        match cool_core::db::fill_init_data(db, "default", table, raw).await {
            Ok(n) => tracing::debug!(table, inserted = n, "FillInitData"),
            Err(e) => tracing::error!(table, error = %e, "FillInitData 失败"),
        }
    }

    tracing::debug!("module dict init finished ...");
}

/// 路由分组
pub fn route_groups<S>() -> cool_core::RouteGroups<S>
where
    S: cool_core::CrudState + Clone + Send + Sync + 'static,
{
    cool_core::RouteGroups::new()
        .open(open_routes::<S>())
        .biz(routes::<S>())
}
