//! mod-space：对齐 cool-admin-go modules/space（文件空间）

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

/// space_info
pub fn space_info() -> TableDef {
    let mut cols = base_cols();
    cols.push(col("name", "varchar(255)", false, "文件名"));
    cols.push(col("url", "varchar(255)", true, "地址"));
    cols.push(col("type", "varchar(255)", true, "类型"));
    cols.push(col("size", "bigint", false, "大小"));
    cols.push(col("classifyId", "bigint", false, "分类ID"));
    cols.push(col("userId", "bigint", false, "用户ID"));
    TableDef {
        name: "space_info".into(),
        columns: cols,
    }
}

/// space_type
pub fn space_type() -> TableDef {
    let mut cols = base_cols();
    cols.push(col("name", "varchar(255)", true, "类别名称"));
    cols.push(col("parentId", "int", false, "父分类ID"));
    TableDef {
        name: "space_type".into(),
        columns: cols,
    }
}

pub fn all_tables() -> Vec<TableDef> {
    vec![space_info(), space_type()]
}

/// 注册 space 模块路由
pub fn routes<S: cool_core::CrudState>() -> Router<S> {
    let info = CrudService::new("space_info")
        .not_null_key("url", "地址不能为空")
        .list_op(
            QueryOp::new()
                .field_eq(&["type", "classifyId", "userId"])
                .key_word_field(&["name", "url"]),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["type", "classifyId", "userId"])
                .key_word_field(&["name", "url"])
                .add_orderby("id", "desc"),
        );

    let ty = CrudService::new("space_type")
        .not_null_key("name", "类别名称不能为空")
        .list_op(
            QueryOp::new()
                .field_eq(&["parentId"])
                .key_word_field(&["name"]),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["parentId"])
                .key_word_field(&["name"]),
        );

    let mut r = Router::new();
    for prefix in ["/space/info", "/admin/space/info"] {
        r = r.merge(crud_router(prefix, info.clone(), ApiAllow::default()));
    }
    for prefix in ["/space/type", "/admin/space/type"] {
        r = r.merge(crud_router(prefix, ty.clone(), ApiAllow::default()));
    }
    r
}

/// 模块注册声明：业务表
pub fn spec() -> cool_core::ModuleSpec {
    cool_core::ModuleSpec::new("space").tables(all_tables())
}

/// 模块初始化（对齐 space/space.go init()）
pub async fn init(_db: &cool_core::db::DbManager, _cache: cool_core::cache::CacheManager) {
    tracing::debug!("module space init start ...");
    tracing::debug!("module space init finished ...");
}

/// 路由分组
pub fn route_groups<S>() -> cool_core::RouteGroups<S>
where
    S: cool_core::CrudState + Clone + Send + Sync + 'static,
{
    cool_core::RouteGroups::new().biz(routes::<S>())
}
