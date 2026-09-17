//! 将宿主发布的出口地区画像呈现给目标进程；配置从命名映射实时读取，节点重启后不会沿用旧值。

use cpcommon::relayContract::EnvironmentProfile;
use windows::core::{s, w, PCSTR, PWSTR};
use windows::Win32::Foundation::{SetLastError, ERROR_INSUFFICIENT_BUFFER};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Time::{DYNAMIC_TIME_ZONE_INFORMATION, TIME_ZONE_INFORMATION};

type GetTziFn = unsafe extern "system" fn(*mut TIME_ZONE_INFORMATION) -> u32;
type GetDynamicTziFn = unsafe extern "system" fn(*mut DYNAMIC_TIME_ZONE_INFORMATION) -> u32;
type GetLocaleFn = unsafe extern "system" fn(PWSTR, i32) -> i32;
type EnumDynamicTziFn = unsafe extern "system" fn(u32, *mut DYNAMIC_TIME_ZONE_INFORMATION) -> u32;

static getTzi: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static getDynamicTzi: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static getUserLocale: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();
static getSystemLocale: super::hookInstall::DetourSlot = super::hookInstall::DetourSlot::new();

// 每次回调读取宿主最新快照；映射失效时返回空并保留系统原值。
fn profile() -> Option<EnvironmentProfile> {
    super::imp::relaySnapshot()?.environmentProfile.clone()
}

// Windows 注册表持有完整历史与 DST 切换规则；按键枚举比复制固定 bias 更准确。
unsafe fn dynamicTimezone(key: &str) -> Option<DYNAMIC_TIME_ZONE_INFORMATION> {
    let kernel = GetModuleHandleW(w!("kernel32.dll")).ok()?;
    let entry = GetProcAddress(kernel, s!("EnumDynamicTimeZoneInformation"))?;
    let enumerate: EnumDynamicTziFn = std::mem::transmute(entry);
    for index in 0..512 {
        let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
        let result = enumerate(index, &mut zone);
        if result != 0 {
            return None;
        }
        if wideString(&zone.TimeZoneKeyName).eq_ignore_ascii_case(key) {
            return Some(zone);
        }
    }
    None
}

// Windows 固定缓冲以首个 NUL 为边界，避免把未使用尾部参与键名比较。
fn wideString(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

// 将完整 Windows 动态时区结构复制给调用方，包含正确的 DST 日期与键名。
unsafe fn applyDynamic(out: *mut DYNAMIC_TIME_ZONE_INFORMATION) {
    if out.is_null() {
        return;
    }
    let Some(profile) = profile() else {
        return;
    };
    if let Some(zone) = dynamicTimezone(profile.windowsTimezone.as_str()) {
        *out = zone;
    }
}

// 静态时区 API 保留原返回状态，只把结构替换为出口时区的完整规则。
unsafe extern "system" fn hookGetTzi(out: *mut TIME_ZONE_INFORMATION) -> u32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let original: GetTziFn = getTzi.original();
    let result = original(out);
    if activity.isUnloading() || out.is_null() {
        return result;
    }
    let Some(profile) = profile() else {
        return result;
    };
    if let Some(zone) = dynamicTimezone(profile.windowsTimezone.as_str()) {
        (*out).Bias = zone.Bias;
        (*out).StandardName = zone.StandardName;
        (*out).StandardDate = zone.StandardDate;
        (*out).StandardBias = zone.StandardBias;
        (*out).DaylightName = zone.DaylightName;
        (*out).DaylightDate = zone.DaylightDate;
        (*out).DaylightBias = zone.DaylightBias;
    }
    result
}

// 动态时区 API 额外同步 Windows 键名，供 Chromium、Node 与 ICU 映射 IANA 时区。
unsafe extern "system" fn hookGetDynamicTzi(out: *mut DYNAMIC_TIME_ZONE_INFORMATION) -> u32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    let original: GetDynamicTziFn = getDynamicTzi.original();
    let result = original(out);
    if !activity.isUnloading() {
        applyDynamic(out);
    }
    result
}

