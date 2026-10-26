//! 多租户数据隔离（对齐 cool-admin-midway `BaseEntity.tenantId` + Subscriber 注入）
//!
//! - 请求级 `TENANT` 作用域保存当前租户 ID
//! - CRUD 读写自动注入 `tenantId` 条件（info/list/page/update/delete）
//! - add 自动写入 `tenantId`
//! - `no_tenant` 局部关闭过滤（自定义 SQL 场景）

use std::cell::Cell;
use std::future::Future;

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
