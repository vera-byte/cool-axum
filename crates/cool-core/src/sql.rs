//! 轻量 SQL 构建器：支持 WHERE / LIKE / IN / JOIN / ORDER / LIMIT，
//! 自动注入软删除条件（deleteTime IS NULL），字段名白名单防注入。

use std::fmt::Write as _;

/// 字段名校验：仅允许 字母/数字/下划线/点
pub fn is_safe_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// 排序字段白名单（对齐 cool-admin-go safeOrderBy）
pub fn safe_order_by(order: &str, sort: &str) -> Option<String> {
    let order = order.replace('`', "");
    let order = order.trim();
    let sort = sort.trim().to_lowercase();
    if order.is_empty() || (sort != "asc" && sort != "desc") {
        return None;
    }
    if !order
        .split(',')
        .all(|p| !p.trim().is_empty() && is_safe_ident(p.trim()))
    {
        return None;
    }
    Some(format!("{order} {sort}"))
}

/// 列名安全引用：MySQL 反引号 / 通用双引号由调用方言决定；此处默认反引号
pub fn quote_ident(col: &str) -> String {
    if is_safe_ident(col) {
        format!("`{col}`")
    } else {
        col.to_string()
    }
}

/// SQL 构建器（SELECT / DELETE / UPDATE 辅助）
#[derive(Debug, Default, Clone)]
pub struct SqlBuilder {
    select: Vec<String>,
    from: String,
    from_alias: Option<String>,
    joins: Vec<String>,
    wheres: Vec<String>,
    order_by: Vec<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    /// 软删除列名，Some 时自动注入 IS NULL
    soft_delete_col: Option<String>,
}

/// 绑定参数值（当前用字面量转义简化，后续可换 prepare 绑定）
#[derive(Debug, Clone)]
pub enum ParamValue {
    Null,
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Json(serde_json::Value),
}

impl From<serde_json::Value> for ParamValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => ParamValue::Null,
            serde_json::Value::Bool(b) => ParamValue::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ParamValue::Int(i)
                } else {
                    ParamValue::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => ParamValue::Str(s),
            other => ParamValue::Json(other),
        }
    }
}

impl From<i64> for ParamValue {
    fn from(v: i64) -> Self {
        ParamValue::Int(v)
    }
}

impl From<i32> for ParamValue {
    fn from(v: i32) -> Self {
        ParamValue::Int(v as i64)
    }
}

impl From<String> for ParamValue {
    fn from(v: String) -> Self {
        ParamValue::Str(v)
    }
}

impl From<&str> for ParamValue {
    fn from(v: &str) -> Self {
        ParamValue::Str(v.to_string())
    }
}

impl From<bool> for ParamValue {
    fn from(v: bool) -> Self {
        ParamValue::Bool(v)
    }
}

impl ParamValue {
    /// 渲染为 SQL 字面量（单引号转义）
    pub fn to_sql_literal(&self) -> String {
        match self {
            ParamValue::Null => "NULL".into(),
            ParamValue::Bool(b) => {
                if *b {
                    "TRUE".into()
                } else {
                    "FALSE".into()
                }
            }
            ParamValue::Int(i) => i.to_string(),
            ParamValue::Float(f) => f.to_string(),
            ParamValue::Str(s) => format!("'{}'", s.replace('\'', "''")),
            ParamValue::Json(v) => match v {
                serde_json::Value::String(s) => {
                    format!("'{}'", s.replace('\'', "''"))
                }
                other => format!("'{}'", other.to_string().replace('\'', "''")),
            },
        }
    }
}

impl SqlBuilder {
    pub fn new(from_table: &str) -> Self {
        Self {
            from: from_table.to_string(),
            soft_delete_col: Some("deleteTime".into()),
            ..Default::default()
        }
    }

    pub fn select_cols(mut self, cols: &str) -> Self {
        if !cols.trim().is_empty() {
            self.select = cols
                .split(',')
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect();
        }
        self
    }

    pub fn soft_delete_col(mut self, col: Option<&str>) -> Self {
        self.soft_delete_col = col.map(|s| s.to_string());
        self
    }

    /// col = value
    pub fn eq(mut self, col: &str, val: impl Into<ParamValue>) -> Self {
        if is_safe_ident(col) {
            self.wheres.push(format!(
                "{} = {}",
                quote_ident(col),
                val.into().to_sql_literal()
            ));
        }
        self
    }

