//! 统一错误类型，映射到 cool-admin-go 错误码

use crate::response::{
    BaseRes, CODE_FAIL, CODE_FORBIDDEN, CODE_INTERNAL, CODE_NOT_FOUND, CODE_PARAM, CODE_SERVER,
};

#[derive(Debug, thiserror::Error)]
pub enum CoolError {
    #[error("{0}")]
    Biz(String),
    #[error("{0}")]
    Param(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Internal(String),
}

impl CoolError {
    pub fn biz(msg: impl Into<String>) -> Self {
        CoolError::Biz(msg.into())
    }
    pub fn param(msg: impl Into<String>) -> Self {
        CoolError::Param(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        CoolError::NotFound(msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        CoolError::Internal(msg.into())
    }

    /// 转换为响应错误码
    pub fn code(&self) -> i32 {
        match self {
            CoolError::Biz(_) => CODE_FAIL,
            CoolError::Param(_) => CODE_PARAM,
            CoolError::Forbidden(_) => CODE_FORBIDDEN,
            CoolError::NotFound(_) => CODE_NOT_FOUND,
            CoolError::Internal(_) => CODE_INTERNAL,
        }
    }

    pub fn to_res(&self) -> BaseRes {
        BaseRes::new(self.code(), self.to_string(), None)
    }
}

impl From<cool_db::DbError> for CoolError {
    fn from(e: cool_db::DbError) -> Self {
        CoolError::Internal(e.to_string())
    }
}

impl From<CoolError> for BaseRes {
    fn from(e: CoolError) -> Self {
        e.to_res()
    }
}

/// 供 axum handler 使用：Result<T, CoolError> 直接转响应
impl axum::response::IntoResponse for CoolError {
    fn into_response(self) -> axum::response::Response {
        self.to_res().into_response()
    }
}

// 避免未使用告警
#[allow(dead_code)]
fn _assert_codes() -> i32 {
    CODE_SERVER
}
