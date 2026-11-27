//! `reach` (#535 §7.3, §7.4; owner 2026-11-24): the PostgreSQL Store URL a
//! Sandbox daemon on each platform dials, which is the URL as configured,
//! never rewritten. A machine that does not share the caller's loopback
//! (docker on Docker Desktop, lima, apple-container) refuses a loopback host,
//! naming its own name for this host and `store connect <store> <url>`;
//! every other URL is passed on as given, `sslmode=verify-full` included
//! (a reach input only: the Store's client reads `sslmode` disable, prefer
//! or require, and `store connect` refuses `verify-full`). Hosts
//! are read as the client (tokio-postgres) reads them: percent-decoded, a
//! host list split at `,` first, every IPv4 form `inet_aton` reads. No
//! message holds the URL's password, and no provider tool runs: every
//! stand-in here logs its call and fails.
#![cfg(unix)]
use crate::standin;
use layerfs_machine::platform::{self as machine, Code, MachineError, Platform, Spec};
use standin::Scratch;

const SECRET: &str = "s3cr3t-pw";

/// The Store every URL here belongs to, which the refusal's advice names.
const STORE: &str = "pg";

/// [`machine::reach`] of Store [`STORE`].
fn reach(spec: &Spec, url: &str) -> Result<String, MachineError> {
    machine::reach(spec, STORE, url)
}

/// The connect every refusal on machine `dev` advises: the Store again,
/// with an address the machine reaches.
const CONNECT: &str = "`layerfs store connect pg <url>`";

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
        Platform::Host => None,
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
        // a list splits at `,` before a host's port or brackets are read,
        // as the client splits it: a bracketed host need not be last
        (at("[::1]:5432,h2"), "::1"),
        (at("h1,[::1]"), "::1"),
        (at("h1:5432,127.0.0.1"), "127.0.0.1"),
        // the userinfo ends at the first `@`, so a password's unescaped
        // `/` does not hide the host after it
        (
            format!("postgres://lfs:{SECRET}/x@localhost/db"),
            "localhost",
        ),
        // percent-encoded hosts, `host`/`hostaddr` values and keys are
        // decoded, as the client decodes them before it dials
        (at("%6Cocalhost"), "localhost"),
        (at("%31%32%37.0.0.1:5432"), "127.0.0.1"),
        (format!("{}?host=%6Cocalhost", at("db1")), "localhost"),
        (format!("{}?h%6Fst=localhost", at("db1")), "localhost"),
        (
            format!("{}?hostaddr=%31%32%37.0.0.1", at("db1")),
            "127.0.0.1",
        ),
        (format!("{}?%68ostaddr=::1", at("db1")), "::1"),
        // the unspecified address reaches this host
        (at("0.0.0.0"), "0.0.0.0"),
        (at("[::]:5432"), "::"),
        (at("[::ffff:0.0.0.0]"), "::ffff:0.0.0.0"),
        (format!("{}?hostaddr=0.0.0.0", at("db1")), "0.0.0.0"),
        // the IPv4 forms `inet_aton` reads, which `getaddrinfo` dials
        (at("127.1"), "127.1"),
        (at("127.0.1"), "127.0.1"),
        (at("2130706433"), "2130706433"),
        (at("127.000.000.001"), "127.000.000.001"),
        (at("0x7f.1"), "0x7f.1"),
        (at("0X7F000001"), "0X7F000001"),
        (at("017700000001"), "017700000001"),
        (at("0177.0.0.1"), "0177.0.0.1"),
        (at("0"), "0"),
        (format!("{}?host=127.1", at("db1")), "127.1"),
        // an IPv6 zone id
        (at("[::1%25lo]:5432"), "::1%lo"),
        (format!("{}?host=::1%25lo", at("db1")), "::1%lo"),
        (format!("{}?host=fe80::1,::1%lo0", at("db1")), "::1%lo0"),
    ]
}

#[test]
fn a_loopback_host_is_refused_where_the_loopback_is_not_shared() {
    let scratch = Scratch::new("reach-loopback");
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        for (url, host) in loopback_forms() {
            let outcome = reach(&spec, &url);
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
            assert!(message.contains(CONNECT), "{message}");
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
        // a socket directory named by a host parameter, encoded or not
        "postgres://db1/layerfs?host=%2Ftmp".to_owned(),
        "postgres://db1/layerfs?h%6Fst=/tmp".to_owned(),
    ];
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        for url in &urls {
            let outcome = reach(&spec, url);
            let Some(name) = this_host(kind) else {
                assert_eq!(outcome.as_ref(), Ok(url), "{kind:?}");
                continue;
            };
            let (code, message) = refused(outcome);
            assert_eq!(code, Code::Unsupported, "{url}: {message}");
            assert!(message.contains("a Unix socket of the caller"), "{message}");
            assert!(message.contains(name), "{message}");
            assert!(message.contains(CONNECT), "{message}");
        }
    }
}

