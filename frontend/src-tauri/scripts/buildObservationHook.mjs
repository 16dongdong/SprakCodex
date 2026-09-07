import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

// 桌面构建先编译本仓库 DLL，再让 Tauri 复制资源；不依赖兄弟目录或手工预编译文件。
// 仓库根由调用方传入；独立目标目录避开旧开发客户端可能映射的 release DLL，失败直接中止而不打包旧文件。
export function buildObservationHook(repositoryRoot) {
  if (process.platform !== "win32") return;
  const backendRoot = resolve(repositoryRoot, "backend");
  const result = spawnSync("cargo", [
    "build", "--manifest-path", resolve(backendRoot, "Cargo.toml"),
    "--package", "codexmanager-direct-hook", "--release", "--locked",
    "--target-dir", resolve(backendRoot, "target/observationBuild"),
  ], { cwd: backendRoot, stdio: "inherit", windowsHide: true });
  if (result.error) throw new Error(`启动观测 DLL 构建失败：${result.error.message}`);
  if (result.status !== 0) throw new Error(`观测 DLL 构建失败，退出码：${result.status}`);
  if (!existsSync(resolve(backendRoot, "target/observationBuild/release/cphook.dll"))) {
    throw new Error("观测 DLL 构建完成但产物缺失");
  }
}
