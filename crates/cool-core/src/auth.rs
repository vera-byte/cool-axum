//! cool-auth：基于官方 sa-token-rust (sa-token-plugin-axum) 的登录鉴权层
//!
//! 对齐 cool-admin-go modules/base 的登录/登出/鉴权能力：
//! - 登录签发 token（Redis 会话可多节点共享）
//! - 登出
//! - SaTokenLayer 提取登录态
//! - SaCheckLoginLayer / SaCheckPermissionLayer 鉴权与权限点

use crate::config::AuthConfig;
use sa_token_plugin_axum::{
    MemoryStorage, RedisStorage, SaCheckLoginLayer, SaCheckPermissionLayer, SaTokenLayer,
    SaTokenState, StpUtil, TokenStyle, TokenValue,
};
use std::sync::Arc;

pub use sa_token_plugin_axum::SaTokenState as AuthState;

/// 初始化 SaTokenState
///
/// - 配置了 redis 地址 → RedisStorage（多节点共享会话，对齐「多节点部署同一单体」）
/// - 否则 → MemoryStorage（单机开发）
/// - Redis 连接 3 秒超时自动回退内存存储，不阻塞启动
pub async fn init_sa_token(auth: &AuthConfig, redis_url: Option<&str>) -> SaTokenState {
    let builder = SaTokenState::builder()
        .token_name("Authorization")
        .timeout(auth.token_expire)
        .active_timeout(auth.refresh_expire)
        .token_style(TokenStyle::Random64)
        .auto_renew(true);

    match redis_url {
        Some(url) => {
            tracing::info!(url, "sa-token 尝试 Redis 会话存储（多节点共享）");
            let connect = RedisStorage::new(url, "cool:sa-token:");
            match tokio::time::timeout(std::time::Duration::from_secs(3), connect).await {
                Ok(Ok(storage)) => {
                    tracing::info!("sa-token Redis 会话存储就绪");
                    builder.storage(Arc::new(storage)).build()
                }
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "Redis 会话存储初始化失败，回退内存存储");
                    builder.storage(Arc::new(MemoryStorage::new())).build()
                }
                Err(_) => {
                    tracing::error!("Redis 会话存储连接超时，回退内存存储");
                    builder.storage(Arc::new(MemoryStorage::new())).build()
                }
            }
        }
        None => {
            tracing::info!("sa-token 使用内存会话存储（单机模式）");
            builder.storage(Arc::new(MemoryStorage::new())).build()
        }
    }
}

/// 登录：签发 token，返回 token 字符串
pub async fn login(login_id: &str) -> Result<String, String> {
    let token = StpUtil::login(login_id).await.map_err(|e| e.to_string())?;
    Ok(token.to_string())
}

/// 按登录人登出（清空该账号全部会话）
pub async fn logout_by_login_id(login_id: &str) -> Result<(), String> {
    StpUtil::logout_by_login_id(login_id)
        .await
        .map_err(|e| e.to_string())
}

/// 按 token 登出当前会话
pub async fn logout_by_token(token: &str) -> Result<(), String> {
    let tv = TokenValue::from(token.to_string());
    StpUtil::logout_by_token(&tv)
        .await
        .map_err(|e| e.to_string())
}

/// 由 token 取登录人 ID
pub async fn get_login_id(token: &str) -> Option<String> {
    let tv = TokenValue::from(token.to_string());
    StpUtil::get_login_id(&tv).await.ok()
}

/// 是否已登录
pub async fn is_login(token: &str) -> bool {
    let tv = TokenValue::from(token.to_string());
    StpUtil::is_login(&tv).await
}

/// 为登录人设置权限点列表（M4 base 模块从 base_sys_perms 读取后调用）
pub async fn set_permissions(login_id: &str, perms: Vec<String>) {
    let _ = StpUtil::set_permissions(login_id, perms).await;
}

/// 为登录人设置角色列表
pub async fn set_roles(login_id: &str, roles: Vec<String>) {
    let _ = StpUtil::set_roles(login_id, roles).await;
}

// ==================== 密码哈希（bcrypt） ====================
//
// cool-admin-go 使用 MD5 存密码，此处升级为 bcrypt（慢哈希、带盐），
// 避免引入已破解的弱哈希。哈希串形如 `$2b$12$...`，与 MD5 不兼容，
// 因此登录支持「bcrypt 优先、MD5 回退」的迁移窗口。

/// 生成 bcrypt 密码哈希
pub fn hash_password(plain: &str) -> Result<String, String> {
    bcrypt::hash(plain, bcrypt::DEFAULT_COST).map_err(|e| e.to_string())
}

/// 校验密码（自动识别 bcrypt / 裸 MD5 旧数据）
pub fn verify_password(plain: &str, stored: &str) -> bool {
    if stored.starts_with("$2") {
        return bcrypt::verify(plain, stored).unwrap_or(false);
    }
    // 兼容 cool-admin-go 旧数据：MD5 hex
    md5_hex(plain) == stored.to_lowercase()
}

/// MD5 hex（仅用于校验 cool-admin-go 旧数据，不再写入新哈希）
fn md5_hex(s: &str) -> String {
    format!("{:x}", md5::compute(s.as_bytes()))
}

/// 获取登录人权限点
pub async fn get_permissions(login_id: &str) -> Vec<String> {
    StpUtil::get_permissions(login_id).await
}

/// 获取登录人角色
pub async fn get_roles(login_id: &str) -> Vec<String> {
    StpUtil::get_roles(login_id).await
}

/// 校验登录人是否具备某权限点
pub async fn has_permission(login_id: &str, perm: &str) -> bool {
    StpUtil::has_permission(login_id, perm).await
}

/// SaTokenLayer：从请求提取 token 并注入登录人 ID（所有路由最外层）
pub fn token_layer(state: SaTokenState) -> SaTokenLayer {
    SaTokenLayer::new(state)
}

/// 登录校验层（挂在需登录的路由组）
pub fn check_login_layer() -> SaCheckLoginLayer {
    SaCheckLoginLayer::new()
}

/// 权限校验层（挂在需权限点的路由组）
pub fn check_permission_layer(perm: &str) -> SaCheckPermissionLayer {
    SaCheckPermissionLayer::new(perm)
}
