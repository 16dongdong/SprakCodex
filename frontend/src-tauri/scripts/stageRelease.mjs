import { existsSync, readFileSync, renameSync, mkdirSync, copyFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// 发布文件名独立于 Tauri 内部默认命名，旧更新器不会误装改变主程序名称的新版本。
const frontend = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const config = JSON.parse(readFileSync(resolve(frontend, "src-tauri/tauri.conf.json"), "utf8"));
const target = process.argv[2] || "";
if (target && !/^[a-zA-Z0-9-]+$/.test(target)) throw new Error("目标架构名称无效");
const folder = resolve(frontend, "src-tauri/target", target, "release/bundle/nsis");
const source = resolve(folder, `SprakCodex_${config.version}_x64-setup.exe`);
const destination = resolve(folder, `SprakCodex-${config.version}-windows-x64-setup.exe`);
if (existsSync(source)) renameSync(source, destination);
if (process.platform === "win32" && !existsSync(destination)) throw new Error("发布安装包缺失");
if (process.platform === "win32") {
  const delivery = resolve(frontend, "../input");
  mkdirSync(delivery, { recursive: true });
  const packagePath = resolve(delivery, `SprakCodex-${config.version}-windows-x64-setup.exe`);
  copyFileSync(destination, packagePath);
  console.log(packagePath);
}
