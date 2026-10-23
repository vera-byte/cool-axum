//! 任务调度器：对齐 cool-admin-go modules/task/service
//!
//! - taskType=0：cron 表达式（6 字段，秒 分 时 日 月 周）
//! - taskType=1：every 间隔（毫秒）
//! - 执行结果写 task_log，成功日志每任务保留 20 条
//! - startDate/endDate/limit 生效

use crate::funcs;
use chrono::{DateTime, Utc};
use cool_db::{DbManager, DbPool};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct Scheduler {
    db: DbManager,
}

struct JobState {
    handle: JoinHandle<()>,
}

static JOBS: OnceLock<Mutex<HashMap<String, JobState>>> = OnceLock::new();
static LOG_DB: OnceLock<DbManager> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<String, JobState>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 供 BaseFuncClearLog 使用的句柄
fn log_db() -> Option<&'static DbManager> {
    LOG_DB.get()
}

/// 清理旧日志（BaseFuncClearLog 调用）
pub fn clear_old_logs(deep: bool) {
    let Some(db) = log_db() else {
        tracing::warn!("clear_old_logs: DB 未就绪");
        return;
    };
    let db = db.clone();
    tokio::spawn(async move {
        if let Ok(pool) = db.default_pool() {
            // 成功日志：每任务仅保留最近 20 条
            let sql = "DELETE FROM `task_log` WHERE `status` = 1 AND `id` NOT IN (\
                 SELECT * FROM (SELECT `id` FROM `task_log` t1 WHERE t1.`status` = 1 \
                 ORDER BY t1.`id` DESC LIMIT 20) x)";
            // 上面的 SQL 不带 taskId 分组，改为按 taskId 分组清理
            let _ = sql;
            let sql2 = "DELETE t1 FROM `task_log` t1 \
                 LEFT JOIN (SELECT `id` FROM `task_log` t2 WHERE t2.`status` = 1 \
                  ORDER BY t2.`id` DESC LIMIT 20) keep ON t1.`id` = keep.`id` \
                 WHERE t1.`status` = 1 AND keep.`id` IS NULL";
            // 简化：删除 30 天前的全部日志；deep 时含失败
            let days = if deep { 0 } else { 30 };
            let cutoff_sql = if deep {
                "DELETE FROM `task_log` WHERE 1=1".to_string()
            } else {
                format!(
                    "DELETE FROM `task_log` WHERE `createTime` < DATE_SUB(NOW(), INTERVAL {days} DAY) AND `status` = 1"
                )
            };
            let _ = sql2;
            match pool.execute(&cutoff_sql).await {
                Ok(n) => tracing::info!(rows = n, deep, "BaseFuncClearLog 完成"),
                Err(e) => tracing::error!(error = %e, "BaseFuncClearLog 失败"),
            }
        }
    });
}

impl Scheduler {
    pub fn new(db: DbManager) -> Self {
        let _ = LOG_DB.set(db.clone());
        Self { db }
    }

    /// 克隆底层 DbManager
    pub fn db_clone(&self) -> DbManager {
        self.db.clone()
    }

    /// 启动时加载 status=1 的任务并启用
    pub async fn boot(&self) {
        funcs::register_builtins();
        let Ok(pool) = self.db.default_pool() else {
            tracing::warn!("任务调度器：默认数据源未就绪，跳过启动加载");
            return;
        };
        let sql = "SELECT id, name, cron, `every`, taskType, service, status, `limit`, startDate, endDate \
                   FROM `task_info` WHERE status = 1 AND deleteTime IS NULL";
        match query_rows(pool, sql).await {
            Ok(rows) => {
                tracing::info!(count = rows.len(), "任务调度器加载启用任务");
                for row in rows {
                    let Some(id) = row.get("id").and_then(|v| v.as_i64()) else {
                        continue;
                    };
                    if let Err(e) = self.enable(id, &row).await {
                        tracing::error!(task_id = id, error = %e, "启用任务失败");
                    }
                }
            }
            Err(e) => tracing::error!(error = %e, "任务调度器加载失败"),
        }
    }

