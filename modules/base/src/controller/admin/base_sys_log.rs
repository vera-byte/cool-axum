//! base_sys_log 控制器（对齐 controller/admin/base_sys_log.go）

use super::base_comm::FileState;
use axum::Router;
use cool_core::controller::{ApiAllow, crud_router};
use cool_core::service::CrudService;
use cool_core::{CrudState, QueryOp};

fn service() -> CrudService {
    CrudService::new("base_sys_log")
        .list_op(
            QueryOp::new()
                .field_eq(&["userId", "action"])
                .key_word_field(&["action", "ip", "params"])
                .add_orderby("id", "desc"),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["userId", "action"])
                .key_word_field(&["action", "ip", "params"])
                .add_orderby("id", "desc"),
        )
}

/// 注册路由
pub fn routes<S: CrudState + FileState>() -> Router<S> {
    let svc = service();
    let api = ApiAllow {
        add: false,
        delete: true,
        update: false,
        info: true,
        list: true,
        page: true,
    };
    let mut r = Router::new();
    for prefix in ["/admin/base/sys/log", "/admin/log"] {
        r = r.merge(crud_router(prefix, svc.clone(), api.clone()));
    }
    r
}
