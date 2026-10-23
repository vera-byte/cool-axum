//! base_sys_role 控制器（对齐 controller/admin/base_sys_role.go）

use axum::Router;
use cool_core::controller::{crud_router, ApiAllow};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};
use super::base_comm::FileState;

fn service() -> CrudService {

    CrudService::new("base_sys_role")
        .unique_key("name", "角色名已存在")
        .not_null_key("name", "角色名不能为空")
        .list_op(QueryOp::new().field_eq(&["label"]).key_word_field(&["name", "label"]))
        .page_op(QueryOp::new().field_eq(&["label"]).key_word_field(&["name", "label"]).add_orderby("id", "desc"))

}

/// 注册路由
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    let svc = service();
    let api = ApiAllow::default();
    let mut r = Router::new();
    for prefix in ["/base/sys/role", "/admin/base/sys/role"] {
        r = r.merge(crud_router(prefix, svc.clone(), api.clone()));
    }
    r
}
