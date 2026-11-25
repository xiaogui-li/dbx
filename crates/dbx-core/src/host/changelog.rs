use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// 自有仓库静态更新日志数据源：更新管理的版本列表与更新日志均从这里读取。
/// 每次请求拼时间戳并带 no-cache，确保拿到 Release 上最新的文件而不是 CDN 旧副本。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:10
const SELF_CHANGELOG_URL: &str = "https://github.com/xiaogui-li/dbx/releases/latest/download/changelog.json";

/// 编译期内置快照：网络失败且磁盘缓存也不存在时的最终兜底，保证更新日志永远有内容可展示。
/// 随构建产出由 `.workbuddy/generate-changelog-json.mjs` 从自有仓库 Release 历史生成。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:12
const BUILTIN_CHANGELOG_JSON: &str = include_str!("../../assets/changelog.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogItem {
    pub title: String,
    #[serde(default)]
    pub desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogSection {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub items: Vec<ChangelogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogRelease {
    pub tag: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sections: Vec<ChangelogSection>,
}

/// One row of the changelog index: everything a version list needs and nothing
/// from the release body, so opening the list never downloads every release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogReleaseSummary {
    pub tag: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogIndex {
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub releases: Vec<ChangelogReleaseSummary>,
}

/// 全量更新日志数据；派生 Default 用于网络、磁盘缓存、内置快照全部失效时的空值兜底。
///
/// @author xiaogui
/// @since 2026-11-25 23:18:36
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogData {
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub releases: Vec<ChangelogRelease>,
}

/// Normalize UI locale / short lang into `cn` or `en` (kept for callers that still pass a language).
pub fn normalize_changelog_lang(lang: &str) -> &'static str {
    let trimmed = lang.trim();
    if trimmed.eq_ignore_ascii_case("cn")
        || trimmed.eq_ignore_ascii_case("zh")
        || trimmed.eq_ignore_ascii_case("zh-CN")
        || trimmed.eq_ignore_ascii_case("zh-TW")
    {
        "cn"
    } else {
        "en"
    }
}

fn is_app_release_tag(tag: &str) -> bool {
    let Some(version) = tag.strip_prefix('v') else {
        return false;
    };
    let mut parts = version.splitn(3, '.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    let Some(patch_and_suffix) = parts.next() else {
        return false;
    };
    if major.is_empty()
        || minor.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || !minor.bytes().all(|byte| byte.is_ascii_digit())
    {
        return false;
    }

    let patch_len = patch_and_suffix.bytes().take_while(u8::is_ascii_digit).count();
    if patch_len == 0 {
        return false;
    }
    let suffix = &patch_and_suffix[patch_len..];
    suffix.is_empty()
        || ((suffix.starts_with('.') || suffix.starts_with('-'))
            && suffix.len() > 1
            && suffix[1..].bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-'))
}

/// 磁盘缓存文件位置：用系统临时目录，应用重启后仍可读取；编译期快照是更底层的兜底。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:16
fn cache_file_path() -> PathBuf {
    std::env::temp_dir().join("dbx-self-build-changelog.json")
}

/// 请求自有仓库静态更新日志：时间戳参数与 no-cache 请求头一起绕过 CDN 与本地代理缓存。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:18
async fn fetch_remote_changelog() -> Result<ChangelogData, String> {
    let client = build_changelog_http_client()?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let url = format!("{SELF_CHANGELOG_URL}?t={stamp}");
    let resp = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, "dbx-changelog")
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .header(reqwest::header::PRAGMA, "no-cache")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Failed to fetch changelog: {e}"))?;
    let data: ChangelogData = resp.json().await.map_err(|e| format!("Failed to parse changelog: {e}"))?;
    if data.releases.is_empty() {
        return Err("Empty changelog payload".to_string());
    }
    Ok(data)
}

/// 把最近一次成功获取的数据写入磁盘缓存；写入失败不影响本次返回。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:20
fn persist_changelog_cache(data: &ChangelogData) {
    if let Ok(json) = serde_json::to_string(data) {
        let _ = std::fs::write(cache_file_path(), json);
    }
}

/// 读取磁盘缓存；文件缺失或内容无效时返回错误，交给编译期快照兜底。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:22
fn read_cached_changelog() -> Result<ChangelogData, String> {
    let json = std::fs::read_to_string(cache_file_path()).map_err(|e| format!("No changelog cache: {e}"))?;
    let data: ChangelogData = serde_json::from_str(&json).map_err(|e| format!("Invalid changelog cache: {e}"))?;
    if data.releases.is_empty() {
        return Err("Empty changelog cache".to_string());
    }
    Ok(data)
}

/// 解析编译期内置快照；这是网络与磁盘缓存都失败时的最后防线。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:24
fn parse_builtin_changelog() -> Result<ChangelogData, String> {
    serde_json::from_str(BUILTIN_CHANGELOG_JSON).map_err(|e| format!("Invalid builtin changelog: {e}"))
}

/// 统一的数据入口：网络优先，成功即更新缓存；失败先读磁盘缓存，再退回内置快照。
/// 语言参数保留是为了兼容现有 IPC 调用，静态文件本身不区分语言。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:26
async fn load_changelog_data(_lang: &str) -> ChangelogData {
    match fetch_remote_changelog().await {
        Ok(data) => {
            persist_changelog_cache(&data);
            data
        }
        Err(_) => read_cached_changelog().or_else(|_| parse_builtin_changelog()).unwrap_or_default(),
    }
}

/// 版本列表（只有 tag / 名称 / 日期）：从同一份数据派生，前端每次打开面板都会走一次网络优先读取。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:30
pub async fn fetch_changelog_index(lang: &str) -> Result<ChangelogIndex, String> {
    let data = load_changelog_data(lang).await;
    Ok(ChangelogIndex {
        updated_at: data.updated_at,
        releases: data
            .releases
            .into_iter()
            .map(|release| ChangelogReleaseSummary { tag: release.tag, name: release.name, date: release.date })
            .collect(),
    })
}

/// 单个版本的更新日志：tag 先经过格式校验，再从同一份数据里查找。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:32
pub async fn fetch_changelog_release(lang: &str, tag: &str) -> Result<ChangelogRelease, String> {
    let tag = tag.trim();
    if !is_app_release_tag(tag) {
        return Err(format!("Invalid changelog release tag: {tag}"));
    }
    let data = load_changelog_data(lang).await;
    data.releases.into_iter().find(|item| item.tag == tag).ok_or_else(|| format!("Changelog release not found: {tag}"))
}

/// 全量更新日志：一次性需要全部正文的调用方使用，同样走网络优先与缓存兜底。
///
/// @author xiaogui
/// @since 2026-11-25 23:05:34
pub async fn fetch_changelog(lang: &str) -> Result<ChangelogData, String> {
    Ok(load_changelog_data(lang).await)
}

fn build_changelog_http_client() -> Result<reqwest::Client, String> {
    let mut builder =
        reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).user_agent("dbx-changelog");

    if let Some(proxy_url) = crate::update::system_proxy_url() {
        let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| format!("Invalid system proxy URL: {e}"))?;
        builder = builder.proxy(proxy);
    }

    builder.build().map_err(|e| format!("Failed to create HTTP client: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{
        is_app_release_tag, normalize_changelog_lang, parse_builtin_changelog, BUILTIN_CHANGELOG_JSON,
        SELF_CHANGELOG_URL,
    };

    #[test]
    fn normalizes_changelog_lang() {
        assert_eq!(normalize_changelog_lang("cn"), "cn");
        assert_eq!(normalize_changelog_lang("zh-CN"), "cn");
        assert_eq!(normalize_changelog_lang("zh-TW"), "cn");
        assert_eq!(normalize_changelog_lang("en"), "en");
        assert_eq!(normalize_changelog_lang("ja"), "en");
    }

    #[test]
    fn recognizes_only_app_release_tags() {
        assert!(is_app_release_tag("v0.5.66"));
        assert!(is_app_release_tag("v0.6.72.5"));
        assert!(is_app_release_tag("v1.2.3-hotfix.1"));
        assert!(!is_app_release_tag("packages-v0.4.42"));
        assert!(!is_app_release_tag("agents-v0.2.64"));
        assert!(!is_app_release_tag("v0.5"));
        assert!(!is_app_release_tag("v0.5.x"));
    }

    #[test]
    fn reads_changelog_from_self_repository_static_file() {
        assert!(SELF_CHANGELOG_URL.contains("xiaogui-li/dbx"));
        assert!(SELF_CHANGELOG_URL.ends_with("/changelog.json"));
        assert!(!SELF_CHANGELOG_URL.contains("dbxio.com"));
    }

    #[test]
    fn builtin_snapshot_covers_published_releases() {
        assert!(BUILTIN_CHANGELOG_JSON.contains("v0.6.72.5"));
        let data = parse_builtin_changelog().expect("builtin changelog must parse");
        assert!(data.releases.len() >= 4);
        assert!(data.releases.iter().any(|release| release.tag == "v0.6.72.5" && !release.sections.is_empty()));
    }

    #[test]
    fn rejects_tags_that_could_escape_the_release_directory() {
        assert!(!is_app_release_tag("v0.6.62/../../latest"));
        assert!(!is_app_release_tag("v0.6.62?x=1"));
        assert!(!is_app_release_tag("../releases-cn"));
    }
}
