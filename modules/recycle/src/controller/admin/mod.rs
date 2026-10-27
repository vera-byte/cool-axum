//! controller/admin：数据回收（仅 /admin/*）

pub mod data;

use axum::Router;
use cool_core::CrudState;

pub fn routes<S: CrudState>() -> Router<S> {
    data::routes::<S>()
}
