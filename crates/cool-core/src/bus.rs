//! 分布式全局事件总线（对齐 cool-admin-midway 事件框架 + `DistributedEventBus`）
//!
//! 参考实现：
//! - `@CoolEvent()` / `@Event(name)` 装饰器 + `CoolEventManager`（`emit` 本进程 / `globalEmit` PM2 IPC）
//! - 本项目的 `DistributedEventBus`（Redis Pub/Sub 真·跨进程跨机器）
//!
//! Rust 侧无装饰器元数据，改用**注册式 API**（等价模块 `init()` 里 `@Event` 绑定方法）：
//! - `EventBus::on(name, handler)` 注册处理器（对应 `@Event(EVENT.X)` 方法）
//! - `emit(name, data)` 仅本进程分发
//! - `global_emit(name, data, mode)` 经 Redis Pub/Sub 投递：`Broadcast` 广播给集群所有实例、
//!   `Random` 抢占投递给其中一个实例（`SET NX` 保证精确一次）
//! - 无 Redis 时降级为进程内模式（`global_emit` 回退 `emit`）
//!
//! 用法（对应 recycle 的 `@Event(EVENT.SOFT_DELETE)` softDelete 处理器）：
//! ```ignore
//! // 模块 init 时注册
//! bus.on(Event::SoftDelete, {
//!     let svc = recycle_svc.clone();
//!     Arc::new(move |data| {
//!         let svc = svc.clone();
//!         Box::pin(async move {
//!             if let Ok(rec) = serde_json::from_value::<SoftDeleteRecord>(data) {
//!                 svc.record(rec).await;
//!             }
//!         })
//!     })
//! });
//! // 触发（本进程）
//! bus.emit(Event::SoftDelete, data).await;
//! // 触发（集群所有实例）
//! bus.global_emit(Event::SoftDelete, data, DeliverMode::Broadcast).await;
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 事件负载（JSON，可跨机器序列化）—— 对齐 TS 的 `...args`
pub type EventData = Value;

/// boxed future（handler 返回类型，保证 object-safe）
pub type BoxFut = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// 事件处理器：`Fn(EventData) -> BoxFut`（等价 `@Event` 标注的方法）
///
/// 注册时用闭包捕获需要的 service（`Arc`），内部 `Box::pin(async move { ... })`。
pub type Handler = Arc<dyn Fn(EventData) -> BoxFut + Send + Sync + 'static>;

/// 系统默认事件（字符串值须与 cool-admin-midway `EVENT` 逐字一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// 数据被软删除
    SoftDelete,
    /// 服务成功启动
    ServerReady,
    /// 服务就绪
    Ready,
    /// ES 数据变更
    EsDataChange,
}

impl Event {
    /// 事件名字符串（与 TS `EVENT` 逐字一致）
    pub fn as_str(&self) -> &'static str {
        match self {
            Event::SoftDelete => "onSoftDelete",
            Event::ServerReady => "onServerReady",
            Event::Ready => "onReady",
            Event::EsDataChange => "esDataChange",
        }
    }
}

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Event {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// 投递模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliverMode {
    /// 广播给集群所有实例（含自身）
    Broadcast,
    /// 随机投递给集群中的一个实例（`SET NX` 抢占保证精确一次）
    Random,
}

/// 事件消息信封（跨进程/跨机器传输的统一结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// 事件名
    pub event: String,
    /// 负载
    pub data: EventData,
    /// 投递模式
    pub mode: DeliverMode,
    /// 消息唯一 id（random 模式抢占去重）
    pub msg_id: String,
    /// 发送方实例 id（broadcast 跳过自身避免重复执行）
    pub from: String,
    /// 发送时间戳（毫秒）
    pub ts: i64,
}

/// 事件总线配置
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// 是否开启分布式投递；关闭后仅进程内生效
    pub enabled: bool,
    /// Redis 连接地址（`redis://[:pass@]host:port/db`）；`None` → 进程内模式
    pub redis_url: Option<String>,
    /// 项目标识，用于频道隔离（多项目共用 Redis 不串扰）
    pub keys: String,
    /// 频道前缀，最终频道为 `{channel_prefix}:{keys}`
    pub channel_prefix: String,
    /// random 模式抢占锁过期时间（秒）
    pub claim_ttl_secs: u64,
    /// Redis 不可用时是否回退进程内广播
    pub fallback_local: bool,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            redis_url: None,
            keys: "cool".into(),
            channel_prefix: "cool:event:bus".into(),
            claim_ttl_secs: 30,
            fallback_local: true,
        }
    }
}

