#!/usr/bin/env node
/**
 * 校验自建构建号在 Rust / 前端 / NSIS 三处保持一致，
 * 并确认前 3 段基线版本与 package.json、Cargo.toml、tauri.conf.json 对齐。
 *
 * 4 段版本号不能写入 Cargo / Tauri 的编译期版本字段（semver 强制 3 段），
 * 因此构建号只能以常量形式分散保存，用本脚本防止漏改其中一处。
 *
 * @author xiaogui
 * @since 2026-11-23 20:36:08
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

function matchOne(content, pattern, label) {
  const match = content.match(pattern);
  if (!match) throw new Error(`未找到 ${label} 中的构建号定义`);
  return match[1];
}

const buildNumbers = {
  rust: matchOne(read("crates/dbx-platform/src/version.rs"), /SELF_BUILD_NUMBER:\s*u32\s*=\s*(\d+)/, "crates/dbx-platform/src/version.rs"),
  frontend: matchOne(read("apps/desktop/src/lib/app/selfBuildVersion.ts"), /SELF_BUILD_NUMBER\s*=\s*(\d+)/, "apps/desktop/src/lib/app/selfBuildVersion.ts"),
  nsis: matchOne(read("src-tauri/windows/nsis/installer.nsi"), /!define SELF_BUILD_NUMBER\s+"(\d+)"/, "src-tauri/windows/nsis/installer.nsi"),
};

const unique = new Set(Object.values(buildNumbers));
if (unique.size !== 1) {
  throw new Error(`自建构建号不一致：${JSON.stringify(buildNumbers)}`);
}

const packageVersion = JSON.parse(read("package.json")).version;
const cargoVersion = matchOne(read("src-tauri/Cargo.toml"), /^version\s*=\s*"([^"]+)"/m, "src-tauri/Cargo.toml");
const tauriVersion = JSON.parse(read("src-tauri/tauri.conf.json")).version;
const coreVersion = packageVersion.split(/[-+]/)[0];
if (packageVersion !== cargoVersion || packageVersion !== tauriVersion) {
  throw new Error(
    `上游基线版本不一致：package.json=${packageVersion} Cargo.toml=${cargoVersion} tauri.conf.json=${tauriVersion}`,
  );
}
if (!/^\d+\.\d+\.\d+$/.test(coreVersion)) {
  throw new Error(`上游基线版本必须是 3 段 semver：${coreVersion}`);
}

console.log(
  `自建版本 ${coreVersion}.${buildNumbers.rust}（Rust / 前端 / NSIS 构建号一致，上游基线 ${packageVersion}）`,
);
