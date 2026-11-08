//! `reach` (#535 §7.3, §7.4, G3): the PostgreSQL Store URL a Sandbox's daemon
//! on a machine dials. A URL whose host is the caller's loopback names a
//! server the machine sees under another name (its host alias); every
//! other host is dialled as given. Each platform's adapter
//! answers with [`dial`] and its own [`Loopback`].
//!
//! The URL may hold the Store's password: no message here holds any part
//! of it.
use crate::error::{Code, MachineError};
use crate::spec::Spec;
use layerfs_sandbox_backend::PostgresUrl;

/// How a machine reaches the caller's loopback.
pub(crate) enum Loopback<'a> {
    /// The daemon shares the caller's loopback: the URL is used as given.
    Shared,
    /// The daemon reaches it under this host name or address.
    Alias(&'a str),
    /// The daemon reaches it under an address read from the machine, only
    /// when a URL names the caller's loopback.
    Lookup(&'a dyn Fn() -> Result<String, MachineError>),
}

/// The URL `spec`'s daemon dials for `url`, given how its machine reaches
/// the caller's loopback.
pub(crate) fn dial(spec: &Spec, url: &str, loopback: Loopback<'_>) -> Result<String, MachineError> {
    if let Loopback::Shared = loopback {
        return Ok(url.to_owned());
    }
    // a host list (`h1:5432,h2:5433`) reads as one host holding a `,`
    let parsed = PostgresUrl::parse(url).filter(|p| !p.host().contains(','));
    let parsed = parsed.ok_or_else(|| {
        refuse(
            spec,
            Code::InvalidInput,
            "its URL is not one postgres://[user@]host[:port]/... URL with one host",
        )
    })?;
    if parsed.query("host").chain(parsed.query("hostaddr")).count() > 0 {
        return Err(refuse(
            spec,
            Code::Unsupported,
            "its URL names the server in a host or hostaddr parameter, which is not rewritten for a machine",
        ));
    }
    if parsed.host().is_empty() {
        return Err(refuse(
            spec,
            Code::Unsupported,
            "its URL names no host (a Unix socket of the caller), which no machine reaches",
        ));
    }
    if !parsed.loopback() {
        return Ok(url.to_owned());
    }
    if parsed.query("sslmode").any(|mode| mode == "verify-full") {
        return Err(refuse(
            spec,
            Code::Unsupported,
            "its URL asks for sslmode=verify-full and names the caller's loopback, which this machine reaches under another name the server's certificate does not name; give the Store a URL whose host every machine can verify",
        ));
    }
    let alias = match loopback {
        Loopback::Alias(alias) => alias.to_owned(),
        Loopback::Lookup(read) => read()?,
        Loopback::Shared => unreachable!("answered above"),
    };
    Ok(parsed.with_host(&alias))
}

fn refuse(spec: &Spec, code: Code, why: &str) -> MachineError {
    MachineError::new(
        code,
        format!(
            "the {} machine {} cannot reach the PostgreSQL Store's server: {why}",
            spec.kind.name(),
            spec.name
        ),
    )
}
