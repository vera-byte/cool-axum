//! recycle 服务：记录 / 恢复 / 清理（对齐 midway RecycleDataService）

use cool_core::db::{DbManager, DbPool};
use cool_core::service::query_rows_on;
use cool_core::tenant::SoftDeleteRecord;
use serde_json::{Value, json};

/// 软删除 → 写入回收站
pub async fn record(db: &DbManager, rec: SoftDeleteRecord) {
    if rec.rows.is_empty() {
        return;
    }
    let Ok(pool) = db.default_pool() else {
        return;
    };
    let entity_info = json!({ "table": rec.table });
    let data = Value::Array(rec.rows.clone());
    let params = json!({ "ids": rec.ids });
    let sql = format!(
        "INSERT INTO `recycle_data` \
         (createTime, updateTime, entityInfo, userId, data, url, params, count) \
         VALUES (CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, {}, {}, {}, {}, {}, {})",
        lit(&entity_info.to_string()),
        match rec.user_id {
            Some(u) => u.to_string(),
            None => "NULL".into(),
        },
        lit(&data.to_string()),
        lit(&rec.url),
        lit(&params.to_string()),
        rec.rows.len()
    );
    if let Err(e) = pool.execute(&sql).await {
        tracing::error!(error = %e, table = %rec.table, "回收站记录失败");
    }
}

/// 恢复数据：按快照把原表 deleteTime 置空，再删回收记录
pub async fn restore(db: &DbManager, ids: &[i64]) -> Result<u64, String> {
    let Ok(pool) = db.default_pool() else {
        return Err("数据库未就绪".into());
    };
    let mut restored = 0u64;
    for id in ids {
        let sql = format!(
            "SELECT id, entityInfo, data FROM `recycle_data` WHERE id = {id} AND deleteTime IS NULL LIMIT 1"
        );
        let rows = query_rows_on(pool, &sql).await.map_err(|e| e.to_string())?;
        let Some(row) = rows.into_iter().next() else {
            continue;
        };
        let table = row
            .get("entityInfo")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .and_then(|v| v.get("table").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_default();
        if table.is_empty() || !is_safe_ident(&table) {
            tracing::warn!(recycle_id = id, table = %table, "回收记录表名非法，跳过");
            continue;
        }
        let data = row
            .get("data")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);

        if let Some(arr) = data.as_array() {
            for item in arr {
                let Some(rid) = item.get("id").and_then(|v| v.as_i64()) else {
                    continue;
                };
                // 软删除恢复：deleteTime 置空
                let up = format!("UPDATE `{table}` SET deleteTime = NULL WHERE id = {rid}");
                let _ = pool.execute(&up).await;
                restored += 1;
            }
        }

        // 回收记录删除（软删）
        let _ = pool
            .execute(&format!(
                "UPDATE `recycle_data` SET deleteTime = CURRENT_TIMESTAMP WHERE id = {id}"
            ))
            .await;
    }
    Ok(restored)
}

/// 清理回收站
/// - `is_all`：清空全部
/// - 否则按 `recycleKeep`（天，来自 base_sys_conf）删除过期；未配置则全部清空
pub async fn clear(db: &DbManager, is_all: bool) -> Result<u64, String> {
    let Ok(pool) = db.default_pool() else {
        return Err("数据库未就绪".into());
    };
    if is_all {
        let n = pool
            .execute(
                "UPDATE `recycle_data` SET deleteTime = CURRENT_TIMESTAMP WHERE deleteTime IS NULL",
            )
            .await
            .map_err(|e| e.to_string())?;
        return Ok(n);
    }
    match get_conf(pool, "recycleKeep").await {
        Some(keep) if !keep.is_empty() => {
            let days: i64 = keep.parse().unwrap_or(0);
            if days <= 0 {
                let n = pool
                    .execute("UPDATE `recycle_data` SET deleteTime = CURRENT_TIMESTAMP WHERE deleteTime IS NULL")
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(n);
            }
            let sql = format!(
                "UPDATE `recycle_data` SET deleteTime = CURRENT_TIMESTAMP \
                 WHERE deleteTime IS NULL AND createTime < DATE_SUB(CURDATE(), INTERVAL {days} DAY)"
            );
            pool.execute(&sql).await.map_err(|e| e.to_string())
        }
        _ => {
            let n = pool
                .execute("UPDATE `recycle_data` SET deleteTime = CURRENT_TIMESTAMP WHERE deleteTime IS NULL")
                .await
                .map_err(|e| e.to_string())?;
            Ok(n)
        }
    }
}

async fn get_conf(pool: &DbPool, key: &str) -> Option<String> {
    let sql = format!(
        "SELECT cValue FROM `base_sys_conf` WHERE cKey = '{}' LIMIT 1",
        key.replace('\'', "''")
    );
    let rows = query_rows_on(pool, &sql).await.ok()?;
    rows.first()
        .and_then(|r| r.get("cValue"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

fn is_safe_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}
