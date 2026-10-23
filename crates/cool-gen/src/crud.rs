//! 从数据库表生成 CRUD 模块代码
//!
//! 输入：表名 + 可选目标模块
//! 输出：
//! - `crates/modules/<module>/src/tables_gen.rs`（或覆盖 `tables.rs`）
//! - `crates/modules/<module>/src/lib_gen.rs` 片段（路由配置建议）
//!
//! 生成的代码风格对齐手写模块（mod-demo / mod-dict）。

use crate::schema::{describe_table, ColumnInfo};
use anyhow::Context;
use cool_db::DbManager;
use std::fmt::Write as _;

pub async fn generate_crud(db: &DbManager, table: &str, module: &str) -> anyhow::Result<()> {
    let columns = describe_table(db, table).await?;
    let module_name = if module.is_empty() {
        format!("crate-{}", table.replace('_', "-"))
    } else {
        module.to_string()
    };

    let tables_rs = render_tables(table, &columns);
    let lib_rs = render_lib(table, &module_name, &columns);

    // 输出目录
    let root = crate::module::find_workspace_root()?;
    let out_dir = root.join("target/gen").join(table);
    std::fs::create_dir_all(&out_dir).context("创建输出目录失败")?;

    let tables_path = out_dir.join("tables.rs");
    let lib_path = out_dir.join("lib.rs");
    std::fs::write(&tables_path, tables_rs)?;
    std::fs::write(&lib_path, lib_rs)?;

    println!("代码生成完成：");
    println!("  表结构:  {}", tables_path.display());
    println!("  路由代码: {}", lib_path.display());
    println!();
    println!("集成步骤：");
    println!("  1. 将 tables.rs 内容合并到 crates/modules/{module_name}/src/tables.rs");
    println!("  2. 将 lib.rs 中的 routes 片段合并到模块 lib.rs");
    println!("  3. 若模块不存在，先执行: cargo run -p cool-gen -- module {module_name}");
    Ok(())
}

/// 生成 tables.rs（TableDef）
fn render_tables(table: &str, columns: &[ColumnInfo]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "//! `{table}` 表结构（由 cool-gen 自动生成）\n"
    );
    out.push_str("use cool_db::{ColumnMeta, TableDef};\n\n");

    // 判断是否有基础列
    let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    let has_id = names.contains(&"id");
    let has_create = names.contains(&"createTime") || names.contains(&"create_time");
    let has_update = names.contains(&"updateTime") || names.contains(&"update_time");
    let has_delete = names.contains(&"deleteTime")
        || names.contains(&"delete_time")
        || names.contains(&"deleted_at")
        || names.contains(&"deletedAt");

    out.push_str("fn col(name: &str, sql_type: &str, not_null: bool, comment: &str) -> ColumnMeta {\n");
    out.push_str("    ColumnMeta {\n");
    out.push_str("        name: name.into(),\n");
    out.push_str("        sql_type: sql_type.into(),\n");
    out.push_str("        not_null,\n");
    out.push_str("        primary_key: false,\n");
    out.push_str("        comment: comment.into(),\n");
    out.push_str("        auto_increment: false,\n");
    out.push_str("        default: None,\n");
    out.push_str("    }\n}\n\n");

    out.push_str("fn col_def(name: &str, sql_type: &str, not_null: bool, comment: &str, default: &str) -> ColumnMeta {\n");
    out.push_str("    ColumnMeta {\n");
    out.push_str("        name: name.into(),\n");
    out.push_str("        sql_type: sql_type.into(),\n");
    out.push_str("        not_null,\n");
    out.push_str("        primary_key: false,\n");
    out.push_str("        comment: comment.into(),\n");
    out.push_str("        auto_increment: false,\n");
    out.push_str("        default: Some(default.into()),\n");
    out.push_str("    }\n}\n\n");

    let _ = writeln!(out, "/// {table}");
    let _ = writeln!(out, "pub fn {table}() -> TableDef {{");
    out.push_str("    let mut cols = Vec::new();\n");

    if !has_id {
        out.push_str(
            "    cols.push(ColumnMeta {\n        name: \"id\".into(), sql_type: \"bigint\".into(),\n        not_null: true, primary_key: true,\n        comment: \"主键\".into(), auto_increment: true, default: None,\n    });\n",
        );
    }
    if !has_create {
        out.push_str("    cols.push(col(\"createTime\", \"datetime\", true, \"创建时间\"));\n");
    }
    if !has_update {
        out.push_str("    cols.push(col(\"updateTime\", \"datetime\", true, \"更新时间\"));\n");
    }
    if !has_delete {
        out.push_str("    cols.push(col(\"deleteTime\", \"datetime\", false, \"删除时间\"));\n");
    }

    for c in columns {
        let name = normalize_col_name(&c.name);
        let sql_type = map_sql_type(&c.column_type);
        let not_null = !c.is_nullable && !is_primary(c);
        let comment = c.column_comment.replace('\"', "'");

        if is_primary(c) {
            let auto = c.extra.to_lowercase().contains("auto_increment");
            let _ = writeln!(
                out,
                "    cols.push(ColumnMeta {{ name: \"{name}\".into(), sql_type: \"{sql_type}\".into(), not_null: true, primary_key: true, comment: \"{comment}\".into(), auto_increment: {auto}, default: None }});"
            );
            continue;
        }

        // id 已在上面按主键输出
        if c.name == "id" {
            continue;
        }

        match &c.column_default {
            Some(d) if !d.is_empty() && d != "NULL" => {
                let d_esc = d.replace('\'', "\\'");
                let _ = writeln!(
                    out,
                    "    cols.push(col_def(\"{name}\", \"{sql_type}\", {not_null}, \"{comment}\", \"{d_esc}\"));"
                );
            }
            _ => {
                let _ = writeln!(
                    out,
                    "    cols.push(col(\"{name}\", \"{sql_type}\", {not_null}, \"{comment}\"));"
                );
            }
        }
    }

    out.push_str("    TableDef {\n");
    let _ = writeln!(out, "        name: \"{table}\".into(),");
    out.push_str("        columns: cols,\n    }\n}\n");
    out
}