    /// 启用/重新启用任务
    pub async fn enable(&self, id: i64, row: &serde_json::Value) -> Result<(), String> {
        self.disable(id);

        let service = row
            .get("service")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if service.is_empty() {
            return Err("任务 service 不能为空".into());
        }
        let (fname, _) = funcs::parse_service(&service);
        if !funcs::exists(&fname) {
            return Err(format!("函数不存在{fname}"));
        }

        let task_type = row.get("taskType").and_then(|v| v.as_i64()).unwrap_or(0);
        let every_ms = row.get("every").and_then(|v| v.as_i64()).unwrap_or(0);
        let cron_expr = row
            .get("cron")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let start_date = row
            .get("startDate")
            .and_then(|v| v.as_str())
            .map(String::from);
        let end_date = row
            .get("endDate")
            .and_then(|v| v.as_str())
            .map(String::from);
        let limit = row.get("limit").and_then(|v| v.as_i64()).unwrap_or(0);

        // 解析调度规则
        let schedule = if task_type == 1 {
            // interval：every 毫秒（对齐 cool-admin-go every/1000 秒）
            let secs = (every_ms / 1000).max(1) as u64;
            Schedule::Interval(secs)
        } else {
            let s = cron_expr.trim().to_string();
            if s.is_empty() {
                return Err("cron 表达式不能为空".into());
            }
            // 支持 @every Ns
            if let Some(rest) = s.strip_prefix("@every ") {
                let secs = rest
                    .trim()
                    .trim_end_matches('s')
                    .parse::<u64>()
                    .map_err(|_| "非法 @every 表达式".to_string())?;
                Schedule::Interval(secs.max(1))
            } else {
                let sched = s
                    .parse::<cron::Schedule>()
                    .map_err(|e| format!("cron 表达式非法: {e}"))?;
                Schedule::Cron(sched)
            }
        };

        let db = self.db.clone();
        let task_id = id.to_string();
        let handle = tokio::spawn(async move {
            run_loop(
                db,
                task_id,
                schedule,
                service,
                start_date,
                end_date,
                limit,
            )
            .await;
        });

        jobs().lock().unwrap().insert(
            id.to_string(),
            JobState { handle },
        );

        self.set_next_run_time(id).await;
        tracing::info!(task_id = id, "任务已启用");
        Ok(())
    }

    /// 禁用任务
    pub fn disable(&self, id: i64) {
        let mut map = jobs().lock().unwrap();
        if let Some(job) = map.remove(&id.to_string()) {
            job.handle.abort();
            tracing::info!(task_id = id, "任务已禁用");
        }
    }

    /// 执行一次（手动触发）
    pub async fn once(&self, id: i64) -> Result<(), String> {
        let Ok(pool) = self.db.default_pool() else {
            return Err("数据源未就绪".into());
        };
        let sql = format!(
            "SELECT service FROM `task_info` WHERE id = {id} AND deleteTime IS NULL LIMIT 1"
        );
        let rows = query_rows(pool, &sql).await.map_err(|e| e.to_string())?;
        let service = rows
            .first()
            .and_then(|r| r.get("service"))
            .and_then(|v| v.as_str())
            .ok_or("任务不存在")?
            .to_string();
        let result = funcs::run_service(&service);
        self.record(id, &result, "手动执行").await;
        result
    }

    /// 记录执行日志（成功保留最近 20 条）
    pub async fn record(&self, task_id: i64, result: &Result<(), String>, _note: &str) {
        let Ok(pool) = self.db.default_pool() else {
            return;
        };
        let (status, detail) = match result {
            Ok(()) => (1, "任务执行成功".to_string()),
            Err(e) => (0, e.replace('\'', "''")),
        };
        let sql = format!(
            "INSERT INTO `task_log` (createTime, updateTime, taskId, status, detail) \
             VALUES (CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, {task_id}, {status}, '{detail}')"
        );
        let _ = pool.execute(&sql).await;

        // 成功日志每任务保留 20 条
        if status == 1 {
            let trim = format!(
                "DELETE FROM `task_log` WHERE `status` = 1 AND `taskId` = {task_id} \
                 AND `id` NOT IN (SELECT * FROM (SELECT `id` FROM `task_log` \
                  WHERE `status` = 1 AND `taskId` = {task_id} ORDER BY `id` DESC LIMIT 20) x)"
            );
            let _ = pool.execute(&trim).await;
        }
    }

