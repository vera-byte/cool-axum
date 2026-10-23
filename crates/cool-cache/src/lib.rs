//! cool-cache：缓存管理器，对齐 cool-admin-go `cool.CacheManager`
//!
//! - **内存模式**：进程内 HashMap + TTL，适合单机开发
//! - **Redis 模式**：多节点共享（配置 `redis.cool` 后自动启用）
//!
//! 与 cool-admin-go 的 gcache API 对齐：Get / Set / Remove / MustGetOrSet，
//! `ttl=0` 表示永不过期。

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("缓存后端错误: {0}")]
    Backend(String),
}

/// 缓存条目（内存模式）
struct MemEntry {
    value: Value,
    expire_at: Option<Instant>,
}

/// 缓存后端
enum Backend {
    Memory(Mutex<HashMap<String, MemEntry>>),
    Redis(redis::aio::ConnectionManager),
}

/// 缓存管理器
#[derive(Clone)]
pub struct CacheManager {
    backend: std::sync::Arc<Backend>,
}

impl CacheManager {
    /// 内存模式
    pub fn new_memory() -> Self {
        Self {
            backend: std::sync::Arc::new(Backend::Memory(Mutex::new(HashMap::new()))),
        }
    }

    /// Redis 模式（连接失败回退内存，不阻塞启动）
    pub async fn new_redis(url: &str) -> Self {
        let client = redis::Client::open(url);
        match client {
            Ok(c) => match redis::aio::ConnectionManager::new(c).await {
                Ok(cm) => {
                    tracing::info!(url, "CacheManager 使用 Redis（多节点共享）");
                    Self {
                        backend: std::sync::Arc::new(Backend::Redis(cm)),
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "CacheManager Redis 连接失败，回退内存");
                    Self::new_memory()
                }
            },
            Err(e) => {
                tracing::error!(error = %e, "CacheManager Redis 配置错误，回退内存");
                Self::new_memory()
            }
        }
    }

    /// 是否 Redis 模式
    pub fn is_redis_mode(&self) -> bool {
        matches!(*self.backend, Backend::Redis(_))
    }

    /// 读取
    pub async fn get(&self, key: &str) -> Option<Value> {
        match &*self.backend {
            Backend::Memory(map) => {
                let mut map = map.lock().unwrap();
                if let Some(entry) = map.get(key) {
                    if let Some(exp) = entry.expire_at
                        && Instant::now() >= exp
                    {
                        map.remove(key);
                        return None;
                    }
                    return Some(entry.value.clone());
                }
                None
            }
            Backend::Redis(cm) => {
                let mut conn = cm.clone();
                let res: Option<String> = redis::cmd("GET")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .ok()
                    .flatten();
                res.and_then(|s| serde_json::from_str(&s).ok())
            }
        }
    }

    /// 写入，`ttl=0` 表示永不过期
    pub async fn set(&self, key: &str, value: Value, ttl: Duration) -> Result<(), CacheError> {
        match &*self.backend {
            Backend::Memory(map) => {
                let expire_at = if ttl.is_zero() {
                    None
                } else {
                    Some(Instant::now() + ttl)
                };
                map.lock()
                    .unwrap()
                    .insert(key.to_string(), MemEntry { value, expire_at });
                Ok(())
            }
            Backend::Redis(cm) => {
                let mut conn = cm.clone();
                let payload = serde_json::to_string(&value)
                    .map_err(|e| CacheError::Backend(e.to_string()))?;
                if ttl.is_zero() {
                    redis::cmd("SET")
                        .arg(key)
                        .arg(payload)
                        .query_async::<()>(&mut conn)
                        .await
                        .map_err(|e| CacheError::Backend(e.to_string()))?;
                } else {
                    redis::cmd("SET")
                        .arg(key)
                        .arg(payload)
                        .arg("EX")
                        .arg(ttl.as_secs().max(1))
                        .query_async::<()>(&mut conn)
                        .await
                        .map_err(|e| CacheError::Backend(e.to_string()))?;
                }
                Ok(())
            }
        }
    }