// LocaleName API 遵守调用方缓冲容量；画像不存在时直接执行原函数。
unsafe fn fillLocale(slot: &super::hookInstall::DetourSlot, buffer: PWSTR, capacity: i32) -> i32 {
    let original: GetLocaleFn = slot.original();
    if buffer.is_null() || capacity <= 0 {
        return original(buffer, capacity);
    }
    let Some(profile) = profile() else {
        return original(buffer, capacity);
    };
    let value = profile.locale.encode_utf16().collect::<Vec<_>>();
    let required = value.len() + 1;
    if required > capacity as usize {
        SetLastError(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    let output = std::slice::from_raw_parts_mut(buffer.0, capacity as usize);
    output[..value.len()].copy_from_slice(&value);
    output[value.len()] = 0;
    required as i32
}

// 用户默认区域入口与系统默认区域共用同一出口画像。
unsafe extern "system" fn hookGetUserLocale(buffer: PWSTR, capacity: i32) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    if activity.isUnloading() {
        let original: GetLocaleFn = getUserLocale.original();
        return original(buffer, capacity);
    }
    fillLocale(&getUserLocale, buffer, capacity)
}

// 系统默认区域入口保持与用户默认区域一致，避免同进程出现两个国家。
unsafe extern "system" fn hookGetSystemLocale(buffer: PWSTR, capacity: i32) -> i32 {
    let activity = super::hookInstall::CallbackActivity::enter();
    if activity.isUnloading() {
        let original: GetLocaleFn = getSystemLocale.original();
        return original(buffer, capacity);
    }
    fillLocale(&getSystemLocale, buffer, capacity)
}

// 动态解析 kernel32 导出后交给统一 DetourSlot，原调用与卸载语义不另起实现。
unsafe fn installSlot(
    slot: &super::hookInstall::DetourSlot,
    name: PCSTR,
    callback: *const (),
    label: &str,
) -> Result<(), String> {
    let kernel =
        GetModuleHandleW(w!("kernel32.dll")).map_err(|_| format!("读取 {label} 模块失败"))?;
    let target = GetProcAddress(kernel, name).ok_or_else(|| format!("读取 {label} 入口失败"))?;
    slot.install(target as *const (), callback)
        .map_err(|error| format!("安装 {label} 入口失败：{error}"))
}

// 四个 API 必须作为同一画像事务安装；任一缺失都拒绝发布 ready，避免时区与语言只同步一半。
pub(super) unsafe fn install() -> Result<(), String> {
    let result = (|| {
        installSlot(
            &getTzi,
            s!("GetTimeZoneInformation"),
            hookGetTzi as *const (),
            "GetTimeZoneInformation",
        )?;
        installSlot(
            &getDynamicTzi,
            s!("GetDynamicTimeZoneInformation"),
            hookGetDynamicTzi as *const (),
            "GetDynamicTimeZoneInformation",
        )?;
        installSlot(
            &getUserLocale,
            s!("GetUserDefaultLocaleName"),
            hookGetUserLocale as *const (),
            "GetUserDefaultLocaleName",
        )?;
        installSlot(
            &getSystemLocale,
            s!("GetSystemDefaultLocaleName"),
            hookGetSystemLocale as *const (),
            "GetSystemDefaultLocaleName",
        )?;
        Ok(())
    })();
    if result.is_err() {
        // 初始化失败时映像会被宿主回收，必须先撤销已经生效的入口，不能留下指向待释放映像的跳板。
        for slot in slots() {
            let _ = slot.disable();
            let _ = slot.release();
        }
    }
    result
}

// 卸载第一阶段先恢复全部地区 API 入口，仍在回调中的线程继续使用 trampoline。
pub(super) fn disable() -> Result<(), String> {
    for slot in slots() {
        slot.disable()?;
    }
    Ok(())
}

// 所有回调排空后释放地区 API trampoline。
pub(super) fn release() -> Result<(), String> {
    for slot in slots() {
        slot.release()?;
    }
    Ok(())
}

// 固定顺序用于安装回滚和卸载，不能遗漏任一已发布入口。
fn slots() -> [&'static super::hookInstall::DetourSlot; 4] {
    [&getTzi, &getDynamicTzi, &getUserLocale, &getSystemLocale]
}
