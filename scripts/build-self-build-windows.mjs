#!/usr/bin/env node
/**
 * 自建 Windows 安装包构建器：把产物文件名与 MSI 版本信息统一为 4 段式版本。
 *
 * Cargo / Tauri 的编译期版本字段强制 3 段 semver，无法直接写 0.6.72.1；
 * 本脚本通过 `--config` 注入 4 段 MSI ProductVersion，并在构建完成后把
 * bundler 生成的 3 段文件名（DBX_0.6.72_...）重命名为 4 段（DBX_0.6.72.1_...）。
 * 编译期版本、应用显示版本与更新链路均保持不变。
 *
 * @author xiaogui
 * @since 2026-11-24 09:19:07
 */
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CONFIG_PATH = join(ROOT, ".workbuddy", "self-build", "tauri.windows.conf.json");
const TAURI_CLI = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js");
const BUNDLE_ROOTS = [
  join(ROOT, "target", "release", "bundle"),
  join(ROOT, "src-tauri", "target", "release", "bundle"),
];

/**
 * 通过版本一致性校验脚本读取当前自建 4 段版本，复用单一真相源。
 *
 * 校验脚本会同时核对 Rust / 前端 / NSIS 三处构建号与上游基线版本，
 * 因此这里无需重复解析常量，任一处漏改都会直接让构建失败。
 *
 * @author xiaogui
 * @since 2026-11-24 08:28:46
 * @returns {string} 形如 0.6.72.1 的自建版本
 */
function readSelfBuildVersion() {
  const probe = spawnSync(process.execPath, [join(ROOT, "scripts", "check-self-build-version.mjs")], {
    cwd: ROOT,
    encoding: "utf8",
  });
  if (probe.status !== 0) {
    throw new Error(`版本一致性校验失败：${probe.stderr || probe.stdout}`);
  }
  const match = probe.stdout.match(/自建版本\s+(\d+\.\d+\.\d+\.\d+)/);
  if (!match) throw new Error(`无法解析自建版本：${probe.stdout}`);
  return match[1];
}

/**
 * 生成供 `tauri build --config` 合并的临时配置，只覆盖 MSI ProductVersion。
 *
 * WiX 接受 4 段数字版本（第 4 段仅作信息展示，Windows Installer 比较时忽略），
 * 该配置与 CI 的 prepare-msi-config.mjs 使用同一字段；文件落在被 gitignore 的
 * .workbuddy 目录，不进入版本库，也不改动 upstream 高频的 tauri.conf.json。
 *
 * @author xiaogui
 * @since 2026-11-24 08:28:47
 * @param {string} fullVersion 自建 4 段版本
 * @returns {string} 生成的配置文件绝对路径
 */
function writeWindowsBundleConfig(fullVersion) {
  mkdirSync(dirname(CONFIG_PATH), { recursive: true });
  const config = { bundle: { windows: { wix: { version: fullVersion } } } };
  writeFileSync(CONFIG_PATH, `${JSON.stringify(config, null, 2)}\n`);
  return CONFIG_PATH;
}

/**
 * 把 bundle 目录中 3 段命名的产物重命名为 4 段命名（含 .sig / .zip 附属文件）。
 *
 * 幂等：已是 4 段命名的文件原样保留；同名目标存在时先移除旧产物再改名，
 * 保证连续多次构建不会累积两份安装包。仅处理 DBX_<基线版本>_ 前缀的文件。
 *
 * @author xiaogui
 * @since 2026-11-24 08:28:48
 * @param {string} baseVersion 上游 3 段基线版本
 * @param {string} fullVersion 自建 4 段版本
 * @returns {{renamed: string[], present: Array<{path: string, bytes: number}>}}
 */
export function applySelfBuildArtifactNames(baseVersion, fullVersion) {
  const fromPrefix = `DBX_${baseVersion}_`;
  const toPrefix = `DBX_${fullVersion}_`;
  const renamed = [];
  const present = [];
  for (const bundleRoot of BUNDLE_ROOTS) {
    if (!existsSync(bundleRoot)) continue;
    for (const kind of ["nsis", "msi"]) {
      const folder = join(bundleRoot, kind);
      if (!existsSync(folder)) continue;
      for (const name of readdirSync(folder)) {
        const source = join(folder, name);
        if (!statSync(source).isFile()) continue;
        let targetName = name;
        if (name.startsWith(fromPrefix)) {
          targetName = toPrefix + name.slice(fromPrefix.length);
        }
        if (!targetName.startsWith(toPrefix)) continue;
        const target = join(folder, targetName);
        if (target !== source) {
          if (existsSync(target)) rmSync(target, { force: true });
          renameSync(source, target);
          renamed.push(`${name} -> ${targetName}`);
        }
        present.push({ path: target, bytes: statSync(target).size });
      }
    }
  }
  return { renamed, present };
}

