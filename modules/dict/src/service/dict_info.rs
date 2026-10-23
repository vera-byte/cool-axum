//! dict_info 服务（对齐 service/dict_info.go）

use cool_core::service::query_rows_on;
use cool_db::DbManager;
use serde_json::{json, Map, Value};

/// Data：获得字典数据（对齐 DictInfoService.Data）
///
/// types 为空查全部；否则按 dict_type.key 过滤。
/// 返回 `{ "typeKey": [ {id,name,parentId,typeId}, ... ] }`
pub async fn data(db: &DbManager, types: Option<Vec<String>>) -> Result<Value, String> {
    let pool = db.default_pool().map_err(|e| e.to_string())?;

    // 查询 dict_type
    let mut type_sql = "SELECT id, `key` FROM `dict_type` WHERE deleteTime IS NULL".to_string();
    if let Some(types) = types {
        if !types.is_empty() {
            let list = types
                .iter()
                .map(|k| format!("'{}'", k.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",");
            // 对齐 Go 语义：按标识 key 过滤（Go 误写 type）
            type_sql.push_str(&format!(" AND `key` IN ({list})"));
        }
    }
    let type_rows = query_rows_on(pool, &type_sql)
        .await
        .map_err(|e| e.to_string())?;
    if type_rows.is_empty() {
        return Ok(json!({}));
    }

    let mut out = Map::new();
    for v in &type_rows {
        let Some(id) = v.get("id").and_then(|x| x.as_i64()) else {
            continue;
        };
        let Some(key) = v.get("key").and_then(|x| x.as_str()) else {
            continue;
        };
        let sql = format!(
            "SELECT id, name, parentId, typeId FROM `dict_info` \
             WHERE typeId = {id} AND deleteTime IS NULL ORDER BY orderNum ASC"
        );
        let result = query_rows_on(pool, &sql)
            .await
            .map_err(|e| e.to_string())?;
        if result.is_empty() {
            continue;
        }
        out.insert(key.to_string(), Value::Array(result));
    }
    Ok(Value::Object(out))
}
