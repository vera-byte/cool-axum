//! base_sys_param：系统参数缓存（对齐 service/base_sys_param.go）

use cool_cache::CacheManager;
use cool_core::service::query_rows_on;
use cool_db::DbManager;
use std::sync::OnceLock;
use std::time::Duration;

const PARAM_PREFIX: &str = "param:";

static CACHE: OnceLock<CacheManager> = OnceLock::new();

/// 注入缓存
pub fn init_cache(cache: CacheManager) {
    let _ = CACHE.set(cache);
}

fn cache() -> Option<&'static CacheManager> {
    CACHE.get()
}

/// 读取系统参数（缓存优先）
pub async fn get_param(db: &DbManager, key: &str) -> Option<String> {
    let rkey = format!("{PARAM_PREFIX}{key}");
    if let Some(c) = cache()
        && let Some(v) = c.get(&rkey).await
    {
        return v.as_str().map(String::from);
    }

    let pool = db.default_pool().ok()?;
    let k = key.replace('\'', "''");
    let sql = format!(
        "SELECT `data` FROM `base_sys_param` WHERE keyName = '{k}' AND deleteTime IS NULL LIMIT 1"
    );
    let rows = query_rows_on(pool, &sql).await.ok()?;
    let data = rows
        .first()
        .and_then(|r| r.get("data"))
        .and_then(|v| v.as_str())
        .map(String::from)?;

    if let Some(c) = cache() {
        let _ = c.set_str(&rkey, &data, Duration::ZERO).await;
    }
    Some(data)
}

/// 写入参数缓存
pub async fn set_param_cache(key: &str, value: &str) {
    if let Some(c) = cache() {
        let _ = c
            .set_str(&format!("{PARAM_PREFIX}{key}"), value, Duration::ZERO)
            .await;
    }
}

/// 全量刷新参数缓存
pub async fn refresh_param_cache(db: &DbManager) {
    let Ok(pool) = db.default_pool() else {
        return;
    };
    let sql = "SELECT keyName, `data` FROM `base_sys_param` WHERE deleteTime IS NULL";
    if let Ok(rows) = query_rows_on(pool, sql).await {
        let count = rows.len();
        for r in rows {
            if let (Some(k), Some(v)) = (
                r.get("keyName").and_then(|x| x.as_str()),
                r.get("data").and_then(|x| x.as_str()),
            ) {
                set_param_cache(k, v).await;
            }
        }
        tracing::info!(count, "参数缓存已刷新");
    }
}
