use super::*;

// 小于一毫秒时展示值会截断为零，但真实传输时间为正，计算结果必须保持准确。
#[test]
fn subMillisecondTransferRetainsRate() {
    let elapsed = Duration::from_micros(500);
    assert_eq!(elapsed.as_millis(), 0);
    assert!((measuredMbps(10, elapsed).unwrap() - 0.16).abs() < f64::EPSILON);
    assert_eq!(measuredMbps(1_000_000, Duration::from_secs(2)), Some(4.0));
}

// 缺失有效测量仍保持未知，不能以补一个毫秒或零兆比特掩盖无效输入。
#[test]
fn invalidMeasurementRemainsUnknown() {
    assert!(measuredMbps(0, Duration::from_secs(1)).is_none());
    assert!(measuredMbps(-1, Duration::from_secs(1)).is_none());
    assert!(measuredMbps(10, Duration::ZERO).is_none());
    assert!(measuredMbps(i64::MAX, Duration::from_nanos(1))
        .unwrap()
        .is_finite());
}
