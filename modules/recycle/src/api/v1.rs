//! api/v1：接口请求类型（按需补充）

use serde::Deserialize;

/// 示例：recycle_item 创建请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecycleItemReq {
    pub name: String,
    #[serde(default)]
    pub remark: Option<String>,
}
