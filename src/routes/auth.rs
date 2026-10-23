//! 登录/登出/当前用户：base_sys_user 查库校验 + bcrypt 密码

use crate::app::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cool_core::response::{fail, ok, BaseRes};
use mod_base::login as login_svc;
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub captcha_id: Option<String>,
    #[serde(default)]
    pub verify_code: Option<String>,
}

/// POST /base/login
pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<LoginReq>,
) -> Json<BaseRes> {
    let ip = addr.ip().to_string();

    if req.username.is_empty() || req.password.is_empty() {
        return Json(fail("账户或密码不正确~"));
    }

    let captcha_id = req.captcha_id.as_deref().unwrap_or("");
    let verify_code = req.verify_code.as_deref().unwrap_or("");
    if !mod_base::captcha::verify(captcha_id, verify_code).await {
        return Json(fail("验证码错误"));
    }

    match login_svc::login(&state.db, &req.username, &req.password, &ip).await {
        Ok(user) => {
            let login_id = user.id.to_string();

            let mut perms = login_svc::get_user_perms(&state.db, user.id).await;
            let mut roles = login_svc::get_user_roles(&state.db, user.id).await;
            if perms.is_empty() && user.username == "admin" {
                perms = vec!["*".to_string()];
            }
            if roles.is_empty() && user.username == "admin" {
                roles = vec!["admin".to_string()];
            }
            cool_auth::set_permissions(&login_id, perms.clone()).await;
            cool_auth::set_roles(&login_id, roles).await;

            match cool_auth::login(&login_id).await {
                Ok(token) => Json(login_svc::login_payload(&user, &token, perms)),
                Err(e) => Json(fail(format!("登录失败: {e}"))),
            }
        }
        Err(msg) => Json(fail(msg)),
    }
}

/// POST /base/logout
pub async fn logout(headers: HeaderMap) -> Json<BaseRes> {
    if let Some(token) = extract_token(&headers) {
        if let Some(login_id) = cool_auth::get_login_id(&token).await {
            let _ = cool_auth::logout_by_login_id(&login_id).await;
        }
    }
    Json(ok(json!({})))
}

/// GET /base/userinfo
pub async fn userinfo(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = extract_token(&headers) else {
        return unauthorized();
    };
    let Some(login_id) = cool_auth::get_login_id(&token).await else {
        return unauthorized();
    };
    let uid: i64 = login_id.parse().unwrap_or(0);

    let user = login_svc::find_user_by_id(&state.db, uid).await;
    let perms = cool_auth::get_permissions(&login_id).await;
    let roles = cool_auth::get_roles(&login_id).await;

    match user {
        Some(u) => Json(ok(json!({
            "id": login_id,
            "userInfo": login_svc::user_json(&u),
            "permissions": perms,
            "roles": roles,
        })))
        .into_response(),
        None => Json(ok(json!({
            "id": login_id,
            "permissions": perms,
            "roles": roles,
        })))
        .into_response(),
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(BaseRes::new(1001, "未登录", None)),
    )
        .into_response()
}

/// 从 Authorization 头取 token（兼容 Bearer 前缀）
fn extract_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get("authorization")
        .or_else(|| headers.get("sa-token"))
        .and_then(|v| v.to_str().ok())?;
    let token = raw.strip_prefix("Bearer ").unwrap_or(raw);
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}
