use super::*;

// 真正从全局堆分配后释放，NULL 代表成功；不能根据 Result<HGLOBAL> 的非空句柄惯例误报失败。
#[test]
fn globalAllocationReturnsNullAfterRelease() {
    use windows::Win32::System::Memory::{GlobalAlloc, GMEM_FIXED};
    let memory = unsafe { GlobalAlloc(GMEM_FIXED, 32) }.unwrap();
    assert!(unsafe { releaseGlobal(memory.0) }.is_null());
}
