//! QueryOp：对齐 cool-admin-go 的 ListQueryOp / PageQueryOp

/// JOIN 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Left,
    Right,
    Inner,
}

/// 关联查询配置
#[derive(Debug, Clone)]
pub struct JoinOp {
    /// 关联表名
    pub table: String,
    /// 别名
    pub alias: String,
    /// 关联条件，如 "a.userId = b.id"
    pub condition: String,
    pub join_type: JoinType,
}

/// List / Page 查询配置
#[derive(Debug, Clone, Default)]
pub struct QueryOp {
    /// 字段等于匹配：请求参数字段名列表，非空时自动 eq 条件
    pub field_eq: Vec<String>,
    /// keyWord 模糊搜索的数据库字段
    pub key_word_field: Vec<String>,
    /// 默认排序 field -> asc|desc（请求未指定 order/sort 时生效）
    pub add_orderby: Vec<(String, String)>,
    /// 自定义 WHERE 片段
    pub where_clauses: Vec<String>,
    /// 查询字段列表（SELECT 列），逗号分隔
    pub select: String,
    /// 关联查询
    pub joins: Vec<JoinOp>,
    /// 主表别名（联表防歧义）
    pub from_alias: String,
}

impl QueryOp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field_eq(mut self, fields: &[&str]) -> Self {
        self.field_eq = fields.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn key_word_field(mut self, fields: &[&str]) -> Self {
        self.key_word_field = fields.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn add_orderby(mut self, field: &str, dir: &str) -> Self {
        self.add_orderby.push((field.to_string(), dir.to_string()));
        self
    }

    pub fn select(mut self, cols: &str) -> Self {
        self.select = cols.to_string();
        self
    }

    pub fn where_sql(mut self, clause: &str) -> Self {
        self.where_clauses.push(clause.to_string());
        self
    }

    pub fn as_alias(mut self, alias: &str) -> Self {
        self.from_alias = alias.to_string();
        self
    }

    pub fn left_join(mut self, table: &str, alias: &str, condition: &str) -> Self {
        self.joins.push(JoinOp {
            table: table.into(),
            alias: alias.into(),
            condition: condition.into(),
            join_type: JoinType::Left,
        });
        self
    }
}

/// 分页请求
#[derive(Debug, Clone)]
pub struct PageReq {
    pub page: i64,
    pub size: i64,
    pub order: Option<String>,
    pub sort: Option<String>,
    pub is_export: bool,
    pub max_export_limit: i64,
}

impl Default for PageReq {
    fn default() -> Self {
        Self {
            page: 1,
            size: 15,
            order: None,
            sort: None,
            is_export: false,
            max_export_limit: 0,
        }
    }
}

/// 列表请求
#[derive(Debug, Clone, Default)]
pub struct ListReq {
    pub order: Option<String>,
    pub sort: Option<String>,
}
