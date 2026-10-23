//! base_sys_login：登录/登出/失败锁定/种子管理员（对齐 service/base_sys_login.go）

use super::base_sys_user::LoginUser;
use crate::service::captcha;
use cool_auth::{hash_password, verify_password};
use cool_cache::CacheManager;
use cool_core::response::BaseRes;
use cool_core::service::query_rows_on;
use cool_db::DbManager;
use serde_json::json;
use std::sync::OnceLock;
use std::time::Duration;

/// 全局缓存（登录失败计数）
static CACHE: OnceLock<CacheManager> = OnceLock::new();

const FAIL_LIMIT: u64 = 5;
const FAIL_WINDOW: Duration = Duration::from_secs(600);

/// 注入缓存管理器
pub fn init_cache(cache: CacheManager) {
    captcha::init_cache(cache.clone());
    let _ = CACHE.set(cache);
}

fn cache() -> Option<&'static CacheManager> {
    CACHE.get()
}

fn fail_key(username: &str, ip: &str) -> String {
    format!("admin:loginFail:{username}:{ip}")
}

/// 是否被临时锁定
pub async fn is_locked(username: &str, ip: &str) -> bool {
    let Some(c) = cache() else {
        return false;
    };
    match c.get(&fail_key(username, ip)).await {
        Some(v) => v.as_u64().unwrap_or(0) >= FAIL_LIMIT,
        None => false,
    }
}

/// 记录一次失败
pub async fn record_fail(username: &str, ip: &str) {
    if let Some(c) = cache() {
        let _ = c.increment(&fail_key(username, ip), FAIL_WINDOW).await;
    }
}

/// 清除失败计数
pub async fn clear_fails(username: &str, ip: &str) {
    if let Some(c) = cache() {
        c.remove(&fail_key(username, ip)).await;
    }
}

/// 登录校验：返回用户或错误消息
pub async fn login(
    db: &DbManager,
    username: &str,
    password: &str,
    ip: &str,
) -> Result<LoginUser, String> {
    if is_locked(username, ip).await {
        return Err("登录失败次数过多,请10分钟后再试~".into());
    }

    // 防用户名枚举
    let Some(user) = super::base_sys_user::find_user_by_username(db, username).await else {
        record_fail(username, ip).await;
        return Err("账户或密码不正确~".into());
    };

    let Some(stored) = super::base_sys_user::get_password_hash(db, user.id).await else {
        record_fail(username, ip).await;
        return Err("账户或密码不正确~".into());
    };

    if !verify_password(password, &stored) {
        record_fail(username, ip).await;
        return Err("账户或密码不正确~".into());
    }

    clear_fails(username, ip).await;
    Ok(user)
}

/// 种子管理员（表为空时初始化）
pub async fn seed_admin_if_empty(db: &DbManager) {
    let Ok(pool) = db.default_pool() else {
        return;
    };
    let cnt_sql = "SELECT COUNT(*) AS count FROM `base_sys_user` WHERE deleteTime IS NULL";
    let Ok(rows) = query_rows_on(pool, cnt_sql).await else {
        return;
    };
    let n = rows
        .first()
        .and_then(|r| r.get("count"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if n > 0 {
        return;
    }

    let Ok(hash) = hash_password("admin123") else {
        return;
    };
    let hash = hash.replace('\'', "''");
    let sql = format!(
        "INSERT INTO `base_sys_user` \
         (createTime, updateTime, username, password, passwordV, name, nickName, status) \
         VALUES (CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 'admin', '{hash}', 1, '超级管理员', 'Admin', 1)"
    );
    match pool.execute(&sql).await {
        Ok(_) => tracing::info!("已初始化种子管理员 admin / admin123（请尽快修改）"),
        Err(e) => tracing::error!(error = %e, "种子管理员初始化失败"),
    }
}

/// 登录成功响应载荷
///
/// 对齐 cool-admin-go TokenResult（expire/token/refreshExpire/refreshToken），
/// 并附带 userInfo/permissions 便于前端。
pub fn login_payload(user: &LoginUser, token: &str, perms: Vec<String>) -> BaseRes {
    BaseRes::new(
        cool_core::response::CODE_OK,
        "success",
        Some(json!({
            "expire": 36000u64,
            "token": token,
            "refreshExpire": 1296000u64,
            "refreshToken": token,
            "userInfo": {
                "id": user.id,
                "username": user.username,
                "name": user.name,
                "nickName": user.nick_name,
                "departmentId": user.department_id,
                "passwordV": user.password_v,
            },
            "permissions": perms,
        })),
    )
}

// 兼容旧路径：cool-axum 通过 mod_base::login::* 访问
pub use super::base_sys_param::refresh_param_cache;
pub use super::base_sys_perms::{get_user_perms, get_user_roles, refresh_perms};
pub use super::base_sys_user::{
    find_user_by_id, find_user_by_username, get_password_hash, prepare_password, user_json,
};
