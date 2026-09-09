//! 原调用 trampoline 的发布必须先于入口改写；回调执行时全局槽已可读取，避免并发进入未初始化槽。
use retour::{Function, GenericDetour, RawDetour};
use std::sync::OnceLock;

// 初始化线程准备好 detour 后调用；重复安装显式失败，启用失败保留已发布但未启用的 trampoline。
pub(super) unsafe fn activateDetour<F: Function>(
    slot: &OnceLock<GenericDetour<F>>,
    detour: GenericDetour<F>,
) -> Result<(), String> {
    slot.set(detour)
        .map_err(|_| "原调用槽已初始化，拒绝重复安装".to_string())?;
    slot.get()
        .ok_or("原调用槽发布失败")?
        .enable()
        .map_err(|_| "启用原生入口失败".into())
}

// 原始 detour 同样先发布 trampoline 再启用；调用侧自行用准确 ABI 转换公开 trampoline 地址。
pub(super) unsafe fn activateRawDetour(
    slot: &OnceLock<RawDetour>,
    detour: RawDetour,
) -> Result<(), String> {
    slot.set(detour)
        .map_err(|_| "原调用槽已初始化，拒绝重复安装".to_string())?;
    slot.get()
        .ok_or("原调用槽发布失败")?
        .enable()
        .map_err(|_| "启用原生入口失败".into())
}

#[cfg(test)]
#[path = "../tests/unit/detourLifecycleTests.rs"]
mod tests;
