//! base_open：开放接口（对齐 controller/admin/base_open.go）
//!
//! captcha / login / eps / refreshToken —— 免登录

use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use cool_core::CrudState;
use cool_core::response::{BaseRes, fail, ok};
use serde_json::json;
use std::net::SocketAddr;

use super::base_comm::FileState;
use crate::api::v1::{BaseOpenCaptchaReq, BaseOpenLoginReq};
use crate::service::{base_open, base_sys_login, base_sys_perms, captcha};

/// 注册 /admin/base/open/* 与 /base/open/*
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    Router::new()
        .route("/admin/base/open/captcha", get(captcha_handler::<S>))
        .route("/base/open/captcha", get(captcha_handler::<S>))
        .route("/admin/base/open/login", post(login_handler::<S>))
        .route("/base/login", post(login_handler::<S>))
        .route("/admin/base/open/logout", post(logout_handler))
        .route("/base/logout", post(logout_handler))
        .route("/admin/base/open/eps", get(eps_handler::<S>))
}

async fn captcha_handler<S>(_st: State<S>, q: Query<BaseOpenCaptchaReq>) -> Json<BaseRes> {
    let color = q.color.as_deref().unwrap_or("#fff");
    let (id, data) = captcha::generate_and_store(q.width, q.height, color).await;
    Json(ok(json!({ "captchaId": id, "data": data })))
}

async fn login_handler<S: CrudState + FileState>(
    State(st): State<S>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<BaseOpenLoginReq>,
) -> Json<BaseRes> {
    let ip = addr.ip().to_string();
    if req.username.is_empty() || req.password.is_empty() {
        return Json(fail("账户或密码不正确~"));
    }

    let captcha_id = req.captcha_id.as_deref().unwrap_or("");
    let verify_code = req.verify_code.as_deref().unwrap_or("");
    if !captcha::verify(captcha_id, verify_code).await {
        return Json(fail("验证码错误"));
    }

    match base_sys_login::login(CrudState::db(&st), &req.username, &req.password, &ip).await {
        Ok(user) => {
            let login_id = user.id.to_string();
            let perms = base_sys_perms::get_user_perms(CrudState::db(&st), user.id).await;
            let mut roles = base_sys_perms::get_user_roles(CrudState::db(&st), user.id).await;
            if roles.is_empty() && user.username == "admin" {
                roles = vec!["admin".to_string()];
            }
            cool_auth::set_permissions(&login_id, perms.clone()).await;
            cool_auth::set_roles(&login_id, roles).await;

            match cool_auth::login(&login_id).await {
                Ok(token) => Json(base_sys_login::login_payload(&user, &token, perms)),
                Err(e) => Json(fail(format!("登录失败: {e}"))),
            }
        }
        Err(msg) => Json(fail(msg)),
    }
}

/// POST /base/logout、/admin/base/open/logout
async fn logout_handler(headers: HeaderMap) -> Json<BaseRes> {
    if let Some(token) = token_from_headers(&headers)
        && let Some(login_id) = cool_auth::get_login_id(&token).await
    {
        let _ = cool_auth::logout_by_login_id(&login_id).await;
    }
    Json(ok(serde_json::json!({})))
}

async fn eps_handler<S: CrudState + FileState>(State(st): State<S>) -> Json<BaseRes> {
    Json(ok(base_open::admin_eps(CrudState::db(&st)).await))
}

/// 从 header 取 token
pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get("authorization")
        .or_else(|| headers.get("sa-token"))
        .and_then(|v| v.to_str().ok())?;
    let t = raw.strip_prefix("Bearer ").unwrap_or(raw);
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