    /// 写入字符串便捷方法
    pub async fn set_str(&self, key: &str, value: &str, ttl: Duration) -> Result<(), CacheError> {
        self.set(key, Value::String(value.to_string()), ttl).await
    }

    /// 删除
    pub async fn remove(&self, key: &str) {
        match &*self.backend {
            Backend::Memory(map) => {
                map.lock().unwrap().remove(key);
            }
            Backend::Redis(cm) => {
                let mut conn = cm.clone();
                let _: Option<u32> = redis::cmd("DEL")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(None);
            }
        }
    }

    /// 是否存在（且未过期）
    pub async fn exists(&self, key: &str) -> bool {
        self.get(key).await.is_some()
    }

    /// 取值，缺失时用 `f` 生成并写入（对齐 MustGetOrSet）
    pub async fn get_or_set(&self, key: &str, ttl: Duration, f: impl FnOnce() -> Value) -> Value {
        if let Some(v) = self.get(key).await {
            return v;
        }
        let v = f();
        let _ = self.set(key, v.clone(), ttl).await;
        v
    }

    /// 自增（用于计数器，如登录失败次数）
    ///
    /// 返回自增后的值；key 不存在时从 0 开始。
    pub async fn increment(&self, key: &str, ttl: Duration) -> Result<u64, CacheError> {
        match &*self.backend {
            Backend::Memory(map) => {
                let mut map = map.lock().unwrap();
                let now = Instant::now();
                let entry = map.entry(key.to_string()).or_insert_with(|| MemEntry {
                    value: Value::from(0u64),
                    expire_at: if ttl.is_zero() { None } else { Some(now + ttl) },
                });
                if let Some(exp) = entry.expire_at
                    && now >= exp
                {
                    entry.value = Value::from(0u64);
                    entry.expire_at = if ttl.is_zero() { None } else { Some(now + ttl) };
                }
                let n = entry.value.as_u64().unwrap_or(0) + 1;
                entry.value = Value::from(n);
                Ok(n)
            }
            Backend::Redis(cm) => {
                let mut conn = cm.clone();
                let n: u64 = redis::cmd("INCR")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| CacheError::Backend(e.to_string()))?;
                // 首次设置过期
                if n == 1 && !ttl.is_zero() {
                    let _: Option<u64> = redis::cmd("EXPIRE")
                        .arg(key)
                        .arg(ttl.as_secs().max(1))
                        .query_async(&mut conn)
                        .await
                        .unwrap_or(None);
                }
                Ok(n)
            }
        }
    }

    /// 清空内存缓存（仅内存模式有效）
    pub fn clear_memory(&self) {
        if let Backend::Memory(map) = &*self.backend {
            map.lock().unwrap().clear();
        }
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new_memory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_set_get_remove() {
        let c = CacheManager::new_memory();
        c.set("k", json_v("v"), Duration::ZERO).await.unwrap();
        assert_eq!(c.get("k").await, Some(json_v("v")));
        c.remove("k").await;
        assert_eq!(c.get("k").await, None);
    }

    #[tokio::test]
    async fn memory_ttl_expire() {
        let c = CacheManager::new_memory();
        c.set("k", json_v("v"), Duration::from_millis(30))
            .await
            .unwrap();
        assert!(c.exists("k").await);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!c.exists("k").await);
    }

    #[tokio::test]
    async fn memory_get_or_set() {
        let c = CacheManager::new_memory();
        let v1 = c.get_or_set("k", Duration::ZERO, || json_v("a")).await;
        assert_eq!(v1, json_v("a"));
        let v2 = c.get_or_set("k", Duration::ZERO, || json_v("b")).await;
        assert_eq!(v2, json_v("a"));
    }

    #[tokio::test]
    async fn memory_increment() {
        let c = CacheManager::new_memory();
        assert_eq!(c.increment("n", Duration::from_secs(60)).await.unwrap(), 1);
        assert_eq!(c.increment("n", Duration::from_secs(60)).await.unwrap(), 2);
    }

    fn json_v(s: &str) -> Value {
        Value::String(s.to_string())
    }
}
