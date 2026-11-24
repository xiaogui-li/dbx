use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use dbx_core::{changelog, update};

use crate::error::AppError;
use crate::state::WebState;

/// 返回 Web 模式的应用版本；自有构建统一显示 4 段版本号。
///
/// @author xiaogui
/// @since 2026-11-23 20:36:03
pub async fn get_version(State(state): State<Arc<WebState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "version": dbx_core::update::self_build_version(env!("CARGO_PKG_VERSION")),
        "demoMode": state.demo_mode
    }))
}

#[derive(serde::Deserialize)]
pub struct UpdateCheckParams {
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub source: Option<dbx_core::DownloadSource>,
    #[serde(default)]
    pub channel: Option<dbx_core::update::UpdateChannel>,
}

pub async fn check_for_updates(Query(params): Query<UpdateCheckParams>) -> Result<Json<serde_json::Value>, AppError> {
    let locale = params.locale.unwrap_or_else(|| "zh-CN".to_string());
    let release = update::fetch_latest_release_for_channel(
        &locale,
        params.source.unwrap_or_default(),
        params.channel.unwrap_or_default(),
    )
    .await
    .map_err(AppError::from)?;
    let current_version = dbx_core::update::self_build_version(env!("CARGO_PKG_VERSION"));
    let info = update::build_update_info(release, &current_version);
    Ok(Json(serde_json::to_value(info).map_err(|e| AppError::from(e.to_string()))?))
}

#[derive(serde::Deserialize)]
pub struct ChangelogParams {
    #[serde(default)]
    pub lang: Option<String>,
}

pub async fn fetch_changelog(
    Query(params): Query<ChangelogParams>,
) -> Result<Json<changelog::ChangelogData>, AppError> {
    let lang = params.lang.unwrap_or_else(|| "en".to_string());
    let data = changelog::fetch_changelog(&lang).await.map_err(AppError::from)?;
    Ok(Json(data))
}

pub async fn fetch_changelog_index(
    Query(params): Query<ChangelogParams>,
) -> Result<Json<changelog::ChangelogIndex>, AppError> {
    let lang = params.lang.unwrap_or_else(|| "en".to_string());
    let index = changelog::fetch_changelog_index(&lang).await.map_err(AppError::from)?;
    Ok(Json(index))
}

pub async fn fetch_changelog_release(
    Path(tag): Path<String>,
    Query(params): Query<ChangelogParams>,
) -> Result<Json<changelog::ChangelogRelease>, AppError> {
    let lang = params.lang.unwrap_or_else(|| "en".to_string());
    let release = changelog::fetch_changelog_release(&lang, &tag).await.map_err(AppError::from)?;
    Ok(Json(release))
}
