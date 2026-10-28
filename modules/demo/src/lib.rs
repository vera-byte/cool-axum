//! mod-demo：对齐 cool-admin-go modules/demo（示例模块）

use axum::Router;
use cool_core::QueryOp;
use cool_core::controller::{ApiAllow, crud_router};
use cool_core::db::{ColumnMeta, TableDef};
use cool_core::service::CrudService;

fn col(name: &str, sql_type: &str, not_null: bool, comment: &str) -> ColumnMeta {
    ColumnMeta {
        name: name.into(),
        sql_type: sql_type.into(),
        not_null,
        primary_key: false,
        comment: comment.into(),
        auto_increment: false,
        default: None,
    }
}

fn base_cols() -> Vec<ColumnMeta> {
    vec![
        ColumnMeta {
            name: "id".into(),
            sql_type: "bigint".into(),
            not_null: true,
            primary_key: true,
            comment: "主键".into(),
            auto_increment: true,
            default: None,
        },
        col("createTime", "datetime", true, "创建时间"),
        col("updateTime", "datetime", true, "更新时间"),
        col("deleteTime", "datetime", false, "删除时间"),
    ]
}

/// demo_sample
pub fn demo_sample() -> TableDef {
    let mut cols = base_cols();
    cols.push(col("name", "varchar(255)", false, "名称"));
    cols.push(col("code", "varchar(255)", false, "编码"));
    cols.push(col("remark", "varchar(255)", false, "备注"));
    TableDef {
        name: "demo_sample".into(),
        columns: cols,
    }
}

/// demo_goods
pub fn demo_goods() -> TableDef {
    let mut cols = base_cols();
    cols.push(col("name", "varchar(255)", false, "名称"));
    cols.push(col("price", "decimal(10,2)", false, "价格"));
    cols.push(col("typeId", "int", false, "分类ID"));
    TableDef {
        name: "demo_goods".into(),
        columns: cols,
    }
}

pub fn all_tables() -> Vec<TableDef> {
    vec![demo_sample(), demo_goods()]
}

/// 注册 demo 模块路由
pub fn routes<S: cool_core::CrudState>() -> Router<S> {
    let sample = CrudService::new("demo_sample")
        .list_op(
            QueryOp::new()
                .field_eq(&["name", "code"])
                .key_word_field(&["name", "code"]),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["name", "code"])
                .key_word_field(&["name", "code"])
                .add_orderby("id", "desc"),
        );

    let goods = CrudService::new("demo_goods")
        .list_op(
            QueryOp::new()
                .field_eq(&["typeId"])
                .key_word_field(&["name"]),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["typeId"])
                .key_word_field(&["name"])
                .add_orderby("id", "desc"),
        );

    let mut r = Router::new();
    for (prefix, svc) in [
        ("/demo/sample", sample.clone()),
        ("/demo/goods", goods.clone()),
        ("/admin/demo/sample", sample.clone()),
        ("/admin/demo/goods", goods),
        ("/admin/demo", sample),
    ] {
        r = r.merge(crud_router(prefix, svc, ApiAllow::default()));
    }
    r
}

/// 模块注册声明：业务表
pub fn spec() -> cool_core::ModuleSpec {
    cool_core::ModuleSpec::new("demo").tables(all_tables())
}

/// 模块初始化（对齐 demo/demo.go init()）
pub async fn init(_db: &cool_core::db::DbManager, _cache: cool_core::cache::CacheManager) {
    tracing::debug!("module demo init start ...");
    tracing::debug!("module demo init finished ...");
}

/// 路由分组
pub fn route_groups<S>() -> cool_core::RouteGroups<S>
where
    S: cool_core::CrudState + Clone + Send + Sync + 'static,
{
    cool_core::RouteGroups::new().biz(routes::<S>())
}
