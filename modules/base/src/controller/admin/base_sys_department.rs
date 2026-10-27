//! base_sys_department 控制器（对齐 controller/admin/base_sys_department.go）

use super::base_comm::FileState;
use axum::Router;
use cool_core::controller::{ApiAllow, crud_router};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};

fn service() -> CrudService {
    CrudService::new("base_sys_department")
        .not_null_key("name", "部门名称不能为空")
        .list_op(
            QueryOp::new()
                .field_eq(&["parentId"])
                .key_word_field(&["name"])
                .add_orderby("orderNum", "asc"),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["parentId"])
                .key_word_field(&["name"])
                .add_orderby("orderNum", "asc"),
        )
}

/// 注册路由
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    let svc = service();
    let api = ApiAllow::default();
    let mut r = Router::new();
    for prefix in ["/admin/base/sys/department", "/admin/department"] {
        r = r.merge(crud_router(prefix, svc.clone(), api.clone()));
    }
    r
}
