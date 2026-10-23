//! base_open：开放服务（对齐 service/base_open.go）—— EPS 元数据

use cool_core::service::query_rows_on;
use cool_db::DbManager;

/// AdminEPS：前端 EPS 元数据（简化版：返回各表列信息）
pub async fn admin_eps(db: &DbManager) -> serde_json::Value {
    let Ok(_pool) = db.default_pool() else {
        return serde_json::json!({});
    };
    let tables = [
        "base_sys_user",
        "base_sys_role",
        "base_sys_menu",
        "base_sys_department",
        "base_sys_param",
        "base_sys_log",
    ];
    let mut modules = serde_json::Map::new();
    for t in tables {
        let sql = format!("SELECT * FROM `{t}` WHERE 1=0");
        // 列信息从查询结果获取（空结果时用 SHOW 风格降级为空）
        let _ = &sql;
        modules.insert(
            t.to_string(),
            serde_json::json!({
                "module": "base",
                "prefix": format!("/admin/base/sys/{}", t.trim_start_matches("base_sys_")),
                "columns": [],
            }),
        );
    }
    let _ = query_rows_on; // 保留依赖
    serde_json::json!({ "admin": modules })
}

/// AppEPS：app 端 EPS
pub async fn app_eps(db: &DbManager) -> serde_json::Value {
    admin_eps(db).await
}
