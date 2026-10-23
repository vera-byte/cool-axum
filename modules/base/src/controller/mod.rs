//! controller 层：路由注册（对齐 modules/base/controller/*.go）
//!
//! - admin：后台管理接口（/admin/base/*）
//! - app：移动端接口（/app/base/*）

pub mod admin;
pub mod app;

use axum::Router;
use cool_core::CrudState;

use admin::base_comm::FileState;

/// 注册 base 模块全部路由
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    admin::routes::<S>().merge(app::routes::<S>())
}
