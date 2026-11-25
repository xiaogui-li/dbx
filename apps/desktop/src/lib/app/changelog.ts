export type ChangelogItem = {
  title: string;
  desc: string;
};

export type ChangelogSection = {
  type: string;
  title: string;
  items: ChangelogItem[];
};

export type ChangelogRelease = {
  tag: string;
  name: string;
  date: string;
  sections: ChangelogSection[];
};

/** One row of the published changelog index: no release body. */
export type ChangelogReleaseSummary = {
  tag: string;
  name: string;
  date: string;
};

export type ChangelogIndexData = {
  updatedAt: string;
  releases: ChangelogReleaseSummary[];
};

export type ChangelogData = {
  updatedAt: string;
  releases: ChangelogRelease[];
};

export type ChangelogLang = "en" | "cn";

/**
 * 前端不再保留内存缓存：每次读取都真实调用后端，由后端执行「网络优先、失败回退磁盘缓存」。
 * 保留该函数是为了兼容调用方，语义变为无需操作。
 *
 * @author xiaogui
 * @since 2026-11-25 23:07:10
 */
export function clearChangelogCache(_lang?: ChangelogLang) {
  // 缓存已下沉到 Rust 层磁盘缓存，前端无状态可清。
}

export function createLatestRequestGuard() {
  let latestRequestId = 0;
  return {
    begin: () => ++latestRequestId,
    isCurrent: (requestId: number) => requestId === latestRequestId,
  };
}

export function changelogLangFromLocale(locale: string): ChangelogLang {
  return locale === "zh-CN" || locale === "zh-TW" ? "cn" : "en";
}

export function changelogWebsiteUrl(lang: ChangelogLang): string {
  return `https://dbxio.com/${lang}/changelog`;
}

export function changelogReleaseUrl(tag: string): string {
  return `https://github.com/xiaogui-li/dbx/releases/tag/${encodeURIComponent(tag)}`;
}

/**
 * 自有构建仓库首页：更新管理里与「在官网查看」并列的跳转目标。
 * 官网按钮保持原动作不变，这里只新增自有仓库入口。
 *
 * @author xiaogui
 * @since 2026-11-25 23:09:20
 */
export function selfBuildRepositoryUrl(): string {
  return "https://github.com/xiaogui-li/dbx";
}

/**
 * Tag to show in version lists. A beta promoted to stable is not renamed —
 * the release keeps its beta tag while its name already reads `DBX vX.Y.Z` —
 * so a stable user would read a beta identifier for what is actually the
 * stable release (#14739). Genuine beta releases never reach the index
 * (the sync only keeps non-prerelease releases), so a `-beta.` tag here is
 * always a promoted build: show its stable base version. The raw tag stays
 * the fetch and link key; only the label changes.
 */
export function displayChangelogTag(tag: string): string {
  const trimmed = tag.trim();
  const match = trimmed.match(/^(v[0-9]+\.[0-9]+\.[0-9]+)-beta\..*$/);
  return match ? match[1] : trimmed;
}

/**
 * Version list: every call goes to the backend, which re-fetches the static
 * changelog.json and falls back to its disk cache (or builtin snapshot) on error.
 */
export async function fetchChangelogIndex(lang: ChangelogLang, _options: { force?: boolean } = {}): Promise<ChangelogIndexData> {
  return loadChangelogIndex(lang);
}

/** Release body for one version, read fresh from the backend on every call. */
export async function fetchChangelogRelease(lang: ChangelogLang, tag: string, _options: { force?: boolean } = {}): Promise<ChangelogRelease> {
  return loadChangelogRelease(lang, tag);
}

/** Full feed in one request; kept for callers that need every release body at once. */
export async function fetchChangelog(lang: ChangelogLang, _options: { force?: boolean } = {}): Promise<ChangelogData> {
  return loadChangelog(lang);
}

async function loadChangelogIndex(lang: ChangelogLang): Promise<ChangelogIndexData> {
  const { fetchChangelogIndex: fetchIndexViaBackend } = await import("@/lib/backend/api");
  const data = await fetchIndexViaBackend(lang);
  if (!data || !Array.isArray(data.releases)) {
    throw new Error("Invalid changelog index payload");
  }

  return {
    updatedAt: typeof data.updatedAt === "string" ? data.updatedAt : "",
    releases: data.releases.filter((release) => release && typeof release.tag === "string"),
  };
}

async function loadChangelogRelease(lang: ChangelogLang, tag: string): Promise<ChangelogRelease> {
  const { fetchChangelogRelease: fetchReleaseViaBackend } = await import("@/lib/backend/api");
  const data = await fetchReleaseViaBackend(lang, tag);
  if (!data || typeof data.tag !== "string") {
    throw new Error("Invalid changelog release payload");
  }

  return {
    tag: data.tag,
    name: typeof data.name === "string" ? data.name : "",
    date: typeof data.date === "string" ? data.date : "",
    sections: Array.isArray(data.sections) ? data.sections.filter((section) => section && Array.isArray(section.items)) : [],
  };
}

async function loadChangelog(lang: ChangelogLang): Promise<ChangelogData> {
  const { fetchChangelog: fetchChangelogViaBackend } = await import("@/lib/backend/api");
  const data = await fetchChangelogViaBackend(lang);
  if (!data || !Array.isArray(data.releases)) {
    throw new Error("Invalid changelog payload");
  }

  return {
    updatedAt: typeof data.updatedAt === "string" ? data.updatedAt : "",
    releases: data.releases.filter((release) => release && typeof release.tag === "string"),
  };
}
