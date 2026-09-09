#![allow(non_snake_case)]

/// 函数 `main`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[cfg(windows)]
fn main() {
    buildEmbeddedObservationModule();
    // 关键：本 crate 既作为独立二进制发布（service 版），也会被桌面端（Tauri）作为依赖引用。
    // 若在依赖构建时也注入 Windows 资源，可能导致链接阶段资源冲突/损坏（例如 LNK1123）。
    // 仅在“主包构建”（`cargo build -p codexmanager-service` / workflow 打包）时才嵌入图标。
    if std::env::var_os("CARGO_PRIMARY_PACKAGE").is_none() {
        return;
    }

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    // 图标路径以当前 crate 为基准跨到前端目录，工作目录变化不影响资源定位。
    let icon_path = manifest_dir.join("../../../frontend/src-tauri/icons/icon.ico");

    println!("cargo:rerun-if-changed={}", icon_path.display());

    if !icon_path.is_file() {
        panic!("Windows icon not found: {}", icon_path.display());
    }

    let mut res = winres::WindowsResource::new();
    res.set_icon(icon_path.to_string_lossy().as_ref());
    res.compile()
        .expect("failed to compile Windows resources (icon)");
}

// 观测模块在服务编译前进入 OUT_DIR，随后由 include_bytes! 链接进所有宿主 EXE；安装包不再携带 DLL 文件。
#[cfg(windows)]
fn buildEmbeddedObservationModule() {
    use std::path::PathBuf;
    use std::process::Command;

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let backend = manifest.join("../..");
    let source = backend.join("crates/directHook/src");
    let common = backend.join("crates/directCommon/src");
    emitRerunForTree(&source);
    emitRerunForTree(&common);
    println!(
        "cargo:rerun-if-changed={}",
        backend.join("crates/directHook/Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        backend.join("crates/directCommon/Cargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        backend.join("Cargo.lock").display()
    );

    let target = backend.join("target/embeddedObservation");
    let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .arg("build")
        .arg("--manifest-path")
        .arg(backend.join("Cargo.toml"))
        .arg("--package")
        .arg("codexmanager-direct-hook")
        .arg("--release")
        .arg("--locked")
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("启动内嵌观测模块构建失败");
    if !status.success() {
        panic!("内嵌观测模块构建失败：{status}");
    }
    let artifact = target.join("release/cphook.dll");
    let output = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("observationHook.dll");
    std::fs::copy(&artifact, &output).unwrap_or_else(|error| {
        panic!(
            "复制内嵌观测模块失败：{} -> {}：{error}",
            artifact.display(),
            output.display()
        )
    });
}

// Cargo 对目录元数据的判定在不同文件系统上不稳定；逐文件声明确保任一载荷源码变化都会重建宿主内嵌字节。
#[cfg(windows)]
fn emitRerunForTree(directory: &std::path::Path) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| {
            panic!("读取内嵌模块源码目录失败：{}：{error}", directory.display())
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("枚举内嵌模块源码失败：{error}"));
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            emitRerunForTree(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

/// 函数 `main`
///
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[cfg(not(windows))]
fn main() {}
