//! middleware 层：对齐 cool-admin-go modules/base/middleware
//!
//! - authority：登录/权限校验
//! - log：操作日志
//! - i18n：国际化（预留）

pub mod authority;
pub mod log;
pub mod token_norm;

pub use authority::*;
pub use log::request_log;
pub use token_norm::normalize_token_header;
