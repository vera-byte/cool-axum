//! base_sys_perms：权限点/菜单/刷新（对齐 service/base_sys_perms.go + base_sys_menu.go）

use cool_core::service::query_rows_on;
use cool_db::DbManager;
use serde_json::Value;

/// 查询用户角色标签
pub async fn get_user_roles(db: &DbManager, user_id: i64) -> Vec<String> {
    let Ok(pool) = db.default_pool() else {
        return Vec::new();
    };
    let sql = format!(
        "SELECT r.label AS label FROM `base_sys_user_role` ur \
         JOIN `base_sys_role` r ON r.id = ur.roleId AND r.deleteTime IS NULL \
         WHERE ur.userId = {user_id} AND ur.deleteTime IS NULL"
    );
    query_rows_on(pool, &sql)
        .await
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("label").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// 查询用户权限点（menu.perms 逗号分隔展开）
pub async fn get_user_perms(db: &DbManager, user_id: i64) -> Vec<String> {
    let Ok(pool) = db.default_pool() else {
        return Vec::new();
    };
    let sql = format!(
        "SELECT DISTINCT m.perms AS perms FROM `base_sys_user_role` ur \
         JOIN `base_sys_role_menu` rm ON rm.roleId = ur.roleId AND rm.deleteTime IS NULL \
         JOIN `base_sys_menu` m ON m.id = rm.menuId AND m.deleteTime IS NULL \
         WHERE ur.userId = {user_id} AND ur.deleteTime IS NULL AND m.perms IS NOT NULL AND m.perms <> ''"
    );
    query_rows_on(pool, &sql)
        .await
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("perms").and_then(|v| v.as_str()).map(String::from))
                .flat_map(|p| {
                    p.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 查询用户菜单树（permmenu.menus）
pub async fn get_user_menus(db: &DbManager, user_id: i64) -> Vec<Value> {
    let Ok(pool) = db.default_pool() else {
        return Vec::new();
    };
    let sql = if user_id == 1 {
        "SELECT id, parentId, name, router, perms, type, icon, orderNum, viewPath, keepAlive, isShow \
         FROM `base_sys_menu` WHERE deleteTime IS NULL ORDER BY orderNum, id".to_string()
    } else {
        format!(
            "SELECT DISTINCT m.id, m.parentId, m.name, m.router, m.perms, m.type, m.icon, \
               m.orderNum, m.viewPath, m.keepAlive, m.isShow \
             FROM `base_sys_menu` m \
             JOIN `base_sys_role_menu` rm ON rm.menuId = m.id AND rm.deleteTime IS NULL \
             JOIN `base_sys_user_role` ur ON ur.roleId = rm.roleId AND ur.deleteTime IS NULL \
             WHERE ur.userId = {user_id} AND m.deleteTime IS NULL \
             ORDER BY m.orderNum, m.id"
        )
    };
    query_rows_on(pool, &sql).await.unwrap_or_default()
}

/// 刷新用户权限到 sa-token 会话
pub async fn refresh_perms(db: &DbManager, user_id: i64) {
    let login_id = user_id.to_string();
    let mut perms = get_user_perms(db, user_id).await;
    let mut roles = get_user_roles(db, user_id).await;
    if perms.is_empty() && user_id == 1 {
        perms = vec!["*".to_string()];
    }
    if roles.is_empty() && user_id == 1 {
        roles = vec!["admin".to_string()];
    }
    cool_auth::set_permissions(&login_id, perms).await;
    cool_auth::set_roles(&login_id, roles).await;
}

/// permmenu 响应（perms + menus）
pub async fn permmenu(db: &DbManager, user_id: i64) -> serde_json::Value {
    let perms = cool_auth::get_permissions(&user_id.to_string()).await;
    let menus = get_user_menus(db, user_id).await;
    serde_json::json!({ "perms": perms, "menus": menus })
}
