//! cool-db：数据库层，对齐 cool-admin-go 的 database 配置（MySQL / PostgreSQL）
//!
//! 提供连接池管理、分组数据源、统一查询入口。

use cool_config::DatabaseEntry;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("数据库连接失败: {0}")]
    Connect(String),
    #[error("SQL 执行失败: {0}")]
    Sql(String),
    #[error("数据库类型不支持: {0}")]
    UnsupportedType(String),
    #[error("数据源分组不存在: {0}")]
    GroupNotFound(String),
}

/// 数据库类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbKind {
    MySql,
    Postgres,
}

/// 连接池（按数据库类型封装）
#[derive(Clone)]
pub enum DbPool {
    MySql(sqlx::MySqlPool),
    Postgres(sqlx::PgPool),
}

impl DbPool {
    pub fn kind(&self) -> DbKind {
        match self {
            DbPool::MySql(_) => DbKind::MySql,
            DbPool::Postgres(_) => DbKind::Postgres,
        }
    }

    /// 执行一条语句，返回受影响行数
    pub async fn execute(&self, sql: &str) -> Result<u64, DbError> {
        match self {
            DbPool::MySql(pool) => {
                let r = sqlx::query(sql).execute(pool).await.map_err(sql_err)?;
                Ok(r.rows_affected())
            }
            DbPool::Postgres(pool) => {
                let r = sqlx::query(sql).execute(pool).await.map_err(sql_err)?;
                Ok(r.rows_affected())
            }
        }
    }

    /// 健康检查
    pub async fn ping(&self) -> Result<(), DbError> {
        match self {
            DbPool::MySql(pool) => sqlx::query("SELECT 1")
                .execute(pool)
                .await
                .map_err(sql_err)
                .map(|_| ()),
            DbPool::Postgres(pool) => sqlx::query("SELECT 1")
                .execute(pool)
                .await
                .map_err(sql_err)
                .map(|_| ()),
        }
    }
}

fn sql_err(e: sqlx::Error) -> DbError {
    DbError::Sql(e.to_string())
}

/// 表列元信息（供 CRUD 引擎与 autoMigrate 使用）
#[derive(Debug, Clone)]
pub struct ColumnMeta {
    pub name: String,
    pub sql_type: String,
    pub not_null: bool,
    pub primary_key: bool,
    pub comment: String,
    pub auto_increment: bool,
    /// 列默认值（DDL DEFAULT 子句，不含 DEFAULT 关键字）
    pub default: Option<String>,
}

/// 表定义（Rust 侧声明，用于 autoMigrate）
#[derive(Debug, Clone)]
pub struct TableDef {
    pub name: String,
    pub columns: Vec<ColumnMeta>,
}

/// 种子数据填充记录表 base_sys_init
pub fn base_sys_init_def() -> TableDef {
    TableDef {
        name: "base_sys_init".into(),
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
            ColumnMeta {
                name: "table".into(),
                sql_type: "varchar(255)".into(),
                not_null: true,
                primary_key: false,
                comment: "表名".into(),
                auto_increment: false,
                default: None,
            },
            ColumnMeta {
                name: "group".into(),
                sql_type: "varchar(255)".into(),
                not_null: true,
                primary_key: false,
                comment: "分组".into(),
                auto_increment: false,
                default: None,
            },
        ],
    }
}

/// FillInitData：对齐 cool-admin-go cool.FillInitData
///
/// `base_sys_init` 记录 (group, table) 已初始化；首次填充 JSON 数组数据并落账，重复启动跳过。
/// 返回插入行数；已初始化或无数据返回 0。
pub async fn fill_init_data(
    db: &DbManager,
    group: &str,
    table: &str,
    json_raw: &str,
) -> Result<u64, DbError> {
    let pool = db.get(group)?;

    // 已初始化？
    let g_esc = group.replace('\'', "''");
    let t_esc = table.replace('\'', "''");
    let check = format!(
        "SELECT COUNT(*) AS count FROM `base_sys_init` WHERE `group` = '{g_esc}' AND `table` = '{t_esc}'"
    );
    let rows = query_count(pool, &check).await?;
    if rows > 0 {
        tracing::debug!(group, table, "种子数据已初始化，跳过");
        return Ok(0);
    }

    // 解析 JSON 数组
    let arr: Vec<serde_json::Value> = serde_json::from_str(json_raw).unwrap_or_default();
    if arr.is_empty() {
        tracing::debug!(group, table, "无可用的初始化数据，跳过");
        return Ok(0);
    }

    let mut inserted = 0u64;
    for row in &arr {
        let Some(map) = row.as_object() else { continue };
        let mut data = map.clone();
        // 空字符串 parent 类字段转 NULL 不做特殊处理——JSON null 直接写 NULL
        let sql = build_insert_json(table, &data);
        // 转义错误不阻断其他行
        data.clear();
        match pool.execute(&sql).await {
            Ok(_) => inserted += 1,
            Err(e) => {
                tracing::error!(group, table, error = %e, "种子数据插入失败");
            }
        }
    }

    // 落账
    let rec =
        format!("INSERT INTO `base_sys_init` (`table`, `group`) VALUES ('{t_esc}', '{g_esc}')");
    let _ = pool.execute(&rec).await;

    tracing::info!(group, table, inserted, "种子数据初始化完成");
    Ok(inserted)
}

