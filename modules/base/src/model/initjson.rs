//! initjson 种子数据（include_str 内嵌，对齐 cool-admin-go resource/initjson）
//!
//! 通过 cool_core::db::fill_init_data 按 (group, table) 去重填充。

pub const MENU: &str = include_str!("../../initjson/base_sys_menu.json");
pub const USER: &str = include_str!("../../initjson/base_sys_user.json");
pub const USER_ROLE: &str = include_str!("../../initjson/base_sys_user_role.json");
pub const ROLE: &str = include_str!("../../initjson/base_sys_role.json");
pub const ROLE_MENU: &str = include_str!("../../initjson/base_sys_role_menu.json");
pub const DEPARTMENT: &str = include_str!("../../initjson/base_sys_department.json");
pub const ROLE_DEPARTMENT: &str = include_str!("../../initjson/base_sys_role_department.json");
pub const PARAM: &str = include_str!("../../initjson/base_sys_param.json");
pub const CONF: &str = include_str!("../../initjson/base_sys_conf.json");
