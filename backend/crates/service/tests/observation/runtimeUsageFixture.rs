//! 仅供显式 ABI 验收：根据匹配的官方 PDB 读取完成事件，不部署到正式应用，不改变参数或返回值。
#![allow(non_snake_case, non_upper_case_globals)]
#![cfg(windows)]
use retour::RawDetour;
use serde_json::json;
use std::{
    ffi::c_void,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{CloseHandle, BOOL, HMODULE, TRUE},
        System::{
            Diagnostics::Debug::ReadProcessMemory,
            LibraryLoader::{GetModuleFileNameW, GetModuleHandleW},
            Threading::{
                CreateEventW, CreateThread, GetCurrentProcess, SetEvent, THREAD_CREATION_FLAGS,
            },
        },
    },
};

const functionRva: usize = 0x063d91e0;
const processAttach: u32 = 1;
const codeViewRva: usize = 0x0eb11620;
const modelPointerOffset: usize = 0x8f8;
const modelLengthOffset: usize = 0x900;
const usageOffset: usize = 0x18;
const readLimit: usize = 512;
const textLimit: usize = 256;
const pdbGuid: [u8; 16] = [
    0x4a, 0x0c, 0x8e, 0x96, 0x7b, 0x09, 0x0c, 0xa8, 0x4c, 0x4c, 0x44, 0x20, 0x50, 0x44, 0x42, 0x2e,
];
type UsageFn = unsafe extern "system-unwind" fn(
    *mut c_void,
    *mut c_void,
    *const u8,
    *const u8,
    usize,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
) -> *mut c_void;
static detour: OnceLock<RawDetour> = OnceLock::new();
static report: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static expectedModel: OnceLock<String> = OnceLock::new();

// 只读取本进程经过边界限制的指定字段；读取失败不会直接解引用无效地址或输出原内存。
fn read(address: usize, length: usize) -> Option<Vec<u8>> {
    if address == 0 || length > readLimit {
        return None;
    }
    let mut bytes = vec![0u8; length];
    let mut count = 0;
    unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            address as *const c_void,
            bytes.as_mut_ptr().cast(),
            length,
            Some(&mut count),
        )
        .ok()?;
    }
    (count == length).then_some(bytes)
}

// 当前验收只覆盖 Windows x64 的 usize；每次从运行时地址读取，不缓存进程地址到下一次运行。
fn word(address: usize) -> Option<usize> {
    Some(usize::from_le_bytes(read(address, 8)?.try_into().ok()?))
}

// Rust String 的布局由对应二进制指令确认；不取得所有权，不释放或改写目标分配。
fn stringAt(address: usize) -> Option<String> {
    textAt(word(address + 8)?, word(address + 16)?)
}

// 仅复制有界 UTF-8 元数据，任何不符合预期的内容均不写入报告。
fn textAt(address: usize, length: usize) -> Option<String> {
    if length == 0 || length > textLimit {
        return None;
    }
    String::from_utf8(read(address, length)?).ok()
}

// 指针仅在原函数调用期间有效，汇总为一次只读采样参数，不在回调结束后保留任何地址。
struct CaptureArguments {
    state: usize,
    thread: usize,
    turn: usize,
    turnLength: usize,
    response: usize,
    usage: usize,
}

// 自身模型字段必须等于测试指定模型；计数还需满足约束，避免把不匹配 ABI 的数据当成用量。
fn capture(arguments: CaptureArguments) -> Option<serde_json::Value> {
    let CaptureArguments {
        state,
        thread,
        turn,
        turnLength,
        response,
        usage,
    } = arguments;
    let model = textAt(
        word(state + modelPointerOffset)?,
        word(state + modelLengthOffset)?,
    )?;
    if Some(&model) != expectedModel.get() {
        return None;
    }
    let response = stringAt(response)?;
    if !response.starts_with("resp_") {
        return None;
    }
    let turn = textAt(turn, turnLength)?;
    let uuid = read(thread, 16)?;
    let hex: String = uuid.iter().map(|byte| format!("{byte:02x}")).collect();
    let thread = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    let bytes = read(usage + usageOffset, 48)?;
    let counts: Vec<i64> = bytes
        .chunks_exact(8)
        .map(|part| i64::from_le_bytes(part.try_into().unwrap()))
        .collect();
    if counts.iter().any(|count| *count < 0)
        || counts[1] > counts[0]
        || counts[4] > counts[3]
        || counts[0].checked_add(counts[3]) != Some(counts[5])
    {
        return None;
    }
    Some(
        json!({"model":model,"threadId":thread,"turnId":turn,"responseId":response,
        "timestampMillis":SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_millis() as u64,
        "usage":{"inputTokens":counts[0],"cachedInputTokens":counts[1],"cacheWriteInputTokens":counts[2],"outputTokens":counts[3],"reasoningOutputTokens":counts[4],"totalTokens":counts[5]}}),
    )
}

