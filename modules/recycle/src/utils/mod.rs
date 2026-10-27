//! utils：工具函数

/// SQL 值转义
pub fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}
