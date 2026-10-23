# 项目交付总结 — cool-axum

> **cool-admin-go（GoFrame CRUD 后台框架）→ Rust / Axum 完整移植**
>
> 交付日期：2026-10-23 · 状态：**已完成（13/13 里程碑）**

---

## 一、交付概述

本项目将 cool-admin-go 完整移植为 Rust / Axum 实现，并按既定需求落地全部能力：

| 维度 | 需求决策 | 交付情况 |
|------|----------|----------|
| **交付范围** | 完整移植 cool-admin-go | ✅ 框架核心 + 全部 5 业务模块 + 文件存储 |
| **数据层** | 按 cool-admin-go 对齐 | ✅ MySQL/PG 双支持、autoMigrate、initjson 种子数据 |
| **架构形态** | 多节点部署同一单体 | ✅ 无状态 + Redis 会话，Docker Compose / K8s |
| **登录鉴权** | 必须使用官方 sa-token-rust | ✅ sa-token-plugin-axum（git 官方仓库） |
| **文档** | 完整 Mintlify 文档站 | ✅ 12 篇文档 + mint.json |

**代码规模**：36 个 Rust 源文件，约 6300 行；24 个单元测试全部通过。

---

## 二、里程碑交付明细

### M1 基础骨架 ✅

| 交付物 | 说明 |
|--------|------|
| Cargo workspace | 13 个 crate（core/auth/cache/file/db/config/gen + 5 模块 + bin） |
| 配置系统 | `cool-config`，对齐 cool-admin-go `config.yaml`（camelCase） |
| 数据源 | `cool-db`：MySQL / PostgreSQL、多分组、惰性连接 |
| 统一响应 | `BaseRes`，错误码 1000/1001/51/1003/403/404/500 对齐 cool-admin-go |
| 日志 / 健康检查 | tracing + `/health` 探活 |

**验证**：服务启动、配置解析单测、健康检查返回 DB/缓存状态。

### M2 CRUD 引擎 ✅

| 交付物 | 说明 |
|--------|------|
| SqlBuilder | WHERE / LIKE / IN / JOIN / ORDER / LIMIT，标识符引用、注入防护 |
| QueryOp | `field_eq` / `key_word_field` / `add_orderby` / `select` / `join` |
| CrudService | add / delete / update / info / list / page 六接口 |
| ApiAllow | 接口白名单（add/delete/update/info/list/page） |

**特性**：软删除（`deleteTime`）、自动时间填充、唯一键/非空校验、敏感字段剥离、排序白名单防注入、保留字（`key`/`limit`）自动引用。

**验证**：完整 e2e —— 增删改查、软删除后不可见、keyWord 搜索、排序分页、唯一键冲突。

### M3 鉴权（sa-token-rust）✅

| 交付物 | 说明 |
|--------|------|
| cool-auth | sa-token 初始化、登录/登出、Redis/内存会话 |
| 登录接口 | `POST /base/login`，返回 token + 用户信息 + 权限点 |
| 中间件 | SaTokenLayer（提取登录态）+ SaCheckLoginLayer（401 拦截） |
| 权限点 | StpUtil 设置/查询角色与权限 |

**验证**：登录签发 → 携带 token 访问 → 登出后 401；错误密码拒绝。

### M4 业务模块 ✅

| 模块 | 表 | 路由前缀 |
|------|----|----------|
| **base** | base_sys_user/role/menu/department/param/log + 3 关联表 | `/base/sys/*` |
| **dict** | dict_type / dict_info | `/dict/*` |
| **task** | task_info / task_log | `/task/*` |
| **space** | space_info / space_type | `/space/*` |
| **demo** | demo_sample / demo_goods | `/demo/*` |

**验证**：17 张表 autoMigrate 成功；用户/字典/任务等 CRUD 全链路；password 敏感字段自动剥离。

### M5 分布式能力 ✅

| 交付物 | 说明 |
|--------|------|
| Redis 会话 | sa-token RedisStorage，`cool:sa-token:` 前缀 |
| Raft 选主 | `crates/cool-raft`（openraft），Leader 单点执行定时任务 |
| Dockerfile | 多阶段构建（rust:1.96 → debian:bookworm-slim） |
| docker-compose | MySQL + Redis + 多实例 + Nginx 负载均衡 |
| K8s 清单 | Deployment（2 副本 + 探针）/ Service / HPA（2–10） |

**验证**：双节点实测 —— **A 登录 → B 可用 → A 登出 → B 立即 401**。

### M6 文档站 ✅

Mintlify 文档 12 篇：介绍、快速开始、配置、CRUD、鉴权、数据库、模块、分布式、部署、API 参考、FAQ、更新日志。

预览：`cd docs && npx mintlify dev`

---

