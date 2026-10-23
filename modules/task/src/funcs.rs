//! 任务函数注册表
//!
//! 对齐 cool-admin-go cool.RegisterFunc / cool.RunFunc：
//! `service` 字段存 `FuncName(arg)` 字符串，调度时查表执行。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// 任务函数签名：参数为括号内字符串，返回错误信息
type TaskFn = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

static REGISTRY: OnceLock<Mutex<HashMap<String, TaskFn>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, TaskFn>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 注册任务函数
pub fn register(name: &str, f: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static) {
    registry()
        .lock()
        .unwrap()
        .insert(name.to_string(), Arc::new(f));
}

/// 是否存在
pub fn exists(name: &str) -> bool {
    registry().lock().unwrap().contains_key(name)
}

/// 解析 `FuncName(arg)` → (name, arg)
pub fn parse_service(s: &str) -> (String, String) {
    let s = s.trim();
    match s.find('(') {
        Some(pos) if s.ends_with(')') => {
            let name = s[..pos].trim().to_string();
            let arg = s[pos + 1..s.len() - 1].to_string();
            (name, arg)
        }
        _ => (s.to_string(), String::new()),
    }
}

/// 执行 `service` 字符串，如 `TaskTest(hello)`
pub fn run_service(service: &str) -> Result<(), String> {
    let (name, arg) = parse_service(service);
    let f = registry()
        .lock()
        .unwrap()
        .get(&name)
        .cloned()
        .ok_or_else(|| format!("函数不存在{name}"))?;
    f(&arg)
}

/// 注册内置任务函数（对齐 cool-admin-go modules/task/funcs）
pub fn register_builtins() {
    // TaskTest：演示任务
    register("TaskTest", |param| {
        tracing::info!(param, "TaskTest Run ~~~~~~~~~~~~~~~~");
        Ok(())
    });

    // BaseFuncClearLog(false)：清理超过保留期的 task_log
    // 参数：true=连失败日志一起清；false=仅清理旧的成功日志（保留最近 20 条/任务）
    register("BaseFuncClearLog", |param| {
        let deep = param.trim() == "true";
        tracing::info!(deep, "BaseFuncClearLog 执行");
        // 具体 SQL 由调度器通过回调注入（这里只做标记，真正清理见 clear_old_logs）
        crate::scheduler::clear_old_logs(deep);
        Ok(())
    });
}
