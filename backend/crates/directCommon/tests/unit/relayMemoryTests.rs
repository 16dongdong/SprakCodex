use super::*;
use std::{sync::Arc, time::SystemTime};

// 同长度快照反复覆盖时，读取结果只能是完整旧值或完整新值，序列号阻止头部 ABA 接受撕裂正文。
#[test]
fn sameLengthSnapshotsNeverTear() {
    let identity = format!(
        "relay-memory-{}-{:?}",
        std::process::id(),
        SystemTime::now()
    );
    let publisher = Arc::new(Publisher::create(&identity).unwrap());
    let first = vec![b'a'; 32 * 1024];
    let second = vec![b'b'; first.len()];
    publisher.write(&first).unwrap();

    let publishing: Vec<_> = [first.clone(), second.clone()]
        .into_iter()
        .map(|snapshot| {
            let writer = publisher.clone();
            std::thread::spawn(move || {
                for _ in 0..1_000 {
                    writer.write(&snapshot).unwrap();
                }
            })
        })
        .collect();
    for _ in 0..2_000 {
        if let Ok(snapshot) = read(&identity) {
            assert!(snapshot == first || snapshot == second);
        }
    }
    for writer in publishing {
        writer.join().unwrap();
    }
}
