//! base_sys_param 控制器（对齐 controller/admin/base_sys_param.go）

use super::base_comm::FileState;
use axum::Router;
use cool_core::controller::{ApiAllow, crud_router};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};

fn service() -> CrudService {
    CrudService::new("base_sys_param")
        .unique_key("keyName", "键位已存在")
        .not_null_key("keyName", "键位不能为空")
        .list_op(
            QueryOp::new()
                .field_eq(&["dataType"])
                .key_word_field(&["keyName", "name", "data"]),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["dataType"])
                .key_word_field(&["keyName", "name", "data"])
                .add_orderby("id", "desc"),
        )
}

/// 注册路由
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    let svc = service();
    let api = ApiAllow::default();
    let mut r = Router::new();
    for prefix in ["/base/sys/param", "/admin/base/sys/param"] {
        r = r.merge(crud_router(prefix, svc.clone(), api.clone()));
    }
    r
}
