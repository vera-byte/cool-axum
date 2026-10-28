//! CrudService：对齐 cool-admin-go 的 cool.Service
//!
//! 只需提供表名 + QueryOp，即可获得 Add/Delete/Update/Info/List/Page 六个标准能力。

use crate::db::{DbManager, DbPool};
use crate::error::CoolError;
use crate::query::{ListReq, PageReq, QueryOp};
use crate::sql::{SqlBuilder, quote_ident};
use serde_json::{Map, Value, json};

/// 写入前变换（密码哈希等）
pub type TransformFn = fn(&mut Map<String, Value>, /*is_update*/ bool) -> Result<(), String>;
/// 更新后回调，返回补充 SQL
pub type AfterUpdateFn = fn(i64, &Map<String, Value>) -> Option<String>;

/// CRUD 服务配置
#[derive(Clone)]
pub struct CrudService {
    /// 表名
    pub table: String,
    /// 数据源分组
    pub group: String,
    /// List 查询配置
    pub list_op: Option<QueryOp>,
    /// Page 查询配置
    pub page_op: Option<QueryOp>,
    /// 新增时强制写入的参数（如 userId）
    pub insert_param: Option<fn() -> Map<String, Value>>,
    /// 写入前变换数据（add/update 通用）：返回 Err 中止写入
    pub transform_data: Option<TransformFn>,
    /// 更新后回调（如 passwordV 递增、会话吊销）
    pub after_update: Option<AfterUpdateFn>,
    /// 唯一键校验：字段名 -> 错误信息
    pub unique_keys: Vec<(String, String)>,
    /// 非空校验：字段名 -> 错误信息
    pub not_null_keys: Vec<(String, String)>,
    /// Info 忽略的字段（逗号分隔）
    pub info_ignore_property: String,
    /// 查询结果需剥离的敏感字段（逗号分隔，如 password）
    pub hidden_fields: Vec<String>,
    /// 软删除列
    pub soft_delete_col: String,
}

