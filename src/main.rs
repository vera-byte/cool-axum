//! cool-axum 入口：对齐 cool-admin-go 的 main.go / internal/cmd

mod app;
mod routes;

use anyhow::Context;
use cool_config::AppConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 配置路径：环境变量 COOL_CONFIG 或默认 config.yaml
    let config_path =
        std::env::var("COOL_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());

    let config = AppConfig::load(&config_path)
        .with_context(|| format!("加载配置文件失败: {config_path}"))?;

    init_tracing(&config.logger.level);

    tracing::info!(
        path = %config_path,
        address = %config.server.address,
        "cool-axum 启动中..."
    );

    app::serve(config).await
}

fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("info,cool_axum={level},cool_core={level}")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
