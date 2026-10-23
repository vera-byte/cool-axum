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
use cool_db::TableDef;

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

/// 模块注册声明（对齐 dict/dict.go init）
pub fn spec() -> cool_core::ModuleSpec {
    cool_core::ModuleSpec::new("dict")
        .tables(all_tables())
        .seeds(vec![
            ("dict_type", initjson::DICT_TYPE),
            ("dict_info", initjson::DICT_INFO),
        ])
}

/// 模块启动钩子
pub async fn init(_db: &cool_db::DbManager) {}
