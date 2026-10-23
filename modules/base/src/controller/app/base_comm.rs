//! app 端 base_comm（对齐 controller/app/base_comm.go）—— AppEPS

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use cool_core::response::{fail, ok, BaseRes};
use cool_core::CrudState;

use crate::controller::admin::base_comm::FileState;
use crate::service::base_open;

/// 注册 /app/base/comm/*
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    Router::new().route("/app/base/comm/eps", get(eps::<S>))
}

async fn eps<S: CrudState + FileState>(State(st): State<S>) -> Json<BaseRes> {
    if !CrudState::db(&st).pool_count() > 0 {
        return Json(fail("eps is not open"));
    }
    Json(ok(base_open::app_eps(CrudState::db(&st)).await))
}