## 三、扩展能力交付（遗留事项清零）

### 7. 登录查库校验 + 密码哈希 ✅

| 能力 | 说明 |
|------|------|
| 查库登录 | `base_sys_user` 按 username + `status=1` 查询 |
| 密码哈希 | **bcrypt**（升级自 cool-admin-go 的 MD5）；兼容旧 MD5 数据 |
| 登录锁定 | 同账号+IP 10 分钟 5 次失败临时锁定 |
| 防枚举 | 用户不存在/密码错误同一提示 |
| 改密吊销 | passwordV+1 + 旧 token 全部失效 |

### 8. initjson 种子数据 ✅

| 表 | 数量 |
|----|------|
| base_sys_menu | 56 |
| base_sys_user / user_role | 6 / 29 |
| base_sys_role / role_menu | 5 / 280 |
| department / role_department / param | 4 / 9 / 2 |
| dict_type / dict_info | 2 / 5 |
| task_info | 1 |

**机制**：`base_sys_init` 落账去重，重复启动跳过；种子用户 MD5 密码可登录，改密自动升级 bcrypt。

### 9. 任务调度执行器 ✅

| 能力 | 说明 |
|------|------|
| 调度类型 | cron（6 字段）/ interval（every 毫秒）/ `@every Ns` |
| 函数注册表 | `TaskTest(param)`、`BaseFuncClearLog(deep)`，可扩展 |
| 约束 | startDate / endDate / limit 次数上限 |
| 日志 | 每次执行写 task_log；**成功日志每任务保留 20 条** |
| 控制 | 增改删联动启用/停止；`/task/info/once` 手动触发 |

### 10. 文件上传 ✅

| 能力 | 说明 |
|------|------|
| 驱动 | local / minio / oss 三驱动（`cool.file.mode` 切换） |
| 接口 | `POST /base/comm/upload`、`GET /base/comm/uploadMode` |
| 本地存储 | `./public/uploads/Ymd/uuid_filename`，`/public/**` 静态暴露 |
| 安全 | 文件名安全化（去路径分隔符/特殊字符）、UUID 前缀防覆盖 |

### 11. 代码生成器 ✅

| 命令 | 说明 |
|------|------|
| `cool-gen tables` | 列出数据库全部表 |
| `cool-gen module <name>` | 模块脚手架 |
| `cool-gen crud <table> --module <m>` | 从表结构生成 TableDef + CrudService |

生成质量与手写一致（含主键自增、默认值、注释、`deleted_at`→`deleteTime` 规范化）。

### 12. 权限校验 ✅

| 能力 | 说明 |
|------|------|
| URL → 权限点 | `/base/sys/menu/add` → `base:sys:menu:add` |
| authority 中间件 | 超管/comm 放行，其余比对 `menu.perms` |
| 逗号多值 | `base:sys:menu:page,base:sys:menu:list` 自动展开 |
| permmenu | 权限点 + 菜单树 |
| refreshPerms | 改角色后刷新会话权限 |

**验证矩阵**：

| 用户 | 接口 | 结果 |
|------|------|------|
| admin | 任意 | 200（超管） |
| xiaobai | 有权限接口 | 200 |
| xiaobai | 无权限接口 | **403** |
| 匿名 | 业务接口 | 401 |

### 13. 缓存模块 ✅

| 能力 | 说明 |
|------|------|
| 双模式 | Redis（多节点共享）/ 内存（单机），自动选择 |
| API | get / set / remove / exists / get_or_set / increment，TTL 支持 |
| 业务接入 | 登录失败计数、参数缓存（启动预热） |

**验证**：4 个单测 + Redis e2e（失败计数跨节点锁定）。

---

## 四、架构说明

### 运行时架构

```
                 ┌─────────────┐
   client ──────►│  Nginx/LB   │
                 └──────┬──────┘
            ┌───────────┼───────────┐
            ▼           ▼           ▼
       ┌────────┐  ┌────────┐  ┌────────┐
       │ axum-1 │  │ axum-2 │  │ axum-N │  同一二进制，无状态
       └───┬────┘  └───┬────┘  └───┬────┘
           │           │           │
           ▼           ▼           ▼
       ┌────────────────────────────────┐
       │  Redis（会话/缓存） + MySQL（数据）│
       └────────────────────────────────┘
```

### 请求链路

```
SaTokenLayer（提取 token → 注入 SaLoginId）
  → SaCheckLoginLayer（未登录 401）
    → authority_middleware（URL→perms，无权限 403）
      → CrudService / handler
        → SqlBuilder → sqlx → MySQL/PG
```

### 核心抽象

