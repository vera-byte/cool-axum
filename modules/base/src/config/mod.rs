//! config：base 模块配置（对齐 modules/base/config/config.go）
//!
//! 配置项来自 cool-config 的 `modules.base` 节。

pub use cool_config::{AuthConfig, BaseMiddlewareConfig, MiddlewareSwitch};

/// 获取鉴权配置
pub fn auth_config() -> Option<AuthConfig> {
    // 运行时从 cool_config::AppConfig 读取，此处提供类型别名与文档
    None
}
