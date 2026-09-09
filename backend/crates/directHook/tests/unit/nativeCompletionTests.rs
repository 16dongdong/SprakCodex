use super::*;

// 仅构造本测试进程持有的元数据内存，验证读取边界；真实 ABI 布局另由官方 CLI 的独立 RPC 核对。
struct Fixture {
    state: Vec<u8>,
    configuration: Vec<u8>,
    response: [usize; 3],
    usage: Vec<u8>,
    thread: [u8; 16],
}
impl Fixture {
    // 指针指向静态元数据或本结构拥有的缓冲区，不包含其他进程的地址或正文。
    fn new(provider: &'static str) -> Self {
        let mut state = vec![0u8; modelLengthOffset + 8];
        let mut configuration = vec![0u8; providerLengthOffset + 8];
        let model = "gpt-5.6-sol";
        put(&mut state, modelPointerOffset, model.as_ptr() as usize);
        put(&mut state, modelLengthOffset, model.len());
        put(&mut state, modelCapacityOffset, model.len());
        put(
            &mut configuration,
            providerPointerOffset,
            provider.as_ptr() as usize,
        );
        put(&mut configuration, providerLengthOffset, provider.len());
        let response = "resp_fixture";
        let mut usage = vec![0u8; usageOffset + 48];
        for (index, count) in [10i64, 2, 0, 3, 1, 13].iter().enumerate() {
            usage[usageOffset + index * 8..usageOffset + index * 8 + 8]
                .copy_from_slice(&count.to_le_bytes());
        }
        Self {
            state,
            configuration,
            response: [response.len(), response.as_ptr() as usize, response.len()],
            usage,
            thread: [0; 16],
        }
    }
    // 调用期间结构仍由测试持有；Args 不跨线程保存，也不把临时地址写进期望值文件。
    fn arguments(&mut self) -> Arguments {
        put(
            &mut self.state,
            configurationArcOffset,
            self.configuration.as_ptr() as usize,
        );
        let turn = "turn-fixture";
        Arguments {
            state: self.state.as_ptr() as usize,
            thread: self.thread.as_ptr() as usize,
            turn: turn.as_ptr() as usize,
            turnLength: turn.len(),
            response: self.response.as_ptr() as usize,
            usage: self.usage.as_ptr() as usize,
        }
    }
}

// 固定字宽写入的是夹具 ABI 字段，不用于修改业务进程。
fn put(bytes: &mut [u8], offset: usize, value: usize) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

// 正常字段完整保留，provider 边界不依赖模型名称是否像官方模型。
#[test]
fn capturesOnlyOfficialMetadata() {
    let mut fixture = Fixture::new("openai");
    let record = capture(fixture.arguments()).unwrap();
    assert_eq!(record.model, "gpt-5.6-sol");
    assert_eq!(record.totalTokens, 13);
    assert_eq!(record.cachedInputTokens, 2);
    let mut custom = Fixture::new("custom");
    assert!(capture(custom.arguments()).is_none());
}

// 错误计数、无效字符串和地址溢出都只拒绝观测，不直接解引用外部地址。
#[test]
fn rejectsInvalidFieldsWithoutDereferencingThem() {
    let mut fixture = Fixture::new("openai");
    fixture.usage[usageOffset..usageOffset + 8].copy_from_slice(&(-1i64).to_le_bytes());
    assert!(capture(fixture.arguments()).is_none());
    let mut fixture = Fixture::new("openai");
    put(&mut fixture.state, modelLengthOffset, 257);
    assert!(capture(fixture.arguments()).is_none());
    assert!(word(usize::MAX, 1).is_none());
    assert!(read(0, 8).is_none());
    let mut fixture = Fixture::new("openai");
    put(&mut fixture.state, modelCapacityOffset, 1usize << 63);
    assert!(
        capture(fixture.arguments()).is_none(),
        "没有轮次上下文时不读取 niche 中的残留字段"
    );
}

// 当前测试程序不匹配官方 PDB，安装必须返回未支持，且不发布 trampoline 或模块状态。
#[test]
fn unknownBuildDoesNotInstallAnEntry() {
    assert!(!install().unwrap());
    assert!(detour.get().is_none());
}
