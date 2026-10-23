//! RocksDB 持久化存储
//!
//! - [`RocksLogStore`]：Raft 日志 + vote / committed 位点
//! - [`RocksStateMachine`]：applied-state（last_log_id + membership）
//!
//! 解决 memstore 无持久化问题：节点 kill 重启后 Raft 状态恢复。
//! 业务数据仍在 MySQL / Redis，此处不做 Raft KV。

mod log;
mod state_machine;

pub use log::RocksLogStore;
pub use state_machine::RocksStateMachine;