impl CrudService {
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            group: "default".into(),
            list_op: None,
            page_op: None,
            insert_param: None,
            transform_data: None,
            after_update: None,
            unique_keys: Vec::new(),
            not_null_keys: Vec::new(),
            info_ignore_property: String::new(),
            hidden_fields: Vec::new(),
            soft_delete_col: "deleteTime".into(),
        }
    }

    pub fn group(mut self, group: &str) -> Self {
        self.group = group.to_string();
        self
    }

    pub fn list_op(mut self, op: QueryOp) -> Self {
        self.list_op = Some(op);
        self
    }

    pub fn page_op(mut self, op: QueryOp) -> Self {
        self.page_op = Some(op);
        self
    }

    pub fn unique_key(mut self, field: &str, msg: &str) -> Self {
        self.unique_keys.push((field.to_string(), msg.to_string()));
        self
    }

    pub fn not_null_key(mut self, field: &str, msg: &str) -> Self {
        self.not_null_keys
            .push((field.to_string(), msg.to_string()));
        self
    }

    pub fn info_ignore_property(mut self, cols: &str) -> Self {
        self.info_ignore_property = cols.to_string();
        self
    }

    /// 查询结果剥离敏感字段（对齐 cool-admin-go ModifyResult）
    pub fn hidden_fields(mut self, cols: &[&str]) -> Self {
        self.hidden_fields = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 写入前变换数据（如密码哈希）
    pub fn transform_data(
        mut self,
        f: fn(&mut Map<String, Value>, bool) -> Result<(), String>,
    ) -> Self {
        self.transform_data = Some(f);
        self
    }

    /// 更新后回调：返回可选的补充 SQL（如 passwordV 递增）
    pub fn after_update(mut self, f: fn(i64, &Map<String, Value>) -> Option<String>) -> Self {
        self.after_update = Some(f);
        self
    }

    fn strip_hidden(&self, v: Value) -> Value {
        if self.hidden_fields.is_empty() {
            return v;
        }
        match v {
            Value::Object(mut m) => {
                for f in &self.hidden_fields {
                    m.remove(f);
                }
                Value::Object(m)
            }
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(|x| self.strip_hidden(x)).collect())
            }
            other => other,
        }
    }

    fn pool<'a>(&self, db: &'a DbManager) -> Result<&'a DbPool, CoolError> {
        db.get(&self.group).map_err(CoolError::from)
    }

    /// 新增，返回 {id}
    pub async fn add(
        &self,
        db: &DbManager,
        mut data: Map<String, Value>,
    ) -> Result<Value, CoolError> {
        // 非空校验
        for (field, msg) in &self.not_null_keys {
            if data.get(field).is_none_or(|v| v.is_null()) {
                return Err(CoolError::biz(msg.clone()));
            }
        }
        // 唯一键校验
        for (field, msg) in &self.unique_keys {
            if let Some(v) = data.get(field)
                && !v.is_null()
            {
                let mut b = SqlBuilder::new(&self.table)
                    .soft_delete_col(Some(&self.soft_delete_col))
                    .eq(field, v.clone());
                if let Some(t) = crate::tenant::current_tenant() {
                    b = b.eq("tenantId", t);
                }
                let n = self.count_sql(db, &b.build_count()).await?;
                if n > 0 {
                    return Err(CoolError::biz(msg.clone()));
                }
            }
        }
        // 追加插入参数
        if let Some(f) = self.insert_param {
            for (k, v) in f() {
                data.insert(k, v);
            }
        }

        // 多租户：自动写入 tenantId（已显式指定则不覆盖）
        if let Some(t) = crate::tenant::current_tenant() {
            data.entry("tenantId".to_string())
                .or_insert_with(|| json!(t));
        }

        // 写入前变换
        if let Some(f) = self.transform_data {
            f(&mut data, false).map_err(CoolError::biz)?;
        }

        // 自动填充创建/更新时间（对齐 gorm autoCreateTime/autoUpdateTime）
        let now = chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S%.3f")
            .to_string();
        data.entry("createTime".to_string())
            .or_insert_with(|| json!(now));
        data.entry("updateTime".to_string())
            .or_insert_with(|| json!(now));

        let sql = SqlBuilder::build_insert(&self.table, &data);
        self.pool(db)?.execute(&sql).await?;

        // 取回自增 id：按唯一排序的最后一行（MySQL/PG 兼容简化：max id）
        let id_sql = format!("SELECT MAX(id) AS id FROM {}", self.table);
        let id = self.query_scalar_i64(db, &id_sql).await?;
        Ok(json!({ "id": id }))
    }

    /// 批量删除（软删除），删除前行快照写入回收站钩子
    pub async fn delete(&self, db: &DbManager, ids: &[i64]) -> Result<Value, CoolError> {
        if ids.is_empty() {
            return Err(CoolError::param("请选择要删除的数据"));
        }
        // 删除前快照（供回收站 restore）
        let id_list = ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut snap_sql = format!("SELECT * FROM {} WHERE `id` IN ({id_list})", self.table);
        if let Some(col) = Some(&self.soft_delete_col) {
            snap_sql.push_str(&format!(" AND {} IS NULL", quote_ident(col)));
        }
        if let Some(t) = crate::tenant::current_tenant() {
            snap_sql.push_str(&format!(" AND `tenantId` = {t}"));
        }
        let rows = self.query_rows(db, &snap_sql).await.unwrap_or_default();

        let Some(sql) = SqlBuilder::build_soft_delete(
            &self.table,
            ids,
            &self.soft_delete_col,
            crate::tenant::current_tenant(),
        ) else {
            return Err(CoolError::param("请选择要删除的数据"));
        };
        let n = self.pool(db)?.execute(&sql).await?;

        if !rows.is_empty() {
            let meta = crate::tenant::current_request_meta();
            crate::tenant::notify_soft_delete(crate::tenant::SoftDeleteRecord {
                table: self.table.clone(),
                ids: ids.to_vec(),
                rows,
                user_id: meta.user_id,
                url: meta.url,
            });
        }
        Ok(json!({ "rows": n }))
    }

    /// 修改（按 id）
    pub async fn update(
        &self,
        db: &DbManager,
        data: Map<String, Value>,
    ) -> Result<Value, CoolError> {
        let id = data
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| CoolError::biz("id不能为空"))?;

        // 唯一键校验（排除自身）
        for (field, msg) in &self.unique_keys {
            if let Some(v) = data.get(field)
                && !v.is_null()
            {
                let b = SqlBuilder::new(&self.table)
                    .soft_delete_col(Some(&self.soft_delete_col))
                    .eq(field, v.clone())
                    .neq("id", id);
                let n = self.count_sql(db, &b.build_count()).await?;
                if n > 0 {
                    return Err(CoolError::biz(msg.clone()));
                }
            }
        }

        // 写入前变换
        let mut data = data;
        if let Some(f) = self.transform_data {
            f(&mut data, true).map_err(CoolError::biz)?;
        }

        // 自动填充更新时间
        data.insert(
            "updateTime".to_string(),
            json!(
                chrono::Utc::now()
                    .format("%Y-%m-%d %H:%M:%S%.3f")
                    .to_string()
            ),
        );

        let sql = SqlBuilder::build_update(
            &self.table,
            &data,
            id,
            Some(&self.soft_delete_col),
            crate::tenant::current_tenant(),
        );
        let n = self.pool(db)?.execute(&sql).await?;

        // 更新后回调（补 SQL，如 passwordV 递增）
        if let Some(f) = self.after_update
            && let Some(extra) = f(id, &data)
        {
            let _ = self.pool(db)?.execute(&extra).await;
        }
        Ok(json!({ "rows": n }))
    }

    /// 详情
    pub async fn info(&self, db: &DbManager, id: i64) -> Result<Value, CoolError> {
        let mut b = SqlBuilder::new(&self.table)
            .soft_delete_col(Some(&self.soft_delete_col))
            .eq("id", id);
        if let Some(t) = crate::tenant::current_tenant() {
            b = b.eq("tenantId", t);
        }
        if !self.info_ignore_property.is_empty() {
            // FieldsEx：全表列无法静态得知，这里用 select * 由调用方在 ModifyResult 处理；
            // 若配置了 list_op.select 则复用
            if let Some(op) = &self.list_op
                && !op.select.is_empty()
            {
                b = b.select_cols(&op.select);
            }
        }
        let sql = b.limit(1).build_select();
        let row = self.query_rows(db, &sql).await?;
        Ok(self.strip_hidden(row.into_iter().next().unwrap_or(Value::Null)))
    }

    /// 列表（默认上限 10000，对齐 cool-admin-go）
    pub async fn list(
        &self,
        db: &DbManager,
        req: &ListReq,
        filters: &Map<String, Value>,
    ) -> Result<Value, CoolError> {
        let op = self.list_op.clone().unwrap_or_default();
        let mut b = self.build_query(&op, filters);
        b = b.limit(10000);

        // 排序：请求优先，否则默认 add_orderby
        let mut ordered = false;
        if let (Some(order), Some(sort)) = (&req.order, &req.sort)
            && let Some(o) = crate::sql::safe_order_by(order, sort)
        {
            b = b.raw_where(""); // no-op 保持链式
            // 通过 order API 再来一次（safe_order_by 已校验）
            for part in o.split(',') {
                let mut it = part.trim().rsplitn(2, ' ');
                if let (Some(dir), Some(field)) = (it.next(), it.next()) {
                    b = b.order(field, dir);
                    ordered = true;
                }
            }
        }
        if !ordered {
            for (f, d) in &op.add_orderby {
                b = b.order(f, d);
            }
        }

        let sql = b.build_select();
        let rows = self.query_rows(db, &sql).await?;
        Ok(self.strip_hidden(Value::Array(rows)))
    }

    /// 分页
    pub async fn page(
        &self,
        db: &DbManager,
        req: &PageReq,
        filters: &Map<String, Value>,
    ) -> Result<Value, CoolError> {
        let op = self
            .page_op
            .clone()
            .or_else(|| self.list_op.clone())
            .unwrap_or_default();
        let mut page = req.page;
        let mut size = req.size;
        if page <= 0 {
            page = 1;
        }
        if size <= 0 {
            size = 10;
        }

        let base = self.build_query(&op, filters);
        let count_sql = base.build_count();
        let total = self.count_sql(db, &count_sql).await?;

        // 导出模式
        if req.is_export {
            let mut b = self.build_query(&op, filters);
            if req.max_export_limit > 0 {
                b = b.limit(req.max_export_limit);
            }
            let sql = b.build_select();
            let list = self.strip_hidden(Value::Array(self.query_rows(db, &sql).await?));
            return Ok(json!({ "list": list, "total": total }));
        }

        let mut b = self.build_query(&op, filters);
        // 排序
        let mut ordered = false;
        if let (Some(order), Some(sort)) = (&req.order, &req.sort)
            && let Some(o) = crate::sql::safe_order_by(order, sort)
        {
            let _ = o;
            for part in crate::sql::safe_order_by(order, sort)
                .unwrap_or_default()
                .split(',')
            {
                let mut it = part.trim().rsplitn(2, ' ');
                if let (Some(dir), Some(field)) = (it.next(), it.next()) {
                    b = b.order(field, dir);
                    ordered = true;
                }
            }
        }
        if !ordered {
            for (f, d) in &op.add_orderby {
                b = b.order(f, d);
            }
        }
        b = b.limit(size).offset((page - 1) * size);

        let sql = b.build_select();
        let list = self.strip_hidden(Value::Array(self.query_rows(db, &sql).await?));
        Ok(json!({
            "list": list,
            "pagination": { "page": page, "size": size, "total": total }
        }))
    }

    fn build_query(&self, op: &QueryOp, filters: &Map<String, Value>) -> SqlBuilder {
        let mut b = SqlBuilder::new(&self.table)
            .soft_delete_col(Some(&self.soft_delete_col))
            .select_cols(&op.select);
        if !op.from_alias.is_empty() {
            b = b.as_alias(&op.from_alias);
        }
        // 多租户隔离
        if let Some(t) = crate::tenant::current_tenant() {
            b = b.eq("tenantId", t);
        }

        for join in &op.joins {
            b = match join.join_type {
                crate::query::JoinType::Left => {
                    b.left_join(&join.table, &join.alias, &join.condition)
                }
                crate::query::JoinType::Right => {
                    b.right_join(&join.table, &join.alias, &join.condition)
                }
                crate::query::JoinType::Inner => {
                    b.inner_join(&join.table, &join.alias, &join.condition)
                }
            };
        }

        // FieldEQ
        for field in &op.field_eq {
            if let Some(v) = filters.get(field)
                && !v.is_null()
            {
                if let Some(s) = v.as_str()
                    && s.is_empty()
                {
                    continue;
                }
                b = b.eq(field, v.clone());
            }
        }

        // keyWord 模糊搜索
        if let Some(kw) = filters.get("keyWord").and_then(|v| v.as_str())
            && !kw.is_empty()
            && !op.key_word_field.is_empty()
        {
            b = b.or_like_any(&op.key_word_field, kw);
        }

        // 自定义 where
        for w in &op.where_clauses {
            b = b.raw_where(w);
        }

        b
    }

    async fn count_sql(&self, db: &DbManager, sql: &str) -> Result<i64, CoolError> {
        let rows = self.query_rows(db, sql).await?;
        let n = rows
            .first()
            .and_then(|r| r.get("count"))
            .or_else(|| rows.first().and_then(|r| r.get("COUNT(*)")))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        Ok(n)
    }

    async fn query_scalar_i64(&self, db: &DbManager, sql: &str) -> Result<i64, CoolError> {
        let rows = self.query_rows(db, sql).await?;
        Ok(rows
            .first()
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0))
    }

    /// 执行查询，返回 JSON 行数组（两库统一为对象数组）
    async fn query_rows(&self, db: &DbManager, sql: &str) -> Result<Vec<Value>, CoolError> {
        let pool = self.pool(db)?;
        query_rows_on(pool, sql).await
    }
}

