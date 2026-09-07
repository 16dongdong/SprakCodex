//! cphook 与本地 relay 之间的私有握手头。
//!
//! 目标进程内的 Winsock hook 把真实 TCP 连接改连到 Cproxy relay。由于 relay 看到的
//! 对端只剩本机地址,hook 必须先发送一个固定长度头,把原始目的地和进程归属交给 relay。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const HEADER_MAGIC: [u8; 8] = *b"CPROXYH1";
pub const HEADER_LEN: usize = 32;

const FAMILY_IPV4: u8 = 4;
const FAMILY_IPV6: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookProxyTarget {
    pub ip: IpAddr,
    pub port: u16,
    pub pid: u32,
}

/// 将目标地址、端口和进程标识编码为固定长度的 Relay 握手头，字段使用网络字节序。
pub fn encode_header(target: &HookProxyTarget) -> [u8; HEADER_LEN] {
    let mut header = [0u8; HEADER_LEN];
    header[..HEADER_MAGIC.len()].copy_from_slice(&HEADER_MAGIC);
    match target.ip {
        IpAddr::V4(ip) => {
            header[8] = FAMILY_IPV4;
            header[12..16].copy_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            header[8] = FAMILY_IPV6;
            header[12..28].copy_from_slice(&ip.octets());
        }
    }
    header[10..12].copy_from_slice(&target.port.to_be_bytes());
    header[28..32].copy_from_slice(&target.pid.to_be_bytes());
    header
}

/// 校验并解析 Relay 握手头；魔数、地址族或端口非法时返回 `None`。
pub fn decode_header(header: &[u8]) -> Option<HookProxyTarget> {
    if header.len() != HEADER_LEN || header[..HEADER_MAGIC.len()] != HEADER_MAGIC {
        return None;
    }
    let port = u16::from_be_bytes([header[10], header[11]]);
    if port == 0 {
        return None;
    }
    let pid = u32::from_be_bytes([header[28], header[29], header[30], header[31]]);
    let ip = match header[8] {
        FAMILY_IPV4 => IpAddr::V4(Ipv4Addr::new(
            header[12], header[13], header[14], header[15],
        )),
        FAMILY_IPV6 => {
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&header[12..28]);
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        _ => return None,
    };
    Some(HookProxyTarget { ip, port, pid })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrips_ipv4() {
        let target = HookProxyTarget {
            ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 8)),
            port: 443,
            pid: 1234,
        };

        let decoded = decode_header(&encode_header(&target)).expect("valid header");

        assert_eq!(decoded, target);
    }

    #[test]
    fn header_roundtrips_ipv6() {
        let target = HookProxyTarget {
            ip: IpAddr::V6(Ipv6Addr::LOCALHOST),
            port: 8443,
            pid: 42,
        };

        let decoded = decode_header(&encode_header(&target)).expect("valid header");

        assert_eq!(decoded, target);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut header = encode_header(&HookProxyTarget {
            ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 1,
            pid: 1,
        });
        header[0] = 0;

        assert!(decode_header(&header).is_none());
    }
}
