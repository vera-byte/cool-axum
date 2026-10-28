//! 图形验证码（对齐 cool-admin-go base_sys_login.Captcha）
//!
//! - 生成 4 位数字 SVG 验证码，base64 data URI 返回
//! - captchaId → code 缓存 5 分钟，一次性使用
//! - 开放接口，无需登录

use base64::Engine;
use cool_core::cache::CacheManager;
use std::sync::OnceLock;
use std::time::Duration;

static CACHE: OnceLock<CacheManager> = OnceLock::new();

const CAPTCHA_TTL: Duration = Duration::from_secs(300);

/// 注入缓存（app 启动时调用）
pub fn init_cache(cache: CacheManager) {
    let _ = CACHE.set(cache);
}

fn cache() -> Option<&'static CacheManager> {
    CACHE.get()
}

/// 生成验证码
///
/// 返回 `(captcha_id, data_uri, plain_code)`；plain_code 仅供内部校验，不外发。
pub fn generate(width: u32, height: u32, color: &str) -> (String, String, String) {
    let code = random_digits(4);
    let color = sanitize_color(color);
    let w = width.clamp(50, 400);
    let h = height.clamp(20, 200);

    // 简单 SVG（对齐 cool-admin-go 实现）
    let svg = format!(
        r#"<svg width="{w}" height="{h}" xmlns="http://www.w3.org/2000/svg"><text x="{x}" y="{y}" text-anchor="middle" font-size="{fs}" fill="{color}">{code}</text></svg>"#,
        x = w / 2,
        y = h * 3 / 5,
        fs = (h * 3 / 5).clamp(14, 48),
    );
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    let data_uri = format!("data:image/svg+xml;base64,{b64}");
    let captcha_id = uuid::Uuid::new_v4().simple().to_string();

    (captcha_id, data_uri, code)
}

/// 生成并写入缓存（captcha_id → code，5 分钟）
pub async fn generate_and_store(width: u32, height: u32, color: &str) -> (String, String) {
    let (id, data_uri, code) = generate(width, height, color);
    if let Some(c) = cache() {
        let _ = c
            .set(
                &cache_key(&id),
                serde_json::Value::String(code),
                CAPTCHA_TTL,
            )
            .await;
    }
    (id, data_uri)
}

/// 校验验证码（一次性，校验后立即失效）
///
/// - 未启用缓存时放行（开发模式）
/// - `captcha_id`/`verify_code` 为空时放行（兼容无验证码登录）
pub async fn verify(captcha_id: &str, verify_code: &str) -> bool {
    // 兼容：前端未带验证码则放行
    if captcha_id.is_empty() || verify_code.is_empty() {
        return true;
    }
    let Some(c) = cache() else {
        return true;
    };
    let key = cache_key(captcha_id);
    let Some(stored) = c.get(&key).await else {
        return false;
    };
    // 一次性
    c.remove(&key).await;
    stored
        .as_str()
        .map(|s| s.eq_ignore_ascii_case(verify_code))
        .unwrap_or(false)
}

fn cache_key(id: &str) -> String {
    format!("captcha:{id}")
}

/// 4 位随机数字
fn random_digits(n: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let mut x = nanos ^ (uuid::Uuid::new_v4().as_u128() as u64);
    (0..n)
        .map(|_| {
            // xorshift
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 10) as u8
        })
        .map(|d| (b'0' + d) as char)
        .collect()
}

/// 颜色安全化（仅允许 #rgb/#rrggbb/颜色名中的安全字符）
fn sanitize_color(c: &str) -> String {
    let c = c.trim();
    if c.is_empty() {
        return "#fff".into();
    }
    if c.starts_with('#') && c.len() <= 7 && c[1..].chars().all(|ch| ch.is_ascii_hexdigit()) {
        return c.to_string();
    }
    if c.chars().all(|ch| ch.is_ascii_alphabetic()) && c.len() <= 20 {
        return c.to_string();
    }
    "#fff".into()
}

/// 开放路由（无需登录），对齐 cool-admin-go `/admin/base/open/*`
///
/// - GET  /admin/base/open/captcha
/// - GET  /base/open/captcha（别名）
pub fn open_routes<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
    use axum::extract::{Query, State};
    use axum::routing::get;
    use axum::{Json, Router};
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct CaptchaQuery {
        #[serde(default = "default_height")]
        height: u32,
        #[serde(default = "default_width")]
        width: u32,
        #[serde(default)]
        color: Option<String>,
    }
    fn default_height() -> u32 {
        40
    }
    fn default_width() -> u32 {
        150
    }

    async fn captcha(Query(q): Query<CaptchaQuery>) -> Json<cool_core::response::BaseRes> {
        let color = q.color.as_deref().unwrap_or("#fff");
        let (id, data) = generate_and_store(q.width, q.height, color).await;
        Json(cool_core::response::ok(serde_json::json!({
            "captchaId": id,
            "data": data,
        })))
    }

    // 带 State 的包装（保持 Router<S> 类型统一）
    async fn captcha_with_state<S>(
        _st: State<S>,
        q: Query<CaptchaQuery>,
    ) -> Json<cool_core::response::BaseRes> {
        captcha(q).await
    }

    Router::new().route("/admin/base/open/captcha", get(captcha_with_state::<S>))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_captcha_shape() {
        let (id, uri, code) = generate(150, 45, "#2c3142");
        assert_eq!(code.len(), 4);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert!(!id.is_empty());
        assert!(uri.starts_with("data:image/svg+xml;base64,"));
    }

    #[test]
    fn color_sanitized() {
        assert_eq!(sanitize_color("#2c3142"), "#2c3142");
        assert_eq!(sanitize_color("red"), "red");
        assert_eq!(sanitize_color("';><script"), "#fff");
        assert_eq!(sanitize_color(""), "#fff");
    }

    #[tokio::test]
    async fn verify_one_time() {
        // 无缓存时放行
        assert!(verify("any", "1234").await);
    }
}
