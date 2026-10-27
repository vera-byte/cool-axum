//! middleware 层：对齐 cool-admin-go modules/base/middleware
//!
//! - authority：登录/权限校验
//! - log：操作日志
//! - i18n：国际化（预留）

pub mod authority;
pub mod log;
pub mod tenant;
pub mod token_norm;
pub mod trace;

pub use authority::*;
pub use log::request_log;
pub use tenant::{TenantLayerState, tenant_middleware};
pub use token_norm::normalize_token_header;
pub use trace::{TraceId, init_node, node_id, node_label, trace_from_headers, trace_id_middleware};
