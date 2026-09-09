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
