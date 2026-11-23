//! `reach` (#535 §7.3, §7.4; owner 2026-11-24): the PostgreSQL Store URL a
//! Sandbox daemon on each platform dials, which is the URL as configured,
//! never rewritten. A machine that does not share the caller's loopback
//! (docker on Docker Desktop, lima, apple-container) refuses a loopback host,
//! naming its own name for this host and `store connect --machine`; every
//! other URL is dialled as given, `sslmode=verify-full` included. No message
//! holds the URL's password, and no provider tool runs: every stand-in here
//! logs its call and fails.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, MachineError, Platform, Spec};
use standin::Scratch;

const SECRET: &str = "s3cr3t-pw";

/// `getent ahostsv4 host.docker.internal` in a Docker Desktop worker
/// container (D-8 live receipt), which the former lookup ran per call.
const RESOLVED: &str = r#"printf '192.168.65.254  STREAM host.docker.internal\n'"#;

/// `kind`'s spec as `standin` makes it, every provider tool a stand-in that
/// logs its call and fails.
fn spec(scratch: &Scratch, kind: Platform) -> Spec {
    for tool in ["docker", "container", "limactl"] {
        scratch.tool(tool, "exit 1");
    }
    scratch.spec(kind)
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

/// What each platform calls this host in its refusal of a loopback URL;
/// `None` where the loopback is shared.
fn this_host(kind: Platform) -> Option<&'static str> {
    match kind {
        Platform::Native => None,
        // a Linux Engine's worker shares the host's network (D-8)
        Platform::Docker if cfg!(target_os = "linux") => None,
        Platform::Docker => Some("host.docker.internal"),
        Platform::Lima => Some("host.lima.internal"),
        Platform::AppleContainer => Some("192.168.64.1"),
    }
}

/// Every spelling of this host's loopback, as the URL's host, and the
/// host the refusal names.
fn loopback_forms() -> Vec<(String, &'static str)> {
    let at = |host: &str| format!("postgres://lfs:{SECRET}@{host}/layerfs");
    vec![
        (loopback_urls()[0].clone(), "127.0.0.1"),
        (loopback_urls()[1].clone(), "localhost"),
        (loopback_urls()[2].clone(), "::1"),
        (at("LocalHost"), "LocalHost"),
        (at("localhost."), "localhost."),
        // the whole of 127.0.0.0/8
        (at("127.0.0.2:5432"), "127.0.0.2"),
        (at("127.255.255.254"), "127.255.255.254"),
        // IPv4-mapped
        (at("[::ffff:127.0.0.1]:5432"), "::ffff:127.0.0.1"),
        (at("[0:0:0:0:0:0:0:1]"), "0:0:0:0:0:0:0:1"),
        // every host of a list, and the host parameters
        (at("db1:5432,localhost:5433"), "localhost"),
        (at("db1:5432,[::1]:5433"), "::1"),
        (format!("{}?host=127.0.0.1", at("db1")), "127.0.0.1"),
        (format!("{}?hostaddr=127.0.0.1", at("db1")), "127.0.0.1"),
        (format!("{}?host=db2,localhost", at("db1")), "localhost"),
        // libpq's key=value words
        (
            format!("host=localhost user=lfs password={SECRET}"),
            "localhost",
        ),
        (
            format!("host = 'db1,127.0.0.9' password='a \\' b{SECRET}'"),
            "127.0.0.9",
        ),
        (format!("host=db1 hostaddr=::1 password={SECRET}"), "::1"),
    ]
}

#[test]
fn a_loopback_host_is_refused_where_the_loopback_is_not_shared() {
    let scratch = Scratch::new("reach-loopback");
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        for (url, host) in loopback_forms() {
            let outcome = machine::reach(&spec, &url);
            let Some(name) = this_host(kind) else {
                assert_eq!(outcome.as_deref(), Ok(url.as_str()), "{kind:?}");
                continue;
            };
            let (code, message) = refused(outcome);
            assert_eq!(code, Code::Unsupported, "{url}: {message}");
            assert!(
                message.contains(&format!("names this host's loopback ({host})")),
                "{message}"
            );
            assert!(message.contains(name), "{message}");
            assert!(
                message.contains("`layerfs store connect <store> <url> --machine dev`"),
                "{message}"
            );
        }
    }
    assert_eq!(scratch.log(), "", "no provider tool runs");
}

/// A Unix socket of the caller (no host, or a host that is a path) never
/// reaches a machine that does not share this host: refused with the same
/// advice; `native` (and a Linux Engine) dial it as given.
#[test]
fn a_unix_socket_is_refused_where_the_loopback_is_not_shared() {
    let scratch = Scratch::new("reach-socket");
    let urls = [
        format!("postgres://lfs:{SECRET}@/layerfs"),
        format!("postgres:///layerfs?host=/var/run/postgresql&password={SECRET}"),
        "postgresql://%2Fvar%2Frun%2Fpostgresql/layerfs".to_owned(),
        format!("dbname=layerfs password={SECRET}"),
        "host=/tmp dbname=layerfs".to_owned(),
    ];
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        for url in &urls {
            let outcome = machine::reach(&spec, url);
            let Some(name) = this_host(kind) else {
                assert_eq!(outcome.as_ref(), Ok(url), "{kind:?}");
                continue;
            };
            let (code, message) = refused(outcome);
            assert_eq!(code, Code::Unsupported, "{url}: {message}");
            assert!(message.contains("a Unix socket of the caller"), "{message}");
            assert!(message.contains(name), "{message}");
            assert!(message.contains("--machine dev`"), "{message}");
        }
    }
}