impl EventBusConfig {
    /// 便捷构造：给定 redis url（可空）与项目标识
    pub fn new(redis_url: Option<String>, keys: impl Into<String>) -> Self {
        Self {
            redis_url,
            keys: keys.into(),
            ..Default::default()
        }
    }
}

type Publisher = redis::aio::ConnectionManager;

struct EventBusInner {
    instance_id: String,
    channel: String,
    config: EventBusConfig,
    handlers: RwLock<HashMap<String, Vec<Handler>>>,
    /// Pub/Sub 发布连接；`local_only` 时为 `None`
    publisher: Option<Publisher>,
    /// 是否已降级为进程内模式
    local_only: bool,
}

/// 分布式全局事件总线
///
/// 内部用 `Arc` 共享（同 `CacheManager`），可 `Clone` 后放入 `AppState` / 传入模块。
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

impl EventBus {
    /// 构造事件总线（尝试连接 Redis，失败降级进程内）；不启动订阅监听
    pub async fn new(config: EventBusConfig) -> Self {
        let instance_id = format!("inst-{}-{}", std::process::id(), uuid::Uuid::new_v4());
        let channel = format!("{}:{}", config.channel_prefix, config.keys);

        // 未启用 / 无 Redis / 配置关闭 → 进程内模式
        let mut local_only = !config.enabled;
        let mut publisher: Option<Publisher> = None;

        if !local_only {
            match config.redis_url.as_deref() {
                None => {
                    local_only = true;
                    tracing::info!("EventBus 无 Redis 配置，降级为进程内模式（global_emit=emit）");
                }
                Some(url) => match Self::connect_publisher(url).await {
                    Ok(p) => {
                        publisher = Some(p);
                        tracing::info!(channel, instance_id, "EventBus 已连接 Redis（分布式投递）");
                    }
                    Err(e) => {
                        local_only = true;
                        tracing::error!(
                            error = %e,
                            "EventBus Redis 连接失败，降级为进程内模式"
                        );
                    }
                },
            }
        }

        Self {
            inner: Arc::new(EventBusInner {
                instance_id,
                channel,
                config,
                handlers: RwLock::new(HashMap::new()),
                publisher,
                local_only,
            }),
        }
    }

    async fn connect_publisher(url: &str) -> redis::RedisResult<Publisher> {
        let client = redis::Client::open(url)?;
        client.get_connection_manager().await
    }

    /// 注册事件处理器（等价 `@Event(name)` 绑定方法）
    ///
    /// 同一事件可注册多个处理器，分发时按注册顺序依次执行。
    pub fn on<F>(&self, event: impl AsRef<str>, handler: F)
    where
        F: Fn(EventData) -> BoxFut + Send + Sync + 'static,
    {
        self.on_handler(event, Arc::new(handler));
    }

    /// 注册已构建好的 `Handler`（`Arc` 包装，便于组合/复用）
    pub fn on_handler(&self, event: impl AsRef<str>, handler: Handler) {
        self.inner
            .handlers
            .write()
            .unwrap()
            .entry(event.as_ref().to_string())
            .or_default()
            .push(handler);
    }

    /// 发送事件，仅当前进程生效
    pub async fn emit(&self, event: impl AsRef<str>, data: impl Into<EventData>) {
        self.dispatch(event.as_ref(), &data.into()).await;
    }

    /// 发送全局事件，跨进程、跨机器生效
    ///
    /// - `DeliverMode::Broadcast`：本实例立即执行，其余实例经 Redis 收到后执行（跳过自身）
    /// - `DeliverMode::Random`  ：不预执行，所有实例（含自身）收到后抢占，保证只执行一次
    ///
    /// 降级（无 Redis）时回退为本进程 `emit`。
    pub async fn global_emit(
        &self,
        event: impl AsRef<str>,
        data: impl Into<EventData>,
        mode: DeliverMode,
    ) {
        let event = event.as_ref().to_string();
        let data = data.into();

        // 降级：无 Redis 时退化为进程内
        let Some(publisher) = self.inner.publisher.clone() else {
            if self.allow_local_fallback() {
                self.dispatch(&event, &data).await;
            }
            return;
        };

        let envelope = Envelope {
            event: event.clone(),
            data: data.clone(),
            mode,
            msg_id: uuid::Uuid::new_v4().to_string(),
            from: self.inner.instance_id.clone(),
            ts: chrono::Utc::now().timestamp_millis(),
        };

        // broadcast：本实例立即执行（其余实例经 Redis 收到后跳过自身）
        // random  ：不预执行，所有实例抢占后执行一次
        if mode == DeliverMode::Broadcast {
            self.dispatch(&event, &data).await;
        }

        match Self::publish(&publisher, &self.inner.channel, &envelope).await {
            Ok(()) => {}
            Err(e) => {
                tracing::error!(error = %e, event, "EventBus publish 失败");
                // 兜底：broadcast 已本地执行过（避免重复）；random 未执行过，本地兜底一次
                if mode == DeliverMode::Random && self.allow_local_fallback() {
                    self.dispatch(&event, &data).await;
                }
            }
        }
    }

