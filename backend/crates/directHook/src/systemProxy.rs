//! 在目标用户进程中只读当前活动网络的静态代理元数据；所有 WinHTTP 返回的字符串按 API 契约释放。
use super::proxyDiscovery::ProxyEndpoints;
use windows::Win32::{
    Foundation::{GlobalFree, HGLOBAL},
    Networking::WinHttp::{
        WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
    },
};

const MAX_PROXY_CHARS: usize = 32768;

// 不保存代理 URL、自动脚本 URL 或 bypass 文本；它们的内存只在一次配置查询中存活。
struct SystemProxy(WINHTTP_CURRENT_USER_IE_PROXY_CONFIG);

impl Drop for SystemProxy {
    // WinHTTP 使用全局堆分配字符串，即使只使用静态代理字段，也必须释放另外两个返回字段。
    fn drop(&mut self) {
        for pointer in [
            self.0.lpszProxy,
            self.0.lpszProxyBypass,
            self.0.lpszAutoConfigUrl,
        ] {
            if !pointer.is_null() && unsafe { GlobalFree(HGLOBAL(pointer.0.cast())) }.is_err() {
                super::imp::log("释放系统代理元数据失败");
            }
        }
    }
}

// 查询仅提供候选端点，不执行 PAC/WPAD 网络发现，也不替应用决定 bypass；无静态代理时不添加任何端点。
pub(super) fn appendCurrent(endpoints: &mut ProxyEndpoints) {
    let mut configured = SystemProxy(WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default());
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut configured.0) }.is_err()
        || configured.0.lpszProxy.is_null()
    {
        return;
    }
    // 配置由 WinHTTP 保证 NUL 结尾；额外限制长度，避免错误配置产生无界分配。
    let pointer = configured.0.lpszProxy.0;
    let Some(length) = (0..MAX_PROXY_CHARS).find(|index| unsafe { *pointer.add(*index) == 0 })
    else {
        return;
    };
    let units = unsafe { std::slice::from_raw_parts(pointer, length) };
    if let Ok(list) = String::from_utf16(units) {
        endpoints.addSystemList(&list);
    }
}
