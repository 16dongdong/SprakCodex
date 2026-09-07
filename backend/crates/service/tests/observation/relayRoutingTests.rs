//! 生产 DLL 的真实 Winsock 验收；仅向本测试创建的两个回环 listener 发送固定字节。
use super::{nativeInjection, runtimePaths};
use cpcommon::{relayContract::RelayConfig, runtimeLease::currentIdentity};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const waitLimit: Duration = Duration::from_secs(5);
const probeBytes: &[u8; 5] = b"PROBE";
const replyBytes: &[u8; 2] = b"OK";

// 目录先创建后复制 DLL；结束时先关闭子进程再删除其独占文件，避免触碰已安装模块。
struct RoutingFixture {
    directory: PathBuf,
    child: Option<Child>,
    output: Option<mpsc::Receiver<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl RoutingFixture {
    // 显式接收新构建的生产 DLL，不从用户进程推断路径，也不启动生产自动扫描。
    fn start(proxyAddress: SocketAddr) -> Self {
        let source =
            PathBuf::from(std::env::var_os("OBSERVATION_TEST_NETWORK_DLL").expect("指定生产 DLL"));
        assert!(source.is_absolute() && source.is_file());
        let directory =
            std::env::temp_dir().join(format!("relayRouting{:032x}", rand::random::<u128>()));
        std::fs::create_dir(&directory).unwrap();
        let mut fixture = Self {
            directory,
            child: None,
            output: None,
            reader: None,
        };
        std::fs::copy(source, fixture.directory.join("cphook.dll")).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "directObservation::relayRoutingTests::networkClientFixture",
                "--ignored",
                "--nocapture",
            ])
            .env("OBSERVATION_NETWORK_FIXTURE", "true")
            .env("HTTPS_PROXY", format!("http://{proxyAddress}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        fixture.child = Some(child);
        let stdout = fixture.child.as_mut().unwrap().stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        fixture.output = Some(receiver);
        fixture.reader = Some(std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line.expect("读取网络夹具输出")).is_err() {
                    break;
                }
            }
        }));
        fixture.expect("CLIENT_READY");
        let identity = nativeInjection::candidate(fixture.child.as_ref().unwrap().id()).unwrap();
        nativeInjection::inject(&identity, &fixture.directory.join("cphook.dll")).unwrap();
        fixture
    }

    // 每个状态检查都有总截止时间；子进程异常不允许导致测试无限等待。
    fn expect(&self, marker: &str) {
        let deadline = Instant::now() + waitLimit;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let line = self
                .output
                .as_ref()
                .unwrap()
                .recv_timeout(remaining)
                .expect("夹具未按时响应");
            if line == marker {
                return;
            }
        }
    }

    // 只原子发布无秘密网络配置；零端口停用通过正式宿主函数执行。
    fn publish(&self, settings: &RelayConfig) {
        runtimePaths::writeAtomically(
            &self.directory.join("hook.json"),
            &serde_json::to_vec(settings).unwrap(),
        )
        .unwrap();
    }

    // 同步 connect 和 Tokio ConnectEx 都发往同一个原始地址；从实际 accept 的 listener 判定路由。
    fn probe(&mut self, destination: SocketAddr, expected: &TcpListener) {
        for mode in ["sync", "async"] {
            writeln!(
                self.child.as_mut().unwrap().stdin.as_mut().unwrap(),
                "{mode} {destination}"
            )
            .unwrap();
            let deadline = Instant::now() + waitLimit;
            let mut stream = loop {
                match expected.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "目标 listener 未收到 {mode} 连接"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("接收连接失败：{error}"),
                }
            };
            // Windows 的 accept 会继承 listener 非阻塞标志；接收阶段改回阻塞并设置超时，避免把尚未到达当协议失败。
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(waitLimit)).unwrap();
            stream.set_write_timeout(Some(waitLimit)).unwrap();
            let mut request = [0u8; 5];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request, probeBytes);
            stream.write_all(replyBytes).unwrap();
            self.expect("CLIENT_OK");
        }
    }
}
impl Drop for RoutingFixture {
    // 仅删除已解析隔离目录中的本次文件；先回收进程和输出线程，再处理 DLL 文件映射。
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if child.try_wait().unwrap().is_none() {
                child.kill().unwrap();
            }
            child.wait().unwrap();
        }
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
        for entry in std::fs::read_dir(&self.directory).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            std::fs::remove_file(entry.path()).unwrap();
        }
        std::fs::remove_dir(&self.directory).unwrap();
    }
}

