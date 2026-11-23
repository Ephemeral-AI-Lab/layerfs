//! The hosts a PostgreSQL connection URL (`postgres://` or `postgresql://`)
//! names, read exactly as its client reads them, so that a machine that
//! does not share the caller's loopback can refuse a URL naming it (the URL
//! itself is never rewritten).
//!
//! The client is tokio-postgres 0.7 (`Config::from_str`, its `UrlParser`):
//! the userinfo ends at the first `@` anywhere in the URL; the host list
//! ends at the first `/` or `?` after it and splits at each `,`; a host is
//! what a `[...]` holds, else what comes before its first `:` (an empty one
//! is a name no lookup resolves, and the client goes on to the next); each host,
//! each query key and a `hostaddr` value are percent-decoded (`%` and two
//! hex digits, any other `%` kept), and a host whose decoded bytes start
//! with `/` is a Unix socket directory. A host is then dialled through
//! `getaddrinfo`, which also reads the legacy IPv4 forms of `inet_aton`
//! (`127.1`, `2130706433`, `0x7f.1`, `017700000001`) and IPv6 zone ids
//! (`::1%lo`); [`ip`] reads both.
//!
//! The URL may hold the Store's password: nothing here keeps it or puts any
//! part of the URL into a message.
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Every server host `url` names, decoded as its client decodes them: each
/// host of its host list, then each one a `host` or `hostaddr` query
/// parameter names, in order. A `host` value the client takes as one host
/// is split at `,` here too, so no spelling of it goes unchecked. An empty
/// host (`h1,,h2`, `?host=`) is left out: the client's lookup of it fails
/// and it dials the next. A URL naming none is answered as one empty host
/// (`""`), which [`socket`] counts as naming no host: its client refuses
/// it (`both host and hostaddr are missing`).
/// `None` when `url` is not one the client reads: no `postgres://` or
/// `postgresql://`, a `[` never closed, anything but `:port` after its
/// `]`, a parameter with no `=`, or a host, key or `hostaddr` value whose
/// decoded bytes are not UTF-8 (a socket directory may be any bytes).
pub fn url_hosts(url: &str) -> Option<Vec<String>> {
    read(url).map(|(hosts, _)| hosts)
}

/// Whether `url` gives a `hostaddr` parameter its client reads: the client
/// then dials those addresses alone (the i-th for the i-th host, whose
/// name only TLS checks) and resolves no host name. `false` for a URL it
/// does not read.
pub fn url_hostaddr(url: &str) -> bool {
    read(url).is_some_and(|(_, hostaddr)| hostaddr)
}

/// [`url_hosts`], and whether a `hostaddr` parameter was given.
fn read(url: &str) -> Option<(Vec<String>, bool)> {
    let rest = (url.strip_prefix("postgres://")).or_else(|| url.strip_prefix("postgresql://"))?;
    // the userinfo ends at the first `@`, wherever it is
    let rest = rest.split_once('@').map_or(rest, |(_, after)| after);
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (list, tail) = rest.split_at(end);
    let mut hosts = Vec::new();
    if !list.is_empty() {
        for chunk in list.split(',') {
            let host = match chunk.strip_prefix('[') {
                Some(v6) => {
                    let (host, after) = v6.split_once(']')?;
                    if !after.is_empty() && !after.starts_with(':') {
                        return None;
                    }
                    host
                }
                None => chunk.split(':').next().unwrap_or(chunk),
            };
            hosts.extend(Some(host_text(host)?).filter(|host| !host.is_empty()));
        }
    }
    // the path, if any, ends at the first `?`; then `key=value` pairs, each
    // key ending at its first `=`, each value at the next `&`
    let mut query = tail.split_once('?').map_or("", |(_, query)| query);
    let mut hostaddr = false;
    while !query.is_empty() {
        let (key, after) = query.split_once('=')?;
        let (value, next) = after.split_once('&').unwrap_or((after, ""));
        query = next;
        match String::from_utf8(decode(key)).ok()?.as_str() {
            "host" => match host_text(value)? {
                path if path.starts_with('/') => hosts.push(path),
                named => hosts.extend(
                    (named.split(','))
                        .filter(|host| !host.is_empty())
                        .map(str::to_owned),
                ),
            },
            "hostaddr" => {
                let value = String::from_utf8(decode(value)).ok()?;
                hosts.extend(value.split(',').map(str::to_owned));
                hostaddr = true;
            }
            _ => {}
        }
    }
    if hosts.is_empty() {
        hosts.push(String::new());
    }
    Some((hosts, hostaddr))
}

/// One host as the client reads it: percent-decoded; a socket directory
/// (decoded bytes starting with `/`) may be any bytes, any other host must
/// be UTF-8.
fn host_text(raw: &str) -> Option<String> {
    let bytes = decode(raw);
    match bytes.first() {
        Some(b'/') => Some(String::from_utf8_lossy(&bytes).into_owned()),
        _ => String::from_utf8(bytes).ok(),
    }
}

