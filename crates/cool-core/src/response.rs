//! 统一响应体与错误码，对齐 cool-admin-go 的 BaseRes / MiddlewareHandlerResponse
//!
//! 错误码约定：
//! - 1000 成功
//! - 1001 业务失败
//! - 51   参数错误
//! - 1003 内部错误
//! - 403  无权限
//! - 404  未找到
//! - 500  服务器错误

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// 成功
pub const CODE_OK: i32 = 1000;
/// 业务失败
pub const CODE_FAIL: i32 = 1001;
/// 参数错误
pub const CODE_PARAM: i32 = 51;
/// 内部错误
pub const CODE_INTERNAL: i32 = 1003;
/// 无权限
pub const CODE_FORBIDDEN: i32 = 403;
/// 未找到
pub const CODE_NOT_FOUND: i32 = 404;
/// 服务器错误
pub const CODE_SERVER: i32 = 500;

/// 统一响应体（对齐 cool-admin-go DefaultHandlerResponse）
#[derive(Debug, Clone, Serialize)]
pub struct BaseRes {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl BaseRes {
    pub fn new(code: i32, message: impl Into<String>, data: Option<serde_json::Value>) -> Self {
        Self {
            code,
            message: message.into(),
            data,
        }
    }
}

/// 正常返回（code=1000）
pub fn ok<T: Serialize>(data: T) -> BaseRes {
    BaseRes {
        code: CODE_OK,
        message: "success".into(),
        data: serde_json::to_value(data).ok(),
    }
}

/// 无数据成功返回
pub fn ok_empty() -> BaseRes {
    BaseRes {
        code: CODE_OK,
        message: "success".into(),
        data: None,
    }
}

/// 失败返回（code=1001）
pub fn fail(message: impl Into<String>) -> BaseRes {
    BaseRes {
        code: CODE_FAIL,
        message: message.into(),
        data: None,
    }
}

impl IntoResponse for BaseRes {
    fn into_response(self) -> Response {
        let status = if self.code == CODE_OK {
            StatusCode::OK
        } else {
            // cool-admin-go 风格：HTTP 200 + 业务 code
            StatusCode::OK
        };
        (status, Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_response_code() {
        let r = ok(serde_json::json!({"id": 1}));
        assert_eq!(r.code, CODE_OK);
        assert_eq!(r.message, "success");
    }

    #[test]
    fn fail_response_code() {
        let r = fail("错误");
        assert_eq!(r.code, CODE_FAIL);
        assert_eq!(r.message, "错误");
    }
}
