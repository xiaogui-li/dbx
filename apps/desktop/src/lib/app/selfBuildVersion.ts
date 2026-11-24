/**
 * 自建构建号：每次自行构建并发布到自有 Release 时递增（从 1 开始）。
 *
 * 前 3 段与上游官方版本对齐，第 4 段用于区分自有构建。
 * 必须与 crates/dbx-platform/src/version.rs 的 SELF_BUILD_NUMBER 保持一致，
 * 由 scripts/check-self-build-version.mjs 在测试阶段校验。
 *
 * @author xiaogui
 * @since 2026-11-24 15:09:44
 */
export const SELF_BUILD_NUMBER = 3;

/**
 * 把上游 3 段版本格式化为自建 4 段版本；已是 4 段时替换为当前构建号。
 *
 * Cargo / Tauri 的编译期版本仍是 3 段 semver，前端展示层在这里补上第 4 段，
 * 使「关于」页与「检查更新」看到的当前版本一致。
 *
 * @author xiaogui
 * @since 2026-11-23 20:36:05
 */
export function formatSelfBuildVersion(version: string): string {
  const normalized = version.trim().replace(/^[vV]/, "");
  const core = normalized.split(/[-+]/)[0].split(".").slice(0, 3);
  while (core.length < 3) core.push("0");
  return `${core.join(".")}.${SELF_BUILD_NUMBER}`;
}
