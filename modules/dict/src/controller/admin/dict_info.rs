//! dict_info 控制器（对齐 controller/admin/dict_info.go）
//!
//! - 通用 CRUD：/admin/dict/info、/dict/info
//! - 开放接口 Data：POST /admin/dict/info/data（**匿名可访问**）

use axum::extract::State;
use axum::http::Method;
use axum::routing::post;
use axum::{Json, Router};
use cool_core::controller::{crud_router, ApiAllow};
use cool_core::response::{ok, BaseRes};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};
use serde::Deserialize;

use crate::service::dict_info as dict_info_svc;

fn service() -> CrudService {
    CrudService::new("dict_info")
        .not_null_key("name", "名称不能为空")
        .not_null_key("typeId", "类型ID不能为空")
        .list_op(QueryOp::new().field_eq(&["typeId", "parentId"]).key_word_field(&["name"]))
        .page_op(
            QueryOp::new()
                .field_eq(&["typeId", "parentId"])
                .key_word_field(&["name"])
                .add_orderby("orderNum", "asc"),
        )
}

/// 需登录 CRUD（不含 /data）
pub fn routes<S: CrudState>() -> Router<S> {
    let svc = service();
    let mut r = Router::new();
    for prefix in ["/dict/info", "/admin/dict/info"] {
        r = r.merge(crud_router(prefix, svc.clone(), ApiAllow::default()));
    }
    r
}

#[derive(Debug, Deserialize)]
pub struct DataReq {
    #[serde(default)]
    pub types: Option<Vec<String>>,
}

/// 开放接口 Data：匿名可访问
///
/// POST /admin/dict/info/data  body: {"types":["type","brand"]}
/// GET  /admin/dict/info/data?types=type&types=brand
pub fn open_routes<S: CrudState>() -> Router<S> {
    Router::new()
        .route("/admin/dict/info/data", post(data_post::<S>).get(data_get::<S>))
        .route("/dict/info/data", post(data_post::<S>).get(data_get::<S>))
}

async fn data_post<S: CrudState>(
    State(st): State<S>,
    Json(req): Json<DataReq>,
) -> Json<BaseRes> {
    Json(match dict_info_svc::data(CrudState::db(&st), req.types).await {
        Ok(v) => ok(v),
        Err(e) => cool_core::response::fail(e),
    })
}

#[derive(Debug, Deserialize)]
pub struct DataQuery {
    #[serde(default)]
    pub types: Option<Vec<String>>,
}

async fn data_get<S: CrudState>(
    State(st): State<S>,
    axum::extract::Query(q): axum::extract::Query<DataQuery>,
) -> Json<BaseRes> {
    Json(match dict_info_svc::data(CrudState::db(&st), q.types).await {
        Ok(v) => ok(v),
        Err(e) => cool_core::response::fail(e),
    })
}

#[allow(dead_code)]
fn _method() -> Method {
    Method::GET
}
