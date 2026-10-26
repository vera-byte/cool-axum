//! 链路追踪中间件
//!
//! - 生成或透传 `X-Trace-Id`，格式 **`{nodeLabel}-{hex}`**，可直接看出请求落在哪个节点
//! - `X-Node-Label` 支持自定义（`server.nodeLabel`），默认 `{nodeId}@{port}`
//! - 透传入站 `X-Trace-Id` 时保留原值于 `X-Parent-Trace-Id`，便于跨节点串联
//! - 响应写回 `X-Trace-Id` / `X-Node-Id` / `X-Node-Label`，日志打印 trace

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

static NODE_ID: RwLock<u64> = RwLock::new(0);
static NODE_PORT: RwLock<String> = RwLock::new(String::new());
static NODE_LABEL: RwLock<Option<String>> = RwLock::new(None);

/// 启动时注入本节点身份
///
/// - `listen_addr`：形如 `:8002` / `0.0.0.0:8002`
/// - `custom_label`：可选，`server.nodeLabel` 自定义节点标签
pub fn init_node(node_id: u64, listen_addr: &str, custom_label: Option<&str>) {
    if let Ok(mut g) = NODE_ID.write() {
        *g = node_id;
    }
    let port = listen_addr
        .rsplit(':')
        .next()
        .filter(|p| !p.is_empty())
        .unwrap_or("")
        .to_string();
    if let Ok(mut g) = NODE_PORT.write() {
        *g = port;
    }
    if let Ok(mut g) = NODE_LABEL.write() {
        *g = custom_label
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
    }
}

/// 本节点 ID（未初始化时为 0）
pub fn node_id() -> u64 {
    NODE_ID.read().map(|g| *g).unwrap_or(0)
}

/// 本节点标签：优先 `server.nodeLabel`，否则 `{nodeId}@{port}`
pub fn node_label() -> String {
    if let Ok(g) = NODE_LABEL.read()
        && let Some(l) = g.as_ref()
    {
        return l.clone();
    }
    let port = NODE_PORT.read().map(|g| g.clone()).unwrap_or_default();
    if !port.is_empty() {
        format!("{}@{}", node_id(), port)
    } else {
        format!("{}", node_id())
    }
}

/// 链路 ID 节点前缀：标签去掉分隔符，保证 `{prefix}-{hex}` 仍可解析
fn trace_prefix() -> String {
    sanitize_label(&node_label())
}

fn sanitize_label(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 从 header 取链路 ID（X-Trace-Id / X-Request-Id / traceparent 极简解析）
pub fn trace_from_headers(headers: &HeaderMap) -> Option<String> {
    for key in ["x-trace-id", "x-request-id", "x-correlation-id"] {
        if let Some(v) = headers.get(key).and_then(|v| v.to_str().ok()) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    // W3C traceparent: 00-<trace_id 32hex>-<span_id 16hex>-01
    if let Some(tp) = headers.get("traceparent").and_then(|v| v.to_str().ok()) {
        let mut it = tp.split('-');
        let _ver = it.next();
        if let Some(tid) = it.next()
            && tid.len() == 32
            && tid.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Some(tid.to_string());
        }
    }
    None
}

/// 请求上下文中的链路 ID
#[derive(Debug, Clone)]
pub struct TraceId(pub String);

/// 链路追踪中间件：放在最外层（先于 request_log）
pub async fn trace_id_middleware(mut req: Request, next: Next) -> Response {
    let parent = trace_from_headers(req.headers());
    let id = new_trace_id();
    // `{nodeLabel}-{hex}`：一眼看出落到哪个节点（标签可自定义）
    let full = format!("{}-{}", trace_prefix(), id);

    req.extensions_mut().insert(TraceId(full.clone()));
    if let Some(p) = &parent
        && p != &full
    {
        req.extensions_mut().insert(ParentTraceId(p.clone()));
    }

    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    // 已有 X-Trace-Id（如转发到 Leader 后回写原始 ID）则不覆盖
    if !headers.contains_key("x-trace-id")
        && let Ok(v) = full.parse()
    {
        headers.insert("x-trace-id", v);
    }
    if let Ok(v) = node_id().to_string().parse() {
        headers.insert("x-node-id", v);
    }
    if let Ok(v) = node_label().parse() {
        headers.insert("x-node-label", v);
    }
    if let Some(p) = &parent
        && p != &full
        && !headers.contains_key("x-parent-trace-id")
        && let Ok(v) = p.parse()
    {
        headers.insert("x-parent-trace-id", v);
    }
    res
}

/// 入站上游链路 ID（若有）
#[derive(Debug, Clone)]
pub struct ParentTraceId(pub String);

/// 当前请求链路 ID（供 handler / 日志读取）
pub fn current_trace(headers: &HeaderMap) -> String {
    // 优先响应侧已写入的值由中间件处理；此处给日志用：从 header 透传或新建
    trace_from_headers(headers).unwrap_or_else(new_trace_id)
}

fn new_trace_id() -> String {
    // 16 hex：时间戳 8 位 + 随机 8 位，足够短且基本唯一
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let rand = random_u64();
    format!("{:08x}{:08x}", (ts & 0xffff_ffff) as u32, (rand & 0xffff_ffff) as u32)
}

fn random_u64() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::thread::current().id().hash(&mut h);
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
        .hash(&mut h);
    std::process::id().hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_label_and_trace_prefix() {
        // 默认标签
        init_node(2, ":8002", None);
        assert_eq!(node_label(), "2@8002");
        let id = format!("{}-{}", trace_prefix(), new_trace_id());
        assert!(id.starts_with("2_8002-"), "默认标签进 trace id: {id}");

        // 自定义标签
        init_node(3, ":8003", Some("bj-gateway"));
        assert_eq!(node_label(), "bj-gateway");
        let id = format!("{}-{}", trace_prefix(), new_trace_id());
        assert!(id.starts_with("bj-gateway-"), "自定义标签进 trace id: {id}");
    }
}
