//! base_sys_menu 控制器（对齐 controller/admin/base_sys_menu.go）

use axum::Router;
use cool_core::controller::{crud_router, ApiAllow};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};
use super::base_comm::FileState;

fn service() -> CrudService {

    CrudService::new("base_sys_menu")
        .not_null_key("name", "菜单名称不能为空")
        .list_op(QueryOp::new().field_eq(&["parentId", "type", "isShow"]).key_word_field(&["name", "router", "perms"]))
        .page_op(QueryOp::new().field_eq(&["parentId", "type", "isShow"]).key_word_field(&["name", "router", "perms"]).add_orderby("orderNum", "asc"))

}

/// 注册路由
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    let svc = service();
    let api = ApiAllow::default();
    let mut r = Router::new();
    for prefix in ["/base/sys/menu", "/admin/base/sys/menu"] {
        r = r.merge(crud_router(prefix, svc.clone(), api.clone()));
    }
    r
}
