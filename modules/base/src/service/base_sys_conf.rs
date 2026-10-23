//! base_sys_conf：系统配置服务（对齐 service/base_sys_conf.go）

/// 配置读取（当前走 cool-config，这里保留业务级配置入口）
pub fn get(key: &str) -> Option<String> {
    // 业务级动态配置，后续可接 base_sys_conf 表
    tracing::debug!(key, "base_sys_conf::get");
    None
}
