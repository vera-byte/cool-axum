//! controller/app：移动端控制器（对齐 controller/app/base_comm.go）

pub mod base_comm;

use axum::Router;
use cool_core::CrudState;

use crate::controller::admin::base_comm::FileState;

pub fn routes<S: CrudState + FileState>() -> Router<S> {
    base_comm::routes::<S>()
}
