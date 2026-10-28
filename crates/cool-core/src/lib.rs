//! cool-core：对齐 cool-admin-go 的 cool 包核心能力
//!
//! - 统一响应体（BaseRes）与错误码
//! - Model 基础字段
//! - SQL 构建器 + QueryOp + CrudService + CrudController（CRUD 引擎）

pub mod bus;
pub mod controller;
pub mod error;
pub mod model;
pub mod module;
pub mod query;
pub mod response;
pub mod service;
pub mod sql;
pub mod tenant;

// 合并进来的基础库（原 cool-auth/cool-cache/cool-config/cool-db/cool-file/cool-raft）
pub mod auth;
pub mod cache;
pub mod config;
pub mod db;
pub mod file;
pub mod raft;

pub use controller::{ApiAllow, CrudState, crud_router};
pub use error::CoolError;
// 分布式事件：类型 + 便捷命名空间 `event`（方法式调用 event.on / event.emit / event.global_emit）
pub use bus::{
    BoxFut, DeliverMode, Event, EventApi, EventBus, EventBusConfig, EventData, Handler, global,
    init_global,
};

/// 事件命名空间实例，方法式调用入口（`cool_core::event.on(...)` 等）
///
/// 放在 crate 根（而非 `bus` 内），避免与各函数的 `event` 形参同名冲突。
#[allow(non_upper_case_globals)]
pub const event: EventApi = EventApi;

pub use model::{Model, ModelMeta};
pub use module::{ModuleSpec, RouteGroups};
pub use query::{JoinOp, JoinType, ListReq, PageReq, QueryOp};
pub use response::{BaseRes, fail, ok};
pub use service::CrudService;
pub use sql::{SqlBuilder, safe_order_by};
