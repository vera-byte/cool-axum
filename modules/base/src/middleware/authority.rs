//! 权限校验中间件：对齐 cool-admin-go modules/base/middleware/authority.go
//!
//! URL → 权限点映射：
//!   `/base/sys/menu/add` → `base:sys:menu:add`
//!   与 base_sys_menu.perms 对照（逗号分隔多个权限点）
//!
//! 规则：
//! - 超管（login_id=1 或 perms 含 `*`）放行
//! - 仅登录接口（/base/comm/* 等）跳过权限点校验
//! - 其余请求：perms 必须包含 URL 对应权限点，否则 403

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use cool_core::response::{BaseRes, CODE_FORBIDDEN};

/// 仅需登录、无需权限点校验的路径前缀（对齐 cool-admin-go AuthComm `/admin/*/comm/*`）
const COMM_PREFIXES: &[&str] = &[
    "/admin/base/comm/",
    "/app/base/comm/",
    "/admin/base/comm/userinfo",
    "/base/health",
    "/health",
    "/admin/task/info/once",
    "/admin/task/info/status",
    "/task/info/once",
    "/task/info/status",
];

/// 是否为 comm 路径（含通配：/*/comm/*）
fn is_comm_path(path: &str) -> bool {
    if COMM_PREFIXES.iter().any(|p| path.starts_with(p)) {
        return true;
    }
    // /admin/xxx/comm/yyy 或 /app/xxx/comm/yyy
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segs.len() >= 3 && (segs[0] == "admin" || segs[0] == "app") && segs[2] == "comm"
}

/// 公开路径（无需登录，防御性保留——外层已有 token 层放行）
const OPEN_PREFIXES: &[&str] = &["/admin/base/open/", "/health", "/base/health", "/public/"];

/// URL → 权限点：去掉查询串，按 `/` 切分后用 `:` 连接
///
/// `/base/sys/menu/add` → `base:sys:menu:add`
pub fn path_to_perm(path: &str) -> String {
    let path = path.split('?').next().unwrap_or(path);
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // 对齐 cool-admin-go：路由含 /admin 前缀时跳过 admin；我们的路由无 admin 前缀
    let segs = if segs.first() == Some(&"admin") {
        &segs[1..]
    } else {
        &segs[..]
    };
    segs.join(":").to_lowercase()
}

/// 是否为公开路径
fn is_open_path(path: &str) -> bool {
    path == "/" || OPEN_PREFIXES.iter().any(|p| path.starts_with(p))
}

/// 权限校验中间件
///
/// 前置条件：外层 SaTokenLayer 已把 SaLoginId 注入 request extensions
pub async fn authority_middleware(mut req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();

    // 公开路径
    if is_open_path(&path) {
        return next.run(req).await;
    }

    // 登录态：优先 SaLoginId extension，缺失时从 header 兜底解析
    let mut login_id = req
        .extensions()
        .get::<sa_token_plugin_axum::SaLoginId>()
        .map(|id| id.0.clone());
    if login_id.is_none()
        && let Some(tok) = extract_token(req.headers())
    {
        login_id = cool_core::auth::get_login_id(&tok).await;
    }
    let Some(login_id) = login_id else {
        return unauthorized("登陆失效～");
    };

    // 仅登录接口（comm）
    if is_comm_path(&path) {
        return next.run(req).await;
    }

    // 超管（userId=1）无条件放行
    if login_id == "1" {
        return next.run(req).await;
    }

    // 权限点含 `*` 放行
    let perms = cool_core::auth::get_permissions(&login_id).await;
    if perms.iter().any(|p| p == "*") {
        return next.run(req).await;
    }

    // URL → 权限点
    let perm = path_to_perm(&path);

    // perms 为空 → 403
    if perms.is_empty() {
        tracing::warn!(login_id, path, "权限点为空，拒绝访问");
        return forbidden();
    }

    // 逗号分隔展开（对齐 menu.perms 多值）
    let allowed = perms
        .iter()
        .flat_map(|p| p.split(','))
        .map(|p| p.trim().to_lowercase())
        .any(|p| p == perm);

    if !allowed {
        tracing::warn!(login_id, path, perm, "无权限访问");
        return forbidden();
    }

    // 注入 perm 供 handler 使用
    req.extensions_mut().insert(perm);
    let _ = &mut req;
    next.run(req).await
}

/// 从请求头提取 token 并解析 login_id
pub async fn login_id_from_headers(headers: &HeaderMap) -> Option<String> {
    let token = extract_token(headers)?;
    cool_core::auth::get_login_id(&token).await
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get("authorization")
        .or_else(|| headers.get("sa-token"))
        .and_then(|v| v.to_str().ok())?;
    let token = raw.strip_prefix("Bearer ").unwrap_or(raw);
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn unauthorized(msg: &str) -> Response {
    let body = BaseRes::new(1001, msg, None);
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json")],
        axum::Json(body),
    )
        .into_response()
}

fn forbidden() -> Response {
    let body = BaseRes::new(CODE_FORBIDDEN, "登录失效或无权限访问~", None);
    (
        StatusCode::FORBIDDEN,
        [(header::CONTENT_TYPE, "application/json")],
        axum::Json(body),
    )
        .into_response()
}

/// 供单元测试：展开逗号分隔权限点
pub fn expand_perms(perms: &[String]) -> Vec<String> {
    perms
        .iter()
        .flat_map(|p| p.split(','))
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect()
}

/// 静态构造 Body 占位（避免 unused warning）
#[allow(dead_code)]
fn _body() -> Body {
    Body::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_perm_basic() {
        assert_eq!(path_to_perm("/base/sys/menu/add"), "base:sys:menu:add");
        assert_eq!(
            path_to_perm("/base/sys/menu/page?x=1"),
            "base:sys:menu:page"
        );
        assert_eq!(
            path_to_perm("/admin/base/sys/role/list"),
            "base:sys:role:list"
        );
        assert_eq!(path_to_perm("/dict/type/info"), "dict:type:info");
    }

    #[test]
    fn expand_multi_perms() {
        let perms = vec!["base:sys:menu:page,base:sys:menu:list".to_string()];
        let list = expand_perms(&perms);
        assert!(list.contains(&"base:sys:menu:page".to_string()));
        assert!(list.contains(&"base:sys:menu:list".to_string()));
    }
}
