//! 数据库表结构内省

use cool_core::db::{DbManager, DbPool};
use serde::Serialize;

/// 列元信息（从 information_schema / SHOW COLUMNS 读取）
#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub column_type: String,
    pub is_nullable: bool,
    pub column_key: String,
    pub extra: String,
    pub column_comment: String,
    pub column_default: Option<String>,
}

/// 列出全部用户表
pub async fn list_tables(db: &DbManager) -> anyhow::Result<()> {
    let pool = db.default_pool()?;
    let rows = match pool {
        DbPool::MySql(p) => {
            let sql = "SELECT CAST(TABLE_NAME AS CHAR), CAST(IFNULL(TABLE_COMMENT,'') AS CHAR) \
                       FROM information_schema.TABLES \
                       WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE' \
                       ORDER BY TABLE_NAME";
            let rows = sqlx::query(sql).fetch_all(p).await?;
            rows.iter()
                .map(|r| {
                    use sqlx::Row;
                    let name: String = r.try_get::<String, _>(0).unwrap_or_default();
                    let comment: String = r.try_get::<String, _>(1).unwrap_or_default();
                    (name, comment)
                })
                .collect::<Vec<_>>()
        }
        DbPool::Postgres(p) => {
            let sql = "SELECT table_name, '' \
                       FROM information_schema.tables \
                       WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
                       ORDER BY table_name";
            let rows = sqlx::query(sql).fetch_all(p).await?;
            rows.iter()
                .map(|r| {
                    use sqlx::Row;
                    let name: String = r.try_get::<String, _>(0).unwrap_or_default();
                    (name, String::new())
                })
                .collect::<Vec<_>>()
        }
    };

    println!("共 {} 张表：\n", rows.len());
    println!("{:<40} 注释", "表名");
    println!("{}", "-".repeat(60));
    for (name, comment) in rows {
        println!("{name:<40} {comment}");
    }
    Ok(())
}

/// 读取指定表的列信息
pub async fn describe_table(db: &DbManager, table: &str) -> anyhow::Result<Vec<ColumnInfo>> {
    let pool = db.default_pool()?;
    let rows: Vec<ColumnInfo> = match pool {
        DbPool::MySql(p) => {
            let t = table.replace('\'', "''");
            let sql = format!(
                "SELECT CAST(COLUMN_NAME AS CHAR), CAST(DATA_TYPE AS CHAR), CAST(COLUMN_TYPE AS CHAR), \
                  CAST(IS_NULLABLE AS CHAR), CAST(IFNULL(COLUMN_KEY,'') AS CHAR), CAST(IFNULL(EXTRA,'') AS CHAR), \
                  CAST(IFNULL(COLUMN_COMMENT,'') AS CHAR), CAST(IFNULL(COLUMN_DEFAULT,'') AS CHAR) \
                 FROM information_schema.COLUMNS \
                 WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{t}' \
                 ORDER BY ORDINAL_POSITION"
            );
            let rows = sqlx::query(&sql).fetch_all(p).await?;
            rows.iter()
                .map(|r| {
                    use sqlx::Row;
                    let yn: String = r.try_get::<String, _>(3).unwrap_or_else(|_| "YES".into());
                    ColumnInfo {
                        name: r.try_get::<String, _>(0).unwrap_or_default(),
                        data_type: r.try_get::<String, _>(1).unwrap_or_default(),
                        column_type: r.try_get::<String, _>(2).unwrap_or_default(),
                        is_nullable: yn.eq_ignore_ascii_case("yes"),
                        column_key: r.try_get::<String, _>(4).unwrap_or_default(),
                        extra: r.try_get::<String, _>(5).unwrap_or_default(),
                        column_comment: r.try_get::<String, _>(6).unwrap_or_default(),
                        column_default: r.try_get::<Option<String>, _>(7).ok().flatten(),
                    }
                })
                .collect()
        }
        DbPool::Postgres(p) => {
            let t = table.replace('\'', "''");
            let sql = format!(
                "SELECT CAST(column_name AS CHAR), CAST(data_type AS CHAR), CAST(data_type AS CHAR), \
                  CAST(is_nullable AS CHAR), CAST('' AS CHAR), CAST('' AS CHAR), \
                  CAST('' AS CHAR), CAST(column_default AS CHAR) \
                 FROM information_schema.columns \
                 WHERE table_schema = 'public' AND table_name = '{t}' \
                 ORDER BY ordinal_position"
            );
            let rows = sqlx::query(&sql).fetch_all(p).await?;
            rows.iter()
                .map(|r| {
                    use sqlx::Row;
                    let yn: String = r.try_get::<String, _>(3).unwrap_or_else(|_| "YES".into());
                    ColumnInfo {
                        name: r.try_get::<String, _>(0).unwrap_or_default(),
                        data_type: r.try_get::<String, _>(1).unwrap_or_default(),
                        column_type: r.try_get::<String, _>(2).unwrap_or_default(),
                        is_nullable: yn.eq_ignore_ascii_case("yes"),
                        column_key: String::new(),
                        extra: String::new(),
                        column_comment: String::new(),
                        column_default: r.try_get::<Option<String>, _>(7).ok().flatten(),
                    }
                })
                .collect()
        }
    };

    if rows.is_empty() {
        anyhow::bail!("表不存在或无列: {table}");
    }
    Ok(rows)
}
