//! cool-gen：代码生成器 CLI
//!
//! 对齐 cool-admin-go cool-tools：
//! - `module <name>`：脚手架新模块（基于 demo 模板）
//! - `crud <table> --module <name>`：从数据库表结构生成 CRUD 模块
//! - `tables`：列出数据库全部表
//!
//! 用法：
//! ```bash
//! cargo run -p cool-gen -- tables
//! cargo run -p cool-gen -- module report
//! cargo run -p cool-gen -- crud demo_goods --module demo
//! ```

mod crud;
mod module;
mod schema;

use anyhow::{Context, bail};
use cool_core::config::AppConfig;
use cool_core::db::DbManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    match args[0].as_str() {
        "tables" => {
            let db = connect().await?;
            schema::list_tables(&db).await?;
        }
        "module" => {
            let name = args.get(1).context("用法: cool-gen module <moduleName>")?;
            module::create_module(name)?;
        }
        "crud" => {
            let table = args
                .get(1)
                .context("用法: cool-gen crud <table> [--module <name>]")?;
            let mut module = String::new();
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--module" | "-m" => {
                        module = args.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    other => bail!("未知参数: {other}"),
                }
            }
            let db = connect().await?;
            crud::generate_crud(&db, table, &module).await?;
        }
        "help" | "-h" | "--help" => print_usage(),
        other => bail!("未知命令: {other}\n"),
    }
    Ok(())
}

fn print_usage() {
    println!(
        r#"cool-gen —— cool-axum 代码生成器

命令:
  tables                         列出数据库全部表
  module <moduleName>            创建模块脚手架（基于 demo 模板）
  crud <table> [options]         从数据库表生成 CRUD 模块代码

crud 选项:
  -m, --module <name>            生成到指定模块（默认 crate-<table>）

示例:
  cargo run -p cool-gen -- tables
  cargo run -p cool-gen -- module report
  cargo run -p cool-gen -- crud demo_goods --module demo
"#
    );
}

async fn connect() -> anyhow::Result<DbManager> {
    let path = std::env::var("COOL_CONFIG").unwrap_or_else(|_| "config.yaml".into());
    let config = AppConfig::load(&path).with_context(|| format!("加载配置失败: {path}"))?;
    let db = DbManager::init(&config.database).await?;
    if db.pool_count() == 0 {
        bail!("数据库未连接，请检查 config.yaml");
    }
    Ok(db)
}
