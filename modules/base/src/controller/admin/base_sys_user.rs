//! base_sys_user 控制器（对齐 controller/admin/base_sys_user.go）
//!
//! add/update 自定义（密码哈希 + passwordV 吊销），其余走通用 CRUD。

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use cool_core::controller::{ApiAllow, crud_router};
use cool_core::response::{BaseRes, fail, ok};
use cool_core::service::CrudService;
use cool_core::{CoolError, CrudState, QueryOp};
use serde_json::{Map, Value, json};

use super::base_comm::FileState;
use crate::service::base_sys_user as user_svc;

fn user_service() -> CrudService {
    CrudService::new("base_sys_user")
        .unique_key("username", "用户名已存在")
        .not_null_key("username", "用户名不能为空")
        .info_ignore_property("password,passwordV")
        .hidden_fields(&["password", "passwordV"])
        .list_op(
            QueryOp::new()
                .field_eq(&["departmentId", "status"])
                .key_word_field(&["name", "username", "nickName", "phone"]),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["departmentId", "status"])
                .key_word_field(&["name", "username", "nickName", "phone"])
                .add_orderby("id", "desc"),
        )
}

/// 注册 /admin/base/sys/user 与 /base/sys/user
/// 兼容短路径 /admin/user/* 与 /user/*（前端习惯）
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    let svc = user_service();
    let api = ApiAllow {
        add: false,
        update: false,
        delete: true,
        info: true,
        list: true,
        page: true,
    };

    let mut r = Router::new();
    for prefix in ["/admin/base/sys/user", "/admin/user"] {
        r = r.merge(crud_router(prefix, svc.clone(), api.clone()));
    }
    for prefix in ["/admin/base/sys/user", "/admin/user"] {
        r = r
            .route(&format!("{prefix}/add"), post(add_user::<S>))
            .route(&format!("{prefix}/update"), post(update_user::<S>));
    }
    r
}

async fn add_user<S: CrudState + FileState>(
    State(st): State<S>,
    Json(body): Json<Value>,
) -> Json<BaseRes> {
    let mut data = as_map(&body);
    let Some(plain) = data.get("password").and_then(|v| v.as_str()) else {
        return Json(fail("密码不能为空"));
    };
    if plain.is_empty() {
        return Json(fail("密码不能为空"));
    }
    let hashed = match user_svc::prepare_password(plain) {
        Ok(h) => h,
        Err(e) => return Json(fail(format!("密码处理失败: {e}"))),
    };
    data.insert("password".into(), json!(hashed));
    data.insert("passwordV".into(), json!(1));

    match user_service().add(CrudState::db(&st), data).await {
        Ok(v) => Json(ok(v)),
        Err(e) => Json(e.to_res()),
    }
}

async fn update_user<S: CrudState + FileState>(
    State(st): State<S>,
    Json(body): Json<Value>,
) -> Json<BaseRes> {
    let mut data = as_map(&body);
    let Some(id) = data.get("id").and_then(|x| x.as_i64()) else {
        return Json(CoolError::biz("id不能为空").to_res());
    };

    let password_changed = match data.get("password").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => {
            let hashed = match user_svc::prepare_password(p) {
                Ok(h) => h,
                Err(e) => return Json(fail(format!("密码处理失败: {e}"))),
            };
            data.insert("password".into(), json!(hashed));
            true
        }
        _ => {
            data.remove("password");
            false
        }
    };

    match user_service().update(CrudState::db(&st), data).await {
        Ok(v) => {
            if password_changed {
                if let Ok(pool) = CrudState::db(&st).default_pool() {
                    let _ = pool.execute(&user_svc::bump_password_v_sql(id)).await;
                }
                let _ = cool_core::auth::logout_by_login_id(&id.to_string()).await;
                tracing::info!(user_id = id, "密码已更新，旧会话已吊销");
            }
            Json(ok(v))
        }
        Err(e) => Json(e.to_res()),
    }
}

fn as_map(v: &Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    }
}
