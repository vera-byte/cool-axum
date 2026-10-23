//! cool-config：对齐 cool-admin-go 的 manifest/config/config.yaml 配置结构

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("读取配置文件失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("解析配置文件失败: {0}")]
    Parse(#[from] serde_yaml::Error),
}

/// server 节
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// 监听地址，如 ":8001"
    #[serde(default = "default_address")]
    pub address: String,
    #[serde(default = "default_openapi_path")]
    pub openapi_path: String,
    #[serde(default = "default_swagger_path")]
    pub swagger_path: String,
    /// 客户端最大 body，默认 100MB
    #[serde(default = "default_max_body")]
    pub client_max_body_size: usize,
}

fn default_address() -> String {
    ":8001".into()
}
fn default_openapi_path() -> String {
    "/api.json".into()
}
fn default_swagger_path() -> String {
    "/swagger".into()
}
fn default_max_body() -> usize {
    104_857_600
}

/// logger 节
#[derive(Debug, Clone, Deserialize)]
pub struct LoggerConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_true")]
    pub stdout: bool,
}

fn default_log_level() -> String {
    "info".into()
}
fn default_true() -> bool {
    true
}

/// database 单库配置
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseEntry {
    /// mysql | pgsql | sqlite
    #[serde(rename = "type", default = "default_db_type")]
    pub db_type: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub pass: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_charset")]
    pub charset: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub debug: bool,
    /// sqlite link
    #[serde(default)]
    pub link: Option<String>,
    /// 对齐 cool-admin-go 字段命名
    #[serde(default = "default_created_at")]
    pub created_at: String,
    #[serde(default = "default_updated_at")]
    pub updated_at: String,
    #[serde(default = "default_deleted_at")]
    pub deleted_at: String,
}

fn default_db_type() -> String {
    "mysql".into()
}
fn default_charset() -> String {
    "utf8mb4".into()
}
fn default_timezone() -> String {
    "Asia/Shanghai".into()
}
fn default_created_at() -> String {
    "createTime".into()
}
fn default_updated_at() -> String {
    "updateTime".into()
}
fn default_deleted_at() -> String {
    "deleteTime".into()
}

/// redis 单实例配置
#[derive(Debug, Clone, Deserialize)]
pub struct RedisEntry {
    #[serde(default = "default_redis_address")]
    pub address: String,
    #[serde(default)]
    pub db: i64,
    #[serde(default)]
    pub password: Option<String>,
}

fn default_redis_address() -> String {
    "127.0.0.1:6379".into()
}

/// cool 节
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct CoolConfig {
    #[serde(default)]
    pub auto_migrate: bool,
    #[serde(default)]
    pub eps: bool,
    #[serde(default)]
    pub file: FileConfig,
}

/// 文件上传配置
#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    /// local | minio | oss
    #[serde(default = "default_file_mode")]
    pub mode: String,
    #[serde(default = "default_file_domain")]
    pub domain: String,
    #[serde(default)]
    pub oss: Option<OssConfig>,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            mode: default_file_mode(),
            domain: default_file_domain(),
            oss: None,
        }
    }
}

fn default_file_mode() -> String {
    "local".into()
}
fn default_file_domain() -> String {
    "http://127.0.0.1:8002".into()
}

/// OSS/Minio 配置
#[derive(Debug, Clone, Deserialize)]
pub struct OssConfig {
    pub endpoint: String,
    #[serde(rename = "accessKeyID")]
    pub access_key_id: String,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: String,
    #[serde(rename = "bucketName")]
    pub bucket_name: String,
    #[serde(rename = "useSSL", default)]
    pub use_ssl: bool,
    #[serde(default = "default_location")]
    pub location: String,
}

fn default_location() -> String {
    "us-east-1".into()
}

/// modules 节（base 登录鉴权等）
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModulesConfig {
    #[serde(default)]
    pub base: BaseModuleConfig,
}

/// raft 集群配置（可选）
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RaftSection {
    /// 是否启用（默认 false：单机模式，任务本地执行）
    #[serde(default)]
    pub enable: bool,
    /// 本节点 ID
    #[serde(default = "default_node_id")]
    pub node_id: u64,
    /// 本节点 API 地址（供其他节点 RPC）
    #[serde(default)]
    pub self_addr: String,
    /// 集群成员：id:addr 列表
    #[serde(default)]
    pub peers: Vec<String>,
    /// RocksDB 持久化目录（每节点独立）
    #[serde(default = "default_raft_data_dir")]
    pub data_dir: String,
}

