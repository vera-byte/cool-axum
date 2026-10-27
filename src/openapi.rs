//! OpenAPI 3.0 导出 + Swagger UI（公开路由，免登录）
//!
//! 对齐 cool-admin-go `openapiPath` / `swaggerPath` 配置。

use serde_json::{Value, json};

/// 生成 OpenAPI 3.0 文档
pub fn openapi_json() -> Value {
    let mut paths = serde_json::Map::new();

    // 健康检查
    paths.insert(
        "/health".into(),
        json!({
            "get": {
                "tags": ["系统"],
                "summary": "健康检查",
                "security": [],
                "responses": { "200": { "description": "OK" } }
            }
        }),
    );
    paths.insert(
        "/".into(),
        json!({
            "get": {
                "tags": ["系统"],
                "summary": "服务信息",
                "security": [],
                "responses": { "200": { "description": "OK" } }
            }
        }),
    );

    // 登录 / 登出
    for (p, summary) in [
        ("/base/login", "登录"),
        ("/admin/base/open/login", "登录（admin 前缀）"),
    ] {
        paths.insert(
            p.into(),
            json!({
                "post": {
                    "tags": ["认证"],
                    "summary": summary,
                    "security": [],
                    "requestBody": {
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "username": { "type": "string" },
                                        "password": { "type": "string" },
                                        "captchaId": { "type": "string" },
                                        "verifyCode": { "type": "string" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": { "200": { "description": "TokenResult" } }
                }
            }),
        );
    }
    paths.insert(
        "/base/logout".into(),
        json!({
            "post": {
                "tags": ["认证"],
                "summary": "登出",
                "responses": { "200": { "description": "OK" } }
            }
        }),
    );

    // 通用 CRUD
    let resources: &[(&str, &str)] = &[
        ("/base/sys/user", "用户"),
        ("/base/sys/role", "角色"),
        ("/base/sys/menu", "菜单"),
        ("/base/sys/department", "部门"),
        ("/base/sys/param", "参数"),
        ("/base/sys/log", "操作日志"),
        ("/dict/type", "字典类型"),
        ("/dict/info", "字典项"),
        ("/task/info", "定时任务"),
        ("/task/log", "任务日志"),
        ("/space/info", "文件"),
        ("/space/type", "文件分类"),
        ("/demo/sample", "示例"),
        ("/demo/goods", "商品"),
    ];
    for (prefix, name) in resources {
        let p = (*prefix).to_string();
        paths.insert(
            format!("{p}/page"),
            json!({
                "post": {
                    "tags": [name],
                    "summary": format!("{name} 分页"),
                    "responses": { "200": { "description": "ServicePage" } }
                }
            }),
        );
        paths.insert(
            format!("{p}/list"),
            json!({
                "post": {
                    "tags": [name],
                    "summary": format!("{name} 列表"),
                    "responses": { "200": { "description": "Array" } }
                }
            }),
        );
        paths.insert(
            format!("{p}/info"),
            json!({
                "get": {
                    "tags": [name],
                    "summary": format!("{name} 详情"),
                    "parameters": [{
                        "name": "id", "in": "query",
                        "schema": { "type": "integer" }, "required": true
                    }],
                    "responses": { "200": { "description": "OK" } }
                }
            }),
        );
        paths.insert(
            format!("{p}/add"),
            json!({
                "post": {
                    "tags": [name],
                    "summary": format!("{name} 新增"),
                    "responses": { "200": { "description": "{id}" } }
                }
            }),
        );
        paths.insert(
            format!("{p}/update"),
            json!({
                "post": {
                    "tags": [name],
                    "summary": format!("{name} 修改"),
                    "responses": { "200": { "description": "{rows}" } }
                }
            }),
        );
        paths.insert(
            format!("{p}/delete"),
            json!({
                "post": {
                    "tags": [name],
                    "summary": format!("{name} 删除"),
                    "requestBody": {
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "ids": {
                                            "type": "array",
                                            "items": { "type": "integer" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": { "200": { "description": "{rows}" } }
                }
            }),
        );
    }

    // 开放数据字典
    paths.insert(
        "/admin/dict/info/data".into(),
        json!({
            "post": {
                "tags": ["字典"],
                "summary": "字典数据（匿名可访问）",
                "security": [],
                "responses": { "200": { "description": "{typeKey: [...]}" } }
            }
        }),
    );

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "cool-axum API",
            "description": "cool-admin-go 的 Rust / Axum 移植 —— 通用 CRUD 后台接口",
            "version": "0.1.0"
        },
        "servers": [
            { "url": "/", "description": "当前服务" }
        ],
        "tags": [
            { "name": "系统" },
            { "name": "认证" },
            { "name": "用户" },
            { "name": "角色" },
            { "name": "菜单" },
            { "name": "部门" },
            { "name": "参数" },
            { "name": "操作日志" },
            { "name": "字典" },
            { "name": "字典类型" },
            { "name": "字典项" },
            { "name": "定时任务" },
            { "name": "任务日志" },
            { "name": "文件" },
            { "name": "文件分类" },
            { "name": "示例" },
            { "name": "商品" }
        ],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "token",
                    "description": "登录接口返回的 token；也可用 sa-token 头 / Cookie"
                }
            }
        },
        "security": [{ "bearerAuth": [] }],
        "paths": Value::Object(paths)
    })
}

/// Swagger UI HTML（引用公开的 OpenAPI JSON）
pub fn swagger_html(api_path: &str) -> String {
    let api = api_path.trim();
    format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>cool-axum API</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css"/>
  <style>body {{ margin: 0; background: #fafafa; }}</style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    window.onload = () => {{
      window.ui = SwaggerUIBundle({{
        url: "{api}",
        dom_id: '#swagger-ui',
        deepLinking: true,
        tryItOutEnabled: true,
        supportedSubmitMethods: ['get', 'post', 'put', 'delete', 'patch'],
        requestInterceptor: (req) => {{
          // 允许手工填 Authorization: Bearer <token>
          return req;
        }}
      }});
    }};
  </script>
</body>
</html>
"##,
        api = api
    )
}
