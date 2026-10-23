# mod-base

对齐 cool-admin-go `modules/base`：系统管理模块（用户/角色/菜单/部门/参数/日志/上传/权限）。

## 目录结构

与 Go 侧一一对应：

```text
base/
├── api/                    # 接口请求类型（对齐 api/v1）
│   ├── mod.rs
│   └── v1.rs               # BaseOpenLoginReq / BaseOpenCaptchaReq
├── config/                 # 模块配置（对齐 config/config.go）
│   └── mod.rs
├── controller/             # 控制器（对齐 controller/）
│   ├── mod.rs
│   ├── admin/              # 后台管理接口
│   │   ├── base_comm.rs    # person / permmenu / refreshPerms / upload
│   │   ├── base_open.rs    # captcha / login / eps（免登录）
│   │   ├── base_sys_user.rs      # add/update 自定义（密码哈希）
│   │   ├── base_sys_role.rs
│   │   ├── base_sys_menu.rs
│   │   ├── base_sys_department.rs
│   │   ├── base_sys_param.rs
│   │   └── base_sys_log.rs
│   └── app/
│       └── base_comm.rs    # AppEPS
├── funcs/                  # 任务函数（对齐 funcs/base_sys_log_clear.go）
│   └── mod.rs              # BaseFuncClearLog
├── middleware/             # 中间件（对齐 middleware/）
│   ├── mod.rs
│   ├── authority.rs        # 登录/权限校验（URL → perms）
│   └── log.rs              # 请求日志
├── model/                  # 数据表定义（对齐 model/）
│   ├── mod.rs              # 全部 TableDef（autoMigrate）
│   └── initjson.rs         # resource/initjson 种子数据
├── resource/initjson/      # 种子数据 JSON（8 张表）
├── service/                # 业务服务（对齐 service/）
│   ├── base_open.rs        # EPS 元数据
│   ├── base_sys_login.rs   # 登录/失败锁定/种子管理员
│   ├── base_sys_user.rs    # 用户查询/密码
│   ├── base_sys_perms.rs   # 权限点/菜单/刷新
│   ├── base_sys_menu.rs    # 菜单服务
│   ├── base_sys_param.rs   # 参数缓存
│   ├── base_sys_role.rs    # 角色服务
│   ├── base_sys_department.rs
│   ├── base_sys_log.rs     # 日志清理
│   ├── base_sys_conf.rs    # 系统配置
│   └── captcha.rs          # 图形验证码
└── utils/                  # 工具函数
    └── mod.rs
```

## 路由前缀

| 分类 | 前缀 | 鉴权 |
|------|------|------|
| 开放 | `/admin/base/open/*`、`/base/open/*` | 免登录（captcha/login/eps） |
| 公共 | `/admin/base/comm/*`、`/base/comm/*` | 仅登录 |
| 管理 | `/admin/base/sys/*`、`/base/sys/*` | 登录 + 权限点 |

同一资源同时挂 `/admin/base/...` 与 `/base/...` 双前缀，兼容 cool-admin-go 前端。

## 入口

```rust
// 开放路由（挂 public 组）
mod_base::open_routes()

// 业务路由（挂 protected 组，登录 + 权限校验）
mod_base::routes()

// 中间件
mod_base::authority::authority_middleware
```
