//! Connection-string rewriting shared by the diagnostic and doctor suites.

/// Add one libpq connection option to a connection string.
///
/// A URL (`…://…`) gets a query parameter whose value is percent-encoded. The
/// libpq keyword form that the docs and CI use gets `name='value'`, because
/// `?`/`&` appended to it is parsed as part of the preceding value and the
/// server refuses the connection.
pub fn with_option(url: &str, name: &str, value: &str) -> String {
    if url.contains("://") {
        let separator = if url.contains('?') { '&' } else { '?' };
        format!("{url}{separator}{name}={}", percent_encode(value))
    } else {
        format!("{url} {name}='{value}'")
    }
}

/// Put an unresolvable host in front of the real fixture host.
///
/// The diagnostic driver tries every configured host in order and treats a
/// failed lookup as "try the next host", so `missing.invalid` exercises the
/// failover path. Rewriting only the URL form would leave the keyword form
/// unchanged and the assertion would silently stop exercising failover.
pub fn with_failing_first_host(conninfo: &str) -> String {
    const HOST: &str = "127.0.0.1";
    const MISSING: &str = "missing.invalid";
    if conninfo.contains("://") {
        return conninfo.replacen(HOST, &format!("{MISSING},{HOST}"), 1);
    }
    for (from, to) in [
        (format!("host={HOST}"), format!("host={MISSING},{HOST}")),
        (format!("host='{HOST}'"), format!("host='{MISSING},{HOST}'")),
    ] {
        if conninfo.contains(&from) {
            return conninfo.replacen(&from, &to, 1);
        }
    }
    conninfo.to_owned()
}

/// Rewrite a connection string's endpoint to `host:port`.
///
/// The physical doctor's byte-counting proxy forwards to the fixture, so the
/// selection it opens must reach the proxy's port in whichever connection
/// string form the environment supplied. A URL keeps its user information,
/// path and query and has its authority's host and port replaced; the libpq
/// keyword form has its `host=` and `port=` values replaced (and added when
/// absent), leaving every other keyword — quoted values with spaces included —
/// untouched.
///
/// Rewriting only `:{port}/` would match the URL form alone: the keyword form
/// would keep connecting straight to the fixture, and the proxy's byte count
/// would silently stop measuring the wire.
pub fn with_host(conninfo: &str, host: &str, port: u16) -> String {
    if conninfo.contains("://") {
        url_with_host(conninfo, host, port)
    } else {
        keyword_with_host(conninfo, host, port)
    }
}

/// [`with_host`] for the physical doctor's loopback proxy, whose listener and
/// forwarder are both `127.0.0.1`.
pub fn with_local_proxy(conninfo: &str, proxy_port: u16) -> String {
    with_host(conninfo, "127.0.0.1", proxy_port)
}

fn url_with_host(url: &str, host: &str, port: u16) -> String {
    let Some(scheme_end) = url.find("://").map(|at| at + 3) else {
        return url.to_owned();
    };
    let (scheme, rest) = url.split_at(scheme_end);
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let userinfo_end = authority.rfind('@').map(|at| at + 1).unwrap_or(0);
    let userinfo = &authority[..userinfo_end];
    format!("{scheme}{userinfo}{host}:{port}{tail}")
}