/**
 * 调用 Tauri CLI 构建全部 Windows bundle。
 *
 * 直接执行 node_modules 内的 tauri.js，避免不同 shell 下 pnpm shim 解析差异；
 * beforeBuildCommand 仍由 Tauri CLI 自行调用，环境变量由当前进程继承。
 *
 * @author xiaogui
 * @since 2026-11-24 08:28:49
 * @param {string} configPath 注入 4 段 MSI 版本的配置文件路径
 * @returns {number} Tauri CLI 退出码
 */
function runTauriBuild(configPath) {
  if (!existsSync(TAURI_CLI)) {
    throw new Error(`未找到 Tauri CLI 入口：${TAURI_CLI}`);
  }
  const result = spawnSync(process.execPath, [TAURI_CLI, "build", "--config", configPath], {
    cwd: ROOT,
    stdio: "inherit",
    env: process.env,
  });
  if (result.error) throw result.error;
  return result.status ?? 1;
}

/**
 * 用构建私钥对 4 段命名的主产物重新签名，使 .sig 内记录的文件名与实际产物一致。
 *
 * Tauri 在签名时把原始（3 段）文件名写进签名的 trusted comment；重命名后
 * 签名内容仍有效（验签只比对文件内容），但元数据会自相矛盾。仅当本次进程
 * 能拿到私钥且产物已有 .sig 时重签，缺失私钥的纯打包场景自动跳过。
 *
 * @author xiaogui
 * @since 2026-11-24 09:19:09
 * @param {Array<{path: string, bytes: number}>} present 4 段命名的产物清单
 * @returns {string[]} 已完成重签名的文件名
 */
function resignSelfBuildArtifacts(present) {
  const hasKey = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY || process.env.TAURI_SIGNING_PRIVATE_KEY_PATH);
  if (!hasKey || !existsSync(TAURI_CLI)) return [];
  const resigned = [];
  for (const item of present) {
    if (!/\.(exe|msi)$/i.test(item.path)) continue;
    if (!existsSync(`${item.path}.sig`)) continue;
    const result = spawnSync(process.execPath, [TAURI_CLI, "signer", "sign", item.path], {
      cwd: ROOT,
      encoding: "utf8",
      env: process.env,
    });
    if (result.status === 0) {
      resigned.push(basename(item.path));
    } else {
      console.warn(`重签名失败 ${item.path}: ${(result.stderr || result.stdout || "").trim()}`);
    }
  }
  return resigned;
}

/**
 * 命令入口：默认完成「注入配置 → 构建 → 重命名」全流程。
 *
 * 支持 --prepare-only（只生成配置）与 --apply-only（只整理已有产物）；
 * 即使 Tauri 退出码非 0，仍会整理已生成的产物，便于排查签名等后处理失败。
 * 产物整理后按 4 段文件名重签名，保证签名元数据与实际交付文件一致。
 *
 * @author xiaogui
 * @since 2026-11-24 09:19:10
 * @returns {number} 进程退出码
 */
function main() {
  const args = process.argv.slice(2);
  const fullVersion = readSelfBuildVersion();
  const baseVersion = fullVersion.split(".").slice(0, 3).join(".");
  const configPath = writeWindowsBundleConfig(fullVersion);
  console.log(`自建版本 ${fullVersion}；MSI 4 段版本配置 ${configPath}`);
  if (args.includes("--prepare-only")) return 0;

  let status = 0;
  if (!args.includes("--apply-only")) status = runTauriBuild(configPath);

  const { renamed, present } = applySelfBuildArtifactNames(baseVersion, fullVersion);
  for (const item of renamed) console.log(`重命名 ${item}`);
  if (!present.length) console.warn("未找到 Windows 安装包产物");
  for (const name of resignSelfBuildArtifacts(present)) console.log(`重签名 ${name}`);
  for (const item of present) {
    console.log(`产物 ${item.path}  ${statSync(item.path).size} bytes`);
  }
  if (status !== 0) {
    console.warn(`tauri build 退出码 ${status}（产物已按 4 段命名整理）`);
  }
  return status;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