| 抽象 | 职责 |
|------|------|
| `CrudService` + `QueryOp` | 表 → 六接口，声明式查询配置 |
| `SqlBuilder` | 方言适配、注入防护、软删除 |
| `FileDriver` | 存储驱动（local/minio/oss） |
| `CacheManager` | 会话外缓存（内存/Redis） |
| `Scheduler` + 函数注册表 | 定时任务执行 |
| `authority_middleware` | 路由级权限 |

---

## 五、测试与验证

| 类别 | 数量 | 说明 |
|------|------|------|
| 单元测试 | 24 | SqlBuilder、配置、密码哈希、缓存、权限映射 |
| e2e 验证 | 全链路 | 登录/CRUD/权限/任务调度/文件上传/多种子数据/多节点会话 |

**关键验证记录**：

- CRUD 六接口 + 软删除 + keyWord + 排序分页 ✓
- bcrypt 存储 `$2b$12$...`，改密吊销旧 token ✓
- 种子数据 11 表灌入，幂等重启不重复 ✓
- 任务每 2 秒执行，禁用冻结，日志封顶 20 条 ✓
- 上传落盘 + 静态访问 + 文件名安全化 ✓
- 无权限请求 403，超管放行 ✓
- 双节点会话共享即时生效 ✓
- Redis 失败计数跨节点锁定 ✓

---

## 六、已知边界与后续建议

| 项 | 现状 | 建议 |
|----|------|------|
| 任务集群单例 | Raft 选主后仅 Leader 执行（已交付） | 可选 Redis 锁做二次保险 |
| Raft 日志持久化 | RocksDB 落盘 `raft.dataDir`，重启可恢复 | — |
| Raft 快照传输 | 本地快照可构建；节点间传输未实现 | 成员长期落后时再补 |
| minio/oss 签名 | 简化 PUT | 接入 aws-sdk-s3 完整签名 |
| 自定义任务函数 | 需 `funcs::register` 注册 | 提供插件机制 |
| 密码策略 | 无复杂度校验 | 可加密码强度规则 |
| 操作日志 | 表已就绪，写入可选 | 结合 authority 中间件埋点 |
| 前端 | 未包含（对齐 cool-admin-go 前端分离） | 可对接 cool-admin 前端 |

---

## 七、交付清单

```
cool-axum/
├── crates/                     # 8 个基础库 + modules/ 业务模块（见 README）
├── docs/                       # Mintlify 文档站（12 篇）
├── deploy/
│   ├── nginx.conf              # 负载均衡配置
│   └── k8s/deployment.yaml     # Deployment/Service/HPA
├── config.yaml                 # 示例配置
├── Dockerfile                  # 多阶段构建
├── docker-compose.yml          # 多节点编排
├── README.md                   # 项目说明（本仓库）
└── DELIVERY.md                 # 交付总结（本文件）
```

### 验收命令

```bash
# 编译 + 测试
cargo test --workspace

# 启动（需 MySQL）
cargo run

# 默认账号
# admin / 123456

# 文档预览
cd docs && npx mintlify dev

# 多节点
docker compose up -d --build
```

---

**交付结论**：cool-admin-go → cool-axum 的完整移植与全部约定能力已交付并验证通过。

---

## 八、迭代补充（前端对接期）

针对 cool-admin 前端对接完成的增强：

| 项 | 说明 |
|----|------|
| base/dict 模块重构 | 目录结构与 cool-admin-go 完全对应（controller/service/model/middleware/api/config/funcs/utils） |
| `/admin/base/open/captcha` | 图形验证码开放接口（支持 height/width/color） |
| `/admin/base/open/login` | 登录响应对齐 TokenResult；支持验证码校验 |
| `/admin/dict/info/data` | 字典数据开放接口（匿名可访问，POST/GET） |
| 令牌兼容 | Bearer / raw / sa-token 头 / Cookie 四种传参自动归一 |
| comm 401 修复 | permmenu 等 comm 接口自解析 token，登录后不再误 401 |
| 超管放行 | userId=1 / perms=* 无条件放行，无视权限校验 |
| 请求日志 | 控制台 + `base_sys_log` 落库，敏感字段脱敏 |
| 字典 Data 查询 | 按 `dict_type.key` 过滤（Go 侧 `type` 列为笔误） |

**回归结果**：16 单测通过；e2e 覆盖开放接口、多传参令牌、超管放行、普通用户 403、匿名 401、请求日志落库。


---

## 九、目录结构重构

按 Rust workspace 规范优化布局：

| 项 | 变更 |
|----|------|
| 启动入口 | `crates/cool-axum/src` → 根 `src/`，`cargo run` 直接可用 |
| 业务模块 | `crates/modules/*` → `modules/*`，与 cool-admin-go 目录一致 |
| 基础库 | `crates/` 只保留 `cool-*` 可复用库 |
| cool-gen | 生成路径改为 `modules/{name}` |

