//! token 规范化：把 sa-token 头 / cookie 中的令牌写入 Authorization
//!
//! 兼容前端多种传参方式，保证 sa-token 层（token_name=Authorization）能取到。

use axum::extract::Request;
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

pub async fn normalize_token_header(mut req: Request, next: Next) -> Response {
    let has_auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if !has_auth {
        // 1) sa-token 头
        if let Some(v) = req
            .headers()
            .get("sa-token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            if let Ok(hv) = HeaderValue::from_str(&v) {
                req.headers_mut().insert(header::AUTHORIZATION, hv);
            }
        } else {
            // 2) cookie 中的 satoken / token / Authorization
            let from_cookie = req
                .headers()
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .and_then(|c| {
                    for part in c.split(';') {
                        let mut kv = part.trim().splitn(2, '=');
                        let k = kv.next().unwrap_or("");
                        let v = kv.next().unwrap_or("");
                        if matches!(k, "satoken" | "sa-token" | "token" | "Authorization" | "authorization")
                            && !v.is_empty()
                        {
                            return Some(v.to_string());
                        }
                    }
                    None
                });
            if let Some(v) = from_cookie {
                if let Ok(hv) = HeaderValue::from_str(&v) {
                    req.headers_mut().insert(header::AUTHORIZATION, hv);
                }
            }
        }
    }

    next.run(req).await
}
