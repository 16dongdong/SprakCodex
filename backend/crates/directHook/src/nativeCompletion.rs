//! 匹配官方 Windows x64 构建的完成事件入口；只复制计量元数据，原函数参数、返回值和展开语义保持不变。
use cpcommon::completionSpool::{self, Completion};
use std::{
    ffi::c_void,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use windows::Win32::System::{
    Diagnostics::Debug::ReadProcessMemory, LibraryLoader::GetModuleHandleW,
    Threading::GetCurrentProcess,
};

const functionRva: usize = 0x063d91e0;
const codeViewRva: usize = 0x0eb11620;
const modelPointerOffset: usize = 0x8f8;
const modelLengthOffset: usize = 0x900;
const modelCapacityOffset: usize = 0x8f0;
const configurationArcOffset: usize = 0x2f8;
const providerPointerOffset: usize = 0x2b88;
const providerLengthOffset: usize = 0x2b90;
const usageOffset: usize = 0x18;
const pdbGuid: [u8; 16] = [
    0x4a, 0x0c, 0x8e, 0x96, 0x7b, 0x09, 0x0c, 0xa8, 0x4c, 0x4c, 0x44, 0x20, 0x50, 0x44, 0x42, 0x2e,
];
const entryBytes: [u8; 16] = [
    0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xec, 0x08,
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
static detour: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();

// 安装在线程初始化阶段、loader lock 外执行；未知 PDB 或入口返回未支持，绝不对其他构建猜偏移。
pub(super) fn install() -> Result<bool, &'static str> {
    let base = unsafe { GetModuleHandleW(None) }
        .map_err(|_| "读取主模块失败")?
        .0 as usize;
    let Some(signature) = read(base.checked_add(codeViewRva).ok_or("主模块地址溢出")?, 24)
    else {
        return Ok(false);
    };
    if &signature[..4] != b"RSDS"
        || signature[4..20] != pdbGuid
        || signature[20..24] != 1u32.to_le_bytes()
    {
        return Ok(false);
    }
    let address = base.checked_add(functionRva).ok_or("完成入口地址溢出")?;
    if read(address, entryBytes.len()).as_deref() != Some(entryBytes.as_slice()) {
        return Ok(false);
    }
    unsafe { detour.install(address as *const (), observed as *const ()) }
        .map_err(|_| "启用完成入口失败")?;
    Ok(true)
}

// 地址来自当前函数调用，只通过有界本进程读取复制，读取失败不直接解引用或输出原始内存。
fn read(address: usize, length: usize) -> Option<Vec<u8>> {
    if address == 0 || length > 512 {
        return None;
    }
    let mut bytes = vec![0; length];
    let mut received = 0;
    unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            address as *const c_void,
            bytes.as_mut_ptr().cast(),
            length,
            Some(&mut received),
        )
        .ok()?;
    }
    (received == length).then_some(bytes)
}

// 偏移加法先检查溢出；本布局只在上面的 x64 构建身份验证成功后使用。
fn word(base: usize, offset: usize) -> Option<usize> {
    Some(usize::from_le_bytes(
        read(base.checked_add(offset)?, 8)?.try_into().ok()?,
    ))
}

// 字符串仅借用其字段，不接管分配；复制限制与文件协议一致，控制字符由最终记录校验拒绝。
fn text(address: usize, length: usize) -> Option<String> {
    if length == 0 || length > 256 {
        return None;
    }
    String::from_utf8(read(address, length)?).ok()
}

// 原函数结束后 response String 已被消费，所有字段必须在调用它之前复制完毕。
struct Arguments {
    state: usize,
    thread: usize,
    turn: usize,
    turnLength: usize,
    response: usize,
    usage: usize,
}

// 模型来源严格为客户端轮次上下文，provider 取生效配置；不把这些字段当作已观察到的 HTTP 路由或响应模型。
fn capture(arguments: Arguments) -> Option<Completion> {
    // previous_turn_settings 为 Option；None 使用 String 容量的高位 niche，其他字段此时不得读取或用于估价。
    let capacity = word(arguments.state, modelCapacityOffset)?;
    if capacity > isize::MAX as usize {
        return None;
    }
    let modelLength = word(arguments.state, modelLengthOffset)?;
    if modelLength > capacity {
        return None;
    }
    let configuration = word(arguments.state, configurationArcOffset)?;
    let provider = text(
        word(configuration, providerPointerOffset)?,
        word(configuration, providerLengthOffset)?,
    )?;
    if provider != "openai" {
        return None;
    }
    let model = text(word(arguments.state, modelPointerOffset)?, modelLength)?;
    let responseId = text(word(arguments.response, 8)?, word(arguments.response, 16)?)?;
    let turnId = text(arguments.turn, arguments.turnLength)?;
    let uuid = read(arguments.thread, 16)?;
    let hex: String = uuid.iter().map(|byte| format!("{byte:02x}")).collect();
    let threadId = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    let bytes = read(arguments.usage.checked_add(usageOffset)?, 48)?;
    let counts: [i64; 6] = std::array::from_fn(|index| {
        i64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    });
    let completion = Completion {
        version: 1,
        provider,
        model,
        threadId,
        turnId,
        responseId,
        timestampMillis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis()
            .try_into()
            .ok()?,
        inputTokens: counts[0],
        cachedInputTokens: counts[1],
        cacheWriteInputTokens: counts[2],
        outputTokens: counts[3],
        reasoningOutputTokens: counts[4],
        totalTokens: counts[5],
    };
    completion.validate().ok()?;
    Some(completion)
}

// 完成目录来自同一内存配置快照；运行期失效或停用时不读取会话字段，也不沿用旧目录。
fn prepare(arguments: Arguments) -> Option<(PathBuf, Completion)> {
    let settings = super::imp::relaySnapshot()?;
    if !settings.completionEnabled {
        return None;
    }
    Some((settings.completionDirectory.clone()?, capture(arguments)?))
}

// 唯一 ABI 入口保持九个实际参数及可展开约定；一次生成只同步发布一个至多 4 KiB 的元数据事务，不在逐包热路径写盘。
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
    let activity = super::hookInstall::CallbackActivity::enter();
    let original: UsageFn = detour.original();
    if activity.isUnloading() {
        return original(
            out, state, thread, turn, turnLength, session, root, response, usage,
        );
    }
    let pending = std::panic::catch_unwind(|| {
        prepare(Arguments {
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
    // 安装先发布 trampoline 再 enable，因此回调执行时原函数指针必定存在；不捕获原函数自身的展开。
    let result = original(
        out, state, thread, turn, turnLength, session, root, response, usage,
    );
    if let Some((directory, completion)) = pending {
        if let Err(error) = completionSpool::publish(&directory, &completion) {
            super::imp::log(error);
        }
    }
    result
}

// 原生完成入口与网络入口进入同一卸载屏障，先恢复目标代码再等待活跃回调。
pub(super) fn disable() -> Result<(), String> {
    detour.disable()
}

// 调用方保证完成回调已排空；释放 trampoline，避免重复部署累积可执行页。
pub(super) fn release() -> Result<(), String> {
    detour.release()
}

#[cfg(test)]
#[path = "../tests/unit/nativeCompletionTests.rs"]
mod tests;
