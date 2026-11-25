//! OushuDB SQL 适配扩展点。
//!
//! 厂商 JDBC 驱动静态分析显示，除 PostgreSQL wire 兼容外，唯一确定的专属
//! 服务端行为是批量插入重写开关 `enable_pbe_rewrite_batchinserts`。DBX 的
//! Rust 驱动不启用 JDBC 式批处理重写，因此这里只提供可测试的常量入口；
//! 后续遇到 OushuDB 特定 SQL 格式时，统一在此收口，避免散落到 core。
//!
//! @author xiaogui
//! @since 2026-11-26 00:50:04

/// 返回 OushuDB 批量插入重写的会话级 GUC 设置语句。
///
/// @author xiaogui
/// @since 2026-11-26 00:50:25
pub fn batch_insert_rewrite_guc_sql(enabled: bool) -> &'static str {
    if enabled {
        "SET enable_pbe_rewrite_batchinserts = ON;"
    } else {
        "SET enable_pbe_rewrite_batchinserts = OFF;"
    }
}

/// OushuDB SQL 方言适配入口，当前与 PostgreSQL 行为一致（原样透传）。
///
/// @author xiaogui
/// @since 2026-11-26 00:50:47
pub fn adapt_sql(sql: &str) -> String {
    sql.to_string()
}

#[cfg(test)]
mod tests {
    use super::{adapt_sql, batch_insert_rewrite_guc_sql};

    #[test]
    fn exposes_oushu_specific_batch_guc() {
        assert_eq!(
            batch_insert_rewrite_guc_sql(true),
            "SET enable_pbe_rewrite_batchinserts = ON;"
        );
        assert_eq!(
            batch_insert_rewrite_guc_sql(false),
            "SET enable_pbe_rewrite_batchinserts = OFF;"
        );
        assert_eq!(adapt_sql("SELECT 1"), "SELECT 1");
    }
}