async fn query_count(pool: &DbPool, sql: &str) -> Result<u64, DbError> {
    // 用 execute 不合适，走轻量查询
    match pool {
        DbPool::MySql(p) => {
            let rows = sqlx::query(sql).fetch_all(p).await.map_err(sql_err)?;
            Ok(first_count(
                &rows.iter().map(mysql_row_value).collect::<Vec<_>>(),
            ))
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sql).fetch_all(p).await.map_err(sql_err)?;
            Ok(first_count(
                &rows.iter().map(pg_row_value).collect::<Vec<_>>(),
            ))
        }
    }
}

fn first_count(rows: &[serde_json::Value]) -> u64 {
    rows.first()
        .and_then(|r| r.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

// 简易行→JSON（仅 count 查询用）
fn mysql_row_value(row: &sqlx::mysql::MySqlRow) -> serde_json::Value {
    use sqlx::Column;
    use sqlx::Row;
    let mut m = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let v = row
            .try_get::<Option<i64>, _>(name.as_str())
            .map(|x| serde_json::json!(x))
            .unwrap_or(serde_json::Value::Null);
        m.insert(name, v);
    }
    serde_json::Value::Object(m)
}

fn pg_row_value(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Column;
    use sqlx::Row;
    let mut m = serde_json::Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let v = row
            .try_get::<Option<i64>, _>(name.as_str())
            .map(|x| serde_json::json!(x))
            .unwrap_or(serde_json::Value::Null);
        m.insert(name, v);
    }
    serde_json::Value::Object(m)
}

/// JSON 对象 → INSERT（列名反引号，值字面量）
fn build_insert_json(table: &str, data: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut cols = Vec::new();
    let mut vals = Vec::new();
    for (k, v) in data {
        // gorm 默认软删除列名 → cool-admin-go 配置列名
        let key = if k == "deleted_at" {
            "deleteTime"
        } else {
            k.as_str()
        };
        // 标识符安全校验（含 table/group 保留字，靠反引号）
        let safe = !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '`');
        if !safe {
            continue;
        }
        cols.push(format!("`{key}`"));
        vals.push(json_lit(v));
    }
    format!(
        "INSERT INTO `{table}` ({}) VALUES ({})",
        cols.join(", "),
        vals.join(", ")
    )
}

