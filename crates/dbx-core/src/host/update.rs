pub use dbx_platform::version::{
    base_version, from_semver_compatible, is_newer_version, normalize_version, parse_version, self_build_version,
    to_semver_compatible, SELF_BUILD_NUMBER,
};

use serde::{Deserialize, Serialize};

/// 自有构建发布仓库：自建版本的「检查更新」与安装包下载只认这里。
///
/// 插件市场 catalog、JDBC/驱动注册表、changelog 数据等下载源定义在各自模块，
/// 不经过本模块，保持指向官方来源不变。
///
/// @author xiaogui
/// @since 2026-11-23 20:35:49
const SELF_RELEASE_REPO: &str = "xiaogui-li/dbx";
const LATEST_JSON_GITHUB_PATH: &str = "https://github.com/xiaogui-li/dbx/releases/latest/download/latest.json";
const GITHUB_RELEASE_API_PREFIX: &str = "https://api.github.com/repos/xiaogui-li/dbx/releases/tags/v";

fn default_promoted() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct TauriRelease {
    pub version: String,
    #[serde(default)]
    pub channel: UpdateChannel,
    #[serde(default)]
    pub release_tag: Option<String>,
    #[serde(default = "default_promoted")]
    pub is_promoted: bool,
    /// Beta build version this stable release was promoted from. A promoted
    /// build is not rebuilt, so the installed binary still carries it.
    #[serde(default)]
    pub promoted_from: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub jdbc_plugin: Option<JdbcPluginLatest>,
    #[serde(skip)]
    pub github: Option<GithubReleaseMetadata>,
    // 英文 release notes，由自有 Release 的正文填充（latest.json 不含此字段）。
    // 仅当用户界面非中文时填充，build_update_info 优先用它作为 release_notes。
    #[serde(skip)]
    pub notes_en: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JdbcPluginLatest {
    pub version: String,
    pub protocol_version: u32,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubReleaseMetadata {
    pub name: Option<String>,
    pub html_url: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub portable_mode: bool,
    pub manual_update_only: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
    pub channel: UpdateChannel,
    pub release_tag: String,
    pub is_promoted: bool,
    pub promoted_from: Option<String>,
}

pub async fn fetch_latest_release(locale: &str, source: crate::DownloadSource) -> Result<TauriRelease, String> {
    fetch_latest_release_for_channel(locale, source, UpdateChannel::Stable).await
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

pub async fn fetch_latest_release_for_channel(
    locale: &str,
    source: crate::DownloadSource,
    channel: UpdateChannel,
) -> Result<TauriRelease, String> {
    let client = build_update_http_client()?;

    let candidates = update_check_candidates(source, channel);
    let resp = fetch_first_available(&client, &candidates).await?;

    let mut release = resp.json::<TauriRelease>().await.map_err(|e| format!("Failed to parse update response: {e}"))?;
    let identity_version = release_identity_version(&release).to_string();
    if let Ok(github) = fetch_github_release_metadata(&client, &identity_version).await {
        release.github = Some(github);
    }
    // 英文界面直接使用自有 Release 的正文作为 notes，不再请求官方 R2 上的 changelog 文件。
    // 这样「检查更新」全程只访问自己的仓库，不会回落到官方发布源。
    if !is_chinese_locale(locale) {
        if let Some(body) = release.github.as_ref().and_then(|metadata| metadata.body.clone()) {
            release.notes_en = Some(body);
        }
    }
    Ok(release)
}

/// Manifests promoted before the Release moved to its stable tag publish the
/// stable base version in `version` but keep the beta tag in `release_tag`;
/// GitHub metadata and R2 release notes are both keyed by that tag.
fn release_identity_version(release: &TauriRelease) -> &str {
    release.release_tag.as_deref().unwrap_or(&release.version)
}

async fn fetch_first_available(client: &reqwest::Client, candidates: &[String]) -> Result<reqwest::Response, String> {
    let mut errors = Vec::with_capacity(candidates.len());
    for url in candidates {
        match client
            .get(url)
            .header(reqwest::header::USER_AGENT, "dbx-update-checker")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await
            .and_then(|response| response.error_for_status())
        {
            Ok(response) => return Ok(response),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(format!("Failed to check updates: {}", errors.join("; ")))
}

/// 返回自建版本的检查更新候选地址。
///
/// 自有构建只认自己仓库的 Release，不再把官方 R2 CDN 或 CNB 镜像列为候选，
/// 避免「检查更新」先命中官方清单而展示官方版本号。下载源设置（official / cnb）
/// 与渠道（stable / beta）在此统一收敛到同一份自有清单。
/// 插件、驱动、catalog 等下载源不经过本函数，保持不变。
///
/// @author xiaogui
/// @since 2026-11-23 20:35:50
fn update_check_candidates(_source: crate::DownloadSource, _channel: UpdateChannel) -> Vec<String> {
    vec![LATEST_JSON_GITHUB_PATH.to_string()]
}

fn is_chinese_locale(locale: &str) -> bool {
    locale == "zh-CN" || locale == "zh-TW"
}

fn build_update_http_client() -> Result<reqwest::Client, String> {
    let mut builder =
        reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).user_agent("dbx-update-checker");

    if let Some(proxy_url) = system_proxy_url() {
        let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| format!("Invalid system proxy URL: {e}"))?;
        builder = builder.proxy(proxy);
    }

    builder.build().map_err(|e| format!("Failed to create HTTP client: {e}"))
}

pub use dbx_platform::proxy::system_proxy_url;

async fn fetch_github_release_metadata(
    client: &reqwest::Client,
    version: &str,
) -> Result<GithubReleaseMetadata, String> {
    let url = format!("{GITHUB_RELEASE_API_PREFIX}{}", normalize_version(version));
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "dbx-update-checker")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("{e}"))?
        .json::<GithubReleaseMetadata>()
        .await
        .map_err(|e| format!("Failed to parse GitHub release response: {e}"))
}

pub fn build_update_info(release: TauriRelease, current_version: &str) -> UpdateInfo {
    let latest_version = normalize_version(&release.version);
    let update_available = is_channel_update_available(release.channel, &latest_version, current_version)
        && stable_release_advances_installed_build(&release, current_version);
    let github = release.github.as_ref();
    let release_notes = non_empty(release.notes_en.as_deref())
        .map(ToOwned::to_owned)
        .or_else(|| canonical_release_notes(release.notes.as_deref()))
        .or_else(|| github.and_then(|metadata| non_empty(metadata.body.as_deref())).map(ToOwned::to_owned))
        .unwrap_or_default();
    let release_name = github
        .and_then(|metadata| non_empty(metadata.name.as_deref()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("DBX v{latest_version}"));
    let release_tag = release.release_tag.unwrap_or_else(|| format!("v{latest_version}"));
    let release_url = github
        .and_then(|metadata| non_empty(metadata.html_url.as_deref()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("https://github.com/{SELF_RELEASE_REPO}/releases/tag/{release_tag}"));
    UpdateInfo {
        update_available,
        portable_mode: false,
        manual_update_only: false,
        current_version: current_version.to_string(),
        release_name,
        release_url,
        release_notes,
        latest_version,
        channel: release.channel,
        release_tag,
        is_promoted: release.is_promoted,
        promoted_from: release.promoted_from,
    }
}

/// Whether downloading this stable release would actually move the user forward.
///
/// A promoted stable release is not rebuilt, so the installed binary keeps the
/// beta version it was published with while the stable manifest exposes the base
/// version. Downloading it is a no-op for that build and a downgrade for a
/// *later* beta of the same base. Someone running an older beta build still
/// needs the stable download (#15341), so the comparison is by version, not by
/// base version.
fn stable_release_advances_installed_build(release: &TauriRelease, current_version: &str) -> bool {
    if !release.is_promoted || release.channel != UpdateChannel::Stable {
        return true;
    }
    let Some(shipped) = shipped_beta_build(release) else {
        return true;
    };
    is_newer_version(&shipped, &normalize_version(current_version))
}

/// The prerelease build a promoted stable manifest says it ships.
///
/// Modern manifests name it in `promoted_from`; before a promotion moved to the
/// stable tag the beta tag sat in `release_tag` while `version` carried the base
/// version. Entries that belong to another base version cannot describe this
/// release — `release_tag_for_version` in `src-tauri` rejects that shape when it
/// creates tags — so they are ignored instead of suppressing an update. That
/// also means a manifest whose `version` moved to the next base while
/// `promoted_from` still names the previous one is treated as a fresh artifact:
/// re-offering it is the safe direction, and the alternative would hide a real
/// update behind a stale field.
fn shipped_beta_build(release: &TauriRelease) -> Option<String> {
    let advertised_base = base_version(&release.version);
    [release.promoted_from.as_deref(), release.release_tag.as_deref()]
        .into_iter()
        .flatten()
        .map(normalize_version)
        .find(|candidate| {
            !candidate.is_empty() && candidate.contains('-') && base_version(candidate) == advertised_base
        })
}

/// Whether `latest_version` is worth offering on `channel` compared to `current_version`.
///
/// This is pure channel arithmetic, so the Tauri updater's version comparator
/// can call it with nothing but two version strings. Suppressing the build that
/// a promoted stable release already ships needs `promoted_from`, so it lives in
/// [`build_update_info`]; that is also the gate the UI reads before it offers a
/// download. Ranking a base release above its own prerelease therefore stays
/// true here - a user on a *different* beta build of the same base version must
/// still get the stable build (#15341).
pub fn is_channel_update_available(channel: UpdateChannel, latest_version: &str, current_version: &str) -> bool {
    if is_newer_version(latest_version, current_version) {
        return true;
    }
    if matches!(channel, UpdateChannel::Beta) {
        // SemVer ranks `0.6.60-beta.N` below `0.6.60`. A user who explicitly
        // opts into beta must still be able to move from the stable build of
        // that base version to its beta builds.
        let latest = normalize_version(latest_version);
        let current = normalize_version(current_version);
        let latest_base = latest.split(['-', '+']).next().unwrap_or_default();
        let current_base = current.split(['-', '+']).next().unwrap_or_default();
        return latest.contains('-') && latest_base == current_base && !current.contains('-');
    }
    false
}

fn canonical_release_notes(notes: Option<&str>) -> Option<String> {
    let notes = non_empty(notes)?;
    // Tauri's generated updater notes are a transport fallback, not the curated release notes.
    if notes.starts_with("## What's Changed") || notes == "See the assets below to download and install." {
        return None;
    }
    Some(notes.to_owned())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_update_info, normalize_version, release_identity_version, GithubReleaseMetadata, TauriRelease,
        UpdateChannel,
    };

    /// 验证自建 4 段版本：检查更新只查自有仓库，且构建号递增时能正确判定为可更新。
    ///
    /// @author xiaogui
    /// @since 2026-11-23 20:35:51
    #[test]
    fn self_build_update_check_uses_own_repository_and_four_part_versions() {
        let candidates = super::update_check_candidates(crate::DownloadSource::Official, UpdateChannel::Stable);
        assert_eq!(candidates, vec!["https://github.com/xiaogui-li/dbx/releases/latest/download/latest.json"]);
        assert!(!candidates.iter().any(|url| url.contains("t8y2/dbx") || url.contains("dl.dbxio.com")));

        let release: TauriRelease = serde_json::from_str(
            r#"{ "version": "0.6.72.2", "channel": "stable", "release_tag": "v0.6.72.2", "platforms": {} }"#,
        )
        .unwrap();
        let info = build_update_info(release, "0.6.72.1");
        assert!(info.update_available);
        assert_eq!(info.latest_version, "0.6.72.2");
        assert_eq!(info.release_url, "https://github.com/xiaogui-li/dbx/releases/tag/v0.6.72.2");

        let same: TauriRelease = serde_json::from_str(r#"{ "version": "0.6.72.1", "platforms": {} }"#).unwrap();
        assert!(!build_update_info(same, "0.6.72.1").update_available);
    }

    #[test]
    fn parses_jdbc_plugin_metadata_from_latest_json() {
        let release: TauriRelease = serde_json::from_str(
            r#"{
              "version": "0.5.12",
              "jdbc_plugin": {
                "version": "0.1.3",
                "protocol_version": 1,
                "url": "https://github.com/t8y2/dbx/releases/latest/download/dbx-jdbc-plugin-latest.zip"
              },
              "platforms": {}
            }"#,
        )
        .unwrap();

        let jdbc = release.jdbc_plugin.unwrap();

        assert_eq!(jdbc.version, "0.1.3");
        assert_eq!(jdbc.protocol_version, 1);
        assert_eq!(jdbc.url, "https://github.com/t8y2/dbx/releases/latest/download/dbx-jdbc-plugin-latest.zip");
        assert_eq!(release.channel, UpdateChannel::Stable);
        assert!(release.is_promoted);
    }

    #[test]
    fn promoted_from_is_read_from_the_manifest_and_exposed_in_update_info() {
        let promoted: TauriRelease = serde_json::from_str(
            r#"{
              "version": "0.6.60",
              "channel": "stable",
              "release_tag": "v0.6.60",
              "is_promoted": true,
              "promoted_from": "0.6.60-beta.20261028.1",
              "platforms": {}
            }"#,
        )
        .unwrap();
        assert_eq!(
            build_update_info(promoted, "0.6.60-beta.20261028.1").promoted_from.as_deref(),
            Some("0.6.60-beta.20261028.1")
        );

        // Manifests written before promotions recorded their source stay valid.
        let legacy: TauriRelease = serde_json::from_str(r#"{ "version": "0.6.59", "platforms": {} }"#).unwrap();
        assert_eq!(build_update_info(legacy, "0.6.59").promoted_from, None);
    }

    #[test]
    fn update_info_prefers_github_release_metadata() {
        let release = TauriRelease {
            version: "0.5.3".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: None,
            is_promoted: false,
            promoted_from: None,
            notes: Some("See the assets below to download and install.".to_string()),
            jdbc_plugin: None,
            github: Some(GithubReleaseMetadata {
                name: Some("DBX v0.5.3".to_string()),
                html_url: Some("https://github.com/t8y2/dbx/releases/tag/v0.5.3".to_string()),
                body: Some("### 新功能\n\n真实发布说明".to_string()),
            }),
            notes_en: None,
        };

        let info = build_update_info(release, "0.5.2");

        assert_eq!(info.release_name, "DBX v0.5.3");
        assert_eq!(info.release_url, "https://github.com/t8y2/dbx/releases/tag/v0.5.3");
        assert_eq!(info.release_notes, "### 新功能\n\n真实发布说明");
        assert!(!info.portable_mode);
    }

    #[test]
    fn update_info_prefers_english_notes_when_present() {
        // 非中文界面用户：notes_en 命中时优先于 GitHub 中文 body，应用内更新提示展示英文
        let release = TauriRelease {
            version: "0.5.3".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: None,
            is_promoted: false,
            promoted_from: None,
            notes: Some("See the assets below to download and install.".to_string()),
            jdbc_plugin: None,
            github: Some(GithubReleaseMetadata {
                name: Some("DBX v0.5.3".to_string()),
                html_url: Some("https://github.com/t8y2/dbx/releases/tag/v0.5.3".to_string()),
                body: Some("### 新功能\n\n真实发布说明".to_string()),
            }),
            notes_en: Some("### New Features\n\nReal release notes".to_string()),
        };

        let info = build_update_info(release, "0.5.2");

        assert_eq!(info.release_notes, "### New Features\n\nReal release notes");
    }

    #[test]
    fn update_info_ignores_generated_notes_when_curated_notes_are_unavailable() {
        let release = TauriRelease {
            version: "0.5.3".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: None,
            is_promoted: false,
            promoted_from: None,
            notes: Some("## What's Changed\n* generated item".to_string()),
            jdbc_plugin: None,
            github: None,
            notes_en: None,
        };

        let info = build_update_info(release, "0.5.2");

        assert_eq!(info.release_notes, "");
    }

    #[test]
    fn beta_release_url_uses_the_prerelease_tag_without_github_metadata() {
        let release = TauriRelease {
            version: "0.6.60-beta.20261028.1".to_string(),
            channel: UpdateChannel::Beta,
            release_tag: Some("v0.6.60-beta.20261028.1".to_string()),
            is_promoted: false,
            promoted_from: None,
            notes: None,
            jdbc_plugin: None,
            github: None,
            notes_en: None,
        };

        let info = build_update_info(release, "0.6.59");

        assert_eq!(info.release_url, "https://github.com/xiaogui-li/dbx/releases/tag/v0.6.60-beta.20261028.1");
    }

    #[test]
    fn release_identity_follows_the_tag_of_a_promoted_beta() {
        let promoted = TauriRelease {
            version: "0.6.60".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: Some("v0.6.60-beta.20261028.1".to_string()),
            is_promoted: true,
            promoted_from: None,
            notes: None,
            jdbc_plugin: None,
            github: None,
            notes_en: None,
        };
        // latest-en.json is keyed by the tag, so a promoted beta must not be matched by its base version.
        assert_eq!(normalize_version(release_identity_version(&promoted)), "0.6.60-beta.20261028.1");

        let legacy = TauriRelease { release_tag: None, ..promoted };
        assert_eq!(release_identity_version(&legacy), "0.6.60");
    }

    #[test]
    fn stable_channel_does_not_reoffer_the_promoted_beta_build() {
        let release = TauriRelease {
            version: "0.6.60".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: Some("v0.6.60-beta.20261028.1".to_string()),
            is_promoted: true,
            promoted_from: None,
            notes: None,
            jdbc_plugin: None,
            github: None,
            notes_en: None,
        };

        let info = build_update_info(release, "0.6.60-beta.20261028.1");

        assert!(!info.update_available);
    }

    #[test]
    fn stable_channel_reoffers_the_release_to_a_different_beta_build_of_the_same_base() {
        // #15341: the user runs 0.6.72-beta.20261119.1 while the stable release
        // was promoted from 0.6.72-beta.20261119.2. Their build is not the
        // shipped artifact, so "no update" is wrong.
        let promoted_release = |promoted_from: Option<&str>| TauriRelease {
            version: "0.6.72".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: Some("v0.6.72".to_string()),
            is_promoted: true,
            promoted_from: promoted_from.map(ToOwned::to_owned),
            notes: None,
            jdbc_plugin: None,
            github: None,
            notes_en: None,
        };

        assert!(
            build_update_info(promoted_release(Some("0.6.72-beta.20261119.2")), "0.6.72-beta.20261119.1")
                .update_available
        );
        assert!(build_update_info(promoted_release(None), "0.6.72-beta.20261119.1").update_available);
        assert!(build_update_info(promoted_release(Some("0.6.72-beta.20261119.2")), "0.6.71").update_available);
        // The promoted build itself already is the stable artifact.
        assert!(
            !build_update_info(promoted_release(Some("0.6.72-beta.20261119.2")), "0.6.72-beta.20261119.2")
                .update_available
        );
        assert!(
            !build_update_info(promoted_release(Some("v0.6.72-beta.20261119.2")), "0.6.72-beta.20261119.2")
                .update_available
        );
        // A later beta of the same base must not be "updated" down to the
        // promoted build, and build metadata is not a different build.
        assert!(
            !build_update_info(promoted_release(Some("0.6.72-beta.20261119.2")), "0.6.72-beta.20261120.1")
                .update_available
        );
        assert!(
            !build_update_info(promoted_release(Some("0.6.72-beta.20261119.2")), "0.6.72-beta.20261119.2+local.7")
                .update_available
        );
        // An empty `promoted_from` names no build, so the base release is offered.
        assert!(build_update_info(promoted_release(Some("")), "0.6.72-beta.20261119.1").update_available);
    }

    #[test]
    fn a_release_tag_from_another_base_does_not_suppress_the_update() {
        // #15341 review: a lagging/incorrect tag must not hide a newer stable
        // release from the build that tag happens to name.
        let release = TauriRelease {
            version: "0.6.73".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: Some("v0.6.72-beta.20261119.2".to_string()),
            is_promoted: true,
            promoted_from: None,
            notes: None,
            jdbc_plugin: None,
            github: None,
            notes_en: None,
        };

        assert!(build_update_info(release, "0.6.72-beta.20261119.2").update_available);
    }

    #[test]
    fn an_unpromoted_stable_release_still_updates_beta_builds_of_its_base() {
        let release = TauriRelease {
            version: "0.6.72".to_string(),
            channel: UpdateChannel::Stable,
            release_tag: Some("v0.6.72".to_string()),
            is_promoted: false,
            promoted_from: None,
            notes: None,
            jdbc_plugin: None,
            github: None,
            notes_en: None,
        };

        assert!(build_update_info(release, "0.6.72-beta.20261119.1").update_available);
    }

    #[test]
    fn beta_channel_offers_same_base_prerelease_from_stable_build() {
        assert!(super::is_channel_update_available(UpdateChannel::Beta, "0.6.60-beta.20261028.1", "0.6.60"));
        assert!(super::is_channel_update_available(
            UpdateChannel::Beta,
            "0.6.60-beta.20261028.2",
            "0.6.60-beta.20261028.1"
        ));
        // Channel arithmetic ranks the base release above its prerelease; the
        // promoted-build suppression belongs to build_update_info, which knows
        // which build the release actually ships.
        assert!(super::is_channel_update_available(UpdateChannel::Stable, "0.6.60", "0.6.60-beta.20261028.1"));
        assert!(!super::is_channel_update_available(UpdateChannel::Stable, "0.6.60", "0.6.60"));
    }

    /// 自有构建的检查更新候选只指向自己的仓库，且与下载源/渠道选择无关。
    ///
    /// @author xiaogui
    /// @since 2026-11-23 20:36:11
    #[test]
    fn update_check_candidates_always_use_the_self_build_repository() {
        let expected = vec!["https://github.com/xiaogui-li/dbx/releases/latest/download/latest.json".to_string()];
        for source in [crate::DownloadSource::Official, crate::DownloadSource::Cnb] {
            for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
                assert_eq!(super::update_check_candidates(source, channel), expected);
            }
        }
    }
}
