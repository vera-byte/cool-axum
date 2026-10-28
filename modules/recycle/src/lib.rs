//! mod-recycle：数据回收站（对齐 cool-admin-midway modules/recycle）
//!
//! 软删除的数据自动进入 `recycle_data`，可在后台**恢复**或**清理**。

pub mod api;
pub mod config;
pub mod controller;
pub mod funcs;
pub mod middleware;
pub mod model;
pub mod service;
pub mod utils;

use axum::Router;
use cool_core::*;
/// 全部表定义
pub fn all_tables() -> Vec<db::TableDef> {
    model::all_tables()
}

/// 开放路由（无）
pub fn open_routes<S: CrudState>() -> Router<S> {
    Router::new()
}

/// 业务路由
pub fn routes<S: CrudState>() -> Router<S> {
    controller::routes::<S>()
}

pub use model::initjson;

/// 模块注册声明
pub fn spec() -> cool_core::ModuleSpec {
    cool_core::ModuleSpec::new("recycle").tables(all_tables())
}

/// 启动钩子：注册软删除回收钩子 + 日清定时任务
pub async fn init(db: &cool_core::db::DbManager, _cache: cool_core::cache::CacheManager) {
    tracing::debug!("module recycle init start ...");

    // 软删除 → 回收站（对齐 midway `@Event(EVENT.SOFT_DELETE) softDelete(params)`）
    // 处理器按事件名注册，由 notify_soft_delete 经事件总线触发
    {
        let db_hook = db.clone();
        event.on(Event::SoftDelete, move |data: EventData| {
            let db = db_hook.clone();
            Box::pin(async move {
                if let Ok(rec) = serde_json::from_value::<tenant::SoftDeleteRecord>(data) {
                    service::data::record(&db, rec).await;
                }
            })
        });
    }

    // 每日清理过期回收数据（对齐 midway schedule）
    let db_task = db.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        tick.tick().await; // 跳过立即执行
        loop {
            tick.tick().await;
            // 每小时检查一次，仅 0 点附近执行清理由 clear 内部按天判断
            let hour: u32 = {
                use chrono::Timelike;
                chrono::Utc::now().hour()
            };
            if hour == 0 {
                tracing::info!("回收站过期数据清理开始");
                match service::data::clear(&db_task, false).await {
                    Ok(n) => tracing::info!(rows = n, "回收站清理完成"),
                    Err(e) => tracing::error!(error = %e, "回收站清理失败"),
                }
            }
        }
    });

    tracing::debug!("module recycle init finished ...");
}

/// 路由分组
pub fn route_groups<S>() -> cool_core::RouteGroups<S>
where
    S: cool_core::CrudState + Clone + Send + Sync + 'static,
{
    cool_core::RouteGroups::new().biz(routes::<S>())
}
