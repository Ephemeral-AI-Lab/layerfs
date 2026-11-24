//! The resolver file a Sandbox's commands see. They run in a network
//! namespace of their own, with no IPv6, so a loopback nameserver in the
//! launcher's `/etc/resolv.conf` (systemd-resolved's `127.0.0.53`, a local
//! dnsmasq) is the Sandbox's own loopback and answers nothing, and an IPv6
//! one is unreachable. Pure, so the rule is tested on any OS:
//!
//! 1. no loopback and no IPv6 `nameserver`: the launcher's file is bound as
//!    it is;
//! 2. else LayerFS writes the Sandbox a file with the launcher's `search`,
//!    `domain` and `options` lines, its IPv4 nameservers that are not this
//!    host's own and, when `127.0.0.53` is among the loopback ones, the IPv4
//!    nameservers of systemd-resolved's upstream file;
//! 3. no IPv4 nameserver left: refused, naming the file and the fix.
//!    LayerFS runs no DNS forwarder.
use std::net::{IpAddr, Ipv4Addr};

/// systemd-resolved's stub address, whose upstream servers are listed in
/// [`SYSTEMD_UPSTREAM`].
pub const SYSTEMD_STUB: &str = "127.0.0.53";
/// The file systemd-resolved lists its upstream servers in.
pub const SYSTEMD_UPSTREAM: &str = "/run/systemd/resolve/resolv.conf";
/// The launcher's resolver file.
pub const LAUNCHER: &str = "/etc/resolv.conf";

/// What the Sandbox gets at `/etc/resolv.conf`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolver {
    /// The launcher's file, bound read-only.
    Bind,
    /// A file LayerFS writes in the Sandbox's directory, with this text.
    Write(String),
    /// No nameserver the Sandbox can reach: the start is refused.
    Refuse(String),
}

/// A `nameserver` value as an address, its IPv6 zone (`%wlp2s0`) left out;
/// a value that is not an address (a malformed line) is none.
fn address(value: &str) -> Option<IpAddr> {
    value.split('%').next().unwrap_or(value).parse().ok()
}

/// Whether a `nameserver` value is this host's own: a loopback address
/// (`127.0.0.0/8`, `::1`, IPv4-mapped loopback) or the unspecified one
/// (`0.0.0.0`, `::`), which reaches this host too.
fn loopback(value: &str) -> bool {
    match address(value) {
        Some(IpAddr::V4(v4)) => v4.is_loopback() || v4.is_unspecified(),
        Some(IpAddr::V6(v6)) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        None => false,
    }
}

/// A `nameserver` value a Sandbox reaches: an IPv4 address that is not
/// this host's own.
fn reachable(value: &str) -> Option<Ipv4Addr> {
    match address(value) {
        Some(IpAddr::V4(v4)) if !loopback(value) => Some(v4),
        _ => None,
    }
}

/// The value of a `nameserver` line, if it is one.
fn nameserver(line: &str) -> Option<&str> {
    let mut words = line.split_whitespace();
    (words.next() == Some("nameserver"))
        .then(|| words.next())
        .flatten()
}

