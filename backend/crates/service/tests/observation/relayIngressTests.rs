use super::*;
use cpcommon::hook_proxy::encode_header;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

// 生成标准库 TLS 客户端的真实握手字节；无需网络、证书信任或业务认证。
fn clientHello(sni: bool) -> Vec<u8> {
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(rustls::RootCertStore::empty())
    .with_no_client_auth();
    config.enable_sni = sni;
    let mut client =
        rustls::ClientConnection::new(Arc::new(config), "chatgpt.com".try_into().unwrap()).unwrap();
    let mut hello = Vec::new();
    client.write_tls(&mut hello).unwrap();
    hello
}

// 逐字节分包固定头仍应解析到同一个目标，不得因第一次 peek 不足八字节误交 Hyper。
#[tokio::test]
async fn fragmentedRelayHeaderPreservesTarget() {
    let target = HookProxyTarget {
        ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)),
        port: 443,
        pid: 123,
    };
    let (mut writer, mut reader) = tokio::io::duplex(1);
    let sending = tokio::spawn(async move {
        writer.write_all(&encode_header(&target)).await.unwrap();
    });
    let Ingress::Relay(parsed) = readIngress(&mut reader).await.unwrap() else {
        panic!("入口类型错误")
    };
    assert_eq!(parsed, target);
    sending.await.unwrap();
}

// 普通 HTTP 前缀必须回放，剩余读取和回复仍使用原连接。
#[tokio::test]
async fn httpPrefixReplayPreservesReadAndWrite() {
    let request = b"CONNECT chatgpt.com:443 HTTP/1.1\r\n\r\n";
    let (mut client, mut socket) = tokio::io::duplex(128);
    client.write_all(request).await.unwrap();
    client.shutdown().await.unwrap();
    let Ingress::Http(prefix) = readIngress(&mut socket).await.unwrap() else {
        panic!("普通入口误识别")
    };
    let mut replayed = replay(socket, prefix);
    let mut received = Vec::new();
    replayed.read_to_end(&mut received).await.unwrap();
    assert_eq!(received, request);
    replayed.write_all(b"OK").await.unwrap();
    let mut response = [0u8; 2];
    client.read_exact(&mut response).await.unwrap();
    assert_eq!(&response, b"OK");
}

// 畸形地址族、零端口和截断私有头都返回错误，不生成虚构目标。
#[tokio::test]
async fn invalidRelayHeaderIsRejected() {
    let valid = encode_header(&HookProxyTarget {
        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 443,
        pid: 123,
    });
    for mutation in [8, 10] {
        let mut header = valid;
        header[mutation] = 0;
        if mutation == 10 {
            header[11] = 0;
        }
        assert!(readIngress(&mut &header[..]).await.is_err());
    }
    assert!(readIngress(&mut &valid[..12]).await.is_err());
}

// TLS 握手跨 TCP 小包完整解析，返回的预读字节与原握手逐字节一致。
#[tokio::test]
async fn fragmentedClientHelloIsParsedWithoutLoss() {
    let hello = clientHello(true);
    let expected = hello.clone();
    let (mut writer, mut reader) = tokio::io::duplex(3);
    let sending = tokio::spawn(async move {
        writer.write_all(&hello).await.unwrap();
    });
    let (host, prefix) = readHello(&mut reader).await.unwrap();
    assert_eq!(host.as_deref(), Some("chatgpt.com"));
    assert_eq!(prefix, expected);
    sending.await.unwrap();
}

// ClientHello 也可能跨多个 TLS record；标准解析器负责边界，不使用固定偏移手工猜测扩展。
#[tokio::test]
async fn clientHelloAcrossTlsRecordsKeepsSni() {
    let hello = clientHello(true);
    let body = &hello[5..];
    let split = body.len() / 2;
    let mut fragmented = Vec::new();
    for part in [&body[..split], &body[split..]] {
        fragmented.extend_from_slice(&hello[..3]);
        fragmented.extend_from_slice(&(part.len() as u16).to_be_bytes());
        fragmented.extend_from_slice(part);
    }
    let (host, prefix) = readHello(&mut &fragmented[..]).await.unwrap();
    assert_eq!(host.as_deref(), Some("chatgpt.com"));
    assert_eq!(prefix, fragmented);
}

// 非 TLS 与没有 SNI 的 TLS 都交回透明隧道，原始字节保持可恢复。
#[tokio::test]
async fn unobservedProtocolReturnsOriginalPrefix() {
    for payload in [b"GET / HTTP/1.1\r\n\r\n".to_vec(), clientHello(false)] {
        let (host, prefix) = readHello(&mut &payload[..]).await.unwrap();
        assert!(host.is_none());
        assert_eq!(prefix, payload);
    }
}
