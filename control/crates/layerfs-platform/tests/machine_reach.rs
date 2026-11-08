//! `reach` (#535 §7.3, §7.4): the PostgreSQL Store URL a Sandbox daemon on
//! each platform dials. A loopback host becomes the platform's host alias or
//! is refused naming what is missing; a rewrite is refused for
//! `sslmode=verify-full`; every other host is dialled as given. No message
//! holds the URL's password. Only apple-container's stand-in `container`
//! and docker's stand-in `docker` run, and only for a loopback URL (they
//! read the network's gateway, and what the worker resolves its host to).
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, MachineError, Platform, Spec};
use standin::Scratch;

const SECRET: &str = "s3cr3t-pw";

/// `container inspect` of a running container on vmnet's default network,
/// as `container` 1.6.1 prints it (G7 receipt `ac-inspect.json`, cut).
const RUNNING: &str = r#"echo '[{"status":{"state":"running","networks":[{"network":"default","ipv4Address":"192.168.64.10/24","ipv4Gateway":"192.168.64.1"}]}}]'"#;

/// `getent ahostsv4 host.docker.internal` in a Docker Desktop worker
/// container (D-8 live receipt): its DNS answers the name.
const RESOLVED: &str = r#"printf '192.168.65.254  STREAM host.docker.internal\n192.168.65.254  DGRAM  \n192.168.65.254  RAW    \n'"#;

