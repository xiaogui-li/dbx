//! OushuDB 连接薄封装。
//!
//! 厂商 JDBC 驱动（`com.oushu.Driver`）静态分析表明其与 pgjdbc 的差异集中在
//! URL 前缀 `jdbc:oushudb:` 与批量插入 GUC `enable_pbe_rewrite_batchinserts`；
//! 协议握手、认证、SSL、系统目录查询均与 PostgreSQL 一致。因此这里只做
//! URL scheme 归一化，其余交给 `dbx-driver-postgres`，避免复制协议实现。
//!
//! @author xiaogui
//! @since 2026-11-26 00:48:03

use deadpool_postgres::Pool;
use std::time::Duration;

/// 把 OushuDB 的 URL scheme 归一化为 PostgreSQL wire 驱动可识别的形式。
///
/// 支持 `oushudb://host:port/db` 与历史配置中的 `jdbc:oushudb://host:port/db`；
/// 已经是 `postgres://` / `postgresql://` 的 URL 原样返回。
///
/// @author xiaogui
/// @since 2026-11-26 00:48:27
pub fn normalize_oushudb_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("oushudb://") {
        format!("postgres://{rest}")
    } else if let Some(rest) = url.strip_prefix("jdbc:oushudb:") {
        let rest = rest.strip_prefix("//").unwrap_or(rest);
        format!("postgres://{rest}")
    } else {
        url.to_string()
    }
}

/// 建立 OushuDB 连接池（默认 10 条物理连接）。
///
/// @author xiaogui
/// @since 2026-11-26 00:48:49
pub async fn connect(url: &str, fallback_timeout: Duration) -> Result<Pool, String> {
    connect_with_max_connections(url, fallback_timeout, 10).await
}

/// 建立带显式物理连接上限的 OushuDB 连接池。
///
/// 会话级池会把上限收敛为 1，避免临时表与 `SET` 状态在多次检出间漂移。
///
/// @author xiaogui
/// @since 2026-11-26 00:49:12
pub async fn connect_with_max_connections(
    url: &str,
    fallback_timeout: Duration,
    max_connections: usize,
) -> Result<Pool, String> {
    let normalized = normalize_oushudb_url(url);
    dbx_driver_postgres::connect_with_max_connections(&normalized, fallback_timeout, max_connections).await
}

/// 从 OushuDB 连接 URL 构造 TLS 取消上下文。
///
/// @author xiaogui
/// @since 2026-11-26 00:49:36
pub fn build_postgres_cancel_context(url: &str) -> Option<dbx_driver_postgres::PostgresCancelContext> {
    let normalized = normalize_oushudb_url(url);
    dbx_driver_postgres::build_postgres_cancel_context(&normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_oushudb_url;

    #[test]
    fn normalizes_oushudb_schemes() {
        assert_eq!(
            normalize_oushudb_url("oushudb://his:pwd@10.0.0.1:5432/postgres?sslmode=require"),
            "postgres://his:pwd@10.0.0.1:5432/postgres?sslmode=require"
        );
        assert_eq!(
            normalize_oushudb_url("jdbc:oushudb://10.0.0.1:5432/postgres"),
            "postgres://10.0.0.1:5432/postgres"
        );
        assert_eq!(
            normalize_oushudb_url("postgres://10.0.0.1:5432/postgres"),
            "postgres://10.0.0.1:5432/postgres"
        );
    }
}
