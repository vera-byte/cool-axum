//! 多租户中间件：按配置对匹配 URL 注入当前用户 tenantId
//!
//! 对齐 cool-admin-midway Subscriber：select/insert/update/delete 自动隔离。

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use cool_core::CrudState;
use cool_core::tenant::{TenantPolicy, url_matches, with_tenant};

/// 供 `from_fn_with_state` 使用：(应用状态, 租户策略)
#[derive(Clone)]
pub struct TenantLayerState<S> {
    pub app: S,
    pub policy: TenantPolicy,
}

pub async fn tenant_middleware<S: CrudState>(
    State(st): State<TenantLayerState<S>>,
    req: Request,
    next: Next,
) -> Response {
    if !st.policy.enable {
        return next.run(req).await;
    }
    let path = req.uri().path().to_string();
    if !url_matches(&st.policy.urls, &path) {
        return next.run(req).await;
    }

    // 取当前登录用户 tenantId（未登录 / 查不到则不限制）
    let mut tenant_id = None;
    if let Some(login_id) = crate::authority::login_id_from_headers(req.headers()).await
        && let Ok(uid) = login_id.parse::<i64>()
    {
        tenant_id = lookup_tenant_id(&st.app, uid).await;
    }

    with_tenant(tenant_id, next.run(req)).await
}

async fn lookup_tenant_id<S: CrudState>(st: &S, user_id: i64) -> Option<i64> {
    use cool_core::service::query_rows_on;
    let pool = st.db().default_pool().ok()?;
    let sql = format!("SELECT tenantId FROM `base_sys_user` WHERE id = {user_id} LIMIT 1");
    let rows = query_rows_on(pool, &sql).await.ok()?;
    rows.first()
        .and_then(|r| r.get("tenantId"))
        .and_then(|v| v.as_i64())
}