// 只写本测试的元数据文件；写入与刷新错误显式返回，由独立验收检查记录完整性，不影响原生成函数结果。
fn writeCapture(captured: &serde_json::Value) -> Result<(), &'static str> {
    let output = report.get().ok_or("报告未初始化")?;
    let mut output = output.lock().map_err(|_| "报告锁损坏")?;
    serde_json::to_writer(&mut *output, captured).map_err(|_| "报告序列化或写入失败")?;
    output.write_all(b"\n").map_err(|_| "报告行结束写入失败")?;
    output.flush().map_err(|_| "报告刷新失败")
}

// 参数位置来自目标指令与源码交叉核对；原函数照常执行，保留可展开 ABI，不拦截其异常或修改返回值。
unsafe extern "system-unwind" fn observed(
    out: *mut c_void,
    state: *mut c_void,
    thread: *const u8,
    turn: *const u8,
    turnLength: usize,
    session: *const u8,
    root: *const u8,
    response: *const u8,
    usage: *const u8,
) -> *mut c_void {
    let captured = std::panic::catch_unwind(|| {
        capture(CaptureArguments {
            state: state as usize,
            thread: thread as usize,
            turn: turn as usize,
            turnLength,
            response: response as usize,
            usage: usage as usize,
        })
    })
    .ok()
    .flatten();
    let original: UsageFn = std::mem::transmute(detour.get().unwrap().trampoline());
    let result = original(
        out, state, thread, turn, turnLength, session, root, response, usage,
    );
    if let Some(captured) = captured {
        if let Err(error) = writeCapture(&captured) {
            eprintln!("ABI 验收记录失败：{error}");
        }
    }
    result
}

// 初始化只对显式测试模式生效，校验 PDB 标识及入口字节后才安装，未知构建不执行补丁。
unsafe extern "system" fn worker(module: *mut c_void) -> u32 {
    if std::env::var("OBSERVATION_TEST_CAPTURE_MODE").as_deref() != Ok("runtime") {
        return 1;
    }
    let main = match GetModuleHandleW(None) {
        Ok(main) => main.0 as usize,
        Err(_) => return 2,
    };
    let Some(signature) = read(main + codeViewRva, 24) else {
        return 3;
    };
    if &signature[..4] != b"RSDS"
        || signature[4..20] != pdbGuid
        || signature[20..24] != 1u32.to_le_bytes()
    {
        return 4;
    }
    let Some(entry) = read(main + functionRva, 16) else {
        return 5;
    };
    if entry
        != [
            0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81,
            0xec, 0x08,
        ]
    {
        return 6;
    }
    let Some(directory) = std::env::var_os("OBSERVATION_TEST_DIRECTORY").map(PathBuf::from) else {
        return 7;
    };
    if !directory.is_absolute() {
        return 8;
    }
    let Ok(output) = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join("runtimeUsage.jsonl"))
    else {
        return 9;
    };
    if report.set(Mutex::new(output)).is_err() {
        return 10;
    }
    let Ok(model) = std::env::var("OBSERVATION_TEST_MODEL") else {
        return 11;
    };
    if expectedModel.set(model).is_err() {
        return 12;
    }
    let Ok(hook) = RawDetour::new((main + functionRva) as *const (), observed as *const ()) else {
        return 13;
    };
    if detour.set(hook).is_err() || detour.get().unwrap().enable().is_err() {
        return 14;
    }
    let mut path = [0u16; 1024];
    let length = GetModuleFileNameW(HMODULE(module), &mut path) as usize;
    if length == 0 || length >= path.len() {
        return 15;
    }
    use std::os::windows::ffi::OsStringExt;
    let path = PathBuf::from(std::ffi::OsString::from_wide(&path[..length]));
    let home = match std::env::var_os("CODEX_HOME") {
        Some(home) => PathBuf::from(home),
        None => return 16,
    };
    static homeMapping: OnceLock<std::os::windows::io::OwnedHandle> = OnceLock::new();
    let Ok(mapping) = cpcommon::runtimeHome::publish(&path, &home) else {
        return 17;
    };
    if homeMapping.set(mapping).is_err() {
        return 18;
    }
    let name = HSTRING::from(cpcommon::hook_ready::event_name(std::process::id(), &path));
    let Ok(event) = CreateEventW(None, true, false, &name) else {
        return 19;
    };
    if SetEvent(event).is_err() {
        let _ = CloseHandle(event);
        return 20;
    }
    0
}

// DllMain 只派发测试初始化线程；线程句柄立即释放，所有实际安装在 loader lock 外执行。
#[no_mangle]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == processAttach {
        unsafe {
            if let Ok(thread) = CreateThread(
                None,
                0,
                Some(worker),
                Some(module.0),
                THREAD_CREATION_FLAGS(0),
                None,
            ) {
                let _ = CloseHandle(thread);
            } else {
                return BOOL(0);
            }
        }
    }
    TRUE
}
