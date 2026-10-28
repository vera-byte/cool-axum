//! ModuleSpec / RouteGroups：模块注册信息（对齐 cool-admin-go modules/*.go init）
//!
//! Go 的 `cool.RegisterController` 在 init 中自动把控制器路由注册进全局 Server；
//! Rust 无副作用 init，由 `spec()` 声明表/种子，`route_groups()` 声明路由，
//! 由 `modules` 注册中心统一装配。

use crate::db::TableDef;

/// 一个业务模块的注册声明
///
/// 种子数据由模块 `init()` 中 FillInitData 自治（对齐 base.go），
/// 此处仅声明 autoMigrate 业务表。
pub struct ModuleSpec {
    /// 模块名（如 "base"）
    pub name: &'static str,
    /// 业务表（autoMigrate）
    pub tables: Vec<TableDef>,
}

impl ModuleSpec {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            tables: Vec::new(),
        }
    }

    pub fn tables(mut self, tables: Vec<TableDef>) -> Self {
        self.tables = tables;
        self
    }
}

/// 模块的路由分组（对齐 cool-admin-go 中间件按路径分层）
///
/// - `open`：免登录（/admin/*/open/*、开放接口）
/// - `comm`：仅登录（/admin/*/comm/*）
/// - `biz`：登录 + 权限点
pub struct RouteGroups<S: Clone + Send + Sync + 'static> {
    pub open: axum::Router<S>,
    pub comm: axum::Router<S>,
    pub biz: axum::Router<S>,
}

impl<S: Clone + Send + Sync + 'static> Default for RouteGroups<S> {
    fn default() -> Self {
        Self {
            open: axum::Router::new(),
            comm: axum::Router::new(),
            biz: axum::Router::new(),
        }
    }
}

impl<S: Clone + Send + Sync + 'static> RouteGroups<S> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(mut self, r: axum::Router<S>) -> Self {
        self.open = self.open.merge(r);
        self
    }

    pub fn comm(mut self, r: axum::Router<S>) -> Self {
        self.comm = self.comm.merge(r);
        self
    }

    pub fn biz(mut self, r: axum::Router<S>) -> Self {
        self.biz = self.biz.merge(r);
        self
    }
}
