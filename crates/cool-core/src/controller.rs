//! CrudController：对齐 cool-admin-go 的 cool.Controller
//!
//! 六个标准接口：POST add / delete / update / list / page，GET info
//! 通过 Api 白名单控制开放能力，未开放返回 404。

use crate::error::CoolError;
use crate::query::{ListReq, PageReq};
use crate::response::{BaseRes, ok};
use crate::service::CrudService;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::Arc;

/// 调用方持有的应用状态（含 DbManager）
pub trait CrudState: Clone + Send + Sync + 'static {
    fn db(&self) -> &crate::db::DbManager;
}

/// API 白名单（对齐 Controller.Api）
#[derive(Debug, Clone)]
pub struct ApiAllow {
    pub add: bool,
    pub delete: bool,
    pub update: bool,
    pub info: bool,
    pub list: bool,
    pub page: bool,
}

impl Default for ApiAllow {
    fn default() -> Self {
        // 默认全开，对齐 cool-admin-go 常见配置
        Self {
            add: true,
            delete: true,
            update: true,
            info: true,
            list: true,
            page: true,
        }
    }
}

impl ApiAllow {
    /// 解析白名单字符串，如 "Add,Delete,Update,Info,List,Page"
    pub fn parse(apis: &[&str]) -> Self {
        let has = |name: &str| apis.iter().any(|a| a.eq_ignore_ascii_case(name));
        Self {
            add: has("Add"),
            delete: has("Delete"),
            update: has("Update"),
            info: has("Info"),
            list: has("List"),
            page: has("Page"),
        }
    }
}

/// 注册 CRUD 路由（挂到 prefix 分组下）
///
/// 路由形态对齐 cool-admin-go：
/// - POST {prefix}/add
/// - POST {prefix}/delete
/// - POST {prefix}/update
/// - GET  {prefix}/info
/// - POST {prefix}/list
/// - POST {prefix}/page
///
/// 返回未注入 state 的 Router，由调用方统一 merge 后 with_state。
pub fn crud_router<S: CrudState>(prefix: &str, service: CrudService, api: ApiAllow) -> Router<S> {
    let svc = Arc::new(service);
    let mut r = Router::new();

    if api.add {
        let s = svc.clone();
        r = r.route(
            &format!("{prefix}/add"),
            post(move |st: State<S>, body: Json<Value>| {
                let s = s.clone();
                async move { handle_add(st, s, body).await }
            }),
        );
    }
    if api.delete {
        let s = svc.clone();
        r = r.route(
            &format!("{prefix}/delete"),
            post(move |st: State<S>, body: Json<Value>| {
                let s = s.clone();
                async move { handle_delete(st, s, body).await }
            }),
        );
    }
    if api.update {
        let s = svc.clone();
        r = r.route(
            &format!("{prefix}/update"),
            post(move |st: State<S>, body: Json<Value>| {
                let s = s.clone();
                async move { handle_update(st, s, body).await }
            }),
        );
    }
    if api.info {
        let s = svc.clone();
        r = r.route(
            &format!("{prefix}/info"),
            get(move |st: State<S>, q: Query<InfoQuery>| {
                let s = s.clone();
                async move { handle_info(st, s, q).await }
            }),
        );
    }
    if api.list {
        let s = svc.clone();
        r = r.route(
            &format!("{prefix}/list"),
            post(move |st: State<S>, body: Json<Value>| {
                let s = s.clone();
                async move { handle_list(st, s, body).await }
            }),
        );
    }
    if api.page {
        // 同时注册 {prefix}/info/page 别名（部分前端拼法）
        for path in [format!("{prefix}/page"), format!("{prefix}/info/page")] {
            let s = svc.clone();
            r = r.route(
                path.as_str(),
                post(move |st: State<S>, body: Json<Value>| {
                    let s = s.clone();
                    async move { handle_page(st, s, body).await }
                }),
            );
        }
    }

    r
}

#[derive(Debug, Deserialize)]
struct InfoQuery {
    id: Option<i64>,
}

fn as_map(v: &Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    }
}

async fn handle_add<S: CrudState>(
    st: State<S>,
    svc: Arc<CrudService>,
    body: Json<Value>,
) -> Json<BaseRes> {
    match svc.add(st.db(), as_map(&body)).await {
        Ok(data) => Json(ok(data)),
        Err(e) => Json(e.to_res()),
    }
}

async fn handle_delete<S: CrudState>(
    st: State<S>,
    svc: Arc<CrudService>,
    body: Json<Value>,
) -> Json<BaseRes> {
    let ids: Vec<i64> = body
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect())
        .unwrap_or_default();
    match svc.delete(st.db(), &ids).await {
        Ok(data) => Json(ok(data)),
        Err(e) => Json(e.to_res()),
    }
}

async fn handle_update<S: CrudState>(
    st: State<S>,
    svc: Arc<CrudService>,
    body: Json<Value>,
) -> Json<BaseRes> {
    match svc.update(st.db(), as_map(&body)).await {
        Ok(data) => Json(ok(data)),
        Err(e) => Json(e.to_res()),
    }
}

async fn handle_info<S: CrudState>(
    st: State<S>,
    svc: Arc<CrudService>,
    q: Query<InfoQuery>,
) -> Json<BaseRes> {
    let Some(id) = q.id else {
        return Json(CoolError::param("请选择要查询的数据").to_res());
    };
    match svc.info(st.db(), id).await {
        Ok(data) => Json(ok(data)),
        Err(e) => Json(e.to_res()),
    }
}

async fn handle_list<S: CrudState>(
    st: State<S>,
    svc: Arc<CrudService>,
    body: Json<Value>,
) -> Json<BaseRes> {
    let m = as_map(&body);
    let req = ListReq {
        order: m.get("order").and_then(|v| v.as_str()).map(String::from),
        sort: m.get("sort").and_then(|v| v.as_str()).map(String::from),
    };
    match svc.list(st.db(), &req, &m).await {
        Ok(data) => Json(ok(data)),
        Err(e) => Json(e.to_res()),
    }
}

async fn handle_page<S: CrudState>(
    st: State<S>,
    svc: Arc<CrudService>,
    body: Json<Value>,
) -> Json<BaseRes> {
    let m = as_map(&body);
    let req = PageReq {
        page: m.get("page").and_then(|v| v.as_i64()).unwrap_or(1),
        size: m.get("size").and_then(|v| v.as_i64()).unwrap_or(15),
        order: m.get("order").and_then(|v| v.as_str()).map(String::from),
        sort: m.get("sort").and_then(|v| v.as_str()).map(String::from),
        is_export: m.get("isExport").and_then(|v| v.as_bool()).unwrap_or(false),
        max_export_limit: m
            .get("maxExportLimit")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    };
    match svc.page(st.db(), &req, &m).await {
        Ok(data) => Json(ok(data)),
        Err(e) => Json(e.to_res()),
    }
}
