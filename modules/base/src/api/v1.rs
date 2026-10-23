//! api/v1：开放接口请求类型（对齐 api/v1/base_open.go）

use serde::Deserialize;

/// POST /admin/base/open/login
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseOpenLoginReq {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub captcha_id: Option<String>,
    #[serde(default)]
    pub verify_code: Option<String>,
}

/// GET /admin/base/open/captcha
#[derive(Debug, Deserialize)]
pub struct BaseOpenCaptchaReq {
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub color: Option<String>,
}

fn default_height() -> u32 {
    40
}
fn default_width() -> u32 {
    150
}
