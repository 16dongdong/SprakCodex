import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

// 桌面构建先编译本仓库 DLL，再让 Tauri 复制资源；不依赖兄弟目录或手工预编译文件。
// 仓库根由调用方传入，Cargo 或产物检查失败直接中止，避免打包陈旧 DLL。
export function buildObservationHook(repositoryRoot) {
  if (process.platform !== "win32") return;
  const backendRoot = resolve(repositoryRoot, "backend");
  const result = spawnSync("cargo", [
    "build", "--manifest-path", resolve(backendRoot, "Cargo.toml"),
    "--package", "codexmanager-direct-hook", "--release", "--locked",
    "--target-dir", resolve(backendRoot, "target"),
  ], { cwd: backendRoot, stdio: "inherit", windowsHide: true });
  if (result.error) throw new Error(`启动观测 DLL 构建失败：${result.error.message}`);
  if (result.status !== 0) throw new Error(`观测 DLL 构建失败，退出码：${result.status}`);
  if (!existsSync(resolve(backendRoot, "target/release/cphook.dll"))) {
    throw new Error("观测 DLL 构建完成但产物缺失");
  }
}
