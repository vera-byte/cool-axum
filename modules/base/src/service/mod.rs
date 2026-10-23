//! service 层：业务服务（对齐 cool-admin-go modules/base/service/*.go）

pub mod base_open;
pub mod base_sys_conf;
pub mod base_sys_department;
pub mod base_sys_log;
pub mod base_sys_login;
pub mod base_sys_menu;
pub mod base_sys_param;
pub mod base_sys_perms;
pub mod base_sys_role;
pub mod base_sys_user;
pub mod captcha;

// 兼容旧路径 re-export（cool-axum 使用 mod_base::login::xxx）
pub use base_sys_login as login;
pub use captcha as captcha_mod;
