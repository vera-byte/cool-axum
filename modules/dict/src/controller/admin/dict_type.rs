//! dict_type 控制器（对齐 controller/admin/dict_type.go）

use axum::Router;
use cool_core::controller::{crud_router, ApiAllow};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};

fn service() -> CrudService {
    CrudService::new("dict_type")
        .unique_key("key", "标识已存在")
        .not_null_key("name", "名称不能为空")
        .not_null_key("key", "标识不能为空")
        .list_op(QueryOp::new().key_word_field(&["name", "key"]))
        .page_op(QueryOp::new().key_word_field(&["name", "key"]).add_orderby("id", "desc"))
}

pub fn routes<S: CrudState>() -> Router<S> {
    let svc = service();
    let mut r = Router::new();
    for prefix in ["/dict/type", "/admin/dict/type"] {
        r = r.merge(crud_router(prefix, svc.clone(), ApiAllow::default()));
    }
    r
}
