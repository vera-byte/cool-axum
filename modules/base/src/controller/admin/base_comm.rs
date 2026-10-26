//! base_comm：公共接口（对齐 controller/admin/base_comm.go）
//!
//! person / permmenu / refreshPerms / upload / uploadMode

use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cool_core::CrudState;
use cool_core::response::{BaseRes, fail, ok};
use cool_file::FileDriver;

use crate::middleware::authority::login_id_from_headers;
use crate::service::{base_sys_perms, base_sys_user};

/// 未登录响应（HTTP 401，对齐 cool-admin-go）
fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(BaseRes::new(1001, "登陆失效～", None)),
    )
        .into_response()
}

/// 文件/DB 能力挂载到应用状态
pub trait FileState: Clone + Send + Sync + 'static {
    fn file_driver(&self) -> &FileDriver;
}

/// 注册 /base/comm/* 与 /admin/base/comm/*
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    Router::new()
        .route("/base/comm/uploadMode", get(upload_mode::<S>))
        .route("/base/comm/upload", post(upload::<S>))
        .route("/base/comm/permmenu", get(permmenu::<S>))
        .route("/base/comm/refreshPerms", post(refresh_perms::<S>))
        .route("/base/comm/person", get(person::<S>))
        .route("/base/userinfo", get(person::<S>))
        .route("/admin/base/comm/userinfo", get(person::<S>))
        .route("/admin/base/comm/uploadMode", get(upload_mode::<S>))
        .route("/admin/base/comm/upload", post(upload::<S>))
        .route("/admin/base/comm/permmenu", get(permmenu::<S>))
        .route("/admin/base/comm/refreshPerms", post(refresh_perms::<S>))
        .route("/admin/base/comm/person", get(person::<S>))
}

/// GET .../person —— 当前登录人信息（整行除 password，对齐 Go Person）
async fn person<S: CrudState + FileState>(State(st): State<S>, headers: HeaderMap) -> Response {
    let Some(login_id) = login_id_from_headers(&headers).await else {
        return unauthorized();
    };
    let uid: i64 = login_id.parse().unwrap_or(0);
    match base_sys_user::person_json(CrudState::db(&st), uid).await {
        Some(u) => Json(ok(u)).into_response(),
        None => Json(fail("用户不存在")).into_response(),
    }
}

/// GET .../permmenu —— 权限点 + 菜单树
async fn permmenu<S: CrudState + FileState>(State(st): State<S>, headers: HeaderMap) -> Response {
    let Some(login_id) = login_id_from_headers(&headers).await else {
        return unauthorized();
    };
    let uid: i64 = login_id.parse().unwrap_or(0);
    Json(ok(base_sys_perms::permmenu(CrudState::db(&st), uid).await)).into_response()
}

/// POST .../refreshPerms
async fn refresh_perms<S: CrudState + FileState>(
    State(st): State<S>,
    headers: HeaderMap,
) -> Response {
    let Some(login_id) = login_id_from_headers(&headers).await else {
        return unauthorized();
    };
    let uid: i64 = login_id.parse().unwrap_or(0);
    base_sys_perms::refresh_perms(CrudState::db(&st), uid).await;
    let perms = cool_auth::get_permissions(&login_id).await;
    Json(ok(serde_json::json!({ "perms": perms }))).into_response()
}

/// GET .../uploadMode
async fn upload_mode<S: CrudState + FileState>(State(st): State<S>) -> Json<BaseRes> {
    Json(ok(st.file_driver().mode_info()))
}

/// POST .../upload —— multipart 字段名 `file`
async fn upload<S: CrudState + FileState>(
    State(st): State<S>,
    mut multipart: Multipart,
) -> Json<BaseRes> {
    let mut filename = String::from("file.bin");
    let mut data: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" || name == "upload" {
            if let Some(fn_) = field.file_name().map(String::from) {
                filename = fn_;
            }
            match field.bytes().await {
                Ok(b) => data = Some(b.to_vec()),
                Err(e) => return Json(fail(format!("读取上传内容失败: {e}"))),
            }
            break;
        }
    }

    let Some(bytes) = data else {
        return Json(fail("上传文件为空"));
    };
    if bytes.is_empty() {
        return Json(fail("上传文件为空"));
    }

    match st.file_driver().upload(&filename, &bytes).await {
        Ok(url) => Json(ok(serde_json::json!({
            "url": url,
            "mode": st.file_driver().mode_name(),
        }))),
        Err(e) => Json(fail(e.to_string())),
    }
}
