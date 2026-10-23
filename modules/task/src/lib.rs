//! mod-task：对齐 cool-admin-go modules/task（定时任务）
//!
//! - CRUD：task_info / task_log
//! - 调度器：cron / interval 两种任务类型，执行日志自动记录

pub mod funcs;
pub mod initjson;
pub mod scheduler;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use cool_core::controller::{crud_router, ApiAllow};
use cool_core::response::{fail, ok, BaseRes};
use cool_core::service::CrudService;
use cool_core::QueryOp;
use cool_db::{ColumnMeta, TableDef};
use scheduler::Scheduler;
use serde_json::{json, Map, Value};

fn col(name: &str, sql_type: &str, not_null: bool, comment: &str) -> ColumnMeta {
    ColumnMeta {
        name: name.into(),
        sql_type: sql_type.into(),
        not_null,
        primary_key: false,
        comment: comment.into(),
        auto_increment: false,
        default: None,
    }
}

fn base_cols() -> Vec<ColumnMeta> {
    vec![
        ColumnMeta {
            name: "id".into(),
            sql_type: "bigint".into(),
            not_null: true,
            primary_key: true,
            comment: "主键".into(),
            auto_increment: true,
            default: None,
        },
        col("createTime", "datetime", true, "创建时间"),
        col("updateTime", "datetime", true, "更新时间"),
        col("deleteTime", "datetime", false, "删除时间"),
    ]
}

/// task_info
pub fn task_info() -> TableDef {
    let mut cols = base_cols();
    cols.push(col("jobId", "varchar(255)", false, "任务ID"));
    cols.push(col("repeatConf", "varchar(255)", false, "重复配置"));
    cols.push(col("name", "varchar(255)", false, "任务名称"));
    cols.push(col("cron", "varchar(255)", false, "cron表达式"));
    cols.push(col("limit", "int", false, "限制次数"));
    cols.push(col("every", "int", false, "间隔时间秒"));
    cols.push(col("remark", "varchar(255)", false, "备注"));
    cols.push(col("status", "int", false, "状态 0关闭 1开启"));
    cols.push(col("startDate", "datetime", false, "开始时间"));
    cols.push(col("endDate", "datetime", false, "结束时间"));
    cols.push(col("data", "varchar(255)", false, "数据"));
    cols.push(col("service", "varchar(255)", false, "执行的服务"));
    cols.push(col("type", "int", false, "类型 0系统 1用户"));
    cols.push(col("nextRunTime", "datetime", false, "下次执行时间"));
    cols.push(col("taskType", "int", false, "任务类型 0cron 1时间间隔"));
    TableDef {
        name: "task_info".into(),
        columns: cols,
    }
}

/// task_log
pub fn task_log() -> TableDef {
    let mut cols = base_cols();
    cols.push(col("taskId", "bigint", false, "任务ID"));
    cols.push(col("status", "int", true, "状态 0失败 1成功"));
    cols.push(col("detail", "text", false, "详情"));
    TableDef {
        name: "task_log".into(),
        columns: cols,
    }
}

pub fn all_tables() -> Vec<TableDef> {
    vec![task_info(), task_log()]
}

fn task_info_service() -> CrudService {
    CrudService::new("task_info")
        .unique_key("name", "任务名称不能重复")
        .not_null_key("name", "任务名称不能为空")
        .list_op(
            QueryOp::new()
                .field_eq(&["status", "type", "taskType"])
                .key_word_field(&["name", "cron", "service", "remark"]),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["status", "type", "taskType"])
                .key_word_field(&["name", "cron", "service", "remark"])
                .add_orderby("id", "desc"),
        )
}

