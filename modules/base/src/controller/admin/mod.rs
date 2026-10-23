//! controller/admin：后台管理控制器（对齐 controller/admin/*.go）

pub mod base_comm;
pub mod base_open;
pub mod base_sys_department;
pub mod base_sys_log;
pub mod base_sys_menu;
pub mod base_sys_param;
pub mod base_sys_role;
pub mod base_sys_user;

use axum::Router;
use cool_core::CrudState;

use base_comm::FileState;

/// 业务路由（需登录 + 权限点），不含 open/comm
pub fn biz_routes<S: CrudState + FileState>() -> Router<S> {
    let mut r = Router::new()
        .merge(base_sys_user::routes::<S>())
        .merge(base_sys_role::routes::<S>())
        .merge(base_sys_menu::routes::<S>())
        .merge(base_sys_department::routes::<S>())
        .merge(base_sys_param::routes::<S>())
        .merge(base_sys_log::routes::<S>());

    // 关联表
    r = r.merge(relations::routes::<S>());
    r
}

/// admin 全部路由（open + comm + biz）
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    base_open::routes::<S>()
        .merge(base_comm::routes::<S>())
        .merge(biz_routes::<S>())
}

mod relations {
    use axum::Router;
    use cool_core::controller::{crud_router, ApiAllow};
    use cool_core::service::CrudService;
    use cool_core::{CrudState, QueryOp};

    use super::base_comm::FileState;

    pub fn routes<S: CrudState + FileState>() -> Router<S> {
        let user_role = CrudService::new("base_sys_user_role")
            .list_op(QueryOp::new().field_eq(&["userId", "roleId"]))
            .page_op(QueryOp::new().field_eq(&["userId", "roleId"]));
        let role_menu = CrudService::new("base_sys_role_menu")
            .list_op(QueryOp::new().field_eq(&["roleId", "menuId"]))
            .page_op(QueryOp::new().field_eq(&["roleId", "menuId"]));
        let role_dept = CrudService::new("base_sys_role_department")
            .list_op(QueryOp::new().field_eq(&["roleId", "departmentId"]))
            .page_op(QueryOp::new().field_eq(&["roleId", "departmentId"]));

        let mut r = Router::new();
        for (prefix, svc) in [
            ("/base/sys/userRole", user_role.clone()),
            ("/base/sys/roleMenu", role_menu.clone()),
            ("/base/sys/roleDepartment", role_dept.clone()),
            ("/admin/base/sys/userRole", user_role.clone()),
            ("/admin/base/sys/roleMenu", role_menu.clone()),
            ("/admin/base/sys/roleDepartment", role_dept),
        ] {
            r = r.merge(crud_router(prefix, svc.clone(), ApiAllow::default()));
        }
        r
    }
}
