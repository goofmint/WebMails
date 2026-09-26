//! Shared "is this host/address safe to contact" checks (design.md §2.2.6,
//! §2.2.10): a loopback, private or link-local destination is never
//! contacted, whether it is named literally (an IP-literal host, checked by
//! [`is_disallowed_host`]) or reached by resolving a domain name (a
//! resolved [`std::net::IpAddr`], checked by [`is_disallowed_ip`]).
//!
//! Both [`crate::agent_bridge::validate`] (rejecting a report's
//! `iconCandidates` whose *literal* host is unsafe — no DNS resolution is
//! performed there, by design) and [`crate::icons::fetch`] (rejecting a
//! download whose destination, after DNS resolution, is unsafe) call into
//! this module, so the address ranges that count as "disallowed" are
//! defined in exactly one place.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use url::Host;

/// Whether `host` is a loopback, private or link-local address. A domain
/// name is never itself disallowed on this basis (no DNS resolution is
/// performed here) — only a literal IPv4/IPv6 host can be.
pub fn is_disallowed_host(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(_) => false,
        Host::Ipv4(addr) => is_disallowed_ipv4(addr),
        Host::Ipv6(addr) => is_disallowed_ipv6(addr),
    }
}

/// Whether a resolved `addr` (the outcome of looking up a domain name, or a
/// literal IP address parsed directly) is a loopback, private or link-local
/// address.
pub fn is_disallowed_ip(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(addr) => is_disallowed_ipv4(addr),
        IpAddr::V6(addr) => is_disallowed_ipv6(addr),
    }
}

fn is_disallowed_ipv4(addr: &Ipv4Addr) -> bool {
    addr.is_loopback() || addr.is_private() || addr.is_link_local()
}

/// Segment-based IPv6 range checks: loopback (`::1`), IPv4-mapped
/// (`::ffff:0:0/96`, converted and re-checked against [`is_disallowed_ipv4`]),
/// unicast link-local (`fe80::/10`) and unique local (`fc00::/7`, IPv6's
/// private-equivalent range).
fn is_disallowed_ipv6(addr: &Ipv6Addr) -> bool {
    if addr.is_loopback() {
        return true;
    }
    let segments = addr.segments();
    if segments[0..5] == [0, 0, 0, 0, 0] && segments[5] == 0xffff {
        let mapped = Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            (segments[6] & 0xff) as u8,
            (segments[7] >> 8) as u8,
            (segments[7] & 0xff) as u8,
        );
        return is_disallowed_ipv4(&mapped);
    }
    if segments[0] & 0xffc0 == 0xfe80 {
        return true; // fe80::/10, unicast link-local
    }
    if segments[0] & 0xfe00 == 0xfc00 {
        return true; // fc00::/7, unique local
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr as V4;

    #[test]
    fn domain_host_is_never_disallowed() {
        assert!(!is_disallowed_host(&Host::Domain("mail.example.com")));
    }

    #[test]
    fn loopback_ipv4_host_is_disallowed() {
        assert!(is_disallowed_host(&Host::Ipv4(V4::new(127, 0, 0, 1))));
    }

    #[test]
    fn private_ipv4_host_is_disallowed() {
        assert!(is_disallowed_host(&Host::Ipv4(V4::new(10, 0, 0, 5))));
    }

    #[test]
    fn public_ipv4_host_is_allowed() {
        assert!(!is_disallowed_host(&Host::Ipv4(V4::new(93, 184, 216, 34))));
    }

    #[test]
    fn loopback_ip_addr_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V4(V4::new(127, 0, 0, 1))));
    }

    #[test]
    fn link_local_ipv6_ip_addr_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn public_ip_addr_is_allowed() {
        assert!(!is_disallowed_ip(&IpAddr::V4(V4::new(93, 184, 216, 34))));
    }
}
