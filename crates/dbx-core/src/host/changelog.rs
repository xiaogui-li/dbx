use serde::{Deserialize, Serialize};

/// 自有仓库发布历史 API：更新管理里的版本列表与更新日志均从这里读取，
/// 不再使用官方 R2 CDN 的 changelog 静态文件。
///
/// @author xiaogui
/// @since 2026-11-25 20:15:18
const SELF_RELEASES_API: &str = "https://api.github.com/repos/xiaogui-li/dbx/releases";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogData {
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub releases: Vec<ChangelogRelease>,
}

/// Normalize UI locale / short lang into `cn` or `en` (matches R2 file naming).
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

/// GitHub Release 的最小字段集合，只取更新管理实际用到的内容。
///
/// @author xiaogui
/// @since 2026-11-25 20:15:20
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// 拉取自有仓库的正式发布历史：过滤草稿与预发布，只保留应用版本 tag。
/// GitHub 默认按发布时间倒序返回，正好满足更新管理的版本列表顺序。
///
/// @author xiaogui
/// @since 2026-11-25 20:15:22
async fn fetch_self_releases() -> Result<Vec<GithubRelease>, String> {
    let url = format!("{SELF_RELEASES_API}?per_page=100");
    let releases: Vec<GithubRelease> = fetch_changelog_json(&url).await?;
    Ok(releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease && is_app_release_tag(release.tag_name.trim()))
        .collect())
}

/// 把 Release 的 Markdown 正文按标题切成前端可直接渲染的分组。
/// 标题保留原文（如 `feat: ...`、`fix: ...`、`新功能`），列表项作为条目；
/// 空分组不展示，引用行与一级标题忽略。
///
/// @author xiaogui
/// @since 2026-11-25 20:15:24
fn parse_release_sections(body: &str) -> Vec<ChangelogSection> {
    let mut sections: Vec<ChangelogSection> = Vec::new();
    let mut current: Option<ChangelogSection> = None;

    for line in body.lines() {
        let line = line.trim();
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        if (2..=4).contains(&hashes) {
            if let Some(title) = line[hashes..].strip_prefix(' ') {
                let title = title.trim();
                if !title.is_empty() {
                    if let Some(section) = current.take() {
                        if !section.items.is_empty() {
                            sections.push(section);
                        }
                    }
                    current = Some(ChangelogSection {
                        r#type: title.to_string(),
                        title: title.to_string(),
                        items: Vec::new(),
                    });
                    continue;
                }
            }
        }
        let item = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
        if let (Some(item), Some(section)) = (item, current.as_mut()) {
            let item = item.trim();
            if !item.is_empty() {
                section.items.push(ChangelogItem { title: item.to_string(), desc: String::new() });
            }
        }
    }
    if let Some(section) = current {
        if !section.items.is_empty() {
            sections.push(section);
        }
    }
    sections
}

async fn fetch_changelog_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let client = build_changelog_http_client()?;
    let resp = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "dbx-changelog")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Failed to fetch changelog: {e}"))?;

    resp.json().await.map_err(|e| format!("Failed to parse changelog: {e}"))
}

/// 版本列表（只有 tag / 名称 / 日期）：更新中心与回滚列表只需要这三项，
/// 不必先下载全部 Release 正文。数据源为自有仓库的发布历史。
///
/// @author xiaogui
/// @since 2026-11-25 20:15:30
pub async fn fetch_changelog_index(_lang: &str) -> Result<ChangelogIndex, String> {
    let releases = fetch_self_releases().await?;
    Ok(ChangelogIndex {
        updated_at: String::new(),
        releases: releases
            .into_iter()
            .map(|release| ChangelogReleaseSummary {
                tag: release.tag_name.trim().to_string(),
                name: release.name,
                date: release.published_at,
            })
            .collect(),
    })
}

/// 单个版本的更新日志：用户展开某版本时才按需解析其 Release 正文。
/// tag 先经过格式校验，避免构造出非应用版本的查询。
///
/// @author xiaogui
/// @since 2026-11-25 20:15:32
pub async fn fetch_changelog_release(_lang: &str, tag: &str) -> Result<ChangelogRelease, String> {
    let tag = tag.trim();
    if !is_app_release_tag(tag) {
        return Err(format!("Invalid changelog release tag: {tag}"));
    }
    let release = fetch_self_releases()
        .await?
        .into_iter()
        .find(|item| item.tag_name.trim() == tag)
        .ok_or_else(|| format!("Changelog release not found: {tag}"))?;
    Ok(ChangelogRelease {
        tag: release.tag_name.trim().to_string(),
        name: release.name,
        date: release.published_at,
        sections: parse_release_sections(&release.body),
    })
}

/// 全量更新日志：桌面端走上面的「索引 + 按需单版本」，这里留给一次性
/// 需要全部正文的调用方。数据源同样为自有仓库的发布历史。
///
/// @author xiaogui
/// @since 2026-11-25 20:15:34
pub async fn fetch_changelog(_lang: &str) -> Result<ChangelogData, String> {
    let releases = fetch_self_releases().await?;
    Ok(ChangelogData {
        updated_at: String::new(),
        releases: releases
            .into_iter()
            .map(|release| ChangelogRelease {
                tag: release.tag_name.trim().to_string(),
                name: release.name,
                date: release.published_at,
                sections: parse_release_sections(&release.body),
            })
            .collect(),
    })
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
    use super::{is_app_release_tag, normalize_changelog_lang, parse_release_sections, SELF_RELEASES_API};

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
    fn reads_release_history_from_self_repository() {
        assert!(SELF_RELEASES_API.contains("xiaogui-li/dbx"));
        assert!(!SELF_RELEASES_API.contains("dbxio.com"));
    }

    #[test]
    fn parses_release_markdown_into_sections() {
        let body = "# DBX v0.6.72.5\n\n> 基于上游构建\n\n## 一、本仓库改动\n\n### fix: 修复某问题\n- 第一条\n- 第二条\n\n## 二、上游更新\n\n### 新功能\n- 上游条目\n";
        let sections = parse_release_sections(body);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "fix: 修复某问题");
        assert_eq!(sections[0].items.len(), 2);
        assert_eq!(sections[0].items[0].title, "第一条");
        assert_eq!(sections[1].title, "新功能");
        assert_eq!(sections[1].items.len(), 1);
    }

    #[test]
    fn rejects_tags_that_could_escape_the_release_directory() {
        assert!(!is_app_release_tag("v0.6.62/../../latest"));
        assert!(!is_app_release_tag("v0.6.62?x=1"));
        assert!(!is_app_release_tag("../releases-cn"));
    }
}
