//! log 中间件：请求日志（对齐 middleware/log.go）
//!
//! - 控制台输出 method/path/status/cost
//! - 写入 base_sys_log（userId / action / ip / params），异步落库不阻塞响应

use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use cool_core::db::DbManager;
use std::sync::OnceLock;
use std::time::Instant;

static DB: OnceLock<DbManager> = OnceLock::new();

/// 超过该大小的 body（上传等）不记录参数，仅透传，避免大 body 占内存
const MAX_LOG_BODY: usize = 64 * 1024;

/// 注入 DB（app 启动时调用）
pub fn init(db: DbManager) {
    let _ = DB.set(db);
}

/// 请求日志中间件
pub async fn request_log(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|s| s.to_string()).unwrap_or_default();
    let trace = req
        .extensions()
        .get::<crate::middleware::trace::TraceId>()
        .map(|t| t.0.clone())
        .or_else(|| crate::middleware::trace::trace_from_headers(req.headers()))
        .unwrap_or_default();
    let (parts, body) = req.into_parts();
    let headers = parts.headers.clone();
    let ip_ext = client_ip_from_ext(&parts);
    // 缓冲 body 以便记录参数并透传给 handler（小 body 记录，大 body/上传仅透传）
    let (handler_body, body_params) = buffer_body(body, &headers).await;
    let req = Request::from_parts(parts, handler_body);
    let start = Instant::now();

    let uid = resolve_user_id(&headers).await;
    let meta = cool_core::tenant::RequestMeta {
        user_id: uid,
        url: path.clone(),
    };
    let res = cool_core::tenant::with_request_meta(meta, next.run(req)).await;

    let status = res.status().as_u16();
    let cost = start.elapsed().as_millis();

    // 控制台日志（含链路 ID；node= 由全局日志格式注入）
    if status >= 400 {
        tracing::warn!(%method, path, status, cost_ms = cost, %trace, "request");
    } else {
        tracing::info!(%method, path, status, cost_ms = cost, %trace, "request");
    }

    // 落库（仅业务接口；跳过健康检查/静态资源）
    if should_record(&path) {
        let action = format!("{method}:{path}");
        let params = build_params(&query, &body_params);
        let ip = if ip_ext.is_empty() {
            client_ip(&headers)
        } else {
            ip_ext
        };
        if let Some(db) = DB.get().cloned() {
            tokio::spawn(async move {
                let uid = resolve_user_id(&headers).await.unwrap_or(0);
                let action_esc = action.replace('\'', "''");
                // params 前缀链路 ID，便于 DB 侧按 trace 检索
                let params_full = if trace.is_empty() {
                    params
                } else {
                    format!("trace={trace} params={params}")
                };
                let params_esc = params_full.replace('\'', "''");
                let ip_esc = ip.replace('\'', "''");
                let sql = format!(
                    "INSERT INTO `base_sys_log` (createTime, updateTime, userId, action, ip, ipAddr, params) \
                     VALUES (CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, {uid}, '{action_esc}', '{ip_esc}', '{ip_esc}', '{params_esc}')"
                );
                match db.default_pool() {
                    Ok(pool) => {
                        if let Err(e) = pool.execute(&sql).await {
                            tracing::error!(error = %e, %trace, "操作日志写入失败");
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, %trace, "操作日志：取默认连接池失败");
                    }
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

/// 构造 params：query + body 参数，敏感字段脱敏，4000 截断
fn build_params(query: &str, body_params: &str) -> String {
    let mut s = String::new();
    if !query.is_empty() {
        s.push_str(&mask_query(query));
    }
    if !body_params.is_empty() {
        if !s.is_empty() {
            s.push('&');
        }
        s.push_str(body_params);
    }
    truncate(&s, 4000)
}

/// query / form 串脱敏（`k=v&k2=v2`）
fn mask_query(q: &str) -> String {
    q.split('&')
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
        .collect::<Vec<_>>()
        .join("&")
}

/// 缓冲请求 body：小 body 提取参数后重建透传；大 body（上传）不记录仅透传
async fn buffer_body(body: Body, headers: &HeaderMap) -> (Body, String) {
    let content_length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    // 无 body 或过大（上传）：不记录 body 参数，直接透传
    if content_length == 0 || content_length > MAX_LOG_BODY {
        return (body, String::new());
    }
    match to_bytes(body, MAX_LOG_BODY).await {
        Ok(bytes) => {
            let params = body_params_from(bytes.as_ref(), headers);
            (Body::from(bytes), params)
        }
        // 理论少见（content-length 与实际不符等），body 已消费，退化为不记录
        Err(_) => (Body::empty(), String::new()),
    }
}

/// 从 body 提取参数（JSON / form-urlencoded），敏感字段脱敏
fn body_params_from(bytes: &[u8], headers: &HeaderMap) -> String {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.contains("application/json") {
        match serde_json::from_slice::<serde_json::Value>(bytes) {
            Ok(v) => mask_json(&v),
            Err(_) => String::new(),
        }
    } else if ct.contains("application/x-www-form-urlencoded") {
        mask_query(&String::from_utf8_lossy(bytes))
    } else {
        // multipart / text 等不记录
        String::new()
    }
}

/// JSON 值脱敏（敏感 key 置 `***`）后序列化
fn mask_json(v: &serde_json::Value) -> String {
    let masked = mask_json_value(v);
    truncate(&serde_json::to_string(&masked).unwrap_or_default(), 4000)
}

fn mask_json_value(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, val)| {
                    if is_sensitive(k) {
                        (k.clone(), Value::String("***".into()))
                    } else {
                        (k.clone(), mask_json_value(val))
                    }
                })
                .collect(),
        ),
        Value::Array(arr) => Value::Array(arr.iter().map(mask_json_value).collect()),
        other => other.clone(),
    }
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
