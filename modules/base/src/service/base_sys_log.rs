//! base_sys_log：操作日志服务（对齐 service/base_sys_log.go）

use cool_core::service::query_rows_on;
use cool_db::DbManager;

/// 清理日志（deep=true 连失败日志一起清，否则清 30 天前的成功日志）
pub async fn clear(db: &DbManager, deep: bool) -> Result<u64, String> {
    let pool = db.default_pool().map_err(|e| e.to_string())?;
    let sql = if deep {
        "DELETE FROM `base_sys_log` WHERE 1=1".to_string()
    } else {
        "DELETE FROM `base_sys_log` WHERE createTime < DATE_SUB(NOW(), INTERVAL 30 DAY)".to_string()
    };
    pool.execute(&sql).await.map_err(|e| e.to_string())
}

/// 日志条数
pub async fn count(db: &DbManager) -> i64 {
    let Ok(pool) = db.default_pool() else {
        return 0;
    };
    let sql = "SELECT COUNT(*) AS count FROM `base_sys_log` WHERE deleteTime IS NULL";
    query_rows_on(pool, sql)
        .await
        .ok()
        .and_then(|rows| rows.first().and_then(|r| r.get("count")).and_then(|v| v.as_i64()))
        .unwrap_or(0)
}
