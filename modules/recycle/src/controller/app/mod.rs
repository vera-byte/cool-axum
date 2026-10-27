//! controller/app：移动端控制器（按需补充）

use axum::Router;
use cool_core::CrudState;

pub fn routes<S: CrudState>() -> Router<S> {
    Router::new()
}
