//! 多租户数据隔离（对齐 cool-admin-midway `BaseEntity.tenantId` + Subscriber 注入）
//!
//! - 请求级 `TENANT` 作用域保存当前租户 ID
//! - CRUD 读写自动注入 `tenantId` 条件（info/list/page/update/delete）
//! - add 自动写入 `tenantId`
//! - `no_tenant` 局部关闭过滤（自定义 SQL 场景）

use std::cell::Cell;
use std::future::Future;
use std::sync::{Arc, RwLock};

/// 租户过滤策略（由 cool-config 注入）
#[derive(Debug, Clone, Default)]
pub struct TenantPolicy {
    pub enable: bool,
    pub urls: Vec<String>,
}

tokio::task_local! {
    /// 当前请求的租户 ID；`None` 表示不限制（未开启 / 超管 / 未匹配 URL）
    static TENANT: Cell<Option<i64>>;
}

/// 当前租户 ID（无作用域时 None）
pub fn current_tenant() -> Option<i64> {
    TENANT.try_with(|c| c.get()).ok().flatten()
}

/// 在指定租户作用域内执行
pub async fn with_tenant<F: Future>(tenant: Option<i64>, f: F) -> F::Output {
    TENANT.scope(Cell::new(tenant), f).await
}

/// 在「不限制租户」作用域内执行（对应 midway `noTenant`）
pub async fn no_tenant<F: Future>(f: F) -> F::Output {
    TENANT.scope(Cell::new(None), f).await
}

/// URL 是否命中租户过滤列表（支持 `*` 单段、`**` 多段）
///
/// `/admin/**` 匹配 `/admin/base/sys/user/page`
/// `/admin/*/user/*` 匹配 `/admin/base/user/add`
pub fn url_matches(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|p| glob_match(p, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let s: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match_glob(&p, &s)
}

fn match_glob(p: &[&str], s: &[&str]) -> bool {
    if p.is_empty() {
        return s.is_empty();
    }
    match p[0] {
        "**" => {
            // 贪婪匹配 0..=n 段
            for i in 0..=s.len() {
                if match_glob(&p[1..], &s[i..]) {
                    return true;
                }
            }
            false
        }
        "*" => !s.is_empty() && match_glob(&p[1..], &s[1..]),
        seg => !s.is_empty() && s[0] == seg && match_glob(&p[1..], &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_basic() {
        let pats = vec!["/admin/**".to_string()];
        assert!(url_matches(&pats, "/admin/base/sys/user/page"));
        assert!(url_matches(&pats, "/admin"));
        assert!(!url_matches(&pats, "/base/sys/user/page"));
    }

    #[test]
    fn glob_star_segment() {
        let pats = vec!["/admin/*/user/*".to_string()];
        assert!(url_matches(&pats, "/admin/base/user/add"));
        assert!(!url_matches(&pats, "/admin/base/role/add"));
    }
}

// ── 请求上下文 / 软删除回收钩子 ──────────────────────────────

/// 请求元信息（删除回收记录用）
#[derive(Debug, Clone, Default)]
pub struct RequestMeta {
    pub user_id: Option<i64>,
    pub url: String,
}

tokio::task_local! {
    static REQ_META: std::cell::RefCell<RequestMeta>;
}

/// 注入请求元信息（中间件调用）
pub async fn with_request_meta<F: Future>(meta: RequestMeta, f: F) -> F::Output {
    REQ_META.scope(std::cell::RefCell::new(meta), f).await
}

/// 当前请求元信息
pub fn current_request_meta() -> RequestMeta {
    REQ_META
        .try_with(|c| c.borrow().clone())
        .unwrap_or_default()
}

/// 软删除记录（回收站）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SoftDeleteRecord {
    pub table: String,
    pub ids: Vec<i64>,
    /// 删除前的行快照
    pub rows: Vec<serde_json::Value>,
    pub user_id: Option<i64>,
    pub url: String,
}

type SoftDeleteHook = Arc<dyn Fn(SoftDeleteRecord) + Send + Sync>;
static SOFT_DELETE_HOOK: RwLock<Option<SoftDeleteHook>> = RwLock::new(None);

/// 注册软删除钩子（软删除事件之外的备用扩展点）
pub fn set_soft_delete_hook(h: SoftDeleteHook) {
    if let Ok(mut g) = SOFT_DELETE_HOOK.write() {
        *g = Some(h);
    }
}

/// 通知软删除（失败不影响业务）
///
/// 经事件总线触发 `Event::SoftDelete`（对齐 cool-admin-midway core
/// `coolEventManager.emit(EVENT.SOFT_DELETE, params)`），回收站等处理器按事件名注册
/// （对齐 `@Event(EVENT.SOFT_DELETE) softDelete(params)`）。软删除为**本进程**事件——
/// 回收落库是共享存储，跨机会产生重复记录，故用 `emit` 而非 `global_emit`。
/// 若注册了 `set_soft_delete_hook` 钩子则同步调用（备用扩展点，通常无需）。
pub fn notify_soft_delete(rec: SoftDeleteRecord) {
    if let Ok(g) = SOFT_DELETE_HOOK.read()
        && let Some(h) = g.as_ref()
    {
        h(rec.clone());
    }
    // 经事件总线触发 onSoftDelete（本进程，fire-and-forget）
    if let Some(bus) = crate::event::global() {
        let data = serde_json::to_value(&rec).unwrap_or(serde_json::Value::Null);
        tokio::spawn(async move {
            bus.emit(crate::event::Event::SoftDelete, data).await;
        });
    }
}
