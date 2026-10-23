//! cool-core：对齐 cool-admin-go 的 cool 包核心能力
//!
//! - 统一响应体（BaseRes）与错误码
//! - Model 基础字段
//! - SQL 构建器 + QueryOp + CrudService + CrudController（CRUD 引擎）

pub mod controller;
pub mod error;
pub mod module;
pub mod model;
pub mod query;
pub mod response;
pub mod service;
pub mod sql;

pub use controller::{crud_router, ApiAllow, CrudState};
pub use error::CoolError;
pub use module::ModuleSpec;
pub use model::{Model, ModelMeta};
pub use query::{JoinOp, JoinType, ListReq, PageReq, QueryOp};
pub use response::{fail, ok, BaseRes};
pub use service::CrudService;
pub use sql::{safe_order_by, SqlBuilder};
