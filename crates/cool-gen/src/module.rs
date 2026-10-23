//! 模块脚手架：对齐 cool-admin-go `cool-tools module <name>`
//!
//! 生成与 `crates/modules/base` 一致的分层目录结构：
//! ```text
//! {name}/
//! ├── Cargo.toml
//! ├── initjson/
//! └── src/
//!     ├── api/{mod,v1}.rs
//!     ├── config/mod.rs
//!     ├── controller/{mod,admin/*,app/*}
//!     ├── funcs/mod.rs
//!     ├── middleware/mod.rs
//!     ├── model/{mod,initjson,{name}_item}.rs
//!     ├── service/{mod,{name}_item}.rs
//!     └── utils/mod.rs
//! ```

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};

pub fn create_module(name: &str) -> anyhow::Result<()> {
    validate_name(name)?;

    let root = find_workspace_root()?;
    let module_dir = root.join("modules").join(name);
    if module_dir.exists() {
        bail!("模块已存在: {}，请先删除", module_dir.display());
    }

    // 目录骨架（对齐 base）
    for d in [
        "src/api",
        "src/config",
        "src/controller/admin",
        "src/controller/app",
        "src/funcs",
        "src/middleware",
        "src/model",
        "src/service",
        "src/utils",
        "initjson",
    ] {
        std::fs::create_dir_all(module_dir.join(d))
            .with_context(|| format!("创建目录 {d}"))?;
    }

    let crate_name = format!("mod-{}", name.replace('_', "-"));
    let entity = format!("{name}_item");
    let entity_pascal = to_pascal(name) + "Item";

    // ── Cargo.toml ──────────────────────────────────────────────
    write(
        &module_dir,
        "Cargo.toml",
        &format!(
            r#"[package]
name = "{crate_name}"
version.workspace = true
edition.workspace = true

[dependencies]
cool-core = {{ workspace = true }}
cool-db = {{ workspace = true }}
cool-auth = {{ workspace = true }}
cool-cache = {{ workspace = true }}
cool-file = {{ workspace = true }}
axum = {{ workspace = true }}
tokio = {{ workspace = true }}
serde = {{ workspace = true }}
serde_json = {{ workspace = true }}
tracing = {{ workspace = true }}
"#
        ),
    )?;

    // ── src/lib.rs ─────────────────────────────────────────────
    write(
        &module_dir,
        "src/lib.rs",
        &format!(
            r#"//! mod-{name}：业务模块（由 cool-gen 生成）
//!
//! 目录结构对齐 crates/modules/base：
//! ```text
//! {name}/
//! ├── api/          接口请求类型
//! ├── config/       模块配置
//! ├── controller/   控制器（admin / app）
//! ├── funcs/        任务函数
//! ├── middleware/   中间件
//! ├── model/        表定义 + initjson
//! ├── service/      业务服务
//! └── utils/        工具函数
//! ```

pub mod api;
pub mod config;
pub mod controller;
pub mod funcs;
pub mod middleware;
pub mod model;
pub mod service;
pub mod utils;

use axum::Router;
use cool_core::CrudState;
use cool_db::TableDef;

/// 全部表定义
pub fn all_tables() -> Vec<TableDef> {{
    model::all_tables()
}}

/// 开放路由（免登录）
pub fn open_routes<S: CrudState>() -> Router<S> {{
    controller::admin::open_routes::<S>()
}}

/// 需登录路由（业务 CRUD）
pub fn routes<S: CrudState>() -> Router<S> {{
    controller::routes::<S>()
}}

// 兼容 re-export
pub use model::initjson;
"#
        ),
    )?;

    // ── api ────────────────────────────────────────────────────
    write(
        &module_dir,
        "src/api/mod.rs",
        "//! api：请求/响应类型定义\n\npub mod v1;\n",
    )?;
    write(
        &module_dir,
        "src/api/v1.rs",
        &format!(
            r#"//! api/v1：接口请求类型（按需补充）

use serde::Deserialize;

/// 示例：{entity} 创建请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Create{entity_pascal}Req {{
    pub name: String,
    #[serde(default)]
    pub remark: Option<String>,
}}
"#
        ),
    )?;

    // ── config ─────────────────────────────────────────────────
    write(
        &module_dir,
        "src/config/mod.rs",
        &format!(
            r#"//! config：模块配置（对齐 modules/{name}/config）
//!
//! 配置项读取 cool-config 的 `modules.{name}` 节。

/// 获取模块配置（按需扩展）
pub fn get(key: &str) -> Option<String> {{
    tracing::debug!(key, "mod-{name} config::get");
    None
}}
"#
        ),
    )?;

    // ── model ──────────────────────────────────────────────────
    write(
        &module_dir,
        "src/model/mod.rs",
        &format!(
            r#"//! model：数据表定义（autoMigrate）

pub mod {entity};
pub mod initjson;

pub use {entity}::*;

/// 全部表
pub fn all_tables() -> Vec<cool_db::TableDef> {{
    vec![{entity}()]
}}
"#
        ),
    )?;
    write(
        &module_dir,
        &format!("src/model/{entity}.rs"),
        &format!(
            r#"//! {entity} 表（由 cool-gen 生成，可按业务扩展字段）

use cool_db::{{ColumnMeta, TableDef}};

fn col(name: &str, sql_type: &str, not_null: bool, comment: &str) -> ColumnMeta {{
    ColumnMeta {{
        name: name.into(),
        sql_type: sql_type.into(),
        not_null,
        primary_key: false,
        comment: comment.into(),
        auto_increment: false,
        default: None,
    }}
}}

/// {entity} 表
pub fn {entity}() -> TableDef {{
    TableDef {{
        name: "{entity}".into(),
        columns: vec![
            ColumnMeta {{
                name: "id".into(),
                sql_type: "bigint".into(),
                not_null: true,
                primary_key: true,
                comment: "主键".into(),
                auto_increment: true,
                default: None,
            }},
            col("createTime", "datetime", true, "创建时间"),
            col("updateTime", "datetime", true, "更新时间"),
            col("deleteTime", "datetime", false, "删除时间"),
            col("name", "varchar(255)", true, "名称"),
            col("remark", "varchar(255)", false, "备注"),
        ],
    }}
}}
"#
        ),
    )?;
    write(
        &module_dir,
        "src/model/initjson.rs",
        &format!(
            r#"//! initjson 种子数据（放 JSON 到 ../initjson/，按需 include_str!）
//!
//! 通过 cool_db::fill_init_data 按 (group, table) 去重填充。

// 示例：
// pub const SEED: &str = include_str!("../../initjson/{entity}.json");
pub const _PLACEHOLDER: () = ();
"#
        ),
    )?;

    // ── service ────────────────────────────────────────────────
    write(
        &module_dir,
        "src/service/mod.rs",
        &format!(
            r#"//! service：业务服务

pub mod {entity};

pub use {entity}::*;
"#
        ),
    )?;
    write(
        &module_dir,
        &format!("src/service/{entity}.rs"),
        &format!(
            r#"//! {entity} 服务（对齐 cool-admin-go service 层）
//!
//! 通用 CRUD 走 CrudService；此处放业务扩展逻辑。

use cool_core::service::query_rows_on;
use cool_db::DbManager;
use serde_json::Value;

/// 示例：按名称查一条
pub async fn find_by_name(db: &DbManager, name: &str) -> Option<Value> {{
    let pool = db.default_pool().ok()?;
    let n = name.replace('\'', "''");
    let sql = format!(
        "SELECT * FROM `{entity}` WHERE name = '{{n}}' AND deleteTime IS NULL LIMIT 1"
    );
    query_rows_on(pool, &sql).await.ok()?.into_iter().next()
}}
"#
        ),
    )?;

    // ── controller ─────────────────────────────────────────────
    write(
        &module_dir,
        "src/controller/mod.rs",
        &format!(
            r#"//! controller：路由注册（对齐 modules/{name}/controller）

pub mod admin;
pub mod app;

use axum::Router;
use cool_core::CrudState;

/// 需登录路由
pub fn routes<S: CrudState>() -> Router<S> {{
    admin::routes::<S>().merge(app::routes::<S>())
}}
"#
        ),
    )?;
    write(
        &module_dir,
        "src/controller/admin/mod.rs",
        &format!(
            r#"//! controller/admin：后台管理控制器

pub mod {entity};

use axum::Router;
use cool_core::CrudState;

/// 需登录路由
pub fn routes<S: CrudState>() -> Router<S> {{
    {entity}::routes::<S>()
}}

/// 开放路由（免登录）
pub fn open_routes<S: CrudState>() -> Router<S> {{
    Router::new()
}}
"#
        ),
    )?;
    write(
        &module_dir,
        &format!("src/controller/admin/{entity}.rs"),
        &format!(
            r#"//! {entity} 控制器（对齐 cool-admin-go controller/admin）

use axum::Router;
use cool_core::controller::{{crud_router, ApiAllow}};
use cool_core::service::CrudService;
use cool_core::{{CrudState, QueryOp}};

/// CrudService 配置
fn service() -> CrudService {{
    CrudService::new("{entity}")
        .unique_key("name", "名称已存在")
        .not_null_key("name", "名称不能为空")
        .list_op(QueryOp::new().key_word_field(&["name", "remark"]))
        .page_op(
            QueryOp::new()
                .key_word_field(&["name", "remark"])
                .add_orderby("id", "desc"),
        )
}}

/// 注册 CRUD 路由（/admin/{name}/{entity} 与 /{name}/{entity}）
pub fn routes<S: CrudState>() -> Router<S> {{
    let svc = service();
    let mut r = Router::new();
    for prefix in ["/{name}/{entity}", "/admin/{name}/{entity}"] {{
        r = r.merge(crud_router(prefix, svc.clone(), ApiAllow::default()));
    }}
    r
}}
"#
        ),
    )?;
    write(
        &module_dir,
        "src/controller/app/mod.rs",
        &format!(
            r#"//! controller/app：移动端控制器（按需补充）

use axum::Router;
use cool_core::CrudState;

pub fn routes<S: CrudState>() -> Router<S> {{
    Router::new()
}}
"#
        ),
    )?;

    // ── middleware / funcs / utils ─────────────────────────────
    write(
        &module_dir,
        "src/middleware/mod.rs",
        &format!(
            r#"//! middleware：模块中间件（对齐 modules/{name}/middleware）
//!
//! 全局登录/权限/日志中间件在 mod-base，这里放模块特有中间件。
"#
        ),
    )?;
    write(
        &module_dir,
        "src/funcs/mod.rs",
        &format!(
            r#"//! funcs：任务函数注册（对齐 modules/{name}/funcs）
//!
//! 注册后可在 task_info.service 中引用：`FuncName(arg)`

/// 示例任务函数
pub async fn {name}_demo_func(param: &str) -> Result<(), String> {{
    tracing::info!(param, "mod-{name} demo func");
    Ok(())
}}
"#
        ),
    )?;
    write(
        &module_dir,
        "src/utils/mod.rs",
        &format!(
            r#"//! utils：工具函数

/// SQL 值转义
pub fn escape_sql(s: &str) -> String {{
    s.replace('\'', "''")
}}
"#
        ),
    )?;

    println!("创建模块成功: {}", module_dir.display());
    println!();
    println!("目录结构：");
    println!("  modules/{name}/");
    println!("  ├── Cargo.toml");
    println!("  ├── initjson/");
    println!("  └── src/");
    println!("      ├── api/{{mod,v1}}.rs");
    println!("      ├── config/mod.rs");
    println!("      ├── controller/{{mod.rs, admin/{entity}.rs, app/mod.rs}}");
    println!("      ├── funcs/mod.rs");
    println!("      ├── middleware/mod.rs");
    println!("      ├── model/{{mod.rs, {entity}.rs, initjson.rs}}");
    println!("      ├── service/{{mod.rs, {entity}.rs}}");
    println!("      └── utils/mod.rs");
    println!();
    println!("后续步骤：");
    println!("  1. 将 `{crate_name}` 加入 workspace Cargo.toml [members] 与 [dependencies]");
    println!("  2. 挂载路由：mod_{name}::routes()（protected）/ mod_{name}::open_routes()（public）");
    println!("  3. autoMigrate 追加 mod_{name}::all_tables()");
    println!("  4. 用 `cool-gen crud <table> --module {name}` 从数据库表生成真实 CRUD");
    Ok(())
}

fn write(dir: &Path, rel: &str, content: &str) -> anyhow::Result<()> {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content).with_context(|| format!("写入 {}", path.display()))?;
    Ok(())
}

fn to_pascal(s: &str) -> String {
    s.split(['_', '-'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        bail!("模块名不能为空");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        bail!("模块名仅允许小写字母/数字/下划线: {name}");
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        bail!("模块名不能以数字开头: {name}");
    }
    Ok(())
}

/// 向上查找含 Cargo.toml 的 workspace 根
pub fn find_workspace_root() -> anyhow::Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let cargo = dir.join("Cargo.toml");
        if cargo.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo) {
                if content.contains("[workspace]") {
                    return Ok(dir);
                }
            }
        }
        if !dir.pop() {
            bail!("未找到 workspace 根目录（Cargo.toml 含 [workspace]），请在 cool-axum 下执行");
        }
    }
}

#[allow(dead_code)]
fn ensure_dir(p: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(p)?;
    Ok(())
}
