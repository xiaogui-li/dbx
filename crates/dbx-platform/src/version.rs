pub fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

pub fn parse_version(version: &str) -> Vec<u64> {
    normalize_version(version).split(['.', '-', '+']).map(|part| part.parse::<u64>().unwrap_or(0)).collect()
}

/// 自建构建号：每次自行构建并发布到自有 Release 时递增（从 1 开始）。
///
/// 前 3 段与上游官方版本保持一致，第 4 段用于区分自有构建，
/// 使自有构建之间也能通过「检查更新」正确排序。
///
/// @author xiaogui
/// @since 2026-11-26 09:24:10
pub const SELF_BUILD_NUMBER: u32 = 7;

/// 生成自建 4 段版本号：取传入版本的前 3 段，追加 [`SELF_BUILD_NUMBER`] 作为第 4 段。
///
/// 对已经是 4 段的输入保持幂等（始终返回当前构建号），避免重复拼接导致版本串变形。
///
/// @author xiaogui
/// @since 2026-11-23 20:35:46
pub fn self_build_version(version: &str) -> String {
    let base = base_version(&normalize_version(version));
    let mut parts = base.splitn(4, '.');
    let major = parts.next().unwrap_or("0");
    let minor = parts.next().unwrap_or("0");
    let patch = parts.next().unwrap_or("0");
    format!("{major}.{minor}.{patch}.{SELF_BUILD_NUMBER}")
}

/// 把 4 段数字版本（如 `0.6.72.2`）映射成合法 semver（`0.6.72+self.2`）。
///
/// Cargo / Tauri / `semver::Version` 均强制 3 段，便携版更新与 updater 清单解析
/// 仍需要 `semver::Version`；build metadata 可无损承载第 4 段且不影响原有 3 段语义。
/// 非 4 段输入原样返回。
///
/// @author xiaogui
/// @since 2026-11-23 20:35:47
pub fn to_semver_compatible(version: &str) -> String {
    let normalized = normalize_version(version);
    let parts: Vec<&str> = normalized.split('.').collect();
    if parts.len() == 4 && parts.iter().all(|part| part.parse::<u64>().is_ok()) {
        return format!("{}.{}.{}+self.{}", parts[0], parts[1], parts[2], parts[3]);
    }
    normalized
}

/// 从 [`to_semver_compatible`] 产生的 semver 还原自建 4 段版本字符串。
///
/// 普通 semver 版本（含官方 beta 预发布）原样返回其字符串形式。
///
/// @author xiaogui
/// @since 2026-11-23 20:35:48
pub fn from_semver_compatible(version: &semver::Version) -> String {
    if let Some(build) = version.build.as_str().strip_prefix("self.") {
        return format!("{}.{}.{}.{}", version.major, version.minor, version.patch, build);
    }
    version.to_string()
}

pub fn base_version(version: &str) -> String {
    normalize_version(version).split(['-', '+']).next().unwrap_or_default().to_string()
}

pub fn is_version_compatible(min_version: &str, current_version: &str) -> bool {
    !is_newer_version(&base_version(min_version), &base_version(current_version))
}

