//! OushuDB 专用驱动 crate。
//!
//! OushuDB 使用 PostgreSQL wire 协议，因此连接层薄封装 `dbx-driver-postgres`，
//! 但对外拥有独立的 `db::oushudb` 命名空间与 URL scheme（`oushudb://`），
//! 并为后续 OushuDB 专属行为（如批量插入 GUC、SQL 方言重写）提供集中扩展点。
//!
//! @author xiaogui
//! @since 2026-11-26 00:47:12

#![recursion_limit = "256"]

pub use dbx_driver_postgres::*;

pub mod oushudb;
pub mod sql;

pub use oushudb::{build_postgres_cancel_context, connect, connect_with_max_connections};
