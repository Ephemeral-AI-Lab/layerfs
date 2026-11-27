//! `reach` (#535 §7.3, §7.4, G3): the PostgreSQL Store URL a Sandbox's daemon
//! on a machine dials. The URL is dialled exactly as configured on every
//! machine: LayerFS rewrites no part of it and inspects no port (owner,
//! 2026-11-24). A machine that does not share the caller's loopback
//! refuses a URL naming that loopback, or a Unix socket of the caller,
//! naming what the machine calls this host, with which the Store is
//! connected again (every machine dials the one URL this computer saved). Each platform's adapter declares its [`Loopback`].
//!
//! The URL may hold the Store's password: no message here holds any part
//! of it but a loopback host.
use crate::platform::error::{Code, MachineError};
use crate::setup::spec::Spec;

/// How a machine reaches the caller's loopback.
pub(crate) enum Loopback {
    /// The daemon shares it: the URL is used as given, unread.
    Shared,
    /// The daemon does not share it: a URL naming it is refused, the
    /// refusal naming how the machine reaches this host instead.
    Apart(String),
}

/// The URL `spec`'s daemon dials for `url`, a URL of PostgreSQL Store
/// `store`, given how its machine reaches the caller's loopback: `url`
/// itself, or a refusal. Every host the URL names is checked, read as its
/// client reads them ([`layerfs_sandbox_backend::url_hosts`]): its host
/// list and its `host` and `hostaddr` parameters. Nothing is resolved: a
/// name is dialled as given.
pub(crate) fn dial(
    spec: &Spec,
    (store, url): (&str, &str),
    loopback: Loopback,
) -> Result<String, MachineError> {
    let Loopback::Apart(this_host) = loopback else {
        return Ok(url.to_owned());
    };
    let hosts = crate::table::postgresql_url::url_hosts(url).ok_or_else(|| {
        refuse(
            spec,
            Code::InvalidInput,
            "its URL is not one postgres://[user@]host[:port][,host[:port]...]/... URL its client reads, so the machine cannot check that it does not name this host's loopback",
        )
    })?;
    let advice = advice(store, &this_host);
    if hosts
        .iter()
        .any(|host| crate::table::postgresql_url::socket(host))
    {
        return Err(refuse(
            spec,
            Code::Unsupported,
            &format!("its URL names no host (a Unix socket of the caller), which no machine reaches; {advice}"),
        ));
    }
    match hosts.iter().find(|host| crate::table::postgresql_url::loopback(host)) {
        None => Ok(url.to_owned()),
        Some(host) => Err(refuse(
            spec,
            Code::Unsupported,
            &format!(
                "its URL names this host's loopback ({host}), which the machine does not share; {advice}"
            ),
        )),
    }
}

/// [`crate::loopback_hint`]: the refusal's advice, for a failed dial of a
/// name that resolves to a loopback address on the machine. Only a URL
/// whose client resolves at least one host name can fail so: an address is
/// read, never resolved, and [`dial`] refused every loopback one; with a
/// `hostaddr` the client dials those addresses alone.
pub(crate) fn hint(spec: &Spec, (store, url): (&str, &str), loopback: Loopback) -> Option<String> {
    let Loopback::Apart(this_host) = loopback else {
        return None;
    };
    if crate::table::postgresql_url::url_hostaddr(url) {
        return None;
    }
    let named = |host: &String| {
        !crate::table::postgresql_url::socket(host) && crate::table::postgresql_url::ip(host).is_none()
    };
    if !crate::table::postgresql_url::url_hosts(url)?.iter().any(named) {
        return None;
    }
    Some(format!(
        "if its host is a loopback address on the {} machine {} (a hosts entry can make a name one), the machine does not share this host's loopback; {}",
        spec.kind.name(),
        spec.name,
        advice(store, &this_host)
    ))
}

/// What to do for a machine that does not share the caller's loopback:
/// every machine dials the URL this computer saved, so the Store is
/// connected again with an address the machine reaches.
fn advice(store: &str, this_host: &str) -> String {
    format!(
        "connect the Store again with an address the machine reaches ({this_host}): `layerfs store connect {store} <url>`"
    )
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
