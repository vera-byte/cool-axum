//! config：模块配置（对齐 modules/recycle/config）
//!
//! 配置项读取 cool-config 的 `modules.recycle` 节。

/// 获取模块配置（按需扩展）
pub fn get(key: &str) -> Option<String> {
    tracing::debug!(key, "mod-recycle config::get");
    None
}
