use super::*;

// 编码不依赖文件存在，保留中文路径并验证进程实例，错误长度不能越界读取。
#[test]
fn codecPreservesUnicodeAndRejectsStaleIdentity() {
    let home = Path::new(r"C:\夹具\独立目录");
    let bytes = encode(42, 123, home).unwrap();
    assert_eq!(decode(&bytes, 42, 123).unwrap(), home);
    assert!(decode(&bytes, 43, 123).is_err());
    assert!(decode(&bytes, 42, 124).is_err());
    assert!(decode(&bytes[..headerBytes], 42, 123).is_err());
    assert!(encode(42, 123, Path::new("relative")).is_err());
}

// 真实分页文件映射没有磁盘副本；最后一个持有者退出后命名对象消失，重复发布不覆盖已有内容。
#[test]
fn mappingLifetimeIsBoundToPublisher() {
    let identity = format!(
        "fixture-{}-{}",
        std::process::id(),
        currentCreationTime().unwrap()
    );
    let home = Path::new(r"C:\夹具\目录甲");
    let created = currentCreationTime().unwrap();
    let publisher = publish(&identity, home).unwrap();
    assert_eq!(read(&identity, std::process::id(), created).unwrap(), home);
    assert!(publish(&identity, Path::new(r"C:\夹具\目录乙")).is_err());
    assert_eq!(read(&identity, std::process::id(), created).unwrap(), home);
    drop(publisher);
    assert!(read(&identity, std::process::id(), created).is_err());
}