/// Base version to display for a build that was promoted to stable without
/// being rebuilt.
///
/// A promoted beta keeps its `X.Y.Z-beta.YYYYMMDD.N` version inside the binary
/// while the release is now the stable `X.Y.Z`. Returns `Some("X.Y.Z")` only
/// when `promoted_from` names this exact build and it carries a prerelease
/// suffix; every other combination — a real beta, a plain stable build, or a
/// promotion of a different build — returns `None` so callers keep the raw
/// version.
pub fn promoted_base_version(build: &str, promoted_from: Option<&str>) -> Option<String> {
    let promoted_from = promoted_from?.trim();
    if promoted_from.is_empty() {
        return None;
    }
    let build = normalize_version(build);
    if build.is_empty() || build != normalize_version(promoted_from) {
        return None;
    }
    let base = base_version(&build);
    (!base.is_empty() && base != build).then_some(base)
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    if let (Ok(latest), Ok(current)) =
        (semver::Version::parse(&normalize_version(latest)), semver::Version::parse(&normalize_version(current)))
    {
        return latest > current;
    }
    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);
    let max_len = latest_parts.len().max(current_parts.len());

    for i in 0..max_len {
        let latest_part = *latest_parts.get(i).unwrap_or(&0);
        let current_part = *current_parts.get(i).unwrap_or(&0);
        if latest_part > current_part {
            return true;
        }
        if latest_part < current_part {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证自建 4 段版本号的生成、semver 兼容映射与逐段比较。
    ///
    /// @author xiaogui
    /// @since 2026-11-24 10:23:42
    #[test]
    fn builds_and_compares_self_build_versions() {
        assert_eq!(self_build_version("0.6.72"), "0.6.72.2");
        assert_eq!(self_build_version("v0.6.72-beta.20261122.1"), "0.6.72.2");
        assert_eq!(self_build_version("0.6.72.7"), "0.6.72.2");
        assert_eq!(to_semver_compatible("0.6.72.2"), "0.6.72+self.2");
        assert_eq!(to_semver_compatible("0.6.72"), "0.6.72");
        assert_eq!(from_semver_compatible(&semver::Version::parse("0.6.72+self.2").unwrap()), "0.6.72.2");
        assert!(is_newer_version("0.6.72.2", "0.6.72.1"));
        assert!(!is_newer_version("0.6.72.1", "0.6.72.2"));
        assert!(!is_newer_version("0.6.72.1", "0.6.72.1"));
    }

    #[test]
    fn normalizes_tag_versions() {
        assert_eq!(normalize_version("v1.2.3"), "1.2.3");
        assert_eq!(normalize_version(" 0.2.0 "), "0.2.0");
    }

    #[test]
    fn compares_semver_like_versions() {
        assert!(is_newer_version("0.2.1", "0.2.0"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(!is_newer_version("0.2.0", "0.2.0"));
        assert!(!is_newer_version("0.1.9", "0.2.0"));
        assert!(is_newer_version("0.6.60-beta.20261028.1", "0.6.59"));
        assert!(is_newer_version("0.6.60-beta.20261029.1", "0.6.60-beta.20261028.1"));
        assert!(is_newer_version("0.6.60", "0.6.60-beta.20261028.1"));
        assert!(!is_newer_version("0.6.59", "0.6.60-beta.20261028.1"));
    }

    #[test]
    fn compares_application_compatibility_by_base_version() {
        assert_eq!(base_version("v0.6.60-beta.20261029.1"), "0.6.60");
        assert_eq!(base_version("0.6.60+build.1"), "0.6.60");
        assert!(is_version_compatible("0.6.60", "0.6.60-beta.20261029.1"));
        assert!(is_version_compatible("0.6.59", "0.6.60-beta.20261029.1"));
        assert!(!is_version_compatible("0.6.61", "0.6.60-beta.20261029.1"));
    }

    #[test]
    fn resolves_the_stable_version_of_a_promoted_build() {
        const BUILD: &str = "0.6.67-beta.20261105.1";
        assert_eq!(promoted_base_version(BUILD, Some(BUILD)).as_deref(), Some("0.6.67"));
        assert_eq!(promoted_base_version(BUILD, Some("v0.6.67-beta.20261105.1")).as_deref(), Some("0.6.67"));
        assert_eq!(promoted_base_version(&format!(" {BUILD} "), Some(BUILD)).as_deref(), Some("0.6.67"));
    }

    #[test]
    fn keeps_the_build_version_when_the_promotion_does_not_name_it() {
        const BUILD: &str = "0.6.67-beta.20261105.1";
        let other = "0.6.67-beta.20261105.2";
        assert_eq!(promoted_base_version(BUILD, Some(other)), None);
        assert_eq!(promoted_base_version(other, Some(BUILD)), None);
        assert_eq!(promoted_base_version(BUILD, None), None);
        assert_eq!(promoted_base_version(BUILD, Some("")), None);
        assert_eq!(promoted_base_version(BUILD, Some("   ")), None);
        // A plain stable build was never a promoted prerelease.
        assert_eq!(promoted_base_version("0.6.67", Some("0.6.67")), None);
        assert_eq!(promoted_base_version("", Some(BUILD)), None);
    }
}