/// 注册 task 模块路由
pub fn routes<S: cool_core::CrudState + TaskState>() -> Router<S> {
    // task_log 只读 + 删除
    let log = CrudService::new("task_log")
        .list_op(
            QueryOp::new()
                .field_eq(&["taskId", "status"])
                .key_word_field(&["detail"])
                .add_orderby("id", "desc"),
        )
        .page_op(
            QueryOp::new()
                .field_eq(&["taskId", "status"])
                .key_word_field(&["detail"])
                .add_orderby("id", "desc"),
        );
    let log_api = ApiAllow {
        add: false,
        delete: true,
        update: false,
        info: true,
        list: true,
        page: true,
    };

    // task_info：delete/info/list/page 走通用，add/update 自定义联动调度器
    let r = crud_router(
        "/task/info",
        task_info_service(),
        ApiAllow {
            add: false,
            update: false,
            ..ApiAllow::default()
        },
    )
    .merge(crud_router("/task/log", log, log_api))
    .route("/task/info/add", post(add_task::<S>))
    .route("/task/info/update", post(update_task::<S>))
    .route("/task/info/once", post(once_task::<S>))
    .route("/task/info/status", get(scheduler_status::<S>));

    r
}

/// 暴露调度器给 handler
pub trait TaskState: Send + Sync + 'static {
    fn scheduler(&self) -> &Scheduler;
}

/// POST /task/info/add —— status=1 时立即启用
async fn add_task<S: cool_core::CrudState + TaskState>(
    State(st): State<S>,
    Json(body): Json<Value>,
) -> Json<BaseRes> {
    let data = as_map(&body);
    let enable = data.get("status").and_then(|v| v.as_i64()).unwrap_or(0) == 1;

    match task_info_service().add(st.db(), data).await {
        Ok(v) => {
            if enable {
                let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
                sync_enable(st.scheduler(), id).await;
            }
            Json(ok(v))
        }
        Err(e) => Json(e.to_res()),
    }
}

/// POST /task/info/update —— 按 status 启用/禁用
async fn update_task<S: cool_core::CrudState + TaskState>(
    State(st): State<S>,
    Json(body): Json<Value>,
) -> Json<BaseRes> {
    let data = as_map(&body);
    let Some(id) = data.get("id").and_then(|x| x.as_i64()) else {
        return Json(fail("id不能为空"));
    };
    let status = data.get("status").and_then(|v| v.as_i64());

    match task_info_service().update(st.db(), data).await {
        Ok(v) => {
            match status {
                Some(1) => sync_enable(st.scheduler(), id).await,
                Some(_) => st.scheduler().disable(id),
                None => {}
            }
            Json(ok(v))
        }
        Err(e) => Json(e.to_res()),
    }
}

/// POST /task/info/once —— 手动执行一次
async fn once_task<S: cool_core::CrudState + TaskState>(
    State(st): State<S>,
    Json(body): Json<Value>,
) -> Json<BaseRes> {
    let Some(id) = body.get("id").and_then(|v| v.as_i64()) else {
        return Json(fail("id不能为空"));
    };
    match st.scheduler().once(id).await {
        Ok(()) => Json(ok(json!({}))),
        Err(e) => Json(fail(e)),
    }
}

/// GET /task/info/status —— 调度器运行状态
async fn scheduler_status<S: cool_core::CrudState + TaskState>(
    State(st): State<S>,
) -> Json<BaseRes> {
    Json(ok(json!({
        "running": st.scheduler().running_count(),
    })))
}

/// 从库里读任务定义并启用
async fn sync_enable(sched: &Scheduler, id: i64) {
    let db = sched.db_clone();
    let Ok(pool) = db.default_pool() else {
        return;
    };
    let sql = format!(
        "SELECT id, name, cron, `every`, taskType, service, status, `limit`, startDate, endDate \
         FROM `task_info` WHERE id = {id} AND deleteTime IS NULL LIMIT 1"
    );
    match cool_core::service::query_rows_on(pool, &sql).await {
        Ok(rows) => {
            if let Some(row) = rows.first() {
                if let Err(e) = sched.enable(id, row).await {
                    tracing::error!(task_id = id, error = %e, "启用任务失败");
                }
            }
        }
        Err(e) => tracing::error!(error = %e, "读取任务定义失败"),
    }
}

fn as_map(v: &Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    }
}

/// 模块注册声明（对齐 task/task.go init）
pub fn spec() -> cool_core::ModuleSpec {
    cool_core::ModuleSpec::new("task")
        .tables(all_tables())
        .seeds(vec![("task_info", initjson::TASK_INFO)])
}

/// 模块启动钩子：注册内置任务函数
pub async fn init(_db: &cool_db::DbManager) {
    funcs::register_builtins();
}
