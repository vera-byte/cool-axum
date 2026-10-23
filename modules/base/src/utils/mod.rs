//! utils：工具函数（对齐 modules/base/utils/utils.go）

/// 逗号分隔展开
pub fn split_perms(perms: &str) -> Vec<String> {
    perms
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// SQL 值转义
pub fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}