    /// col LIKE '%val%'
    pub fn like_any(mut self, col: &str, val: &str) -> Self {
        if is_safe_ident(col) {
            let v = ParamValue::Str(format!("%{}%", val.replace('%', "\\%")));
            self.wheres
                .push(format!("{} LIKE {}", quote_ident(col), v.to_sql_literal()));
        }
        self
    }

    /// 多字段 OR LIKE（keyWord 搜索）
    pub fn or_like_any(mut self, cols: &[String], val: &str) -> Self {
        let mut parts = Vec::new();
        for col in cols {
            if is_safe_ident(col) {
                let v = ParamValue::Str(format!("%{}%", val.replace('%', "\\%")));
                parts.push(format!("{} LIKE {}", quote_ident(col), v.to_sql_literal()));
            }
        }
        if !parts.is_empty() {
            self.wheres.push(format!("({})", parts.join(" OR ")));
        }
        self
    }

    /// col IN (...)
    pub fn where_in(mut self, col: &str, vals: &[serde_json::Value]) -> Self {
        if !is_safe_ident(col) || vals.is_empty() {
            return self;
        }
        let items: Vec<String> = vals
            .iter()
            .map(|v| ParamValue::from(v.clone()).to_sql_literal())
            .collect();
        self.wheres
            .push(format!("{} IN ({})", quote_ident(col), items.join(",")));
        self
    }

    /// col <> value
    pub fn neq(mut self, col: &str, val: impl Into<ParamValue>) -> Self {
        if is_safe_ident(col) {
            self.wheres.push(format!(
                "{} <> {}",
                quote_ident(col),
                val.into().to_sql_literal()
            ));
        }
        self
    }

    /// 自定义 WHERE 片段（调用方保证安全）
    pub fn raw_where(mut self, clause: &str) -> Self {
        if !clause.trim().is_empty() {
            self.wheres.push(clause.to_string());
        }
        self
    }

    /// 主表别名（联表时避免列名歧义）
    pub fn as_alias(mut self, alias: &str) -> Self {
        self.from_alias = Some(alias.to_string());
        self
    }

    /// LEFT JOIN table AS alias ON condition
    pub fn left_join(mut self, table: &str, alias: &str, condition: &str) -> Self {
        self.joins
            .push(format!("LEFT JOIN {table} AS {alias} ON {condition}"));
        self
    }
    pub fn right_join(mut self, table: &str, alias: &str, condition: &str) -> Self {
        self.joins
            .push(format!("RIGHT JOIN {table} AS {alias} ON {condition}"));
        self
    }
    pub fn inner_join(mut self, table: &str, alias: &str, condition: &str) -> Self {
        self.joins
            .push(format!("INNER JOIN {table} AS {alias} ON {condition}"));
        self
    }

    /// 追加排序（field asc|desc，白名单校验）
    pub fn order(mut self, field: &str, direction: &str) -> Self {
        if let Some(o) = safe_order_by(field, direction) {
            self.order_by.push(o);
        }
        self
    }

    pub fn limit(mut self, n: i64) -> Self {
        self.limit = Some(n);
        self
    }
    pub fn offset(mut self, n: i64) -> Self {
        self.offset = Some(n);
        self
    }

    fn where_clause(&self) -> String {
        let mut parts = self.wheres.clone();
        if let Some(col) = &self.soft_delete_col
            && is_safe_ident(col)
        {
            let qualified = match &self.from_alias {
                Some(a) if is_safe_ident(a) => format!("{a}.{col}"),
                _ => col.clone(),
            };
            parts.push(format!("{qualified} IS NULL"));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", parts.join(" AND "))
        }
    }

    fn render_from(&self) -> String {
        match &self.from_alias {
            Some(a) if is_safe_ident(a) => format!("{} AS {}", self.from, a),
            _ => self.from.clone(),
        }
    }

