//! ModuleSpec：模块注册信息（对齐 cool-admin-go modules/*.go 的 init 注册）

use cool_db::TableDef;

/// 一个业务模块的注册声明
pub struct ModuleSpec {
    /// 模块名（如 "base"）
    pub name: &'static str,
    /// 业务表（autoMigrate）
    pub tables: Vec<TableDef>,
    /// initjson 种子数据：(表名, JSON)
    pub seeds: Vec<(&'static str, &'static str)>,
}

impl ModuleSpec {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            tables: Vec::new(),
            seeds: Vec::new(),
        }
    }

    pub fn tables(mut self, tables: Vec<TableDef>) -> Self {
        self.tables = tables;
        self
    }

    pub fn seeds(mut self, seeds: Vec<(&'static str, &'static str)>) -> Self {
        self.seeds = seeds;
        self
    }
}