/// A name that resolves to a loopback address on the machine (a hosts
/// entry) is not resolved to be refused up front: its one dial fails, and
/// the failure carries this hint, the refusal's own advice. A shared
/// loopback has none.
#[test]
fn a_failed_dial_carries_the_loopback_hint_where_it_is_not_shared() {
    let scratch = Scratch::new("reach-hint");
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        let hint = machine::loopback_hint(&spec);
        let Some(name) = this_host(kind) else {
            assert_eq!(hint, None, "{kind:?}");
            continue;
        };
        let hint = hint.unwrap();
        assert!(hint.contains("loopback address"), "{hint}");
        assert!(hint.contains(name), "{hint}");
        assert!(
            hint.contains("`layerfs store connect <store> <url> --machine dev`"),
            "{hint}"
        );
        // the same advice as the up-front refusal
        let (_, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
        let advice = &hint[hint.find("give the Store").unwrap()..];
        assert!(message.ends_with(advice), "{message} / {hint}");
    }
    assert_eq!(scratch.log(), "", "no provider tool runs");
}

/// The per-call cost of D-8 (#758 B6): every docker Sandbox call on a
/// PostgreSQL Store ran `docker exec <worker> getent ahostsv4
/// host.docker.internal` in the caller. No URL is rewritten now, so no
/// `docker` command runs, whatever the URL names; the worker's own `docker`
/// answering changes nothing.
#[test]
fn a_docker_sandbox_call_runs_no_docker_command() {
    let scratch = Scratch::new("reach-docker");
    let spec = spec(&scratch, Platform::Docker);
    scratch.tool("docker", RESOLVED);
    let named = format!("postgres://lfs:{SECRET}@host.docker.internal:5432/layerfs");
    for _call in 0..3 {
        assert_eq!(machine::reach(&spec, &named), Ok(named.clone()));
        let outcome = machine::reach(&spec, &loopback_urls()[0]);
        match cfg!(target_os = "linux") {
            true => assert_eq!(outcome, Ok(loopback_urls()[0].clone())),
            false => assert_eq!(refused(outcome).0, Code::Unsupported),
        }
    }
    assert_eq!(scratch.log(), "", "no docker command runs");
}

/// apple-container has no name for this host: its refusal names the
/// gateway its last start recorded (#66), else vmnet's default, read from
/// no `container` call.
#[test]
fn apple_container_names_its_recorded_gateway() {
    let scratch = Scratch::new("reach-apple-recorded");
    let mut spec = spec(&scratch, Platform::AppleContainer);
    spec.state.gateway = Some("192.168.65.1".parse().unwrap());
    let (_, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert!(
        message.contains("192.168.65.1, on which the server must listen"),
        "{message}"
    );
    spec.state.gateway = None;
    let (_, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert!(message.contains("192.168.64.1"), "{message}");
    assert_eq!(scratch.log(), "", "no container inspect");
}

#[test]
fn every_other_host_is_dialled_as_given_everywhere() {
    let scratch = Scratch::new("reach-remote");
    let urls = [
        // nothing is rewritten, so the server's certificate check holds
        format!("postgres://lfs:{SECRET}@db.example.com:5432/layerfs?sslmode=verify-full"),
        format!("postgres://lfs:{SECRET}@host.docker.internal/layerfs?sslmode=verify-full"),
        format!("postgres://lfs:{SECRET}@host.lima.internal/layerfs?sslmode=verify-ca"),
        "postgresql://lfs@10.0.0.5/layerfs".to_owned(),
        "postgres://[2001:db8::1]:5432/layerfs".to_owned(),
        format!("postgres://lfs:{SECRET}@h1:5432,h2:5433/db"),
        // the host parameters are dialled as given once checked
        format!("postgres://lfs:{SECRET}@db/layerfs?hostaddr=10.0.0.5&host=db2"),
        format!("host=db.example.com port=5433 password='p w{SECRET}' sslmode=verify-full"),
        // ports are never read: a host's port is the server's
        "postgres://u@10.0.0.5:5432/db".to_owned(),
        // a name is never resolved, even one only this host could
        "postgres://u@localhost.example.com/db".to_owned(),
        "postgres://u@128.0.0.1/db".to_owned(),
    ];
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        for url in &urls {
            assert_eq!(machine::reach(&spec, url).as_ref(), Ok(url), "{kind:?}");
        }
    }
    assert_eq!(scratch.log(), "", "no provider tool runs");
}

#[test]
fn a_url_reach_cannot_read_is_refused_off_a_shared_loopback() {
    let scratch = Scratch::new("reach-unread");
    // lima does not share the loopback on any host OS
    let lima = spec(&scratch, Platform::Lima);
    let native = spec(&scratch, Platform::Native);
    for url in [
        // a password with an unescaped `/` cuts the authority: not read
        format!("postgres://lfs:{SECRET}/x@localhost/db"),
        // a list whose bracketed IPv6 host is not last
        format!("postgres://lfs:{SECRET}@[::1]:5432,h2/db"),
        // words that are not key=value, or an unclosed quote
        format!("localhost {SECRET}"),
        format!("host='localhost password={SECRET}"),
    ] {
        let (got, message) = refused(machine::reach(&lima, &url));
        assert_eq!(got, Code::InvalidInput, "{message}");
        assert!(message.contains("cannot check"), "{message}");
        assert_eq!(machine::reach(&native, &url).as_ref(), Ok(&url));
    }
}

#[test]
fn an_invalid_spec_is_refused_before_the_url_is_read() {
    let scratch = Scratch::new("reach-spec");
    let mut spec = spec(&scratch, Platform::Docker);
    spec.name = "not plain".into();
    let (code, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert_eq!(code, Code::InvalidInput);
    assert!(message.contains("not a plain name"), "{message}");
}
