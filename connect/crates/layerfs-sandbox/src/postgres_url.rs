//! A PostgreSQL connection URL (`postgres://` or `postgresql://`) split at
//! its host, so a Sandbox's daemon can be given the same URL under another
//! host name (Docker Desktop's `host.docker.internal`, a machine's alias).
//!
//! The URL may hold the Store's password: [`PostgresUrl`] has no `Debug`,
//! and nothing here puts any part of the URL into a message.

/// A URL as `scheme://[userinfo@]host[:port][/path][?query]`, borrowed.
pub struct PostgresUrl<'a> {
    scheme: &'a str,
    /// Everything before the last `@` of the authority, password included.
    userinfo: Option<&'a str>,
    /// The host, without an IPv6 address's brackets.
    host: &'a str,
    /// `:<port>`, or empty.
    port: &'a str,
    /// The path and query, from the first `/` or `?` on.
    tail: &'a str,
}

impl<'a> PostgresUrl<'a> {
    /// Splits `url`; `None` when it has no `://` or an unclosed `[`.
    pub fn parse(url: &'a str) -> Option<Self> {
        let (scheme, rest) = url.split_once("://")?;
        let end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        let (userinfo, host_port) = match authority.rsplit_once('@') {
            Some((userinfo, host_port)) => (Some(userinfo), host_port),
            None => (None, authority),
        };
        let (host, port) = match host_port.strip_prefix('[') {
            Some(v6) => v6.split_once(']')?,
            None => match host_port.rsplit_once(':') {
                Some((host, _)) => (host, &host_port[host.len()..]),
                None => (host_port, ""),
            },
        };
        Some(Self {
            scheme,
            userinfo,
            host,
            port,
            tail,
        })
    }

    /// The host, without an IPv6 address's brackets.
    pub fn host(&self) -> &'a str {
        self.host
    }

    /// Whether the host is this host's loopback: `localhost`, `127.0.0.1`
    /// or `::1`.
    pub fn loopback(&self) -> bool {
        matches!(self.host, "127.0.0.1" | "localhost" | "::1")
    }

    /// The same URL with `host` in place of its host; everything else,
    /// the userinfo included, is kept byte for byte.
    pub fn with_host(&self, host: &str) -> String {
        let user = self.userinfo.map(|u| format!("{u}@")).unwrap_or_default();
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        format!("{}://{user}{host}{}{}", self.scheme, self.port, self.tail)
    }
}
