//! base_sys_department：部门服务（对齐 service/base_sys_department.go）

use cool_core::service::query_rows_on;
use cool_db::DbManager;

/// 按角色查部门 ID（数据权限）
pub async fn get_by_role_ids(db: &DbManager, role_ids: &[i64], is_admin: bool) -> Vec<i64> {
    if is_admin {
        return vec![0]; // 0 表示不限
    }
    let Ok(pool) = db.default_pool() else {
        return Vec::new();
    };
    if role_ids.is_empty() {
        return Vec::new();
    }
    let ids = role_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT DISTINCT departmentId FROM `base_sys_role_department` \
         WHERE roleId IN ({ids}) AND deleteTime IS NULL"
    );
    query_rows_on(pool, &sql)
        .await
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("departmentId").and_then(|v| v.as_i64()))
                .collect()
        })
        .unwrap_or_default()
}