/// `text` percent-decoded as the `percent-encoding` crate decodes it: `%`
/// and two hex digits (either case) is that byte, any other `%` is kept.
fn decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
        match (byte, hex(at + 1), hex(at + 2)) {
            (b'%', Some(high), Some(low)) => {
                decoded.push((high * 16 + low) as u8);
                at += 3;
            }
            _ => {
                decoded.push(byte);
                at += 1;
            }
        }
    }
    decoded
}

/// Whether `host` (one of [`url_hosts`]) names no TCP host: none at all
/// (the empty host a URL naming none is answered as), or a Unix socket
/// directory, a path. The client reads only a leading `/` as a path: any
/// other host (`@name` included) is a name it looks up.
pub fn socket(host: &str) -> bool {
    host.is_empty() || host.starts_with('/')
}

/// Whether `host` (one of [`url_hosts`]) names this host's loopback
/// without being resolved: `localhost` (any case, with or without its
/// final dot), any address of 127.0.0.0/8 or the unspecified address
/// `0.0.0.0` (which reaches this host), `::1` or `::`, or an IPv4-mapped
/// one of those, in any spelling [`ip`] reads. Any other name is not
/// resolved here: one resolving to a loopback address fails at its dial.
pub fn loopback(host: &str) -> bool {
    if bare(host).eq_ignore_ascii_case("localhost") {
        return true;
    }
    let local = |v4: Ipv4Addr| v4.is_loopback() || v4.is_unspecified();
    match ip(host) {
        Some(IpAddr::V4(v4)) => local(v4),
        Some(IpAddr::V6(v6)) => {
            v6.is_loopback() || v6.is_unspecified() || v6.to_ipv4_mapped().is_some_and(local)
        }
        None => false,
    }
}

/// The address `host` spells, if it spells one, as the machine's
/// `getaddrinfo` reads it before it resolves anything: an IPv6 address
/// (brackets and a zone id `%<zone>` dropped), or an IPv4 address in any
/// form `inet_aton` accepts. A final dot is dropped first.
pub fn ip(host: &str) -> Option<IpAddr> {
    let host = bare(host);
    if let Ok(v6) = host.parse::<Ipv6Addr>() {
        return Some(IpAddr::V6(v6));
    }
    inet_aton(host).map(IpAddr::V4)
}

/// `host` without the brackets of an IPv6 address, its zone id and a
/// final dot.
fn bare(host: &str) -> &str {
    let host = (host.strip_prefix('[').and_then(|h| h.strip_suffix(']'))).unwrap_or(host);
    let host = host.split('%').next().unwrap_or(host);
    host.strip_suffix('.').unwrap_or(host)
}

/// `text` as `inet_aton` reads it (glibc's and musl's, which differ only
/// on a bare `0x`: musl refuses it, and glibc's reading has changed between
/// versions; here it is zero, which can only read more hosts as
/// addresses, never fewer): one to four
/// numbers separated by `.`, each decimal, octal (a leading `0`) or hex
/// (`0x`), each starting with a digit; the last fills the bits the earlier
/// ones leave (`a`, `a.b` with `b` < 2^24, `a.b.c` with `c` < 2^16,
/// `a.b.c.d`), every earlier one a byte.
fn inet_aton(text: &str) -> Option<Ipv4Addr> {
    let mut parts = Vec::with_capacity(4);
    for part in text.split('.') {
        if parts.len() == 4 {
            return None;
        }
        parts.push(number(part)?);
    }
    let (last, earlier) = parts.split_last()?;
    if earlier.iter().any(|byte| *byte > 0xff) {
        return None;
    }
    let bits = 8 * (4 - earlier.len() as u32);
    if (bits < 32 && *last >> bits != 0) || *last > u64::from(u32::MAX) {
        return None;
    }
    let high = earlier.iter().fold(0u64, |value, byte| (value << 8) | byte);
    let value = (high << bits) | last;
    Some(Ipv4Addr::from(u32::try_from(value).ok()?))
}

/// One number of an `inet_aton` address: decimal, octal after a leading
/// `0`, hex after `0x` or `0X`; `None` when it does not start with a digit,
/// holds another character or overflows.
fn number(part: &str) -> Option<u64> {
    if !part.as_bytes().first()?.is_ascii_digit() {
        return None;
    }
    let (digits, radix) = match part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
        Some(hex) => (hex, 16),
        None if part.len() > 1 && part.starts_with('0') => (&part[1..], 8),
        None => (part, 10),
    };
    if digits.is_empty() {
        return Some(0);
    }
    if !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    u64::from_str_radix(digits, radix).ok()
}
