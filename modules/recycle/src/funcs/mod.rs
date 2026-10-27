//! funcs：任务函数注册（对齐 modules/recycle/funcs）
//!
//! 注册后可在 task_info.service 中引用：`FuncName(arg)`

/// 示例任务函数
pub async fn recycle_demo_func(param: &str) -> Result<(), String> {
    tracing::info!(param, "mod-recycle demo func");
    Ok(())
}
