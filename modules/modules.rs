//! 模块注册：对齐 cool-admin-go `modules/modules.go`
//!
//! Go 通过 blank import 触发各模块 `init()`；
//! Rust 无副作用 init，此处显式注册全部模块的表/种子/启动钩子。
//!
//! 新增模块时：在此追加一行 `spec`/`init`，并在 Cargo.toml 引入依赖。

use cool_core::ModuleSpec;
use cool_db::{DbManager, TableDef};

/// 全部模块注册（对齐 modules.go 的 blank import 列表）
pub fn all_specs() -> Vec<ModuleSpec> {
    vec![
        mod_base::spec(),
        mod_dict::spec(),
        mod_demo::spec(),
        mod_task::spec(),
        mod_space::spec(),
    ]
}

/// 全部业务表（autoMigrate 用）
pub fn all_tables() -> Vec<TableDef> {
    all_specs().into_iter().flat_map(|s| s.tables).collect()
}

/// 全部 initjson 种子：(模块名, 表名, JSON)
///
/// 模块名仅用于日志；落库分组固定 default（对齐 model.GroupName()）
pub fn all_seeds() -> Vec<(&'static str, &'static str, &'static str)> {
    all_specs()
        .into_iter()
        .flat_map(|s| {
            let name = s.name;
            s.seeds
                .into_iter()
                .map(move |(table, json)| (name, table, json))
        })
        .collect()
}

/// 模块启动钩子（对齐各模块 init()）
pub async fn boot_all(db: &DbManager, cache: cool_cache::CacheManager) {
    mod_base::init(db, cache).await;
    mod_dict::init(db).await;
    mod_demo::init(db).await;
    mod_task::init(db).await;
    mod_space::init(db).await;
}
