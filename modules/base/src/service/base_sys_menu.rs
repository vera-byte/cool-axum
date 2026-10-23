//! base_sys_menu：菜单服务（对齐 service/base_sys_menu.go）
//!
//! 菜单/权限查询逻辑集中在 base_sys_perms，本文件提供菜单专用入口。

use super::base_sys_perms;
use cool_db::DbManager;
use serde_json::Value;

/// 获取菜单权限点
pub async fn get_perms(db: &DbManager, user_id: i64) -> Vec<String> {
    base_sys_perms::get_user_perms(db, user_id).await
}

/// 获取菜单树
pub async fn get_menus(db: &DbManager, user_id: i64) -> Vec<Value> {
    base_sys_perms::get_user_menus(db, user_id).await
}