/// A name that resolves to a loopback address on the machine (a hosts
/// entry) is not resolved to be refused up front: its one dial fails, and
/// the failure carries this hint, the refusal's own advice. A shared
/// loopback has none, and neither has a URL naming no host by name: an
/// address is read, never resolved, so only a name can turn out to be
/// this host's loopback.
#[test]
fn a_failed_dial_carries_the_loopback_hint_where_it_is_not_shared() {
    let scratch = Scratch::new("reach-hint");
    let named = format!("postgres://lfs:{SECRET}@db.example.com:5432/layerfs");
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        let hint = machine::loopback_hint(&spec, STORE, &named);
        for addressed in [
            format!("postgres://lfs:{SECRET}@10.0.0.5:5432/layerfs"),
            format!("postgres://lfs:{SECRET}@[2001:db8::1],10.1/layerfs"),
            format!("postgres://lfs:{SECRET}@10.0.0.5/layerfs?hostaddr=10.0.0.6"),
            // with a hostaddr its client dials that address alone
            format!("postgres://lfs:{SECRET}@db.example.com/layerfs?hostaddr=10.0.0.6"),
        ] {
            assert_eq!(machine::loopback_hint(&spec, STORE, &addressed), None);
        }
        // one name among addresses can still be a loopback one
        let mixed = format!("postgres://lfs:{SECRET}@10.0.0.5,db.example.com/layerfs");
        assert_eq!(machine::loopback_hint(&spec, STORE, &mixed), hint);
        let Some(name) = this_host(kind) else {
            assert_eq!(hint, None, "{kind:?}");
            continue;
        };
        let hint = hint.unwrap();
        assert!(!hint.contains(SECRET), "{hint}");
        assert!(hint.contains("loopback address"), "{hint}");
        assert!(hint.contains(name), "{hint}");
        assert!(hint.contains(CONNECT), "{hint}");
        // the same advice as the up-front refusal
        let (_, message) = refused(reach(&spec, &loopback_urls()[0]));
        let advice = &hint[hint.find("connect the Store again").unwrap()..];
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
        assert_eq!(reach(&spec, &named), Ok(named.clone()));
        let outcome = reach(&spec, &loopback_urls()[0]);
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
    let (_, message) = refused(reach(&spec, &loopback_urls()[0]));
    assert!(
        message.contains("192.168.65.1, on which the server must listen"),
        "{message}"
    );
    spec.state.gateway = None;
    let (_, message) = refused(reach(&spec, &loopback_urls()[0]));
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
        // valid host lists: a port on some hosts only, a bracketed host
        // anywhere in the list
        format!("postgres://lfs:{SECRET}@h1:5432,h2/db"),
        format!("postgres://lfs:{SECRET}@h1,[2001:db8::1]/db"),
        format!("postgres://lfs:{SECRET}@[2001:db8::1]:5432,h2/db"),
        // the host parameters are dialled as given once checked
        format!("postgres://lfs:{SECRET}@db/layerfs?hostaddr=10.0.0.5&host=db2"),
        // a URL with no host of its own, naming one by parameter
        format!("postgres:///layerfs?host=db2&password={SECRET}"),
        // addresses `inet_aton` reads that are not this host's loopback
        "postgres://u@10.1/db".to_owned(),
        "postgres://u@0x80.1/db".to_owned(),
        "postgres://u@[fe80::1%25eth0]/db".to_owned(),
        // ports are never read: a host's port is the server's
        "postgres://u@10.0.0.5:5432/db".to_owned(),
        // a name is never resolved, even one only this host could
        "postgres://u@localhost.example.com/db".to_owned(),
        "postgres://u@128.0.0.1/db".to_owned(),
    ];
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        for url in &urls {
            assert_eq!(reach(&spec, url).as_ref(), Ok(url), "{kind:?}");
        }
    }
    assert_eq!(scratch.log(), "", "no provider tool runs");
}

#[test]
fn a_url_reach_cannot_read_is_refused_off_a_shared_loopback() {
    let scratch = Scratch::new("reach-unread");
    // lima does not share the loopback on any host OS
    let lima = spec(&scratch, Platform::Lima);
    let native = spec(&scratch, Platform::Host);
    for url in [
        // not a postgres:// URL: libpq's key=value words are never a
        // Store's URL (layerfs-store's `postgresql::address::refuse_login`
        // refuses them), nor are other words
        format!("host=localhost user=lfs password={SECRET}"),
        format!("dbname=layerfs password={SECRET}"),
        format!("host=db.example.com port=5433 password='p w{SECRET}'"),
        format!("localhost {SECRET}"),
        format!("host='localhost password={SECRET}"),
        format!("mysql://lfs:{SECRET}@db/x"),
        // what the client itself refuses to read
        format!("postgres://lfs:{SECRET}@[::1/db"),
        format!("postgres://lfs:{SECRET}@[::1]x/db"),
        format!("postgres://lfs:{SECRET}@db/x?sslmode"),
        format!("postgres://lfs:{SECRET}@%FF/x"),
        format!("postgres://lfs:{SECRET}@db/x?%FF=localhost"),
    ] {
        let (got, message) = refused(reach(&lima, &url));
        assert_eq!(got, Code::InvalidInput, "{message}");
        assert!(message.contains("cannot check"), "{message}");
        assert_eq!(reach(&native, &url).as_ref(), Ok(&url));
    }
}

#[test]
fn an_invalid_spec_is_refused_before_the_url_is_read() {
    let scratch = Scratch::new("reach-spec");
    let mut spec = spec(&scratch, Platform::Docker);
    spec.name = "not plain".into();
    let (code, message) = refused(reach(&spec, &loopback_urls()[0]));
    assert_eq!(code, Code::InvalidInput);
    assert!(message.contains("not a plain name"), "{message}");
}