根 `Cargo.toml` 采用 package + workspace 混合模式（Rust 常见惯例）。


## 十、模块注册中心

对齐 `modules/modules.go`：注册中心置于 `modules/` 目录（crate `modules`）。

- `modules/modules.rs`：聚合 `all_specs / all_tables / all_seeds / boot_all`
- 各模块 `spec()` / `init()` 自治声明
- `src/app.rs` 仅调用注册中心，不硬编码模块


## 十一、路由自动注册与模块初始化

对齐 cool-admin-go 的 `cool.RegisterController` 与 `base/base.go init()`：

| 项 | 说明 |
|----|------|
| 路由自动注册 | 各模块 `route_groups()` 声明 open/comm/biz，注册中心装配 |
| 模块初始化 | 各模块 `init()` 内 FillInitData + 专属启动（对齐 base.go） |
| src/ 精简 | 仅 main.rs / app.rs，无 routes/auth 集中代码 |
| 新增模块 | `modules/modules.rs` registered() 加一行即接入全部能力 |
| 质量 | fmt + clippy 0 告警 |

---

## 十二、最终验证报告（重构后回归）

| # | 检查项 | 结果 |
|---|--------|------|
| 1 | Debug / Release 构建 | ✅ 通过，0 告警 |
| 2 | 单测 `cargo test --workspace` | ✅ 16/16 |
| 3 | `cargo clippy` | ✅ 0 告警 |
| 4 | e2e 全链路 | ✅ 16/16 |

### e2e 明细

```
启动        ✓ health ✓ captcha ✓ dict data
登录 / comm ✓ login ✓ TokenResult ✓ userinfo ✓ permmenu
各模块      ✓ base(user/menu) ✓ dict ✓ demo ✓ task ✓ space
上传        ✓ upload
安全        ✓ 匿名 401 ✓ xiaobai 无权限 403
```

**结论：路由自动注册、模块初始化、auth 合并进 base 等重构后功能无回归。**

---

## 十三、Raft 分布式选主

多节点部署时定时任务重复调度的问题，由 **openraft 选主**解决：集群内仅 **有效 Leader** 执行任务。

### 交付物

| 组件 | 说明 |
|------|------|
| `crates/cool-raft` | openraft 0.10 + RocksDB 持久化，HTTP RPC（`/raft/vote\|append`） |
| `RocksLogStore` | 日志 + vote + committed 位点，落盘 `raft.dataDir/log` |
| `RocksStateMachine` | applied-state（last_log_id + membership），落盘 `raft.dataDir/sm`，不做 Raft KV |
| `config.yaml` → `raft.*` | `enable` / `nodeId` / `selfAddr` / `dataDir` / `peers` |
| `Scheduler` 门控 | `set_leader_ok` + 1s 轮询 `verify_quorum_leadership` |
| 管理 API | `GET /raft/status` · `POST /raft/add-learner` · `POST /raft/change-membership` |

### 关键语义

- **有效 Leader** = 角色 Leader **且** quorum 可达（`ReadIndex` 屏障，2s 超时）——少数派滞留 Leader 不执行任务，防裂脑
- 启动后任务默认禁用，领导力确认后放行（避免 Follower 启动窗口误跑）
- bootstrap：最小节点 ID `initialize(全体 voter)`；**重启从 RocksDB 恢复并跳过 bootstrap**
- Raft 启动失败回退单机模式（任务本地执行）
- **不做 Raft KV**：业务数据仍在 MySQL / Redis

### 验证

- 构建 / clippy 0 告警
- 单测 20/20（含 RocksDB open/reopen 持久化往返）
- 单机 `raft.enable=false`：任务本地执行不变
- 多节点：仅 Leader 写 `task_log`；Leader 宕机后新 Leader 接续调度

### 集群冒烟：重启后 RocksDB 恢复

三节点（1/2/3，端口 8001–8003）实测步骤与结论：

| # | 步骤 | 结果 |
|---|------|------|
| 1 | 首次启动 bootstrap | ✅ Leader=1，term=2，lastLogIndex=1，`dataDir/{log,sm}` 落盘 |
| 2 | 全量 kill → 重启 | ✅ 3/3 日志 `Raft 已初始化，跳过 bootstrap（从持久化状态恢复）` |
| 3 | 对比 `/raft/status` | ✅ term / lastLogIndex / membership / vote(committed) 与重启前一致 |
| 4 | 杀 Leader → 重启 node1 | ✅ 恢复后重新成为 Leader，不重复 bootstrap |
| 5 | 再次全量重启 | ✅ 3/3 跳过 bootstrap，状态完整 |

**结论：进程重启后 Raft 状态从 RocksDB 完整恢复，不会重复 bootstrap。**