fn keyword_with_host(conninfo: &str, host: &str, port: u16) -> String {
    let bytes = conninfo.as_bytes();
    let mut out = String::with_capacity(conninfo.len() + host.len() + 16);
    let mut seen_host = false;
    let mut seen_port = false;
    let mut at = 0;
    while at < bytes.len() {
        let space_start = at;
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        out.push_str(&conninfo[space_start..at]);
        if at == bytes.len() {
            break;
        }
        let key_start = at;
        while at < bytes.len() && (bytes[at] == b'_' || bytes[at].is_ascii_alphanumeric()) {
            at += 1;
        }
        let key_end = at;
        if key_end == key_start || bytes.get(key_end) != Some(&b'=') {
            // Not `keyword=value`: keep the tail exactly as it came in.
            out.push_str(&conninfo[key_start..]);
            break;
        }
        at += 1;
        if bytes.get(at) == Some(&b'\'') {
            at += 1;
            while at < bytes.len() {
                match bytes[at] {
                    b'\\' => at = (at + 2).min(bytes.len()),
                    b'\'' if bytes.get(at + 1) == Some(&b'\'') => at += 2,
                    b'\'' => {
                        at += 1;
                        break;
                    }
                    _ => at += 1,
                }
            }
        } else {
            while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
                at += 1;
            }
        }
        match &conninfo[key_start..key_end] {
            "host" if !seen_host => {
                out.push_str("host=");
                out.push_str(host);
                seen_host = true;
            }
            "port" if !seen_port => {
                out.push_str("port=");
                out.push_str(&port.to_string());
                seen_port = true;
            }
            _ => out.push_str(&conninfo[key_start..at]),
        }
    }
    if !seen_host {
        out.push_str(&format!(" host={host}"));
    }
    if !seen_port {
        out.push_str(&format!(" port={port}"));
    }
    out
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{with_failing_first_host, with_host, with_local_proxy, with_option};

    #[test]
    fn url_form_rewrites_options_and_endpoints() {
        assert_eq!(
            with_option("postgresql://u@h/db", "options", "-c role=r"),
            "postgresql://u@h/db?options=-c%20role%3Dr"
        );
        assert_eq!(
            with_option(
                "postgresql://u@h/db?sslmode=disable",
                "options",
                "-c role=r"
            ),
            "postgresql://u@h/db?sslmode=disable&options=-c%20role%3Dr"
        );
        assert_eq!(
            with_host(
                "postgresql://u:p@127.0.0.1:5432/db?sslmode=disable",
                "127.0.0.1",
                9000
            ),
            "postgresql://u:p@127.0.0.1:9000/db?sslmode=disable"
        );
        assert_eq!(
            with_host("postgres://127.0.0.1/db", "127.0.0.1", 9000),
            "postgres://127.0.0.1:9000/db"
        );
        assert_eq!(
            with_host("postgresql://u@127.0.0.1:5432", "127.0.0.1", 9000),
            "postgresql://u@127.0.0.1:9000"
        );
        assert_eq!(
            with_local_proxy("postgresql://u@127.0.0.1:5432/db", 9000),
            "postgresql://u@127.0.0.1:9000/db"
        );
    }

    #[test]
    fn keyword_form_rewrites_options_and_endpoints() {
        assert_eq!(
            with_option("host=h port=1 sslmode=disable", "options", "-c role=r"),
            "host=h port=1 sslmode=disable options='-c role=r'"
        );
        assert_eq!(
            with_host("host=127.0.0.1 port=5432 sslmode=disable", "127.0.0.1", 9000),
            "host=127.0.0.1 port=9000 sslmode=disable"
        );
        assert_eq!(
            with_host("host='127.0.0.1' port='5432'", "127.0.0.1", 9000),
            "host=127.0.0.1 port=9000"
        );
        assert_eq!(
            with_host(
                "host=127.0.0.1 port=5432 options='-c role=r'",
                "127.0.0.1",
                9000
            ),
            "host=127.0.0.1 port=9000 options='-c role=r'"
        );
        assert_eq!(
            with_local_proxy("host=127.0.0.1 port=5432", 9000),
            "host=127.0.0.1 port=9000"
        );
    }

    #[test]
    fn url_failover_puts_the_missing_host_first() {
        assert_eq!(
            with_failing_first_host("postgresql://u:p@127.0.0.1:5432/db?sslmode=disable"),
            "postgresql://u:p@missing.invalid,127.0.0.1:5432/db?sslmode=disable"
        );
    }

    #[test]
    fn keyword_failover_puts_the_missing_host_first() {
        assert_eq!(
            with_failing_first_host("host=127.0.0.1 port=5432 sslmode=disable"),
            "host=missing.invalid,127.0.0.1 port=5432 sslmode=disable"
        );
    }
}
