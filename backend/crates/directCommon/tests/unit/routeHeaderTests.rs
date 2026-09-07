#![allow(non_snake_case)]
use super::*;

// 明确标记代理连接，并保留实际地址族、端口和来源进程，原 CONNECT 不再是没有出口信息的裸流。
#[test]
fn proxyRouteRoundTripsBothFamilies() {
    for ip in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        let target = HookProxyTarget {
            ip,
            port: 12345,
            pid: 42,
        };
        let route = decodeRoute(&encodeRoute(&target, RouteKind::HttpProxy)).unwrap();
        assert_eq!(
            route,
            HookRoute {
                target,
                kind: RouteKind::HttpProxy
            }
        );
    }
}

// 不接受未知标志或把任意远端地址伪装为已识别本机 HTTP 代理；旧直接连接头仍有效。
#[test]
fn invalidRouteKindsAreRejected() {
    let target = HookProxyTarget {
        ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)),
        port: 443,
        pid: 1,
    };
    assert!(decodeRoute(&encodeRoute(&target, RouteKind::HttpProxy)).is_none());
    let mut header = encode_header(&target);
    assert_eq!(decodeRoute(&header).unwrap().kind, RouteKind::Direct);
    header[9] = 255;
    assert!(decodeRoute(&header).is_none());
}
