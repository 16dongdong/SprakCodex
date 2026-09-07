//! Relay 与普通 HTTP 共用入口：按字节流读取完整前缀，使用 rustls 解码握手，并保留探测消费的字节。
use cpcommon::hook_proxy::{decodeRoute, HookRoute, HEADER_LEN, HEADER_MAGIC};
use std::io::{self, Cursor};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

const maxHelloBytes: usize = 64 * 1024;
const readChunkBytes: usize = 4096;
const tlsHandshakeRecord: u8 = 22;

// 探测结果把私有头和普通 HTTP 分开；普通入口携带预读字节，转交 Hyper 前必须回放。
pub(super) enum Ingress {
    Http(Vec<u8>),
    Relay(HookRoute),
}

// TCP 分包不是协议边界；先读足魔数，再读取并校验全部固定头。EOF 和非法头返回 IO 错误。
pub(super) async fn readIngress<S: AsyncRead + Unpin>(stream: &mut S) -> io::Result<Ingress> {
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header[..HEADER_MAGIC.len()]).await?;
    if header[..HEADER_MAGIC.len()] != HEADER_MAGIC {
        return Ok(Ingress::Http(header[..HEADER_MAGIC.len()].to_vec()));
    }
    stream.read_exact(&mut header[HEADER_MAGIC.len()..]).await?;
    decodeRoute(&header)
        .map(Ingress::Relay)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Relay 目标头无效"))
}

// 通过 rustls 的标准 ClientHello 解析跨 TCP/TLS 分片读取 SNI；探测字节原样返回，不改客户端握手。
// 未识别协议、无 SNI 或不兼容握手返回 None，由调用方按原始目标透传；不把未识别等同于应断连。
pub(super) async fn readHello<S: AsyncRead + Unpin>(
    stream: &mut S,
) -> io::Result<(Option<String>, Vec<u8>)> {
    let mut prefix = Vec::new();
    let mut acceptor = rustls::server::Acceptor::default();
    let mut chunk = [0u8; readChunkBytes];
    loop {
        let remaining = maxHelloBytes - prefix.len();
        if remaining == 0 {
            return Ok((None, prefix));
        }
        let count = stream
            .read(&mut chunk[..remaining.min(readChunkBytes)])
            .await?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "握手尚未完成连接已关闭",
            ));
        }
        prefix.extend_from_slice(&chunk[..count]);
        if prefix[0] != tlsHandshakeRecord {
            return Ok((None, prefix));
        }
        acceptor.read_tls(&mut Cursor::new(&chunk[..count]))?;
        match acceptor.accept() {
            Ok(Some(accepted)) => {
                let host = accepted
                    .client_hello()
                    .server_name()
                    .map(str::to_ascii_lowercase);
                return Ok((host, prefix));
            }
            Ok(None) => {}
            Err(_) => return Ok((None, prefix)),
        }
    }
}

// 把预读字节接回原 socket 的读取端，写入仍直达同一连接；同时适用于 Hyper、TLS 和透明隧道。
pub(super) fn replay<S>(stream: S, prefix: Vec<u8>) -> impl AsyncRead + AsyncWrite + Unpin + Send
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let (reader, writer) = tokio::io::split(stream);
    tokio::io::join(Cursor::new(prefix).chain(reader), writer)
}

#[cfg(test)]
#[path = "../../tests/observation/relayIngressTests.rs"]
mod tests;