fn json_lit(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".into(),
        serde_json::Value::Bool(b) => {
            if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        other => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

/// 多数据源管理器：group -> pool
#[derive(Clone)]
pub struct DbManager {
    pools: Arc<HashMap<String, DbPool>>,
}

impl DbManager {
    /// 根据配置初始化全部数据源
    ///
    /// 单个数据源连接失败不阻断启动（记录错误，健康检查可探测到），
    /// 与 cool-admin-go 的宽松启动行为对齐。
    pub async fn init(databases: &HashMap<String, DatabaseEntry>) -> Result<Self, DbError> {
        let mut pools = HashMap::new();
        for (group, entry) in databases {
            match connect_entry(entry).await {
                Ok(pool) => {
                    tracing::info!(group = %group, db_type = %entry.db_type, "数据源已注册（惰性连接）");
                    pools.insert(group.clone(), pool);
                }
                Err(e) => {
                    tracing::error!(group = %group, error = %e, "数据源连接失败，启动继续");
                }
            }
        }
        Ok(Self {
            pools: Arc::new(pools),
        })
    }

    /// 获取指定分组连接池
    pub fn get(&self, group: &str) -> Result<&DbPool, DbError> {
        self.pools
            .get(group)
            .ok_or_else(|| DbError::GroupNotFound(group.to_string()))
    }

    /// 获取默认分组
    pub fn default_pool(&self) -> Result<&DbPool, DbError> {
        self.get("default")
    }

    /// 全部健康检查
    pub async fn ping_all(&self) -> Result<(), DbError> {
        for (group, pool) in self.pools.iter() {
            pool.ping()
                .await
                .map_err(|e| DbError::Connect(format!("{group}: {e}")))?;
        }
        Ok(())
    }

    /// 已建立连接池数量
    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }
}

async fn connect_entry(entry: &DatabaseEntry) -> Result<DbPool, DbError> {
    let kind = entry.db_type.to_lowercase();
    match kind.as_str() {
        "mysql" => {
            let url = format!(
                "mysql://{}:{}@{}:{}/{}?charset={}&timezone={}",
                entry.user.as_deref().unwrap_or("root"),
                entry.pass.as_deref().unwrap_or(""),
                entry.host.as_deref().unwrap_or("127.0.0.1"),
                entry.port.as_deref().unwrap_or("3306"),
                entry.name.as_deref().unwrap_or(""),
                entry.charset,
                entry.timezone,
            );
            // connect_lazy：启动不被数据库可用性阻塞，实际连接由 acquire_timeout 兜底
            let pool = MySqlPoolOptions::new()
                .max_connections(10)
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect_lazy(&url)
                .map_err(|e| DbError::Connect(e.to_string()))?;
            Ok(DbPool::MySql(pool))
        }
        "pgsql" | "postgres" | "postgresql" => {
            let url = format!(
                "postgres://{}:{}@{}:{}/{}",
                entry.user.as_deref().unwrap_or("postgres"),
                entry.pass.as_deref().unwrap_or(""),
                entry.host.as_deref().unwrap_or("127.0.0.1"),
                entry.port.as_deref().unwrap_or("5432"),
                entry.name.as_deref().unwrap_or(""),
            );
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .acquire_timeout(std::time::Duration::from_secs(5))
                .connect_lazy(&url)
                .map_err(|e| DbError::Connect(e.to_string()))?;
            Ok(DbPool::Postgres(pool))
        }
        other => Err(DbError::UnsupportedType(other.to_string())),
    }
}

/// 生成 autoMigrate 的 CREATE TABLE 语句（幂等：IF NOT EXISTS）
///
/// 列名统一加反引号（MySQL）/ 双引号（PG），规避 `key`/`limit` 等保留字
pub fn build_create_table_sql(kind: DbKind, def: &TableDef) -> String {
    let quote = |s: &str| -> String {
        match kind {
            DbKind::MySql => format!("`{s}`"),
            DbKind::Postgres => format!("\"{s}\""),
        }
    };

    let mut cols = Vec::new();
    let mut pk_cols: Vec<String> = Vec::new();

    for col in &def.columns {
        let mut line = format!("  {} {}", quote(&col.name), map_type(kind, &col.sql_type));
        if col.primary_key {
            pk_cols.push(quote(&col.name));
            // 自增主键
            match kind {
                DbKind::MySql => line.push_str(" AUTO_INCREMENT"),
                DbKind::Postgres => {
                    // PG 用 GENERATED，简化：BIGSERIAL 语义由 BIGINT + 序列替代，此处仍用 BIGINT 主键
                }
            }
        }
        if col.not_null && !col.primary_key {
            line.push_str(" NOT NULL");
        }
        if let Some(d) = &col.default {
            line.push_str(&format!(" DEFAULT {d}"));
        }
        if !col.comment.is_empty() {
            match kind {
                DbKind::MySql => {
                    line.push_str(&format!(" COMMENT '{}'", col.comment.replace('\'', "''")));
                }
                DbKind::Postgres => {}
            }
        }
        cols.push(line);
    }

    if !pk_cols.is_empty() {
        cols.push(format!("  PRIMARY KEY ({})", pk_cols.join(", ")));
    }

    format!(
        "CREATE TABLE IF NOT EXISTS {} (\n{}\n)",
        quote(&def.name),
        cols.join(",\n")
    )
}

fn map_type(kind: DbKind, sql_type: &str) -> String {
    let t = sql_type.to_lowercase();
    match kind {
        DbKind::MySql => sql_type.to_string(),
        DbKind::Postgres => {
            // 常见 MySQL 类型映射到 PG
            if t.starts_with("bigint") {
                "BIGINT".into()
            } else if t.starts_with("int") {
                "INTEGER".into()
            } else if t.starts_with("tinyint(1)") || t == "bool" || t == "boolean" {
                "BOOLEAN".into()
            } else if t.starts_with("varchar") {
                t.replacen("varchar", "VARCHAR", 1)
            } else if t.starts_with("text") || t.starts_with("longtext") {
                "TEXT".into()
            } else if t.starts_with("datetime") || t.starts_with("timestamp") {
                "TIMESTAMP".into()
            } else if t.starts_with("decimal") {
                t.replacen("decimal", "NUMERIC", 1)
            } else if t.starts_with("json") {
                "JSONB".into()
            } else {
                sql_type.to_string()
            }
        }
    }
}
