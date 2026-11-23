//! A PostgreSQL connection URL (`postgres://` or `postgresql://`) split at
//! its host, so the hosts it names can be read: a machine that does not
//! share the caller's loopback refuses a URL naming it (the URL itself is
//! never rewritten).
//!
//! The URL may hold the Store's password: [`PostgresUrl`] has no `Debug`,
//! and nothing here puts any part of the URL into a message.

/// A URL as `scheme://[userinfo@]host[:port][/path][?query]`, borrowed.
pub struct PostgresUrl<'a> {
    /// The host, without an IPv6 address's brackets.
    host: &'a str,
    /// The path and query, from the first `/` or `?` on.
    tail: &'a str,
}

impl<'a> PostgresUrl<'a> {
    /// Splits `url`; `None` when it has no `://`, an unclosed `[`, or a
    /// port that is not 1 to 5 digits (a host list `h1,h2:5433` reads as
    /// one host holding a `,`).
    pub fn parse(url: &'a str) -> Option<Self> {
        let (_scheme, rest) = url.split_once("://")?;
        let end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        // everything before the last `@` is the userinfo, password included
        let host_port = authority.rsplit_once('@').map_or(authority, |(_, at)| at);
        let (host, port) = match host_port.strip_prefix('[') {
            Some(v6) => v6.split_once(']')?,
            None => match host_port.rsplit_once(':') {
                Some((host, _)) => (host, &host_port[host.len()..]),
                None => (host_port, ""),
            },
        };
        let port_ok = match port.strip_prefix(':') {
            Some(digits) => {
                (1..=5).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
            }
            None => port.is_empty(),
        };
        if !port_ok {
            return None;
        }
        Some(Self { host, tail })
    }

    /// The host, without an IPv6 address's brackets.
    pub fn host(&self) -> &'a str {
        self.host
    }

    /// Every server host the URL names: each host of its host list
    /// (`h1,h2:5433`, ports and IPv6 brackets dropped), then each one a
    /// `host` or `hostaddr` parameter names. `%2F` reads as `/`, so a
    /// socket directory reads as one. An empty host is libpq's default, a
    /// Unix socket.
    pub fn hosts(&self) -> Vec<String> {
        let mut hosts = list(self.host);
        for value in self.query("host").chain(self.query("hostaddr")) {
            hosts.extend(list(value));
        }
        hosts
    }

    /// The values of the query parameter `key`, in order (not
    /// percent-decoded).
    pub fn query(&self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let query = self.tail.split_once('?').map_or("", |(_, query)| query);
        query.split('&').filter_map(move |pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (name == key).then_some(value)
        })
    }
}

/// Every server host a connection string names: a URL's
/// ([`PostgresUrl::hosts`]), or the `host` and `hostaddr` values of
/// libpq's `key=value` words (single quotes and backslash escapes read as
/// libpq reads them); words naming neither name the default Unix socket
/// (`""`). `None` when it is neither form.
pub fn conninfo_hosts(conninfo: &str) -> Option<Vec<String>> {
    if conninfo.contains("://") {
        return PostgresUrl::parse(conninfo).map(|url| url.hosts());
    }
    let mut hosts = Vec::new();
    let mut named = false;
    for (key, value) in words(conninfo)? {
        if key == "host" || key == "hostaddr" {
            named = true;
            hosts.extend(list(&value));
        }
    }
    if !named {
        hosts.push(String::new());
    }
    Some(hosts)
}

/// libpq's `key = value` words, or `None` when one is malformed.
fn words(text: &str) -> Option<Vec<(String, String)>> {
    let mut chars = text.chars().peekable();
    let mut found = Vec::new();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            return Some(found);
        }
        let mut key = String::new();
        while let Some(c) = chars.next_if(|c| *c != '=' && !c.is_whitespace()) {
            key.push(c);
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if key.is_empty() || chars.next() != Some('=') {
            return None;
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let quoted = chars.next_if_eq(&'\'').is_some();
        let mut value = String::new();
        loop {
            match chars.next() {
                Some('\\') => value.push(chars.next()?),
                Some('\'') if quoted => break,
                Some(c) if !quoted && c.is_whitespace() => break,
                Some(c) => value.push(c),
                None if quoted => return None,
                None => break,
            }
        }
        found.push((key, value));
    }
}

/// The hosts of one comma-separated list, each [`bare`], `%2F` read as `/`.
fn list(text: &str) -> Vec<String> {
    let text = text.replace("%2F", "/").replace("%2f", "/");
    text.split(',').map(|host| bare(host).to_owned()).collect()
}

/// One host of a host list without its port or an IPv6 address's brackets.
fn bare(host: &str) -> &str {
    if let Some(v6) = host.strip_prefix('[') {
        return v6.split_once(']').map_or(v6, |(v6, _)| v6);
    }
    match host.rsplit_once(':') {
        // `h:5432`, not an unbracketed IPv6 address (`::1`)
        Some((name, port)) if !name.contains(':') && port.bytes().all(|b| b.is_ascii_digit()) => {
            name
        }
        _ => host,
    }
}

/// Whether `host` names this host's loopback without being resolved:
/// `localhost` (any case, with or without its final dot), any address of
/// 127.0.0.0/8, `::1`, or an IPv4-mapped `::ffff:127.x.x.x`. Any other name
/// is not resolved here: one resolving to a loopback address fails at its
/// dial.
pub fn loopback(host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback(),
        Ok(std::net::IpAddr::V6(v6)) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        Err(_) => false,
    }
}
