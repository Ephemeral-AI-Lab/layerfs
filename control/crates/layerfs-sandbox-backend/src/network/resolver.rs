//! The resolver file a Sandbox's commands see. They run in a network
//! namespace of their own, so a loopback nameserver in the launcher's
//! `/etc/resolv.conf` (systemd-resolved's `127.0.0.53`, a local dnsmasq) is
//! the Sandbox's own loopback and answers nothing. Pure, so the rule is
//! tested on any OS:
//!
//! 1. no loopback `nameserver`: the launcher's file is bound as it is;
//! 2. a loopback one: LayerFS writes the Sandbox a file with the launcher's
//!    `search`, `domain` and `options` lines, its other nameservers and,
//!    when `127.0.0.53` is among the loopback ones, the nameservers of
//!    systemd-resolved's upstream file;
//! 3. no nameserver left: refused, naming the loopback one and the fix.
//!    LayerFS runs no DNS forwarder.
use std::net::IpAddr;

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

/// Whether a `nameserver` value is a loopback address (`127.0.0.0/8`,
/// `::1`, or IPv4-mapped loopback). A value that is not an address (a
/// malformed line) is not one.
fn loopback(value: &str) -> bool {
    let address = value.split('%').next().unwrap_or(value);
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.is_loopback(),
        Ok(IpAddr::V6(v6)) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        Err(_) => false,
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
    if local.is_empty() {
        return Resolver::Bind;
    }
    let mut kept: Vec<String> = Vec::new();
    let mut keep = |server: &str| {
        if !loopback(server) && !kept.iter().any(|seen| seen == server) {
            kept.push(server.to_owned());
        }
    };
    servers.iter().for_each(|server| keep(server));
    if local.contains(&SYSTEMD_STUB) {
        if let Some(upstream) = systemd {
            upstream.lines().filter_map(nameserver).for_each(&mut keep);
        }
    }
    if kept.is_empty() {
        let named = local.join(", ");
        let upstream = match local.contains(&SYSTEMD_STUB) {
            true => format!(" and {SYSTEMD_UPSTREAM} lists no other server"),
            false => String::new(),
        };
        return Resolver::Refuse(format!(
            "this machine's {LAUNCHER} names only the loopback nameserver {named}{upstream}; a \
             Sandbox has a loopback of its own, where no resolver answers, and LayerFS runs no \
             DNS forwarder: make the resolver listen on an address this machine routes and name \
             it in {LAUNCHER}, or use systemd-resolved"
        ));
    }
    let mut text = format!(
        "# written by LayerFS for this Sandbox's own network, from {LAUNCHER} with its \
         loopback nameservers replaced\n"
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
}
