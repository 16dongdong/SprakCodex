use super::*;

// 构造本测试拥有的完整目录，单独写字段以保持 ABI 填充字节初始化；不使用真实进程地址。
fn snapshot() -> Vec<u64> {
    let mut allocation = vec![0u64; 32];
    for (offset, name, pid, next) in [
        (0usize, "CODEX.EXE", 111usize, 128u32),
        (128, "other.exe", 222, 0),
    ] {
        let words: Vec<_> = name.encode_utf16().collect();
        unsafe {
            let record = allocation
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<BasicProcess>();
            let text = allocation
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset + std::mem::size_of::<BasicProcess>())
                .cast::<u16>();
            std::ptr::copy_nonoverlapping(words.as_ptr(), text, words.len());
            std::ptr::addr_of_mut!((*record).nextOffset).write(next);
            std::ptr::addr_of_mut!((*record).pid).write(pid);
            std::ptr::addr_of_mut!((*record).name.length).write((words.len() * 2) as u16);
            std::ptr::addr_of_mut!((*record).name.maximumLength).write((words.len() * 2) as u16);
            std::ptr::addr_of_mut!((*record).name.buffer).write(text);
        }
    }
    allocation
}

// 转为字节视图不延长分配寿命；用于验证内核目录中的链偏移和名字范围检查。
fn view(allocation: &[u64]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            allocation.as_ptr().cast(),
            std::mem::size_of_val(allocation),
        )
    }
}

// 完整 ASCII 名称命中，其他条目不产生候选；不读取命令行或环境值。
#[test]
fn basicDirectorySelectsExactNames() {
    assert_eq!(parseBasic(view(&snapshot())).unwrap(), vec![111]);
}

// 截断、无前进链和范围外名字都在解引用前拒绝，禁止把部分损坏目录当成成功扫描。
#[test]
fn invalidDirectoryNeverDereferencesForeignNames() {
    assert!(parseBasic(&[]).is_err());
    let mut allocation = snapshot();
    unsafe {
        (*allocation.as_mut_ptr().cast::<BasicProcess>()).nextOffset = 1;
    }
    assert!(parseBasic(view(&allocation)).is_err());
    let mut allocation = snapshot();
    unsafe {
        (*allocation.as_mut_ptr().cast::<BasicProcess>())
            .name
            .buffer = usize::MAX as *const u16;
    }
    assert!(parseBasic(view(&allocation)).is_err());
    let mut allocation = snapshot();
    unsafe {
        (*allocation.as_mut_ptr().cast::<BasicProcess>())
            .name
            .length = 1;
    }
    assert!(parseBasic(view(&allocation)).is_err());
}

// 两次旧 API 查询之间持续存在的目标应被新目录看到；不因其他真实进程恰好启动或退出造成竞态断言。
#[test]
fn liveDirectoryMatchesStableToolhelpTargets() {
    let before: std::collections::HashSet<_> = toolhelpPids().unwrap().into_iter().collect();
    let current: std::collections::HashSet<_> = targetPids().unwrap().into_iter().collect();
    let after: std::collections::HashSet<_> = toolhelpPids().unwrap().into_iter().collect();
    assert!(before.intersection(&after).all(|pid| current.contains(pid)));
}

// 只读基准用于选择扫描频率，不断言机器调度耗时，也不将枚举耗时等同于首请求捕获保证。
#[test]
#[ignore = "显式执行一百次只读目录查询并输出耗时"]
fn measureDirectoryLatency() {
    let started = std::time::Instant::now();
    for _ in 0..100 {
        targetPids().unwrap();
    }
    println!(
        "进程目录：基本信息类={}，100 次耗时微秒={}",
        !legacyCatalog.load(Ordering::Relaxed),
        started.elapsed().as_micros()
    );
}
