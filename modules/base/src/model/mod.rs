//! model 层：数据表定义（autoMigrate），对齐 cool-admin-go modules/base/model/*.go
//!
//! 包含 initjson 种子数据加载器。
//!
//! 对齐 cool-admin-go modules/base/model/*.go 的 gorm 列定义

pub mod initjson;

use cool_db::TableDef;

fn col(
    name: &str,
    sql_type: &str,
    not_null: bool,
    primary_key: bool,
    comment: &str,
) -> cool_db::ColumnMeta {
    cool_db::ColumnMeta {
        name: name.into(),
        sql_type: sql_type.into(),
        not_null,
        primary_key,
        comment: comment.into(),
        auto_increment: primary_key,
        default: None,
    }
}

/// 带默认值的列
fn col_def(
    name: &str,
    sql_type: &str,
    not_null: bool,
    comment: &str,
    default: &str,
) -> cool_db::ColumnMeta {
    cool_db::ColumnMeta {
        name: name.into(),
        sql_type: sql_type.into(),
        not_null,
        primary_key: false,
        comment: comment.into(),
        auto_increment: false,
        default: Some(default.into()),
    }
}

/// 公共基础列：id / createTime / updateTime / deleteTime
pub fn base_cols() -> Vec<cool_db::ColumnMeta> {
    vec![
        col("id", "bigint", true, true, "主键"),
        col("createTime", "datetime", true, false, "创建时间"),
        col("updateTime", "datetime", true, false, "更新时间"),
        col("deleteTime", "datetime", false, false, "删除时间"),
    ]
}

fn table(name: &str, mut cols: Vec<cool_db::ColumnMeta>) -> TableDef {
    let mut all = base_cols();
    all.append(&mut cols);
    TableDef {
        name: name.into(),
        columns: all,
    }
}

/// base_sys_user
pub fn base_sys_user() -> TableDef {
    table(
        "base_sys_user",
        vec![
            col("departmentId", "bigint", false, false, "部门ID"),
            col("name", "varchar(255)", false, false, "姓名"),
            col("username", "varchar(100)", true, false, "用户名"),
            col("password", "varchar(255)", true, false, "密码"),
            col_def("passwordV", "int", true, "密码版本", "1"),
            col("nickName", "varchar(255)", false, false, "昵称"),
            col("headImg", "varchar(255)", false, false, "头像"),
            col("phone", "varchar(20)", false, false, "手机"),
            col("email", "varchar(255)", false, false, "邮箱"),
            col_def("status", "int", true, "状态 0禁用 1启用", "1"),
            col("remark", "varchar(255)", false, false, "备注"),
            col("socketId", "varchar(255)", false, false, "socketId"),
        ],
    )
}

/// base_sys_role
pub fn base_sys_role() -> TableDef {
    table(
        "base_sys_role",
        vec![
            col("userId", "varchar(255)", true, false, "用户ID"),
            col("name", "varchar(255)", true, false, "名称"),
            col("label", "varchar(50)", false, false, "角色标签"),
            col("remark", "varchar(255)", false, false, "备注"),
            col_def("relevance", "int", true, "数据权限是否关联上下级", "1"),
        ],
    )
}

/// base_sys_menu
pub fn base_sys_menu() -> TableDef {
    table(
        "base_sys_menu",
        vec![
            col("parentId", "bigint", false, false, "父菜单ID"),
            col("name", "varchar(255)", true, false, "菜单名称"),
            col("router", "varchar(255)", false, false, "菜单地址"),
            col("perms", "varchar(255)", false, false, "权限标识"),
            col("type", "int", true, false, "类型 0目录 1菜单 2按钮"),
            col("icon", "varchar(255)", false, false, "图标"),
            col_def("orderNum", "int", true, "排序", "0"),
            col("viewPath", "varchar(255)", false, false, "视图地址"),
            col_def("keepAlive", "int", true, "路由缓存", "1"),
            col_def("isShow", "int", true, "是否显示", "1"),
        ],
    )
}

/// base_sys_department
pub fn base_sys_department() -> TableDef {
    table(
        "base_sys_department",
        vec![
            col("name", "varchar(255)", true, false, "部门名称"),
            col("parentId", "bigint", false, false, "上级部门ID"),
            col("orderNum", "int", true, false, "排序"),
        ],
    )
}

/// base_sys_param
pub fn base_sys_param() -> TableDef {
    table(
        "base_sys_param",
        vec![
            col("keyName", "varchar(255)", true, false, "键位"),
            col("name", "varchar(255)", true, false, "名称"),
            col("data", "text", true, false, "数据"),
            col("dataType", "int", true, false, "数据类型 0字符串 1数组 2键值对"),
            col("remark", "varchar(255)", false, false, "备注"),
        ],
    )
}

/// base_sys_log
pub fn base_sys_log() -> TableDef {
    table(
        "base_sys_log",
        vec![
            col("userId", "bigint", false, false, "用户ID"),
            col("action", "varchar(255)", true, false, "行为"),
            col("ip", "varchar(255)", false, false, "ip"),
            col("ipAddr", "varchar(255)", false, false, "ip地址"),
            col("params", "text", false, false, "参数"),
        ],
    )
}

/// base_sys_user_role
pub fn base_sys_user_role() -> TableDef {
    table(
        "base_sys_user_role",
        vec![
            col("userId", "bigint", true, false, "用户ID"),
            col("roleId", "bigint", true, false, "角色ID"),
        ],
    )
}

/// base_sys_role_menu
pub fn base_sys_role_menu() -> TableDef {
    table(
        "base_sys_role_menu",
        vec![
            col("roleId", "bigint", true, false, "角色ID"),
            col("menuId", "bigint", true, false, "菜单ID"),
        ],
    )
}

/// base_sys_role_department
pub fn base_sys_role_department() -> TableDef {
    table(
        "base_sys_role_department",
        vec![
            col("roleId", "bigint", true, false, "角色ID"),
            col("departmentId", "bigint", true, false, "部门ID"),
        ],
    )
}

/// 全部 base 表
pub fn all_tables() -> Vec<TableDef> {
    vec![
        base_sys_user(),
        base_sys_role(),
        base_sys_menu(),
        base_sys_department(),
        base_sys_param(),
        base_sys_log(),
        base_sys_user_role(),
        base_sys_role_menu(),
        base_sys_role_department(),
    ]
}
