#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(non_snake_case)]
mod updateRun;
#[cfg(windows)]
mod watchdog;

// 无参数或未知模式直接失败；旧单作业参数继续兼容已安装版本发起的更新。
fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = if arguments.first().and_then(|value| value.to_str()) == Some("watchdog") {
        #[cfg(windows)]
        {
            watchdog::run(&arguments[1..])
        }
        #[cfg(not(windows))]
        {
            Err("看门狗仅支持 Windows".into())
        }
    } else if arguments.len() == 1 {
        updateRun::runLegacy(std::path::Path::new(&arguments[0]))
    } else {
        Err("updateAgent 启动参数无效".into())
    };
    if let Err(error) = result {
        if recordFailure(&arguments, &error).is_err() {
            std::process::exit(3);
        }
        std::process::exit(1);
    }
}

// 失败日志只写调用方已经指定的作业或看门狗日志位置，不把诊断散落到安装目录。
fn recordFailure(arguments: &[std::ffi::OsString], error: &str) -> Result<(), String> {
    use std::io::Write;
    let path = if arguments.first().and_then(|value| value.to_str()) == Some("watchdog") {
        arguments.get(2).map(std::path::PathBuf::from)
    } else {
        arguments
            .first()
            .map(std::path::PathBuf::from)
            .map(|path| path.with_extension("result.log"))
    };
    let Some(path) = path else {
        return Err("缺少失败日志路径".into());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|failure| failure.to_string())?;
    }
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|failure| failure.to_string())?;
    writeln!(log, "失败：{error}").map_err(|failure| failure.to_string())
}
