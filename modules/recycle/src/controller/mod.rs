//! controller：admin 回收站接口

pub mod admin;

use axum::Router;
use cool_core::CrudState;

pub fn routes<S: CrudState>() -> Router<S> {
    admin::routes::<S>()
}