    async fn publish(
        publisher: &Publisher,
        channel: &str,
        envelope: &Envelope,
    ) -> redis::RedisResult<()> {
        let payload = serde_json::to_string(envelope)
            .map_err(|e| redis::RedisError::from(std::io::Error::other(e.to_string())))?;
        let mut conn = publisher.clone();
        redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .query_async::<()>(&mut conn)
            .await
    }

    fn allow_local_fallback(&self) -> bool {
        self.inner.config.fallback_local
    }

    /// 执行事件：按注册顺序调用所有处理器（逐个 await，保证顺序与完成）
    pub async fn dispatch(&self, event: &str, data: &EventData) {
        let handlers: Vec<Handler> = {
            let map = self.inner.handlers.read().unwrap();
            map.get(event).cloned().unwrap_or_default()
        };
        for h in handlers {
            let fut = h(data.clone());
            fut.await;
        }
    }

    /// 启动 Redis 订阅监听（进程内模式下为空操作）
    ///
    /// 监听任务持有本总线的克隆用于分发；任务常驻，随进程退出结束。
    /// 建议在处理器注册完成后再调用，避免漏掉早到消息。
    pub fn start_listener(&self) {
        if self.inner.local_only || self.inner.publisher.is_none() {
            return;
        }
        let bus = self.clone();
        let url = match self.inner.config.redis_url.clone() {
            Some(u) => u,
            None => return,
        };
        let channel = self.inner.channel.clone();

        tokio::spawn(async move {
            // 独立订阅连接（PubSub 与发布连接分离）
            loop {
                match Self::run_subscriber(&url, &channel, &bus).await {
                    Ok(()) => {
                        tracing::warn!("EventBus 订阅流结束，尝试重连");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "EventBus 订阅异常，2s 后重连");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    async fn run_subscriber(url: &str, channel: &str, bus: &EventBus) -> redis::RedisResult<()> {
        use futures_util::StreamExt;

        let client = redis::Client::open(url)?;
        let mut pubsub = client.get_async_pubsub().await?;
        pubsub.subscribe(channel).await?;

        let mut stream = pubsub.into_on_message();
        while let Some(msg) = stream.next().await {
            let payload = msg.get_payload_bytes();
            if let Ok(env) = serde_json::from_slice::<Envelope>(payload) {
                bus.handle_envelope(env).await;
            }
        }
        Ok(())
    }

    /// 处理一条事件消息（broadcast 跳过自身 / random 抢占）
    pub(crate) async fn handle_envelope(&self, env: Envelope) {
        match env.mode {
            DeliverMode::Random => {
                if self.claim(&env.msg_id).await {
                    self.dispatch(&env.event, &env.data).await;
                }
            }
            DeliverMode::Broadcast => {
                if env.from != self.inner.instance_id {
                    self.dispatch(&env.event, &env.data).await;
                }
            }
        }
    }

    /// random 模式下用 `SET NX` 抢占，保证集群内只执行一次
    async fn claim(&self, msg_id: &str) -> bool {
        // 进程内模式无抢占方，直接放行（单实例 = 抢占即得）
        let Some(publisher) = self.inner.publisher.clone() else {
            return true;
        };
        let key = format!("cool:event:claim:{}:{}", self.inner.config.keys, msg_id);
        let ttl = self.inner.config.claim_ttl_secs.max(1);
        let mut conn = publisher.clone();
        let res: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(&self.inner.instance_id)
            .arg("EX")
            .arg(ttl)
            .arg("NX")
            .query_async(&mut conn)
            .await
            .unwrap_or(None);
        res.as_deref() == Some("OK")
    }

    /// 本实例 id
    pub fn instance_id(&self) -> &str {
        &self.inner.instance_id
    }

    /// 最终频道名
    pub fn channel(&self) -> &str {
        &self.inner.channel
    }

    /// 是否已降级为进程内模式
    pub fn is_local_only(&self) -> bool {
        self.inner.local_only
    }

    /// 某事件已注册的处理器数量（测试/诊断用）
    pub fn handler_count(&self, event: impl AsRef<str>) -> usize {
        self.inner
            .handlers
            .read()
            .unwrap()
            .get(event.as_ref())
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

// ── 全局单例与便捷 API ─────────────────────────────────────
//
// 用法（对应 cool.event.emit / global_emit / on）：
// ```ignore
// use cool_core::event;
//
// event::on("order:created", |data| Box::pin(async move { /* … */ }));
// event::emit("order:created", json!({ "id": 1 })).await;
// event::global_emit("order:created", json!({ "id": 1 }), DeliverMode::Broadcast).await;
// ```

static GLOBAL_BUS: RwLock<Option<EventBus>> = RwLock::new(None);
/// 总线尚未初始化时缓存的处理器，`init_global` 时冲刷（保证注册顺序无关）
static PENDING: RwLock<Vec<(String, Handler)>> = RwLock::new(Vec::new());

/// 初始化并设置全局事件总线（app 启动时调用）
///
/// 会把此前 `on` 缓存的处理器冲刷进总线。
pub fn init_global(bus: EventBus) {
    if let Ok(mut pending) = PENDING.write() {
        for (event, handler) in pending.drain(..) {
            bus.on_handler(event, handler);
        }
    }
    if let Ok(mut g) = GLOBAL_BUS.write() {
        *g = Some(bus);
    }
}

/// 获取全局事件总线（未初始化则 `None`）
pub fn global() -> Option<EventBus> {
    GLOBAL_BUS.read().ok().and_then(|g| g.clone())
}

/// 注册事件处理器（`cool.event.on`）
///
/// 处理器 = `Fn(EventData) -> BoxFut`。总线未初始化时先缓存，初始化后自动生效。
pub fn on<F>(event: impl AsRef<str>, handler: F)
where
    F: Fn(EventData) -> BoxFut + Send + Sync + 'static,
{
    on_handler(event, Arc::new(handler));
}

/// 注册已构建好的 `Handler`（`cool.event.on`）
pub fn on_handler(event: impl AsRef<str>, handler: Handler) {
    match global() {
        Some(bus) => bus.on_handler(event, handler),
        None => {
            if let Ok(mut p) = PENDING.write() {
                p.push((event.as_ref().to_string(), handler));
            }
        }
    }
}

/// 发送事件，仅当前进程生效（`cool.event.emit`）
pub async fn emit(event: impl AsRef<str>, data: impl Into<EventData>) {
    if let Some(bus) = global() {
        bus.emit(event, data).await;
    }
}

/// 发送全局事件，跨进程、跨机器生效（`cool.event.global_emit`）
///
/// `mode`：`DeliverMode::Broadcast` 广播给所有实例 / `DeliverMode::Random` 随机一个（精确一次）。
pub async fn global_emit(event: impl AsRef<str>, data: impl Into<EventData>, mode: DeliverMode) {
    if let Some(bus) = global() {
        bus.global_emit(event, data, mode).await;
    }
}

/// 是否已降级为进程内模式
pub fn is_local_only() -> bool {
    global().map(|b| b.is_local_only()).unwrap_or(true)
}

/// 本实例 id
pub fn instance_id() -> String {
    global()
        .map(|b| b.instance_id().to_string())
        .unwrap_or_default()
}

// ── 便捷命名空间：`event.on` / `event.emit` / `event.global_emit` ─────────
//
// 用法（对应 cool.event.on / emit / global_emit）：
// ```ignore
// use cool_core::event;
//
// event::on("order:created", |data| Box::pin(async move { /* … */ }));
// // 或拿到实例后：
// cool_core::event.on("order:created", handler);
// cool_core::event.emit("order:created", json!({ "id": 1 })).await;
// cool_core::event.global_emit("order:created", data, DeliverMode::Broadcast).await;
// ```

/// 事件命名空间（方法式调用 `event.on` / `event.emit` / `event.global_emit`）
#[derive(Debug, Clone, Copy, Default)]
pub struct EventApi;

impl EventApi {
    /// 注册处理器（同 [`on`]）
    pub fn on<F>(&self, event: impl AsRef<str>, handler: F)
    where
        F: Fn(EventData) -> BoxFut + Send + Sync + 'static,
    {
        on(event, handler);
    }

    /// 注册已构建好的 `Handler`（同 [`on_handler`]）
    pub fn on_handler(&self, event: impl AsRef<str>, handler: Handler) {
        on_handler(event, handler);
    }

    /// 仅本进程分发（同 [`emit`]）
    pub async fn emit(&self, event: impl AsRef<str>, data: impl Into<EventData>) {
        emit(event, data).await;
    }

    /// 分布式投递（同 [`global_emit`]）
    pub async fn global_emit(
        &self,
        event: impl AsRef<str>,
        data: impl Into<EventData>,
        mode: DeliverMode,
    ) {
        global_emit(event, data, mode).await;
    }

    /// 是否已降级为进程内模式
    pub fn is_local_only(&self) -> bool {
        is_local_only()
    }

    /// 本实例 id
    pub fn instance_id(&self) -> String {
        instance_id()
    }

    /// 取全局总线（进阶：`start_listener` 等）
    pub fn global(&self) -> Option<EventBus> {
        global()
    }

    /// 初始化全局总线（框架启动时调用）
    pub fn init_global(&self, bus: EventBus) {
        init_global(bus);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn local_config() -> EventBusConfig {
        EventBusConfig {
            enabled: true,
            redis_url: None,
            keys: "test".into(),
            ..Default::default()
        }
    }

    async fn local_bus() -> EventBus {
        EventBus::new(local_config()).await
    }

    fn counter(
        count: &'static AtomicUsize,
    ) -> impl Fn(EventData) -> BoxFut + Send + Sync + 'static {
        move |_data: EventData| {
            let count = count;
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    #[test]
    fn event_str_values_match_ts() {
        assert_eq!(Event::SoftDelete.as_str(), "onSoftDelete");
        assert_eq!(Event::ServerReady.as_str(), "onServerReady");
        assert_eq!(Event::Ready.as_str(), "onReady");
        assert_eq!(Event::EsDataChange.as_str(), "esDataChange");
    }

    #[tokio::test]
    async fn local_emit_dispatches_to_handler() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let bus = local_bus().await;
        assert!(bus.is_local_only());
        bus.on("demo:global", counter(&COUNT));

        bus.emit("demo:global", serde_json::json!({"a": 2})).await;
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn multiple_handlers_fire_in_order() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let bus = local_bus().await;
        bus.on(Event::SoftDelete, counter(&COUNT));
        bus.on(Event::SoftDelete, counter(&COUNT));

        assert_eq!(bus.handler_count(Event::SoftDelete), 2);
        bus.emit(Event::SoftDelete, serde_json::json!({})).await;
        assert_eq!(COUNT.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn broadcast_skips_self_but_handles_other() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let bus = local_bus().await;
        bus.on("demo:global", counter(&COUNT));

        // 自身广播（已本地执行过）→ 跳过
        bus.handle_envelope(Envelope {
            event: "demo:global".into(),
            data: serde_json::json!({}),
            mode: DeliverMode::Broadcast,
            msg_id: "m1".into(),
            from: bus.instance_id().to_string(),
            ts: 0,
        })
        .await;
        assert_eq!(COUNT.load(Ordering::SeqCst), 0);

        // 其他实例广播 → 执行
        bus.handle_envelope(Envelope {
            event: "demo:global".into(),
            data: serde_json::json!({}),
            mode: DeliverMode::Broadcast,
            msg_id: "m2".into(),
            from: "other-instance".into(),
            ts: 0,
        })
        .await;
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn random_handles_once_in_local_mode() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let bus = local_bus().await;
        bus.on("demo:random", counter(&COUNT));

        // 进程内模式 claim 直接放行 → 执行一次
        bus.handle_envelope(Envelope {
            event: "demo:random".into(),
            data: serde_json::json!({}),
            mode: DeliverMode::Random,
            msg_id: "r1".into(),
            from: "other-instance".into(),
            ts: 0,
        })
        .await;
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn envelope_serializes_roundtrip() {
        let env = Envelope {
            event: "onSoftDelete".into(),
            data: serde_json::json!({"table": "demo_item"}),
            mode: DeliverMode::Broadcast,
            msg_id: "mid".into(),
            from: "inst-1".into(),
            ts: 123,
        };
        let s = serde_json::to_string(&env).unwrap();
        let back: Envelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back.event, "onSoftDelete");
        assert_eq!(back.mode, DeliverMode::Broadcast);
        assert_eq!(back.data["table"], "demo_item");
    }

    #[tokio::test]
    async fn global_emit_falls_back_to_local_without_redis() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let bus = local_bus().await;
        bus.on("demo:global", counter(&COUNT));

        // 无 Redis → global_emit 回退本地分发一次
        bus.global_emit("demo:global", serde_json::json!({}), DeliverMode::Broadcast)
            .await;
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }
}