/// 执行 SELECT，将行转为 JSON 对象（MySQL/PG 双支持）
pub async fn query_rows_on(pool: &DbPool, sql: &str) -> Result<Vec<Value>, CoolError> {
    match pool {
        DbPool::MySql(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| CoolError::internal(e.to_string()))?;
            Ok(rows.iter().map(mysql_row_to_json).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| CoolError::internal(e.to_string()))?;
            Ok(rows.iter().map(pg_row_to_json).collect())
        }
    }
}

fn mysql_row_to_json(row: &sqlx::mysql::MySqlRow) -> Value {
    use sqlx::Column;
    use sqlx::Row;
    use sqlx::TypeInfo;
    let mut m = Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let ti = col.type_info();
        let tn = ti.name();
        let v = match tn {
            "INT" | "BIGINT" | "SMALLINT" | "TINYINT" | "MEDIUMINT" | "YEAR" => row
                .try_get::<i64, _>(name.as_str())
                .map(|x| json!(x))
                .or_else(|_| row.try_get::<i32, _>(name.as_str()).map(|x| json!(x)))
                .unwrap_or(Value::Null),
            "FLOAT" | "DOUBLE" | "DECIMAL" | "NEWDECIMAL" => row
                .try_get::<f64, _>(name.as_str())
                .map(|x| json!(x))
                .unwrap_or(Value::Null),
            "TIMESTAMP" | "DATETIME" | "DATE" => row
                .try_get::<Option<chrono::NaiveDateTime>, _>(name.as_str())
                .map(|x| {
                    x.map(|d| json!(d.format("%Y-%m-%d %H:%M:%S").to_string()))
                        .unwrap_or(Value::Null)
                })
                .or_else(|_| {
                    row.try_get::<Option<chrono::NaiveDate>, _>(name.as_str())
                        .map(|x| x.map(|d| json!(d.to_string())).unwrap_or(Value::Null))
                })
                .or_else(|_| {
                    row.try_get::<Option<String>, _>(name.as_str())
                        .map(|x| x.map(|v| json!(v)).unwrap_or(Value::Null))
                })
                .unwrap_or(Value::Null),
            _ => {
                // 字符串 / json 等
                if let Ok(Some(s)) = row.try_get::<Option<String>, _>(name.as_str()) {
                    json!(s)
                } else {
                    Value::Null
                }
            }
        };
        m.insert(name, v);
    }
    Value::Object(m)
}

