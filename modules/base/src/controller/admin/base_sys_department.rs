//! base_sys_department 控制器（对齐 controller/admin/base_sys_department.go）

use axum::Router;
use cool_core::controller::{crud_router, ApiAllow};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};
use super::base_comm::FileState;

fn service() -> CrudService {

    CrudService::new("base_sys_department")
        .not_null_key("name", "部门名称不能为空")
        .list_op(QueryOp::new().field_eq(&["parentId"]).key_word_field(&["name"]).add_orderby("orderNum", "asc"))
        .page_op(QueryOp::new().field_eq(&["parentId"]).key_word_field(&["name"]).add_orderby("orderNum", "asc"))

}

/// 注册路由
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    let svc = service();
    let api = ApiAllow::default();
    let mut r = Router::new();
    for prefix in ["/base/sys/department", "/admin/base/sys/department"] {
        r = r.merge(crud_router(prefix, svc.clone(), api.clone()));
    }
    r
}
