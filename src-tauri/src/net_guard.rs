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

/// Whether `addr` falls in `0.0.0.0/8` — "this network", RFC 791/1122 — a
/// broader range than just the single unspecified address `0.0.0.0` that
/// [`Ipv4Addr::is_unspecified`] alone would catch.
fn is_in_this_network_ipv4(addr: &Ipv4Addr) -> bool {
    addr.octets()[0] == 0
}

/// Whether `addr` falls in the shared/carrier-grade-NAT range
/// `100.64.0.0/10` (RFC 6598) — used by ISPs and cloud providers for
/// address sharing, and reachable only from inside that same private
/// network, so it is exactly as unsafe a destination as RFC 1918 private
/// space.
fn is_in_cgnat_range_ipv4(addr: &Ipv4Addr) -> bool {
    let octets = addr.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn is_disallowed_ipv4(addr: &Ipv4Addr) -> bool {
    addr.is_loopback()
        || addr.is_private()
        || addr.is_link_local()
        || addr.is_unspecified()
        || addr.is_broadcast()
        || addr.is_multicast()
        || is_in_this_network_ipv4(addr)
        || is_in_cgnat_range_ipv4(addr)
}

/// Segment-based IPv6 range checks: unspecified (`::`), loopback (`::1`),
/// IPv4-mapped (`::ffff:0:0/96`, converted and re-checked against
/// [`is_disallowed_ipv4`]), unicast link-local (`fe80::/10`), unique local
/// (`fc00::/7`, IPv6's private-equivalent range) and multicast
/// (`ff00::/8`).
fn is_disallowed_ipv6(addr: &Ipv6Addr) -> bool {
    if addr.is_unspecified() || addr.is_loopback() || addr.is_multicast() {
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

    // --- newly widened ranges ------------------------------------------------

    #[test]
    fn unspecified_ipv4_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V4(V4::new(0, 0, 0, 0))));
    }

    #[test]
    fn broadcast_ipv4_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V4(V4::new(255, 255, 255, 255))));
    }

    #[test]
    fn multicast_ipv4_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V4(V4::new(224, 0, 0, 1))));
    }

    #[test]
    fn this_network_ipv4_slash_8_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V4(V4::new(0, 1, 2, 3))));
    }

    #[test]
    fn cgnat_ipv4_slash_10_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V4(V4::new(100, 64, 0, 1))));
        assert!(is_disallowed_ip(&IpAddr::V4(V4::new(100, 127, 255, 255))));
    }

    #[test]
    fn addresses_just_outside_the_cgnat_range_are_allowed() {
        assert!(!is_disallowed_ip(&IpAddr::V4(V4::new(100, 63, 255, 255))));
        assert!(!is_disallowed_ip(&IpAddr::V4(V4::new(100, 128, 0, 0))));
    }

    #[test]
    fn unspecified_ipv6_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
    }

    #[test]
    fn multicast_ipv6_is_disallowed() {
        assert!(is_disallowed_ip(&IpAddr::V6(Ipv6Addr::new(
            0xff02, 0, 0, 0, 0, 0, 0, 1
        ))));
    }
}