    /// SELECT 语句
    pub fn build_select(&self) -> String {
        let cols = if self.select.is_empty() {
            "*".to_string()
        } else {
            self.select.join(", ")
        };
        let from = self.render_from();
        let mut sql = format!("SELECT {cols} FROM {from}");
        for j in &self.joins {
            let _ = write!(sql, " {j}");
        }
        sql.push_str(&self.where_clause());
        if !self.order_by.is_empty() {
            let _ = write!(sql, " ORDER BY {}", self.order_by.join(", "));
        }
        if let Some(l) = self.limit {
            let _ = write!(sql, " LIMIT {l}");
        }
        if let Some(o) = self.offset {
            let _ = write!(sql, " OFFSET {o}");
        }
        sql
    }

    /// SELECT COUNT(*) 语句
    pub fn build_count(&self) -> String {
        let from = self.render_from();
        let mut sql = format!("SELECT COUNT(*) AS count FROM {from}");
        for j in &self.joins {
            let _ = write!(sql, " {j}");
        }
        sql.push_str(&self.where_clause());
        sql
    }

    /// INSERT 语句，data 为列名->值
    pub fn build_insert(table: &str, data: &serde_json::Map<String, serde_json::Value>) -> String {
        let mut cols = Vec::new();
        let mut vals = Vec::new();
        for (k, v) in data {
            if !is_safe_ident(k) {
                continue;
            }
            cols.push(quote_ident(k));
            vals.push(ParamValue::from(v.clone()).to_sql_literal());
        }
        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quote_ident(table),
            cols.join(", "),
            vals.join(", ")
        )
    }

    /// UPDATE 语句（自动带软删除条件，不更新 deleteTime）
    pub fn build_update(
        table: &str,
        data: &serde_json::Map<String, serde_json::Value>,
        id: i64,
        soft_delete_col: Option<&str>,
        tenant_id: Option<i64>,
    ) -> String {
        let mut sets = Vec::new();
        for (k, v) in data {
            if !is_safe_ident(k) || k == "id" || k == "deleteTime" {
                continue;
            }
            sets.push(format!(
                "{} = {}",
                quote_ident(k),
                ParamValue::from(v.clone()).to_sql_literal()
            ));
        }
        let mut sql = format!(
            "UPDATE {} SET {} WHERE `id` = {id}",
            quote_ident(table),
            sets.join(", ")
        );
        if let Some(col) = soft_delete_col
            && is_safe_ident(col)
        {
            let _ = write!(sql, " AND {} IS NULL", quote_ident(col));
        }
        if let Some(t) = tenant_id {
            let _ = write!(sql, " AND `tenantId` = {t}");
        }
        sql
    }

    /// 软删除：UPDATE deleteTime = now
    pub fn build_soft_delete(
        table: &str,
        ids: &[i64],
        soft_delete_col: &str,
        tenant_id: Option<i64>,
    ) -> Option<String> {
        if ids.is_empty() || !is_safe_ident(soft_delete_col) {
            return None;
        }
        let list: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
        let mut sql = format!(
            "UPDATE {} SET {} = CURRENT_TIMESTAMP WHERE `id` IN ({}) AND {} IS NULL",
            quote_ident(table),
            quote_ident(soft_delete_col),
            list.join(","),
            quote_ident(soft_delete_col)
        );
        if let Some(t) = tenant_id {
            let _ = write!(sql, " AND `tenantId` = {t}");
        }
        Some(sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_whitelist() {
        assert!(safe_order_by("id,name", "asc").is_some());
        assert!(safe_order_by("id", "DESC").is_some());
        assert!(safe_order_by("id;drop", "asc").is_none());
        assert!(safe_order_by("id", "asc;drop").is_none());
        assert!(safe_order_by("", "asc").is_none());
    }

    #[test]
    fn select_with_soft_delete() {
        let b = SqlBuilder::new("demo_sample").eq("name", "x").limit(10);
        let sql = b.build_select();
        assert!(sql.contains("deleteTime IS NULL"));
        assert!(sql.contains("`name` = 'x'"));
        assert!(sql.contains("LIMIT 10"));
    }

    #[test]
    fn insert_escapes_quote() {
        let mut m = serde_json::Map::new();
        m.insert("name".into(), serde_json::json!("a'b"));
        let sql = SqlBuilder::build_insert("t", &m);
        assert!(sql.contains("'a''b'"));
    }

    #[test]
    fn unsafe_ident_rejected() {
        let b = SqlBuilder::new("t").eq("id;drop", 1);
        assert_eq!(b.wheres.len(), 0);
    }
}
