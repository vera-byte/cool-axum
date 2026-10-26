//! dict_type（对齐 model/dict_type.go）

use cool_db::{ColumnMeta, TableDef};

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

/// dict_type 表
pub fn dict_type() -> TableDef {
    TableDef {
        name: "dict_type".into(),
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
            col("name", "varchar(255)", true, "名称"),
            col("key", "varchar(255)", true, "标识"),
        ],
    }
}
