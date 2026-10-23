//! cool-file：文件上传驱动层，对齐 cool-admin-go cool/coolfile + contrib/files
//!
//! - `local`：落盘 `./public/uploads/Ymd/`，静态 `/public` 暴露
//! - `minio` / `oss`：对象存储（S3 兼容 PUT）
//!
//! 驱动由 `cool.file.mode` 配置选择：local | minio | oss

use cool_config::{FileConfig, OssConfig};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("上传文件为空")]
    Empty,
    #[error("保存文件失败: {0}")]
    Save(String),
    #[error("对象存储上传失败: {0}")]
    Remote(String),
    #[error("无法找到指定文件上传类型 {0}")]
    UnknownMode(String),
}

/// 上传结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct UploadResult {
    /// 可访问 URL
    pub url: String,
    /// 存储模式
    pub mode: String,
}

/// 本地磁盘驱动
pub struct LocalDriver {
    /// 存储根目录
    pub root: PathBuf,
    /// 访问域名（如 http://127.0.0.1:8001）
    pub domain: String,
}

impl LocalDriver {
    pub fn new(file: &FileConfig) -> Self {
        let root = PathBuf::from("./public/uploads");
        if !root.exists() {
            let _ = std::fs::create_dir_all(&root);
        }
        Self {
            root,
            domain: file.domain.trim_end_matches('/').to_string(),
        }
    }

    async fn upload(&self, filename: &str, data: &[u8]) -> Result<String, FileError> {
        let dir = chrono::Local::now().format("%Y%m%d").to_string();
        let dir_path = self.root.join(&dir);
        std::fs::create_dir_all(&dir_path).map_err(|e| FileError::Save(e.to_string()))?;

        let unique = format!("{}_{}", uuid::Uuid::new_v4().simple(), sanitize_filename(filename));
        let path = dir_path.join(&unique);
        std::fs::write(&path, data).map_err(|e| FileError::Save(e.to_string()))?;

        Ok(format!(
            "{}/public/uploads/{}/{}",
            self.domain, dir, unique
        ))
    }
}

/// 对象存储驱动（minio / oss，S3 兼容 REST PUT）
pub struct ObjectDriver {
    pub mode: String,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    #[allow(dead_code)]
    pub secret_key: String,
    pub domain: String,
}

impl ObjectDriver {
    pub fn from_oss(mode: &str, file: &FileConfig, oss: &OssConfig) -> Self {
        let endpoint = if oss.use_ssl {
            format!("https://{}", oss.endpoint.trim_end_matches('/'))
        } else {
            format!("http://{}", oss.endpoint.trim_end_matches('/'))
        };
        Self {
            mode: mode.to_string(),
            endpoint,
            bucket: oss.bucket_name.clone(),
            access_key: oss.access_key_id.clone(),
            secret_key: oss.secret_access_key.clone(),
            domain: file.domain.trim_end_matches('/').to_string(),
        }
    }

    async fn upload(&self, filename: &str, data: &[u8]) -> Result<String, FileError> {
        let dir = chrono::Local::now().format("%Y%m%d").to_string();
        let key = format!(
            "{}/{}_{}",
            dir,
            uuid::Uuid::new_v4().simple(),
            sanitize_filename(filename)
        );
        let url = format!("{}/{}/{}", self.endpoint, self.bucket, key);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| FileError::Remote(e.to_string()))?;

        let resp = client
            .put(&url)
            .header("Authorization", format!("AWS {}:{}", self.access_key, self.secret_key))
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| FileError::Remote(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(FileError::Remote(format!("{status}: {body}")));
        }

        if self.domain.is_empty() {
            Ok(url)
        } else {
            Ok(format!("{}/{}", self.domain, key))
        }
    }
}

/// 文件驱动（enum，避免 trait object 安全问题）
pub enum FileDriver {
    Local(LocalDriver),
    Object(ObjectDriver),
}

impl FileDriver {
    /// 模式信息
    pub fn mode_info(&self) -> serde_json::Value {
        match self {
            FileDriver::Local(d) => serde_json::json!({
                "mode": "local",
                "type": "local",
                "domain": d.domain,
                "root": d.root.display().to_string(),
            }),
            FileDriver::Object(d) => serde_json::json!({
                "mode": d.mode,
                "type": d.mode,
                "endpoint": d.endpoint,
                "bucket": d.bucket,
                "domain": d.domain,
            }),
        }
    }

    /// 保存文件，返回可访问 URL
    pub async fn upload(&self, filename: &str, data: &[u8]) -> Result<String, FileError> {
        if data.is_empty() {
            return Err(FileError::Empty);
        }
        match self {
            FileDriver::Local(d) => d.upload(filename, data).await,
            FileDriver::Object(d) => d.upload(filename, data).await,
        }
    }

    /// 模式名
    pub fn mode_name(&self) -> &str {
        match self {
            FileDriver::Local(_) => "local",
            FileDriver::Object(d) => &d.mode,
        }
    }
}

/// 文件名安全化：去掉路径分隔符与危险字符
fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() {
        "file.bin".into()
    } else {
        cleaned
    }
}

/// 按配置创建驱动
pub fn create_driver(file: &FileConfig) -> Result<FileDriver, FileError> {
    match file.mode.as_str() {
        "local" => Ok(FileDriver::Local(LocalDriver::new(file))),
        "minio" | "oss" => {
            let oss = file
                .oss
                .as_ref()
                .ok_or_else(|| FileError::UnknownMode(format!("{}: 缺少 oss 配置", file.mode)))?;
            Ok(FileDriver::Object(ObjectDriver::from_oss(&file.mode, file, oss)))
        }
        other => Err(FileError::UnknownMode(other.to_string())),
    }
}

/// 静态根目录（local 模式下 /public 映射）
pub fn local_public_root() -> PathBuf {
    PathBuf::from("./public")
}
