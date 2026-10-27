# cool-axum

> **cool-admin-go 的 Rust / Axum 完整移植** —— 通用 CRUD 后台管理框架

[![Rust](https://img.shields.io/badge/Rust-1.85+-orange)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/Axum-0.8-blue)](https://github.com/tokio-rs/axum)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

cool-axum 将 [cool-admin-go](https://github.com/cool-team-official/cool-admin-go)（GoFrame CRUD 后台框架）移植为 Rust / Axum 实现，提供开箱即用的后台管理服务端：**通用 CRUD 引擎、登录鉴权、权限校验、定时任务、文件上传、代码生成器**，支持多节点水平扩展。

```bash
git clone <repo> && cd cool-axum
cargo run
# 打开 http://127.0.0.1:8001 ，默认账号 admin / 123456
```

---

## 特性

| 能力 | 说明 |
|------|------|
| **通用 CRUD 引擎** | 定义表结构即得 `add/delete/update/info/list/page` 六接口，软删除、QueryOp 查询、敏感字段过滤 |
| **登录鉴权** | 官方 [sa-token-rust](https://github.com/sa-tokens/sa-token-rust)，bcrypt 密码、登录锁定、passwordV 吊销 |
| **权限校验** | URL → 权限点自动映射，`base_sys_menu.perms` 比对，超管/comm 放行 |
| **业务模块** | base（用户/角色/菜单/部门/参数/日志）、dict、task、space、demo |
| **定时任务** | cron / interval 双模式，函数注册表，执行日志自动保留 |
| **文件上传** | local / minio / oss 三驱动，按日分目录，文件名安全化 |
| **请求日志** | 每请求控制台输出 + `base_sys_log` 落库（敏感字段脱敏） |
| **链路追踪** | `X-Trace-Id` 含节点标签，`server.nodeLabel` 可定制 |
| **多租户** | `tenantId` 列级数据隔离，`cool.tenant` 配置开关 |
| **数据回收站** | 软删除进 `recycle_data`，可恢复 / 清理 |
| **缓存** | 内存 / Redis 双模式，登录失败计数、参数缓存 |
| **多节点部署** | 无状态应用 + Redis 共享会话，Docker Compose / K8s 水平扩展 |
| **Raft 选主** | openraft 集群选主，仅 Leader 执行定时任务，防重复调度 / 裂脑 |
| **分布式事件** | Redis Pub/Sub 事件总线，`global_emit` 广播 / 随机（精确一次）跨机器投递 |
| **代码生成器** | `cool-gen` 从数据库表一键生成 CRUD 模块 |
| **文档站** | 完整 Mintlify 文档（`docs/`） |

---

## 技术栈

| 组件 | 选型 |
|------|------|
| Web 框架 | [axum](https://github.com/tokio-rs/axum) 0.8 |
| 鉴权 | [sa-token-rust](https://github.com/sa-tokens/sa-token-rust) / sa-token-plugin-axum |
| 数据库 | [sqlx](https://github.com/launchbadge/sqlx)（MySQL / PostgreSQL） |
| 缓存 / 会话 | Redis |
| 任务调度 | cron 表达式解析 + tokio 循环 |
| 分布式选主 | [openraft](https://docs.rs/openraft)（Leader 执行定时任务） |
| 密码哈希 | bcrypt（兼容旧 MD5） |
| 文档 | Mintlify |

---

## 快速开始

### 环境要求

- Rust 1.85+（edition 2024）
- MySQL 8.x 或 PostgreSQL 14+
- Redis 7+（多节点部署必须；单机开发可省略）

### 配置

编辑 `config.yaml`：

```yaml
server:
  address: ":8001"

database:
  default:
    type: "mysql"
    host: "127.0.0.1"
    port: "3306"
    user: "root"
    pass: "123456"
    name: "cooltest"

redis:
  cool:
    address: "127.0.0.1:6379"
    db: 0

cool:
  autoMigrate: true   # 启动自动建表 + 灌入种子数据
```

### 运行

```bash
cargo run
# 或指定配置
COOL_CONFIG=config.yaml cargo run
```

首次启动自动：建 18 张表 → 灌入 initjson 种子数据 → 初始化管理员。

### 验证

```bash
# 健康检查
curl http://127.0.0.1:8001/health

# 登录
TOKEN=$(curl -s -X POST http://127.0.0.1:8001/admin/base/open/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"123456"}' | jq -r .data.token)

# 调用 CRUD
curl -X POST http://127.0.0.1:8001/admin/base/sys/user/page \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"page":1,"size":10}'
```

---

## 项目结构

```
cool-axum/
├── Cargo.toml             # package（bin）+ workspace
├── src/                   # 启动入口（cargo run 直接可用）
│   ├── main.rs
│   └── app.rs             # 装配 + health/index
├── modules/               # 业务模块（对齐 cool-admin-go/modules）
│   ├── modules.rs         # 模块注册中心（对齐 modules/modules.go）
│   ├── base/              # 用户/角色/菜单/部门/参数/日志/上传/权限/登录
│   ├── dict/              # 数据字典
│   ├── task/              # 定时任务 + 调度器
│   ├── space/             # 文件空间
│   └── demo/              # 示例模块
├── crates/                # 基础设施库
│   ├── cool-config/       # 配置加载（对齐 cool-admin-go config.yaml）
│   ├── cool-db/           # 数据源、autoMigrate、种子填充
│   ├── cool-core/         # CRUD 引擎：SqlBuilder / QueryOp / CrudService
│   ├── cool-auth/         # sa-token 封装：登录、权限、密码哈希
│   ├── cool-cache/        # 缓存：内存 / Redis 双模式
│   ├── cool-file/         # 文件上传：local / minio / oss
│   ├── cool-raft/         # openraft 选主 + RocksDB 持久化 + HTTP RPC
│   └── cool-gen/          # 代码生成器 CLI
├── docs/                  # Mintlify 文档站
├── deploy/                # nginx.conf / K8s 清单
├── config.yaml            # 配置文件
├── Dockerfile
└── docker-compose.yml     # 多节点部署
```

---

## 核心用法

### 通用 CRUD

```rust
use cool_core::{CrudService, QueryOp, controller::{crud_router, ApiAllow}};

let svc = CrudService::new("demo_sample")
    .unique_key("code", "编码已存在")
    .hidden_fields(&["password"])
    .page_op(QueryOp::new()
        .field_eq(&["name"])
        .key_word_field(&["name", "code"])
        .add_orderby("id", "desc"));

let router = crud_router("/demo/sample", svc, ApiAllow::default());
```

自动生成 6 个接口：`add / delete / update / info / list / page`。

### 登录鉴权

```bash
POST /admin/base/open/login     # 登录（bcrypt 校验 + 失败锁定）
POST /admin/base/open/logout    # 登出
GET  /admin/base/comm/userinfo  # 当前用户
GET  /base/comm/permmenu  # 权限点 + 菜单树
```

改密自动 `passwordV+1` 并吊销全部旧会话。

### 权限校验

URL 自动映射权限点：

```
/admin/base/sys/menu/add  →  base:sys:menu:add
```

与 `base_sys_menu.perms` 比对，超管（`userId=1` 或 perms 含 `*`）放行。

### 定时任务

```bash
POST /task/info/add
{"name":"心跳","taskType":1,"every":2000,"service":"TaskTest(hello)","status":1}
```

支持 cron（6 字段）/ interval（毫秒）、startDate/endDate/limit、执行日志每任务保留 20 条。

多节点下开启 Raft 选主可保证**仅 Leader 执行任务**：

```yaml
raft:
  enable: true
  nodeId: 1
  selfAddr: "http://127.0.0.1:8001"
  dataDir: "./data/raft"
  peers:
    - "1:http://127.0.0.1:8001"
    - "2:http://127.0.0.1:8002"
    - "3:http://127.0.0.1:8003"
```

Raft 状态用 RocksDB 持久化到 `dataDir`（重启可恢复）。`GET /raft/status` 查看角色与 Leader；详见 [分布式架构](docs/distributed.mdx)。

### 多租户（可选）

列级数据隔离，对齐 cool-admin-midway v8.0：

```yaml
cool:
  tenant:
    enable: true
    urls: ["/admin/**", "/demo/**"]
```

- CRUD 自动按登录用户的 `tenantId` 过滤 / 写入
- 业务代码零修改；`no_tenant()` 可临时关闭隔离
- 详见 [docs/tenant.mdx](docs/tenant.mdx)

### 文件上传

```bash
curl -X POST http://127.0.0.1:8001/base/comm/upload \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@photo.png"
# → {"code":1000,"data":{"url":"http://.../public/uploads/20261023/<uuid>_photo.png","mode":"local"}}
```

### 代码生成器

详见 [docs/codegen.mdx](docs/codegen.mdx)。

```bash
cargo run -p cool-gen -- tables                            # 列出全部表
cargo run -p cool-gen -- module report                     # 创建模块脚手架
cargo run -p cool-gen -- crud demo_goods --module demo     # 从表生成 CRUD
```

---

## 部署

### Docker Compose（多实例）

```bash
docker compose up -d --build
docker compose up -d --scale cool-axum=3
```

组成：MySQL + Redis + N 个应用实例 + Nginx 负载均衡。

### Kubernetes

```bash
kubectl apply -f deploy/k8s/
```

含 Deployment（2 副本 + 探针）、Service、HPA（CPU 70% 扩容 2–10）。

多节点共享 Redis 会话，已验证：**节点 A 登录 → 节点 B 可用 → A 登出 → B 立即 401**。

多节点定时任务请开启 `raft.enable`（见「定时任务」），由 Leader 单点调度。

---

## 文档

```bash
cd docs && npx mintlify dev
```

- 介绍 / 快速开始 / 配置
- CRUD / 鉴权 / 数据库 / 业务模块
- 分布式 / 部署
- API 参考 / FAQ / 更新日志

---

## 测试

```bash
cargo test --workspace   # 16 个单测
cargo clippy --workspace --all-targets   # 0 告警
```

单测覆盖 SQL 构建器、配置解析、密码哈希、缓存、权限映射等。

**最近回归验证**（重构后）：构建 0 告警 · 单测 16/16 · clippy 0 告警 · e2e 16/16
（启动 / 开放接口 / 登录 / comm / 五模块 CRUD / 上传 / 权限隔离）

---

## 与 cool-admin-go 的对应关系

| cool-admin-go | cool-axum |
|---------------|-----------|
| `cool/`（Model/Service/Controller） | `crates/cool-core` |
| `cool/coolconfig` | `crates/cool-config` |
| `cool/cooldb` / `initdb` | `crates/cool-db` |
| `cool/coolfile` + `contrib/files` | `crates/cool-file` |
| `cool.CacheManager` | `crates/cool-cache` |
| JWT 登录 | sa-token-rust |
| `modules/base` | `modules/base` |
| `modules/dict` | `modules/dict` |
| `modules/task` | `modules/task` |
| `modules/space` | `modules/space` |
| `modules/demo` | `modules/demo` |
| `cool-tools` | `crates/cool-gen` |
| `docs/`（vuepress） | `docs/`（Mintlify） |

详见 [DELIVERY.md](DELIVERY.md) 交付总结。

---

## License

MIT
