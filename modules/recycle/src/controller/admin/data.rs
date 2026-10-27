//! recycle 数据回收站控制器（对齐 midway AdminRecycleDataController）
//!
//! - GET  /admin/recycle/data/info
//! - POST /admin/recycle/data/page   （join 用户名）
//! - POST /admin/recycle/data/restore

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use cool_core::controller::{ApiAllow, crud_router};
use cool_core::response::{BaseRes, fail, ok};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};
use serde_json::{Value, json};

use crate::service::data as data_svc;

fn service() -> CrudService {
    CrudService::new("recycle_data")
        .list_op(
            QueryOp::new()
                .as_alias("a")
                .select("a.*, b.name AS userName, b.username AS username")
                .left_join("base_sys_user", "b", "a.userId = b.id")
                .field_eq(&["a.userId"])
                .key_word_field(&["a.url", "b.name"])
                .add_orderby("a.id", "desc"),
        )
        .page_op(
            QueryOp::new()
                .as_alias("a")
                .select("a.*, b.name AS userName, b.username AS username")
                .left_join("base_sys_user", "b", "a.userId = b.id")
                .field_eq(&["a.userId"])
                .key_word_field(&["a.url", "b.name"])
                .add_orderby("a.id", "desc"),
        )
}

/// 注册路由（仅 /admin，对齐 controller/admin）
pub fn routes<S: CrudState>() -> Router<S> {
    let svc = service();
    // 只开 info / page（不可在此改删数据）
    let api = ApiAllow {
        add: false,
        delete: true,
        update: false,
        info: true,
        list: true,
        page: true,
    };
    let mut r = Router::new();
    r = r.merge(crud_router("/admin/recycle/data", svc, api));
    r = r.route("/admin/recycle/data/restore", post(restore::<S>));
    r = r.route("/admin/recycle/clear", post(clear::<S>));
    r
}

/// POST /admin/recycle/data/restore  body: {"ids":[1,2]}
async fn restore<S: CrudState>(State(st): State<S>, Json(body): Json<Value>) -> Json<BaseRes> {
    let ids: Vec<i64> = body
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
        .unwrap_or_default();
    if ids.is_empty() {
        return Json(fail("请选择要恢复的数据"));
    }
    match data_svc::restore(CrudState::db(&st), &ids).await {
        Ok(n) => Json(ok(json!({ "rows": n }))),
        Err(e) => Json(fail(e)),
    }
}

/// POST /admin/recycle/clear  body: {"all": true|false}
async fn clear<S: CrudState>(State(st): State<S>, Json(body): Json<Value>) -> Json<BaseRes> {
    let is_all = body.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
    match data_svc::clear(CrudState::db(&st), is_all).await {
        Ok(n) => Json(ok(json!({ "rows": n }))),
        Err(e) => Json(fail(e)),
    }
}
