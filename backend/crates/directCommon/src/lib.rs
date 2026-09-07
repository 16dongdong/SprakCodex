//! cpcommon:Host 与 cphook 之间的编译期共享契约。
//!
//! 它编译为 rlib 静态链入 `Cproxy.exe` 和 `cphook.dll`,本身不产出 DLL。

pub mod hook_proxy;
pub mod hook_ready;
pub mod runtime_paths;
