//! cool-axum 入口：对齐 cool-admin-go 的 main.go / internal/cmd

mod app;
mod log_rotate;

use anyhow::Context;
use cool_config::{AppConfig, LoggerConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 配置路径：环境变量 COOL_CONFIG 或默认 config.yaml
    let config_path = std::env::var("COOL_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());

    let config = AppConfig::load(&config_path)
        .with_context(|| format!("加载配置文件失败: {config_path}"))?;

    // 先注入节点身份，保证后续每行日志都带 node=
    mod_base::middleware::init_node(
        config.raft.node_id,
        &config.server.address,
        config.server.node_label.as_deref(),
    );

    init_tracing(&config.logger).map_err(|e| anyhow::anyhow!("初始化日志失败: {e}"))?;

    tracing::info!(
        path = %config_path,
        address = %config.server.address,
        node = %mod_base::middleware::node_label(),
        level = %config.logger.level,
        format = %config.logger.format,
        "cool-axum 启动中..."
    );

    app::serve(config).await
}

/// 初始化日志
///
/// - `level`：trace/debug/info/warn/error
/// - `format`：text（默认，含 node= 前缀）| json（结构化，含 node 字段）
/// - `stdout` / `file`：可同时开启（tee 到标准输出与轮转文件）
fn init_tracing(cfg: &LoggerConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io;
    use tracing::Subscriber;
    use tracing_subscriber::fmt::format::Writer;
    use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
    use tracing_subscriber::layer::{Layer, SubscriberExt};
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(cfg.level.as_str()));

    /// text：每行固定携带节点标识
    struct NodeFormat;

    impl<S, N> FormatEvent<S, N> for NodeFormat
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        N: for<'a> FormatFields<'a> + 'static,
    {
        fn format_event(
            &self,
            ctx: &FmtContext<'_, S, N>,
            mut writer: Writer<'_>,
            event: &tracing::Event<'_>,
        ) -> std::fmt::Result {
            let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
            write!(writer, "{} ", ts)?;
            write!(writer, "{} ", event.metadata().level())?;
            write!(writer, "node={} ", mod_base::middleware::node_label())?;
            ctx.field_format().format_fields(writer.by_ref(), event)?;
            writeln!(writer)
        }
    }

    /// json：每条含 timestamp/level/node/message/字段
    struct JsonNodeFormat;

    impl<S, N> FormatEvent<S, N> for JsonNodeFormat
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        N: for<'a> FormatFields<'a> + 'static,
    {
        fn format_event(
            &self,
            _ctx: &FmtContext<'_, S, N>,
            mut writer: Writer<'_>,
            event: &tracing::Event<'_>,
        ) -> std::fmt::Result {
            use tracing::field::{Field, Visit};

            struct V<'a> {
                map: &'a mut serde_json::Map<String, serde_json::Value>,
            }
            impl Visit for V<'_> {
                fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                    let key = if field.name() == "message" {
                        "message".to_string()
                    } else {
                        field.name().to_string()
                    };
                    self.map.insert(
                        key,
                        serde_json::json!(format!("{value:?}").trim_matches('"')),
                    );
                }
                fn record_str(&mut self, field: &Field, value: &str) {
                    let key = if field.name() == "message" {
                        "message".to_string()
                    } else {
                        field.name().to_string()
                    };
                    self.map.insert(key, serde_json::json!(value));
                }
                fn record_i64(&mut self, field: &Field, value: i64) {
                    self.map.insert(field.name().to_string(), serde_json::json!(value));
                }
                fn record_u64(&mut self, field: &Field, value: u64) {
                    self.map.insert(field.name().to_string(), serde_json::json!(value));
                }
                fn record_bool(&mut self, field: &Field, value: bool) {
                    self.map.insert(field.name().to_string(), serde_json::json!(value));
                }
            }

            let mut map = serde_json::Map::new();
            map.insert(
                "timestamp".into(),
                serde_json::json!(chrono::Utc::now().to_rfc3339_opts(
                    chrono::SecondsFormat::Millis,
                    true
                )),
            );
            map.insert(
                "level".into(),
                serde_json::json!(event.metadata().level().to_string()),
            );
            map.insert(
                "node".into(),
                serde_json::json!(mod_base::middleware::node_label()),
            );
            event.record(&mut V { map: &mut map });
            map.entry("message".to_string())
                .or_insert_with(|| serde_json::json!(""));
            writeln!(writer, "{}", serde_json::Value::Object(map))
        }
    }

    let use_json = cfg.format.trim().eq_ignore_ascii_case("json");

    // 可选：轮转文件
    let file_writer = match &cfg.file {
        Some(f) => {
            let w = log_rotate::RotatingFile::open(&f.path, f.max_size, f.max_files)?;
            Some(log_rotate::SharedFile(std::sync::Arc::new(w)))
        }
        None => None,
    };

    if use_json {
        let mut layers = Vec::new();
        if cfg.stdout {
            layers.push(
                tracing_subscriber::fmt::layer()
                    .event_format(JsonNodeFormat)
                    .with_writer(io::stdout)
                    .boxed(),
            );
        }
        if let Some(w) = &file_writer {
            layers.push(
                tracing_subscriber::fmt::layer()
                    .event_format(JsonNodeFormat)
                    .with_writer(w.clone())
                    .boxed(),
            );
        }
        tracing_subscriber::registry().with(filter).with(layers).init();
    } else {
        let mut layers = Vec::new();
        if cfg.stdout {
            layers.push(
                tracing_subscriber::fmt::layer()
                    .event_format(NodeFormat)
                    .with_writer(io::stdout)
                    .boxed(),
            );
        }
        if let Some(w) = &file_writer {
            layers.push(
                tracing_subscriber::fmt::layer()
                    .event_format(NodeFormat)
                    .with_writer(w.clone())
                    .boxed(),
            );
        }
        tracing_subscriber::registry().with(filter).with(layers).init();
    }

    if let Some(f) = &cfg.file {
        tracing::info!(
            path = %f.path,
            max_size = f.max_size,
            max_files = f.max_files,
            "日志文件输出已启用（按大小轮转）"
        );
    }
    Ok(())
}
