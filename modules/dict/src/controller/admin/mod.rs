//! controller/admin（对齐 controller/admin/*.go）

pub mod dict_info;
pub mod dict_type;

use axum::Router;
use cool_core::CrudState;

/// 需登录路由
pub fn routes<S: CrudState>() -> Router<S> {
    dict_type::routes::<S>().merge(dict_info::routes::<S>())
}

/// 开放路由（/data 免登录）
pub fn open_routes<S: CrudState>() -> Router<S> {
    dict_info::open_routes::<S>()
}