/// The resolver file for a Sandbox from the launcher's `launcher` text and,
/// when it can be read, systemd-resolved's upstream file `systemd`.
pub fn resolver_file(launcher: &str, systemd: Option<&str>) -> Resolver {
    let servers: Vec<&str> = launcher.lines().filter_map(nameserver).collect();
    let local: Vec<&str> = servers.iter().copied().filter(|s| loopback(s)).collect();
    let ipv6 = (servers.iter()).any(|s| !loopback(s) && matches!(address(s), Some(IpAddr::V6(_))));
    if local.is_empty() && !ipv6 {
        return Resolver::Bind;
    }
    let mut kept: Vec<Ipv4Addr> = Vec::new();
    let mut keep = |server: &str| {
        if let Some(v4) = reachable(server).filter(|v4| !kept.contains(v4)) {
            kept.push(v4);
        }
    };
    servers.iter().for_each(|server| keep(server));
    if local.contains(&SYSTEMD_STUB) {
        if let Some(upstream) = systemd {
            upstream.lines().filter_map(nameserver).for_each(&mut keep);
        }
    }
    if kept.is_empty() {
        let named = servers.join(", ");
        let upstream = match local.contains(&SYSTEMD_STUB) {
            true => format!(" and {SYSTEMD_UPSTREAM} lists no IPv4 server"),
            false => String::new(),
        };
        return Resolver::Refuse(format!(
            "this machine's {LAUNCHER} names no IPv4 nameserver a Sandbox reaches (it names \
             {named}){upstream}; a Sandbox has a loopback of its own, where no resolver answers, \
             and no IPv6, and LayerFS runs no DNS forwarder: make the resolver listen on an IPv4 \
             address this machine routes and name it in {LAUNCHER}, or use systemd-resolved"
        ));
    }
    let mut text = format!(
        "# written by LayerFS for this Sandbox's own network, from {LAUNCHER} with its \
         loopback and IPv6 nameservers replaced\n"
    );
    for server in &kept {
        text.push_str(&format!("nameserver {server}\n"));
    }
    for line in launcher.lines() {
        let first = line.split_whitespace().next();
        if matches!(first, Some("search" | "domain" | "options")) {
            text.push_str(line.trim());
            text.push('\n');
        }
    }
    Resolver::Write(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_without_a_loopback_nameserver_is_bound_as_it_is() {
        let docker = "# Based on host file\nnameserver 192.168.65.7\n";
        assert_eq!(resolver_file(docker, None), Resolver::Bind);
        assert_eq!(resolver_file("", None), Resolver::Bind);
    }

    #[test]
    fn the_systemd_stub_is_replaced_by_its_upstream_and_search_and_options_kept() {
        let lima = "nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch ctc\n";
        let upstream =
            "# This is /run/systemd/resolve/resolv.conf\nnameserver 192.168.5.2\nsearch ctc\n";
        let Resolver::Write(text) = resolver_file(lima, Some(upstream)) else {
            panic!("expected a written file");
        };
        let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(
            lines,
            [
                "nameserver 192.168.5.2",
                "options edns0 trust-ad",
                "search ctc"
            ]
        );
        assert!(!text.contains("127.0.0.53"));
    }

    #[test]
    fn a_mixed_list_keeps_its_routed_servers() {
        let mixed = "nameserver 127.0.0.1\nnameserver 10.0.0.2\nnameserver ::1\ndomain example\n";
        let Resolver::Write(text) = resolver_file(mixed, None) else {
            panic!("expected a written file");
        };
        assert!(text.contains("nameserver 10.0.0.2\n"));
        assert!(text.contains("domain example\n"));
        assert!(!text.contains("127.0.0.1") && !text.contains("::1"));
    }

    #[test]
    fn a_loopback_only_resolver_with_no_upstream_is_refused() {
        let dnsmasq = "nameserver 127.0.0.1\n";
        let Resolver::Refuse(why) = resolver_file(dnsmasq, None) else {
            panic!("expected a refusal");
        };
        assert!(why.contains("127.0.0.1"), "{why}");
        assert!(why.contains("no DNS forwarder"), "{why}");
        // the stub with no readable upstream, or an upstream of only loopback
        let stub = "nameserver 127.0.0.53\n";
        assert!(matches!(resolver_file(stub, None), Resolver::Refuse(_)));
        let Resolver::Refuse(why) = resolver_file(stub, Some("nameserver 127.0.0.1\n")) else {
            panic!("expected a refusal");
        };
        assert!(why.contains(SYSTEMD_UPSTREAM), "{why}");
        // an IPv4-mapped loopback is loopback too
        assert!(matches!(
            resolver_file("nameserver ::ffff:127.0.0.1\n", None),
            Resolver::Refuse(_)
        ));
    }

    #[test]
    fn a_dual_stack_upstream_listing_ipv6_first_keeps_only_its_ipv4_servers() {
        // systemd's order on a dual-stack host: glibc reads only the first
        // three servers, all IPv6 here, which a Sandbox cannot reach
        let stub = "nameserver 127.0.0.53\noptions edns0 trust-ad\nsearch lan\n";
        let upstream = "nameserver fe80::1%wlp2s0\nnameserver 2001:db8::53\n\
                        nameserver 2001:db8::54\nnameserver 192.168.1.1\n";
        let Resolver::Write(text) = resolver_file(stub, Some(upstream)) else {
            panic!("expected a written file");
        };
        let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(
            lines,
            [
                "nameserver 192.168.1.1",
                "options edns0 trust-ad",
                "search lan"
            ]
        );
        // a launcher file that names IPv6 servers itself, first, with no
        // loopback one, is written too, keeping its IPv4 server
        let direct = "nameserver 2001:db8::53\nnameserver fe80::1%eth0\nnameserver 10.0.0.2\n";
        let Resolver::Write(text) = resolver_file(direct, None) else {
            panic!("expected a written file");
        };
        let servers: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("nameserver"))
            .collect();
        assert_eq!(servers, ["nameserver 10.0.0.2"]);
    }

    #[test]
    fn an_ipv6_only_resolver_is_refused_naming_the_file() {
        let v6 = "nameserver 2001:db8::53\nnameserver fe80::1%eth0\nsearch lan\n";
        let Resolver::Refuse(why) = resolver_file(v6, None) else {
            panic!("expected a refusal");
        };
        assert!(why.contains(LAUNCHER), "{why}");
        assert!(why.contains("2001:db8::53"), "{why}");
        assert!(why.contains("IPv4"), "{why}");
        // the stub whose upstream lists only IPv6 servers
        let upstream = "nameserver 2001:db8::53\nnameserver fe80::1%wlp2s0\n";
        let Resolver::Refuse(why) = resolver_file("nameserver 127.0.0.53\n", Some(upstream)) else {
            panic!("expected a refusal");
        };
        assert!(
            why.contains(SYSTEMD_UPSTREAM) && why.contains(LAUNCHER),
            "{why}"
        );
    }

    #[test]
    fn a_loopback_and_ipv6_mix_keeps_only_routed_ipv4_servers() {
        let mixed =
            "nameserver 127.0.0.1\nnameserver 2001:db8::53\nnameserver 0.0.0.0\nnameserver 10.0.0.2\n";
        let Resolver::Write(text) = resolver_file(mixed, None) else {
            panic!("expected a written file");
        };
        let servers: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("nameserver"))
            .collect();
        assert_eq!(servers, ["nameserver 10.0.0.2"]);
        // with no IPv4 server left it is refused
        let Resolver::Refuse(why) =
            resolver_file("nameserver ::1\nnameserver 2001:db8::53\n", None)
        else {
            panic!("expected a refusal");
        };
        assert!(why.contains("::1") && why.contains("2001:db8::53"), "{why}");
    }
}
