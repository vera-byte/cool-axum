//! base_sys_user：用户查询 / 密码（对齐 service/base_sys_user.go）

use cool_auth::{hash_password, verify_password};
use cool_core::service::query_rows_on;
use cool_db::DbManager;
use serde_json::Value;

/// 登录用户信息
pub struct LoginUser {
    pub id: i64,
    pub username: String,
    pub name: Option<String>,
    pub nick_name: Option<String>,
    pub department_id: Option<i64>,
    pub password_v: i64,
    pub status: i64,
}

/// 按用户名查启用中的用户
pub async fn find_user_by_username(db: &DbManager, username: &str) -> Option<LoginUser> {
    let pool = db.default_pool().ok()?;
    let uname = username.replace('\'', "''");
    let sql = format!(
        "SELECT id, username, name, nickName, departmentId, password, passwordV, status \
         FROM `base_sys_user` \
         WHERE username = '{uname}' AND status = 1 AND deleteTime IS NULL LIMIT 1"
    );
    let rows = query_rows_on(pool, &sql).await.ok()?;
    row_to_user(rows.into_iter().next()?)
}

/// 按 id 查用户
pub async fn find_user_by_id(db: &DbManager, id: i64) -> Option<LoginUser> {
    let pool = db.default_pool().ok()?;
    let sql = format!(
        "SELECT id, username, name, nickName, departmentId, password, passwordV, status \
         FROM `base_sys_user` WHERE id = {id} AND deleteTime IS NULL LIMIT 1"
    );
    let rows = query_rows_on(pool, &sql).await.ok()?;
    row_to_user(rows.into_iter().next()?)
}

fn row_to_user(r: Value) -> Option<LoginUser> {
    Some(LoginUser {
        id: r.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
        username: r.get("username")?.as_str()?.to_string(),
        name: r.get("name").and_then(|v| v.as_str()).map(String::from),
        nick_name: r.get("nickName").and_then(|v| v.as_str()).map(String::from),
        department_id: r.get("departmentId").and_then(|v| v.as_i64()),
        password_v: r.get("passwordV").and_then(|v| v.as_i64()).unwrap_or(1),
        status: r.get("status").and_then(|v| v.as_i64()).unwrap_or(1),
    })
}

/// 取用户密码哈希
pub async fn get_password_hash(db: &DbManager, user_id: i64) -> Option<String> {
    let pool = db.default_pool().ok()?;
    let sql = format!(
        "SELECT password FROM `base_sys_user` WHERE id = {user_id} AND deleteTime IS NULL LIMIT 1"
    );
    let rows = query_rows_on(pool, &sql).await.ok()?;
    rows.first()
        .and_then(|r| r.get("password"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// 新增/修改密码：bcrypt 哈希
pub fn prepare_password(plain: &str) -> Result<String, String> {
    hash_password(plain)
}

/// 校验密码（bcrypt 优先，兼容旧 MD5）
pub fn verify_user_password(plain: &str, stored: &str) -> bool {
    verify_password(plain, stored)
}

/// passwordV 递增 SQL（改密后吊销旧 token）
pub fn bump_password_v_sql(user_id: i64) -> String {
    format!(
        "UPDATE `base_sys_user` SET passwordV = passwordV + 1, updateTime = CURRENT_TIMESTAMP WHERE id = {user_id}"
    )
}

/// 用户转 JSON（剥离敏感字段）
pub fn user_json(u: &LoginUser) -> Value {
    serde_json::json!({
        "id": u.id,
        "username": u.username,
        "name": u.name,
        "nickName": u.nick_name,
        "departmentId": u.department_id,
        "status": u.status,
        "passwordV": u.password_v,
    })
}

/// person / userinfo：整行除 password（对齐 Go `FieldsEx("password")`）
pub async fn person_json(db: &DbManager, user_id: i64) -> Option<Value> {
    let pool = db.default_pool().ok()?;
    let sql = format!(
        "SELECT * FROM `base_sys_user` WHERE id = {user_id} AND deleteTime IS NULL LIMIT 1"
    );
    let mut rows = query_rows_on(pool, &sql).await.ok()?;
    let mut row = rows.pop()?;
    if let Value::Object(map) = &mut row {
        map.remove("password");
    }
    Some(row)
}