/// `kind`'s spec as `standin` makes it.
fn spec(scratch: &Scratch, kind: Platform) -> Spec {
    if kind == Platform::AppleContainer {
        scratch.tool("container", RUNNING);
    }
    if kind == Platform::Docker {
        scratch.tool("docker", RESOLVED);
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

/// The URL with `host` for its loopback host, as the three above.
fn rewritten(host: &str) -> [String; 3] {
    [
        format!("postgres://lfs:{SECRET}@{host}:5432/layerfs?sslmode=disable"),
        format!("postgresql://lfs:{SECRET}@{host}/layerfs"),
        format!("postgres://lfs:{SECRET}@{host}:6543/layerfs"),
    ]
}

#[test]
fn each_platform_rewrites_a_loopback_host() {
    let scratch = Scratch::new("reach-loopback");
    for kind in Platform::ALL {
        let spec = spec(&scratch, kind);
        let alias = match kind {
            Platform::Native => "unchanged",
            // a Linux Engine's worker shares the host's network (D-8)
            Platform::Docker if cfg!(target_os = "linux") => "unchanged",
            Platform::Docker => "192.168.65.254",
            Platform::Lima => "192.168.5.2",
            Platform::AppleContainer => "192.168.64.1",
        };
        for (i, url) in loopback_urls().iter().enumerate() {
            let outcome = machine::reach(&spec, url);
            match alias {
                "unchanged" => assert_eq!(outcome.as_deref(), Ok(url.as_str())),
                alias => assert_eq!(outcome, Ok(rewritten(alias)[i].clone()), "{kind:?}"),
            }
        }
    }
}

#[test]
fn apple_container_reads_its_gateway_only_for_a_loopback_url() {
    let scratch = Scratch::new("reach-apple");
    let spec = spec(&scratch, Platform::AppleContainer);
    let remote = format!("postgres://lfs:{SECRET}@db.example.com:5432/layerfs");
    assert_eq!(machine::reach(&spec, &remote), Ok(remote.clone()));
    assert_eq!(scratch.log(), "", "a remote URL reads nothing");
    let outcome = machine::reach(&spec, &loopback_urls()[1]);
    assert_eq!(outcome, Ok(rewritten("192.168.64.1")[1].clone()));
    assert_eq!(scratch.log(), "container inspect lfs-dev\n");
    // a stopped container reports no network: refused, naming why
    scratch.tool(
        "container",
        r#"echo '[{"status":{"state":"stopped","networks":[]}}]'"#,
    );
    let (code, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert_eq!(code, Code::Unavailable);
    assert!(message.contains("reports no network gateway"), "{message}");
    assert!(message.contains("must be running"), "{message}");
    // an answer that is not an IPv4 address is not put in the URL
    scratch.tool(
        "container",
        r#"echo '[{"status":{"networks":[{"ipv4Gateway":"evil/x@y"}]}}]'"#,
    );
    let (code, _) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert_eq!(code, Code::Unavailable);
}

/// #66: a gateway its last start recorded is used with no `container`
/// call; a stale one is dialled as recorded (the connection then fails,
/// explicitly), never looked up again. A record without one (from before
/// #66) still reads it from the machine.
#[test]
fn apple_container_uses_the_gateway_its_start_recorded() {
    let scratch = Scratch::new("reach-apple-recorded");
    let mut spec = spec(&scratch, Platform::AppleContainer);
    spec.state.gateway = Some("192.168.65.1".parse().unwrap());
    for (url, want) in loopback_urls().iter().zip(rewritten("192.168.65.1")) {
        assert_eq!(machine::reach(&spec, url), Ok(want));
    }
    assert_eq!(
        scratch.log(),
        "",
        "no container inspect with a recorded gateway"
    );
    spec.state.gateway = None;
    let outcome = machine::reach(&spec, &loopback_urls()[0]);
    assert_eq!(outcome, Ok(rewritten("192.168.64.1")[0].clone()));
    assert_eq!(scratch.log(), "container inspect lfs-dev\n");
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
        let spec = spec(&scratch, kind);
        for url in &urls {
            assert_eq!(machine::reach(&spec, url).as_ref(), Ok(url), "{kind:?}");
        }
    }
}

#[test]
fn a_rewrite_is_refused_for_verify_full() {
    let scratch = Scratch::new("reach-tls");
    let url = format!("postgres://lfs:{SECRET}@localhost:5432/layerfs?sslmode=verify-full");
    let rewriting: &[Platform] = match cfg!(target_os = "linux") {
        true => &[Platform::Lima],
        false => &[Platform::Docker, Platform::Lima],
    };
    for &kind in rewriting {
        let (code, message) = refused(machine::reach(&spec(&scratch, kind), &url));
        assert_eq!(code, Code::Unsupported);
        assert!(message.contains("sslmode=verify-full"), "{message}");
    }
    // native rewrites nothing, so its certificate check still holds
    assert_eq!(
        machine::reach(&spec(&scratch, Platform::Native), &url),
        Ok(url.clone())
    );
    // verify-ca does not check the host name
    let ca = url.replace("verify-full", "verify-ca");
    assert_eq!(
        machine::reach(&spec(&scratch, Platform::Lima), &ca),
        Ok(ca.replace("localhost", "192.168.5.2"))
    );
}

#[test]
fn a_url_reach_cannot_read_is_refused_off_native() {
    let scratch = Scratch::new("reach-unread");
    // lima rewrites a loopback host on every host OS
    let lima = spec(&scratch, Platform::Lima);
    let native = spec(&scratch, Platform::Native);
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
        let (got, message) = refused(machine::reach(&lima, &url));
        assert_eq!(got, code, "{message}");
        assert!(message.contains(why), "{message}");
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

/// D-8: a docker Sandbox runs in the Sandbox image's root, which cannot
/// resolve `host.docker.internal`, so a loopback URL gets the address the
/// running worker container resolves the name to. Only a loopback URL
/// reads it; a stopped worker, or one resolving no IPv4 address, is refused
/// `Unavailable` and puts nothing in the URL.
#[cfg(not(target_os = "linux"))]
#[test]
fn docker_reads_its_worker_s_host_address_only_for_a_loopback_url() {
    let scratch = Scratch::new("reach-docker");
    let spec = spec(&scratch, Platform::Docker);
    let remote = format!("postgres://lfs:{SECRET}@db.example.com:5432/layerfs");
    assert_eq!(machine::reach(&spec, &remote), Ok(remote.clone()));
    assert_eq!(scratch.log(), "", "a remote URL reads nothing");
    let outcome = machine::reach(&spec, &loopback_urls()[1]);
    assert_eq!(outcome, Ok(rewritten("192.168.65.254")[1].clone()));
    assert_eq!(
        scratch.log(),
        "docker -H unix:///var/run/docker.sock exec lfs-dev getent ahostsv4 host.docker.internal\n"
    );
    // a worker that is not running resolves nothing
    scratch.tool(
        "docker",
        r#"echo "Error response from daemon: container lfs-dev is not running" >&2; exit 1"#,
    );
    let (code, message) = refused(machine::reach(&spec, &loopback_urls()[0]));
    assert_eq!(code, Code::Unavailable);
    assert!(message.contains("must be running"), "{message}");
    // an unresolved name (getent exits 2), or an answer that is not an
    // IPv4 address, is not put in the URL
    for body in ["exit 2", r#"echo 'evil/x@y STREAM host.docker.internal'"#] {
        scratch.tool("docker", body);
        let (code, _) = refused(machine::reach(&spec, &loopback_urls()[0]));
        assert_eq!(code, Code::Unavailable);
    }
}