/// 生成 lib.rs 路由片段
fn render_lib(table: &str, module: &str, columns: &[ColumnInfo]) -> String {
    // 猜测可过滤字段（短文本/枚举列）
    let eq_fields: Vec<String> = columns
        .iter()
        .filter(|c| {
            let t = c.column_type.to_lowercase();
            (t.contains("int") || t.starts_with("tinyint") || t.starts_with("varchar"))
                && !matches!(c.name.as_str(), "password" | "createTime" | "updateTime" | "deleteTime")
        })
        .map(|c| format!("\"{}\"", normalize_col_name(&c.name)))
        .take(4)
        .collect();
    let kw_fields: Vec<String> = columns
        .iter()
        .filter(|c| {
            let t = c.column_type.to_lowercase();
            (t.contains("char") || t.contains("text")) && c.name != "password"
        })
        .map(|c| format!("\"{}\"", normalize_col_name(&c.name)))
        .take(4)
        .collect();

    let mut out = String::new();
    let _ = writeln!(out, "//! `{table}` CRUD 路由片段（由 cool-gen 自动生成）\n");
    out.push_str("use cool_core::controller::{crud_router, ApiAllow};\n");
    out.push_str("use cool_core::service::CrudService;\n");
    out.push_str("use cool_core::QueryOp;\n");
    out.push_str("use axum::Router;\n\n");

    let _ = writeln!(out, "/// 注册 {table} 路由");
    let _ = writeln!(out, "pub fn routes_{table}() -> Router<S> where S: cool_core::CrudState {{");
    let _ = writeln!(out, "    let svc = CrudService::new(\"{table}\")");
    if !eq_fields.is_empty() {
        let _ = writeln!(
            out,
            "        .list_op(QueryOp::new().field_eq(&[{}]){})",
            eq_fields.join(", "),
            if kw_fields.is_empty() {
                String::new()
            } else {
                format!(".key_word_field(&[{}])", kw_fields.join(", "))
            }
        );
        let _ = writeln!(
            out,
            "        .page_op(QueryOp::new().field_eq(&[{}]){}.add_orderby(\"id\", \"desc\"));",
            eq_fields.join(", "),
            if kw_fields.is_empty() {
                String::new()
            } else {
                format!(".key_word_field(&[{}])", kw_fields.join(", "))
            }
        );
    } else {
        out.push_str("        .list_op(QueryOp::new())\n");
        out.push_str("        .page_op(QueryOp::new().add_orderby(\"id\", \"desc\"));\n");
    }
    let _ = writeln!(out, "    crud_router(\"/{module}/{table}\", svc, ApiAllow::default())");
    out.push_str("}\n");
    out
}

/// 列名规范化：gorm 风格 → 保留原名（表结构已按 cool 命名）
fn normalize_col_name(name: &str) -> String {
    // deleted_at → deleteTime（对齐 cool 配置）
    match name {
        "deleted_at" | "deletedAt" => "deleteTime".into(),
        "created_at" => "createTime".into(),
        "updated_at" => "updateTime".into(),
        other => other.to_string(),
    }
}

fn is_primary(c: &ColumnInfo) -> bool {
    c.column_key.eq_ignore_ascii_case("pri") || c.extra.to_lowercase().contains("auto_increment")
}

fn map_sql_type(column_type: &str) -> String {
    let t = column_type.to_lowercase();
    if t.starts_with("tinyint(1)") {
        "tinyint".into()
    } else {
        column_type.to_string()
    }
}