// 两个真实 listener 区分原路径与 Relay；缺失、开启、runtime 退出、重启、停用和损坏逐项验证。
#[test]
#[ignore = "需要显式指定 OBSERVATION_TEST_NETWORK_DLL，仅注入本测试创建的客户端"]
fn productionModuleHonorsRuntimeLifetime() {
    for address in [
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
    ] {
        verifyAddressFamilyLifetime(address);
    }
}

// 同一套实际 Winsock 往返分别验证两个地址族，代理端点来自夹具自身环境而不是宿主端口列表。
fn verifyAddressFamilyLifetime(address: std::net::IpAddr) {
    let original = TcpListener::bind((address, 0)).unwrap();
    let relay = TcpListener::bind((address, 0)).unwrap();
    original.set_nonblocking(true).unwrap();
    relay.set_nonblocking(true).unwrap();
    let destination = original.local_addr().unwrap();
    let mut fixture = RoutingFixture::start(destination);
    fixture.probe(destination, &original);
    let (identitySender, identityReceiver) = mpsc::channel();
    let (exitSender, exitReceiver) = mpsc::channel();
    let owner = std::thread::spawn(move || {
        identitySender.send(currentIdentity().unwrap()).unwrap();
        exitReceiver.recv().unwrap();
    });
    let mut settings = RelayConfig {
        relayPort: relay.local_addr().unwrap().port(),
        forceProxyTcp: true,
        owner: Some(identityReceiver.recv().unwrap()),
        caCertificatePath: None,
    };
    fixture.publish(&settings);
    fixture.probe(destination, &relay);
    exitSender.send(()).unwrap();
    owner.join().unwrap();
    // 文件内容未变，DLL 已缓存的线程句柄也必须失效。
    fixture.probe(destination, &original);
    settings.owner = Some(currentIdentity().unwrap());
    fixture.publish(&settings);
    fixture.probe(destination, &relay);
    runtimePaths::writeRelayConfig(&fixture.directory.join("hook.json"), 0, None).unwrap();
    fixture.probe(destination, &original);
    fixture.publish(&settings);
    fixture.probe(destination, &relay);
    std::fs::write(fixture.directory.join("hook.json"), b"{").unwrap();
    fixture.probe(destination, &original);
    std::fs::remove_file(fixture.directory.join("hook.json")).unwrap();
    fixture.probe(destination, &original);
    assert_eq!(
        original.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        relay.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

// 父测试传入确切回环地址，真实执行 std/Tokio socket；禁止解析外部地址或修改代理环境。
#[test]
#[ignore = "仅由路由生命周期父测试通过 stdin 驱动"]
fn networkClientFixture() {
    assert_eq!(
        std::env::var("OBSERVATION_NETWORK_FIXTURE").as_deref(),
        Ok("true")
    );
    println!("CLIENT_READY");
    std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let (mode, address) = line.split_once(' ').unwrap();
        let address: SocketAddr = address.parse().unwrap();
        assert!(address.ip().is_loopback());
        match mode {
            "sync" => synchronousProbe(address),
            "async" => asynchronousProbe(address),
            _ => panic!("未知夹具连接方式"),
        }
        println!("CLIENT_OK");
        std::io::stdout().flush().unwrap();
    }
}

// std 路径调用真实 Winsock connect/send，连接和读写都有明确超时。
fn synchronousProbe(address: SocketAddr) {
    let mut stream = TcpStream::connect_timeout(&address, waitLimit).unwrap();
    stream.set_read_timeout(Some(waitLimit)).unwrap();
    stream.set_write_timeout(Some(waitLimit)).unwrap();
    stream.write_all(probeBytes).unwrap();
    let mut reply = [0u8; 2];
    stream.read_exact(&mut reply).unwrap();
    assert_eq!(&reply, replyBytes);
}

// Tokio 路径使用 Windows 异步连接实现，覆盖已缓存 ConnectEx 指针而不是仅验证同步入口。
fn asynchronousProbe(address: SocketAddr) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(waitLimit, async {
                let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
                stream.write_all(probeBytes).await.unwrap();
                let mut reply = [0u8; 2];
                stream.read_exact(&mut reply).await.unwrap();
                assert_eq!(&reply, replyBytes);
            })
            .await
            .unwrap();
        });
}
