//! recycle_data 表（对齐 cool-admin-midway RecycleDataEntity）

pub mod initjson;

use cool_core::db::{ColumnMeta, TableDef};

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

fn col_def(name: &str, sql_type: &str, not_null: bool, comment: &str, def: &str) -> ColumnMeta {
    let mut c = col(name, sql_type, not_null, comment);
    c.default = Some(def.into());
    c
}

/// recycle_data：软删除数据回收站
pub fn recycle_data() -> TableDef {
    TableDef {
        name: "recycle_data".into(),
        columns: vec![
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
            col("tenantId", "bigint", false, "租户ID"),
            // 对齐 midway entityInfo: {dataSourceName, entity} —— 此处为 {table}
            col("entityInfo", "text", false, "表信息 JSON"),
            col("userId", "bigint", false, "操作人"),
            col("data", "text", false, "被删除的数据 JSON 数组"),
            col("url", "varchar(255)", false, "请求的接口"),
            col("params", "text", false, "请求参数 JSON"),
            col_def("count", "int", true, "删除数据条数", "1"),
        ],
    }
}

pub fn all_tables() -> Vec<TableDef> {
    vec![recycle_data()]
}
