//! funcs：任务函数注册（对齐 modules/base/funcs/base_sys_log_clear.go）

use cool_core::db::DbManager;
use std::sync::OnceLock;

static DB: OnceLock<DbManager> = OnceLock::new();

/// 注入 DB（供任务函数使用）
pub fn init(db: DbManager) {
    let _ = DB.set(db);
}

/// BaseFuncClearLog：清理日志任务
///
/// param == "true" 时连失败日志一起清
pub async fn base_func_clear_log(param: &str) -> Result<(), String> {
    let Some(db) = DB.get() else {
        return Err("数据库未就绪".into());
    };
    let deep = param.trim() == "true";
    tracing::info!(deep, "清理日志 BaseFuncClearLog");
    super::service::base_sys_log::clear(db, deep)
        .await
        .map(|_| ())
}
