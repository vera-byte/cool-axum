//! log 中间件：请求日志（对齐 middleware/log.go）
//!
//! - 控制台输出 method/path/status/cost
//! - 写入 base_sys_log（userId / action / ip / params），异步落库不阻塞响应

use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use cool_db::DbManager;
use std::sync::OnceLock;
use std::time::Instant;

static DB: OnceLock<DbManager> = OnceLock::new();

/// 注入 DB（app 启动时调用）
pub fn init(db: DbManager) {
    let _ = DB.set(db);
}

/// 请求日志中间件
pub async fn request_log(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|s| s.to_string()).unwrap_or_default();
    let (parts, _body) = req.into_parts();
    let headers = parts.headers.clone();
    let ip_ext = client_ip_from_ext(&parts);
    let req = Request::from_parts(parts, _body);
    let start = Instant::now();

    let res = next.run(req).await;

    let status = res.status().as_u16();
    let cost = start.elapsed().as_millis();

    // 控制台日志
    if status >= 400 {
        tracing::warn!(%method, path, status, cost_ms = cost, "request");
    } else {
        tracing::info!(%method, path, status, cost_ms = cost, "request");
    }

    // 落库（仅业务接口；跳过健康检查/静态资源）
    if should_record(&path) {
        let action = format!("{method}:{path}");
        let params = build_params(&query, &headers);
        let ip = if ip_ext.is_empty() {
            client_ip(&headers)
        } else {
            ip_ext
        };
        if let Some(db) = DB.get().cloned() {
            tokio::spawn(async move {
                let uid = resolve_user_id(&headers).await.unwrap_or(0);
                let action_esc = action.replace('\'', "''");
                let params_esc = params.replace('\'', "''");
                let ip_esc = ip.replace('\'', "''");
                let sql = format!(
                    "INSERT INTO `base_sys_log` (createTime, updateTime, userId, action, ip, ipAddr, params) \
                     VALUES (CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, {uid}, '{action_esc}', '{ip_esc}', '{ip_esc}', '{params_esc}')"
                );
                if let Ok(pool) = db.default_pool()
                    && let Err(e) = pool.execute(&sql).await
                {
                    tracing::error!(error = %e, "操作日志写入失败");
                }
            });
        }
    }

    res
}

/// 是否需要记录（跳过 health / 静态 / 登录验证码）
fn should_record(path: &str) -> bool {
    if path.starts_with("/public/") || path == "/" || path == "/health" || path == "/base/health" {
        return false;
    }
    // 保留登录日志（审计需要），但跳过验证码
    !path.contains("/captcha")
}

/// 构造 params：优先 query；对敏感字段脱敏
fn build_params(query: &str, headers: &HeaderMap) -> String {
    let _ = headers;
    if query.is_empty() {
        return String::new();
    }
    // query 参数脱敏
    let parts: Vec<String> = query
        .split('&')
        .map(|kv| {
            let mut it = kv.splitn(2, '=');
            let k = it.next().unwrap_or("");
            let v = it.next().unwrap_or("");
            if is_sensitive(k) {
                format!("{k}=***")
            } else {
                format!("{k}={v}")
            }
        })
        .collect();
    let s = parts.join("&");
    truncate(&s, 4000)
}

fn is_sensitive(key: &str) -> bool {
    let k = key.to_lowercase();
    k.contains("password")
        || k.contains("passwd")
        || k.contains("token")
        || k.contains("secret")
        || k.contains("verifycode")
        || k.contains("captcha")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

fn client_ip_from_ext(req_parts: &axum::http::request::Parts) -> String {
    use axum::extract::ConnectInfo;
    if let Some(ConnectInfo(addr)) = req_parts
        .extensions
        .get::<ConnectInfo<std::net::SocketAddr>>()
    {
        return addr.ip().to_string();
    }
    String::new()
}

fn client_ip(headers: &HeaderMap) -> String {
    if let Some(xff) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        && !xff.is_empty()
    {
        return xff;
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// 从 token 解析 userId（未登录为 0）
async fn resolve_user_id(headers: &HeaderMap) -> Option<i64> {
    let login_id = crate::middleware::authority::login_id_from_headers(headers).await?;
    login_id.parse().ok()
}

#[allow(dead_code)]
fn _status_ok() -> StatusCode {
    StatusCode::OK
}