fn pg_row_to_json(row: &sqlx::postgres::PgRow) -> Value {
    use sqlx::Column;
    use sqlx::Row;
    use sqlx::TypeInfo;
    let mut m = Map::new();
    for col in row.columns() {
        let name = col.name().to_string();
        let tn = col.type_info().name();
        let v = match tn {
            "INT2" | "INT4" | "INT8" => row
                .try_get::<Option<i64>, _>(name.as_str())
                .map(|x| x.map(|v| json!(v)).unwrap_or(Value::Null))
                .or_else(|_| {
                    row.try_get::<Option<i32>, _>(name.as_str())
                        .map(|x| x.map(|v| json!(v)).unwrap_or(Value::Null))
                })
                .unwrap_or(Value::Null),
            "FLOAT4" | "FLOAT8" | "NUMERIC" => row
                .try_get::<Option<f64>, _>(name.as_str())
                .map(|x| x.map(|v| json!(v)).unwrap_or(Value::Null))
                .unwrap_or(Value::Null),
            "BOOL" => row
                .try_get::<Option<bool>, _>(name.as_str())
                .map(|x| x.map(|v| json!(v)).unwrap_or(Value::Null))
                .unwrap_or(Value::Null),
            "TIMESTAMP" | "TIMESTAMPTZ" | "DATE" => row
                .try_get::<Option<chrono::NaiveDateTime>, _>(name.as_str())
                .map(|x| {
                    x.map(|d| json!(d.format("%Y-%m-%d %H:%M:%S").to_string()))
                        .unwrap_or(Value::Null)
                })
                .or_else(|_| {
                    row.try_get::<Option<chrono::NaiveDate>, _>(name.as_str())
                        .map(|x| x.map(|d| json!(d.to_string())).unwrap_or(Value::Null))
                })
                .unwrap_or(Value::Null),
            _ => {
                if let Ok(Some(s)) = row.try_get::<Option<String>, _>(name.as_str()) {
                    json!(s)
                } else {
                    Value::Null
                }
            }
        };
        m.insert(name, v);
    }
    Value::Object(m)
}
