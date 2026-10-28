//! Model 基础定义，对齐 cool-admin-go 的 cool.Model
//!
//! 基础字段：id / createTime / updateTime / deleteTime（软删除）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 所有业务模型的公共元信息
pub trait ModelMeta {
    /// 表名
    fn table_name() -> &'static str;
    /// 数据源分组，默认 default
    fn group_name() -> &'static str {
        "default"
    }
}

/// 基础模型字段（对齐 cool-admin-go cool.Model）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Model {
    pub id: u64,
    /// 创建时间
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
    /// 软删除时间（null 表示未删除）
    pub delete_time: Option<DateTime<Utc>>,
}

impl Model {
    pub fn new() -> Self {
        Self::default()
    }
}

/// 基础字段 DDL 片段（供 autoMigrate 使用）
pub fn base_columns() -> Vec<crate::db::ColumnMeta> {
    vec![
        crate::db::ColumnMeta {
            name: "id".into(),
            sql_type: "bigint".into(),
            not_null: true,
            primary_key: true,
            comment: "主键".into(),
            auto_increment: true,
            default: None,
        },
        crate::db::ColumnMeta {
            name: "createTime".into(),
            sql_type: "datetime".into(),
            not_null: true,
            primary_key: false,
            comment: "创建时间".into(),
            auto_increment: false,
            default: None,
        },
        crate::db::ColumnMeta {
            name: "updateTime".into(),
            sql_type: "datetime".into(),
            not_null: true,
            primary_key: false,
            comment: "更新时间".into(),
            auto_increment: false,
            default: None,
        },
        crate::db::ColumnMeta {
            name: "deleteTime".into(),
            sql_type: "datetime".into(),
            not_null: false,
            primary_key: false,
            comment: "删除时间".into(),
            auto_increment: false,
            default: None,
        },
    ]
}
