//! `reach` (#535 §7.3, §7.4): the PostgreSQL Store URL a Sandbox daemon on
//! each platform dials. A loopback host becomes the platform's host alias or
//! is refused naming what is missing; a rewrite is refused for
//! `sslmode=verify-full`; every other host is dialled as given. No message
//! holds the URL's password. Nothing runs: no stand-in tool is called.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_machine::{self as machine, Code, MachineError, Platform, Spec};
use standin::Scratch;

const SECRET: &str = "s3cr3t-pw";

/// `kind`'s spec as `standin` makes it; a firecracker one gets its nested
/// lima host and, when given, its network number.
fn spec(scratch: &Scratch, kind: Platform, network: Option<u8>) -> Spec {
    let mut spec = scratch.spec(kind);
    if kind == Platform::Firecracker {
        let mut host = scratch.spec(Platform::Lima);
        host.nested = true;
        spec.host = Some(Box::new(host));
    }
    spec.state.network = network;
    spec
}

/// A refusal's code and message; the message never holds the password.
fn refused(outcome: Result<String, MachineError>) -> (Code, String) {
    let error = outcome.unwrap_err();
    assert!(!error.message.contains(SECRET), "{error:?}");
    assert!(!format!("{error:?}").contains(SECRET), "{error:?}");
    assert!(!error.vm_exists);
    (error.code, error.message)
}

fn loopback_urls() -> [String; 3] {
    [
        format!("postgres://lfs:{SECRET}@127.0.0.1:5432/layerfs?sslmode=disable"),
        format!("postgresql://lfs:{SECRET}@localhost/layerfs"),
        format!("postgres://lfs:{SECRET}@[::1]:6543/layerfs"),
    ]
}

/// The URL with `host` for its loopback host, as the three above.
fn rewritten(host: &str) -> [String; 3] {
    [
        format!("postgres://lfs:{SECRET}@{host}:5432/layerfs?sslmode=disable"),
        format!("postgresql://lfs:{SECRET}@{host}/layerfs"),
        format!("postgres://lfs:{SECRET}@{host}:6543/layerfs"),
    ]
}

#[test]
fn each_platform_rewrites_or_refuses_a_loopback_host() {
    let scratch = Scratch::new("reach-loopback");
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind, Some(7));
        let alias = match kind {
            Platform::Native => Some("unchanged"),
            Platform::Docker => Some("host.docker.internal"),
            Platform::Lima => Some("host.lima.internal"),
            Platform::Firecracker => Some("172.30.7.1"),
            Platform::AppleContainer | Platform::Microsandbox | Platform::Ssh => None,
        };
        for (i, url) in loopback_urls().iter().enumerate() {
            let outcome = machine::reach(&spec, url);
            match alias {
                Some("unchanged") => assert_eq!(outcome.as_deref(), Ok(url.as_str())),
                Some(alias) => assert_eq!(outcome, Ok(rewritten(alias)[i].clone()), "{kind:?}"),
                None => {
                    let (code, message) = refused(outcome);
                    assert_eq!(code, Code::Unsupported);
                    let why = match kind {
                        Platform::Ssh => "an ssh machine cannot reach the caller's loopback",
                        Platform::AppleContainer => "apple-container host alias not verified (G7)",
                        _ => "microsandbox host alias not verified (G7)",
                    };
                    assert_eq!(
                        message,
                        format!(
                            "the {} machine dev cannot reach the PostgreSQL Store's server: {why}",
                            kind.name()
                        )
                    );
                }
            }
        }
    }
}

#[test]
fn a_firecracker_machine_without_its_network_number_is_refused() {
    let scratch = Scratch::new("reach-firecracker");
    let spec = spec(&scratch, Platform::Firecracker, None);
    let (code, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert_eq!(code, Code::Unsupported);
    assert!(
        message.ends_with(
            "its network number (the n of 172.30.<n>.1, recorded at create) is not known"
        ),
        "{message}"
    );
    // a server elsewhere needs no network number
    let remote = format!("postgres://lfs:{SECRET}@db.example.com:5432/layerfs");
    assert_eq!(machine::reach(&spec, &remote), Ok(remote.clone()));
}

#[test]
fn every_other_host_is_dialled_as_given_everywhere() {
    let scratch = Scratch::new("reach-remote");
    let urls = [
        format!("postgres://lfs:{SECRET}@db.example.com:5432/layerfs?sslmode=verify-full"),
        "postgresql://lfs@10.0.0.5/layerfs".to_owned(),
        "postgres://[2001:db8::1]:5432/layerfs".to_owned(),
        // only the three loopback names are rewritten
        "postgres://u@127.0.0.2/db".to_owned(),
    ];
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind, Some(3));
        for url in &urls {
            assert_eq!(machine::reach(&spec, url).as_ref(), Ok(url), "{kind:?}");
        }
    }
}

#[test]
fn a_rewrite_is_refused_for_verify_full() {
    let scratch = Scratch::new("reach-tls");
    let url = format!("postgres://lfs:{SECRET}@localhost:5432/layerfs?sslmode=verify-full");
    for kind in [Platform::Docker, Platform::Lima, Platform::Firecracker] {
        let (code, message) = refused(machine::reach(&spec(&scratch, kind, Some(2)), &url));
        assert_eq!(code, Code::Unsupported);
        assert!(message.contains("sslmode=verify-full"), "{message}");
    }
    // native rewrites nothing, so its certificate check still holds
    assert_eq!(
        machine::reach(&spec(&scratch, Platform::Native, None), &url),
        Ok(url.clone())
    );
    // verify-ca does not check the host name
    let ca = url.replace("verify-full", "verify-ca");
    assert_eq!(
        machine::reach(&spec(&scratch, Platform::Docker, None), &ca),
        Ok(ca.replace("localhost", "host.docker.internal"))
    );
}

#[test]
fn a_url_reach_cannot_read_is_refused_off_native() {
    let scratch = Scratch::new("reach-unread");
    let docker = spec(&scratch, Platform::Docker, None);
    let native = spec(&scratch, Platform::Native, None);
    for (url, code, why) in [
        (
            format!("host=localhost user=lfs password={SECRET}"),
            Code::InvalidInput,
            "not one postgres://",
        ),
        (
            format!("postgres://lfs:{SECRET}@h1:5432,h2:5433/db"),
            Code::InvalidInput,
            "not one postgres://",
        ),
        // a password with an unescaped `/` cuts the authority: not read
        (
            format!("postgres://lfs:{SECRET}/x@localhost/db"),
            Code::InvalidInput,
            "not one postgres://",
        ),
        (
            format!("postgres://lfs:{SECRET}@/layerfs"),
            Code::Unsupported,
            "names no host",
        ),
        (
            format!("postgres:///layerfs?host=/var/run/postgresql&password={SECRET}"),
            Code::Unsupported,
            "host or hostaddr parameter",
        ),
        (
            format!("postgres://lfs:{SECRET}@db/layerfs?hostaddr=127.0.0.1"),
            Code::Unsupported,
            "host or hostaddr parameter",
        ),
    ] {
        let (got, message) = refused(machine::reach(&docker, &url));
        assert_eq!(got, code, "{message}");
        assert!(message.contains(why), "{message}");
        assert_eq!(machine::reach(&native, &url).as_ref(), Ok(&url));
    }
}

#[test]
fn an_invalid_spec_is_refused_before_the_url_is_read() {
    let scratch = Scratch::new("reach-spec");
    let mut spec = spec(&scratch, Platform::Docker, None);
    spec.name = "not plain".into();
    let (code, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert_eq!(code, Code::InvalidInput);
    assert!(message.contains("not a plain name"), "{message}");
}
