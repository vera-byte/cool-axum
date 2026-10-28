use async_trait::async_trait;
use sa_token_core::SaTokenListener;

struct AuditListener;

// 实现 SaTokenListener，在登录、登出、踢人等生命周期上挂审计、统计或缓存清理。钩子都有默认空实现，按需覆盖即可。

// 何时使用
// 写登录审计日志、刷新在线人数。
// 踢人 / 顶号后通知客户端。
// 权限变更后清本地缓存。

#[async_trait]
impl SaTokenListener for AuditListener {
    async fn on_login(&self, login_id: &str, token: &str, login_type: &str) {
        let _ = (login_id, token, login_type);
    }
    async fn on_logout(&self, login_id: &str, token: &str, login_type: &str) {
        let _ = (login_id, token, login_type);
    }
    async fn on_kick_out(&self, login_id: &str, token: &str, login_type: &str) {
        let _ = (login_id, token, login_type);
    }
    async fn on_replaced(&self, login_id: &str, token: &str, login_type: &str) {
        let _ = (login_id, token, login_type);
    }
    async fn on_renew_timeout(
        &self,
        login_id: &str,
        token: &str,
        login_type: &str,
        timeout_seconds: i64,
    ) {
        let _ = (login_id, token, login_type, timeout_seconds);
    }
    async fn on_banned(&self, login_id: &str, login_type: &str) {
        let _ = (login_id, login_type);
    }
    async fn on_unbanned(&self, login_id: &str, service: &str, login_type: &str) {
        let _ = (login_id, service, login_type);
    }
    async fn on_open_safe(&self, token: &str, service: &str) {
        let _ = (token, service);
    }
    async fn on_close_safe(&self, token: &str, service: &str) {
        let _ = (token, service);
    }
    async fn on_safe_verify(&self, token: &str, service: &str) {
        let _ = (token, service);
    }
    async fn on_grant_changed(&self, login_id: &str, login_type: &str) {
        let _ = (login_id, login_type);
    }
    async fn on_event(&self, event: &sa_token_core::SaTokenEvent) {
        let _ = event; // 每个事件都会再走一遍
    }
}