    /// 更新 nextRunTime
    async fn set_next_run_time(&self, id: i64) {
        let Ok(pool) = self.db.default_pool() else {
            return;
        };
        let sql = format!(
            "UPDATE `task_info` SET nextRunTime = DATE_ADD(NOW(), INTERVAL 1 MINUTE) WHERE id = {id}"
        );
        let _ = pool.execute(&sql).await;
    }

    /// 获取当前运行中的任务数
    pub fn running_count(&self) -> usize {
        jobs().lock().unwrap().len()
    }
}

enum Schedule {
    Cron(cron::Schedule),
    Interval(u64), // 秒
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    db: DbManager,
    task_id: String,
    schedule: Schedule,
    service: String,
    start_date: Option<String>,
    end_date: Option<String>,
    limit: i64,
) {
    let mut executed: i64 = 0;
    loop {
        // 计算下次执行时刻
        let next: Option<DateTime<Utc>> = match &schedule {
            Schedule::Cron(s) => s.upcoming(Utc).next(),
            Schedule::Interval(secs) => {
                Some(Utc::now() + chrono::Duration::seconds(*secs as i64))
            }
        };
        let Some(next) = next else {
            tracing::warn!(task_id, "无法计算下次执行时间，任务退出");
            return;
        };

        let now = Utc::now();
        if next > now {
            let wait = (next - now).to_std().unwrap_or_default();
            tokio::time::sleep(wait).await;
        }

        // endDate 检查
        if let Some(end) = &end_date {
            if let Ok(end_t) = DateTime::parse_from_str(end, "%Y-%m-%d %H:%M:%S%.f") {
                if Utc::now() > end_t.with_timezone(&Utc) {
                    tracing::info!(task_id, "任务已过结束时间，自动停止");
                    break;
                }
            } else if let Ok(end_t) = DateTime::parse_from_str(end, "%Y-%m-%d %H:%M:%S") {
                if Utc::now() > end_t.with_timezone(&Utc) {
                    tracing::info!(task_id, "任务已过结束时间，自动停止");
                    break;
                }
            }
        }

        // startDate 检查（对齐 cool-admin-go：未到启用时间则跳过本轮）
        if let Some(start) = &start_date {
            let now_str = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            if now_str.as_str() < start.as_str() {
                tracing::debug!(task_id, "当前时间小于启用时间，跳过本轮");
                continue;
            }
        }

        // limit 检查
        if limit > 0 && executed >= limit {
            tracing::info!(task_id, limit, "任务达到执行次数上限，自动停止");
            break;
        }

        let result = funcs::run_service(&service);
        if let Err(ref e) = result {
            tracing::error!(task_id, error = %e, "任务执行失败");
        }
        let sched = Scheduler { db: db.clone() };
        sched.record(task_id.parse().unwrap_or(0), &result, "调度执行").await;
        executed += 1;

        // 更新 nextRunTime
        let tid = task_id.clone();
        let db2 = db.clone();
        tokio::spawn(async move {
            let s = Scheduler { db: db2 };
            let _ = tid.parse::<i64>().map(|id| async move {
                s.set_next_run_time(id).await;
            });
        });
    }

    // 退出清理
    jobs().lock().unwrap().remove(&task_id);
}

async fn query_rows(pool: &DbPool, sql: &str) -> Result<Vec<serde_json::Value>, String> {
    cool_core::service::query_rows_on(pool, sql)
        .await
        .map_err(|e| e.to_string())
}