fn default_node_id() -> u64 {
    1
}

fn default_raft_data_dir() -> String {
    "./data/raft".to_string()
}

/// modules.base
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BaseModuleConfig {
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub middleware: BaseMiddlewareConfig,
}

/// 鉴权配置：对齐 cool-admin-go jwt 节，但底层换成 sa-token
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    #[serde(default)]
    pub sso: bool,
    #[serde(default = "default_auth_secret")]
    pub secret: String,
    /// token 有效期（秒）
    #[serde(default = "default_token_expire")]
    pub token_expire: i64,
    /// refresh token 有效期（秒）
    #[serde(default = "default_refresh_expire")]
    pub refresh_expire: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            sso: false,
            secret: default_auth_secret(),
            token_expire: default_token_expire(),
            refresh_expire: default_refresh_expire(),
        }
    }
}

fn default_auth_secret() -> String {
    "cool-base88776655".into()
}
fn default_token_expire() -> i64 {
    10 * 3600
}
fn default_refresh_expire() -> i64 {
    1_296_000
}

/// base 中间件开关
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BaseMiddlewareConfig {
    #[serde(default)]
    pub authority: MiddlewareSwitch,
    #[serde(default)]
    pub log: MiddlewareSwitch,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MiddlewareSwitch {
    #[serde(default = "default_true")]
    pub enable: bool,
}

impl Default for MiddlewareSwitch {
    fn default() -> Self {
        Self { enable: true }
    }
}

/// 顶层配置
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub logger: LoggerConfig,
    /// key 为分组名（default / baseConfig ...）
    #[serde(default)]
    pub database: std::collections::HashMap<String, DatabaseEntry>,
    /// key 为分组名（cool ...）
    #[serde(default)]
    pub redis: std::collections::HashMap<String, RedisEntry>,
    #[serde(default)]
    pub cool: CoolConfig,
    #[serde(default)]
    pub modules: ModulesConfig,
    /// Raft 集群（可选）
    #[serde(default)]
    pub raft: RaftSection,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            address: default_address(),
            openapi_path: default_openapi_path(),
            swagger_path: default_swagger_path(),
            client_max_body_size: default_max_body(),
        }
    }
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            stdout: default_true(),
        }
    }
}

impl AppConfig {
    /// 从 YAML 文件加载配置
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        let cfg: AppConfig = serde_yaml::from_str(&raw)?;
        Ok(cfg)
    }

    /// 从 YAML 字符串加载（测试用）
    pub fn from_yaml(raw: &str) -> Result<Self, ConfigError> {
        Ok(serde_yaml::from_str(raw)?)
    }

    /// 获取默认数据库配置
    pub fn default_database(&self) -> Option<&DatabaseEntry> {
        self.database.get("default")
    }

    /// 获取 cool redis 配置
    pub fn cool_redis(&self) -> Option<&RedisEntry> {
        self.redis.get("cool")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aligned_yaml() {
        let raw = r#"
server:
  address: ":8001"
  openapiPath: "/api.json"
  clientMaxBodySize: 100
logger:
  level: "info"
database:
  default:
    type: "mysql"
    host: "127.0.0.1"
    port: "3306"
    createdAt: "createTime"
    deletedAt: "deleteTime"
cool:
  autoMigrate: true
modules:
  base:
    auth:
      secret: "s"
      tokenExpire: 100
"#;
        let cfg = AppConfig::from_yaml(raw).unwrap();
        assert_eq!(cfg.server.address, ":8001");
        assert_eq!(cfg.server.openapi_path, "/api.json");
        assert_eq!(cfg.server.client_max_body_size, 100);
        let db = cfg.default_database().unwrap();
        assert_eq!(db.db_type, "mysql");
        assert_eq!(db.created_at, "createTime");
        assert_eq!(db.deleted_at, "deleteTime");
        assert!(cfg.cool.auto_migrate);
        assert_eq!(cfg.modules.base.auth.token_expire, 100);
    }
}
