//! base_sys_role：角色服务（对齐 service/base_sys_role.go）

use cool_core::service::query_rows_on;
use cool_db::DbManager;

/// 按用户查角色 ID 列表
pub async fn get_by_user(db: &DbManager, user_id: i64) -> Vec<i64> {
    let Ok(pool) = db.default_pool() else {
        return Vec::new();
    };
    let sql = format!(
        "SELECT roleId FROM `base_sys_user_role` WHERE userId = {user_id} AND deleteTime IS NULL"
    );
    query_rows_on(pool, &sql)
        .await
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("roleId").and_then(|v| v.as_i64()))
                .collect()
        })
        .unwrap_or_default()
}
