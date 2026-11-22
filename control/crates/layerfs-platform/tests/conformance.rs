//! The platform conformance kit (#536 §11 step 9): checks every machine
//! platform passes, run over `Platform::ALL`, so a new platform is covered
//! as soon as it is a variant. Nothing real is touched: provider tools are
//! the stand-ins of `support/standin.rs`, which log their calls and refuse.
//!
//! Per platform it checks:
//! - its profile against the one support table (`support`,
//!   `sandbox_allowed`) on every host, and against the committed
//!   `control/SUPPORT.json`, row by row;
//! - that each lifecycle call (create, start, stop, status, remove) either
//!   runs through the
//!   platform's provider tool (here, its stand-in) or is refused before any
//!   effect with the fixed refusal its `How` declares;
//! - `reach` against the §7.3 rules: a loopback host is shared or rewritten
//!   to the platform's alias; a rewrite is refused for
//!   `sslmode=verify-full`; every other host is dialled as given; no error
//!   text or `Debug` holds the password;
//! - `forward` (#535 G4): a daemon's port is used as it is, carried by a
//!   detached `ssh -L 127.0.0.1:<free port>:127.0.0.1:<guest port>` that
//!   runs through the platform's own tools, or refused before any tool
//!   runs with the platform's fixed code and reason;
//! - that the options its profile declares are the `Spec` fields
//!   `Spec::validate` accepts for it, and only those.
//!
//! The per-platform declarations the kit needs are [`loopback`] and
//! [`daemon_port`], exhaustive matches: a new platform does not compile
//! here until it says how its machine reaches the caller's loopback and how
//! the caller reaches its Sandbox daemons.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
#[path = "../../layerfs-sandbox-backend/tests/support/support_json.rs"]
mod support_json;
use layerfs_platform::{
    self as machine, platforms, sandbox_allowed, support, Backend, CallSite, Code, ExecutorKind,
    Forward, HostOs, How, KernelRoute, MachineError, OptKind, Platform, Route, Spec, StoreKind,
};
use layerfs_sandbox_backend::MountDriver;
use standin::Scratch;
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Profile facts, the support table and SUPPORT.json
// ---------------------------------------------------------------------------

/// The hosts the table answers for: the named ones and `Other`.
fn every_host() -> impl Iterator<Item = HostOs> {
    HostOs::NAMED.into_iter().chain([HostOs::Other])
}

/// The mount driver a backend must have to serve `route`.
fn driver(route: KernelRoute) -> MountDriver {
    match route {
        KernelRoute::Fuse => MountDriver::Fuse,
        KernelRoute::Nfs => MountDriver::LoopbackNfs,
    }
}

#[test]
fn each_platform_is_named_once_and_parses_back() {
    let mut names = BTreeSet::new();
    for platform in Platform::ALL {
        let profile = platform.profile();
        assert_eq!(profile.name, platform.name());
        assert!(names.insert(profile.name), "{} named twice", profile.name);
        assert_eq!(Platform::parse(profile.name), Some(platform));
        assert!(!profile.description.is_empty(), "{platform:?}");
        assert!(!profile.hosts.is_empty(), "{platform:?} is on no host");
        assert!(!profile.hosts.contains(&HostOs::Other), "{platform:?}");
        assert_eq!(platform.built_in(), profile.how == How::BuiltIn);
        assert_eq!(platform.created(), profile.how == How::Created);
    }
}

#[test]
fn each_profile_is_its_support_rows_on_every_host() {
    for platform in Platform::ALL {
        let profile = platform.profile();
        for host in every_host() {
            let row = support(platform, host);
            let at = format!("{platform:?} on {host:?}");
            assert_eq!((row.platform, row.host), (platform, host), "{at}");
            assert_eq!(row.how, profile.how, "{at}");
            assert_eq!(row.options, profile.options, "{at}");
            if profile.hosts.contains(&host) {
                assert_eq!(row.backend, platform.backend(host), "{at}");
                assert_eq!(row.kernel, platform.kernel(host), "{at}");
                let sqlite = match platform.executor(host) {
                    ExecutorKind::Local => CallSite::InProcess,
                    ExecutorKind::Worker => CallSite::Worker,
                };
                assert_eq!(row.sqlite, Some(sqlite), "{at}");
                assert_eq!(
                    platform.route(host) == Route::InProcess,
                    sqlite == CallSite::InProcess,
                    "{at}"
                );
            } else {
                assert_eq!(
                    (row.backend, row.kernel, row.sqlite),
                    (None, None, None),
                    "{at}"
                );
            }
            // a backend serves exactly one kernel route, which it can mount
            assert_eq!(row.backend.is_some(), row.kernel.is_some(), "{at}");
            if let (Some(backend), Some(route)) = (row.backend, row.kernel) {
                assert_eq!(backend.capabilities().mount_driver, driver(route), "{at}");
            }
            match row.backend {
                Some(backend) => assert_eq!(
                    row.description,
                    format!("{}; {}", profile.description, backend.description()),
                    "{at}"
                ),
                None => assert_eq!(row.description, profile.description, "{at}"),
            }
        }
    }
}

#[test]
fn each_refusal_comes_from_the_table_and_postgresql_needs_sqlite() {
    for platform in Platform::ALL {
        for host in every_host() {
            let row = support(platform, host);
            let at = format!("{platform:?} on {host:?}");
            let sqlite = sandbox_allowed(platform, host, StoreKind::Sqlite);
            let postgresql = sandbox_allowed(platform, host, StoreKind::Postgresql);
            // a Sandbox runs exactly where the platform is on the host with a backend
            assert_eq!(sqlite.is_ok(), row.backend.is_some(), "{at}");
            if let Err(reason) = sqlite {
                assert!(!reason.is_empty(), "{at}");
                assert_eq!(postgresql, Err(reason), "{at}: one refusal for both kinds");
            }
            assert_eq!(row.postgresql.sandbox, postgresql.is_ok(), "{at}");
            assert_eq!(row.postgresql.reason, postgresql.err(), "{at}");
            assert_eq!(row.postgresql.store_calls, CallSite::InProcess, "{at}");
            let supports = row.supports();
            assert_eq!(supports.sqlite, sqlite.is_ok(), "{at}");
            assert_eq!(supports.postgresql_sandbox, postgresql.is_ok(), "{at}");
            assert_eq!(supports.reason, postgresql.err(), "{at}");
        }
    }
}

#[test]
fn each_platform_row_in_support_json_is_its_table_row() {
    let rows = support_json::rows();
    for host in HostOs::NAMED {
        let in_file: Vec<&support_json::Row> =
            rows.iter().filter(|r| r.host == host.as_str()).collect();
        let names: Vec<&str> = in_file.iter().map(|r| r.platform.as_str()).collect();
        assert_eq!(names, Platform::ALL.map(Platform::name), "{host:?}");
        for (table, file) in platforms(host).iter().zip(in_file) {
            let at = format!("{:?} on {host:?}", table.platform);
            assert_eq!(file.how, table.how.as_str(), "{at}");
            assert_eq!(
                file.backend.as_deref(),
                table.backend.map(Backend::stored_name),
                "{at}"
            );
            assert_eq!(
                file.kernel.as_deref(),
                table.kernel.map(KernelRoute::as_str),
                "{at}"
            );
            assert_eq!(
                file.sqlite.as_deref(),
                table.sqlite.map(CallSite::as_str),
                "{at}"
            );
            assert_eq!(
                file.postgresql_store_calls,
                table.postgresql.store_calls.as_str(),
                "{at}"
            );
            assert_eq!(file.postgresql_sandbox, table.postgresql.sandbox, "{at}");
            assert_eq!(
                file.postgresql_reason.as_deref(),
                table.postgresql.reason,
                "{at}"
            );
            let options: Vec<(&str, &str)> = (table.options.iter())
                .map(|o| (o.name, o.kind.as_str()))
                .collect();
            let written: Vec<(&str, &str)> = (file.options.iter())
                .map(|(n, k)| (n.as_str(), k.as_str()))
                .collect();
            assert_eq!(written, options, "{at}");
            assert_eq!(file.description, table.description, "{at}");
        }
    }
    let hosts: BTreeSet<&str> = rows.iter().map(|r| r.host.as_str()).collect();
    let named: BTreeSet<&str> = HostOs::NAMED.iter().map(|h| h.as_str()).collect();
    assert_eq!(hosts, named);
}

// ---------------------------------------------------------------------------
// Lifecycle against the stand-in tools
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Call {
    Create,
    Start,
    Stop,
    Status,
    Remove,
}

const CALLS: [Call; 5] = [
    Call::Create,
    Call::Start,
    Call::Stop,
    Call::Status,
    Call::Remove,
];

/// The fixed refusal a platform's `How` declares for `call`, or `None`
/// when a platform of that kind does it.
fn refusal(how: How, call: Call) -> Option<&'static str> {
    match (call, how) {
        (Call::Create, How::BuiltIn) => Some("is built in and never created"),
        (Call::Remove, How::BuiltIn) => Some("is built in and never removed"),
        _ => None,
    }
}

/// Every refusal text above, so a call a platform does is never answered
/// with one.
const REFUSALS: [&str; 2] = [
    "is built in and never created",
    "is built in and never removed",
];

/// The guest transports any platform may drive beside its own tool.
const TRANSPORTS: [&str; 2] = ["ssh", "tar"];

/// Stand-ins for every platform's tool and the transports: each logs its
/// call (its name alone in `tools.log`, as arguments span lines) and
/// refuses, so a call reaches the provider and stops there.
fn refusing_tools(scratch: &Scratch) {
    let tools = Platform::ALL.into_iter().filter_map(Platform::tool);
    let names: BTreeSet<&str> = tools.chain(TRANSPORTS).collect();
    for name in names {
        let body = format!(
            "echo {name} >> \"$SCRATCH/tools.log\"\necho \"stand-in $0 refuses\" >&2\nexit 1"
        );
        scratch.tool(name, &body);
    }
}

/// A valid spec of `platform`, built only from what its profile declares.
/// The docker worker container's image comes from the bundle (#535 E7) and
/// is passed as `os_image`.
fn valid_spec(scratch: &Scratch, platform: Platform) -> Spec {
    let mut spec = scratch.spec(platform);
    if platform == Platform::Docker {
        spec.os_image = Some("debian:trixie-slim".into());
    }
    spec.validate()
        .unwrap_or_else(|e| panic!("{platform:?}: {e}"));
    spec
}

fn run(call: Call, scratch: &Scratch, spec: &Spec) -> Result<(), MachineError> {
    let bundle = scratch.bundle();
    match call {
        Call::Create => machine::create(spec, &bundle).map(drop),
        Call::Start => machine::start(spec, &bundle).map(drop),
        Call::Stop => machine::stop(spec),
        Call::Status => machine::status(spec).map(drop),
        Call::Remove => machine::remove(spec),
    }
}

#[test]
fn each_lifecycle_call_runs_through_the_tool_or_gives_the_fixed_refusal() {
    for platform in Platform::ALL {
        let scratch = Scratch::new("kit-life");
        refusing_tools(&scratch);
        let spec = valid_spec(&scratch, platform);
        let profile = platform.profile();
        let own: BTreeSet<&str> = profile.tool.into_iter().chain(TRANSPORTS).collect();
        for call in CALLS {
            let _ = std::fs::remove_file(scratch.log_file());
            let _ = std::fs::remove_file(scratch.path("tools.log"));
            let outcome = run(call, &scratch, &spec);
            let log = scratch.log();
            let tools = standin::read(&scratch.path("tools.log"));
            let at = format!("{platform:?} {call:?}: {outcome:?}\n{log}");
            if let Some(why) = refusal(profile.how, call) {
                let error = outcome.expect_err(&at);
                assert_eq!(error.code, Code::InvalidInput, "{at}");
                assert_eq!(
                    error.message,
                    format!("the {} machine dev {why}", platform.name()),
                    "{at}"
                );
                assert!(!error.vm_exists, "{at}");
                assert!(log.is_empty(), "refused after an effect: {at}");
                continue;
            }
            // done: through its own tool, or refused for a host fact first
            if let Err(error) = &outcome {
                assert!(!REFUSALS.iter().any(|r| error.message.ends_with(r)), "{at}");
                assert!(
                    !log.is_empty() || error.code == Code::Unsupported,
                    "neither ran its tool nor named what the host lacks: {at}"
                );
                assert!(!error.vm_exists || !log.is_empty(), "{at}");
            }
            for tool in tools.lines() {
                assert!(own.contains(tool), "drove another platform's {tool}: {at}");
            }
        }
    }
}

#[test]
fn an_in_process_platform_has_nothing_to_start_stop_or_observe() {
    let host = HostOs::current();
    for platform in Platform::ALL
        .into_iter()
        .filter(|p| p.route(host) == Route::InProcess)
    {
        let scratch = Scratch::new("kit-in-process");
        refusing_tools(&scratch);
        let spec = valid_spec(&scratch, platform);
        assert!(machine::start(&spec, &scratch.bundle()).unwrap().is_none());
        machine::stop(&spec).unwrap();
        let status = machine::status(&spec).unwrap();
        assert_eq!(status.state, machine::State::Running, "{platform:?}");
        assert!(scratch.log().is_empty(), "{platform:?}: {}", scratch.log());
    }
}

// ---------------------------------------------------------------------------
// reach
// ---------------------------------------------------------------------------

const SECRET: &str = "kit-s3cr3t";

/// How a platform's machine reaches the caller's loopback (#535 §7.3,
/// `control/README.md`). The one declaration this kit needs per platform.
enum Loopback {
    /// The daemon shares it: the URL is used as given.
    Shared,
    /// A loopback host becomes this host name or address.
    Alias(String),
}

fn loopback(spec: &Spec) -> Loopback {
    match spec.kind {
        Platform::Native => Loopback::Shared,
        // a Linux Engine's worker shares the host's network; Docker
        // Desktop's resolves `host.docker.internal` (`reach_spec`; D-8)
        Platform::Docker if cfg!(target_os = "linux") => Loopback::Shared,
        Platform::Docker => Loopback::Alias("192.168.65.254".into()),
        Platform::Lima => Loopback::Alias("192.168.5.2".into()),
        // the gateway the stand-in `container inspect` reports (`reach_spec`)
        Platform::AppleContainer => Loopback::Alias("192.168.64.1".into()),
    }
}

/// A loopback URL in three spellings, given its host's text.
fn with_host(host: &str, query: &str) -> [String; 3] {
    [
        format!("postgres://lfs:{SECRET}@{host}:5432/layerfs{query}"),
        format!("postgresql://lfs:{SECRET}@{host}/layerfs{query}"),
        format!("postgres://lfs:{SECRET}@{host}:6543/layerfs{query}"),
    ]
}

/// The same three URLs with each loopback host spelled differently.
fn loopback_urls(query: &str) -> [String; 3] {
    let [a, _, _] = with_host("127.0.0.1", query);
    let [_, b, _] = with_host("localhost", query);
    let [_, _, c] = with_host("[::1]", query);
    [a, b, c]
}

/// A refusal: its code and message; no part of the password in either its
/// message, its `Display` or its `Debug`.
fn refused(platform: Platform, outcome: Result<String, MachineError>) -> (Code, String) {
    let error = outcome.unwrap_err();
    for text in [
        error.message.clone(),
        error.to_string(),
        format!("{error:?}"),
    ] {
        assert!(!text.contains(SECRET), "{platform:?}: {text}");
    }
    assert!(!error.vm_exists);
    (error.code, error.message)
}

/// Each platform's spec for `reach`, its stand-in tools answering.
fn reach_spec(scratch: &Scratch, platform: Platform) -> Spec {
    if platform == Platform::AppleContainer {
        scratch.tool(
            "container",
            r#"echo '[{"status":{"state":"running","networks":[{"ipv4Gateway":"192.168.64.1"}]}}]'"#,
        );
    }
    if platform == Platform::Docker {
        scratch.tool(
            "docker",
            r#"printf '192.168.65.254  host.docker.internal\n'"#,
        );
    }
    valid_spec(scratch, platform)
}

#[test]
fn each_platform_reaches_a_loopback_server_as_it_declares() {
    let scratch = Scratch::new("kit-reach");
    for platform in Platform::ALL {
        let spec = reach_spec(&scratch, platform);
        let prefix = format!(
            "the {} machine dev cannot reach the PostgreSQL Store's server: ",
            platform.name()
        );
        for query in ["", "?sslmode=disable", "?sslmode=verify-ca"] {
            for (i, url) in loopback_urls(query).iter().enumerate() {
                let outcome = machine::reach(&spec, url);
                match loopback(&spec) {
                    Loopback::Shared => assert_eq!(outcome.as_ref(), Ok(url)),
                    Loopback::Alias(alias) => assert_eq!(
                        outcome,
                        Ok(with_host(&alias, query)[i].clone()),
                        "{platform:?}"
                    ),
                }
            }
        }
        // the server's certificate names the loopback host, not an alias
        for url in loopback_urls("?sslmode=verify-full") {
            let outcome = machine::reach(&spec, &url);
            match loopback(&spec) {
                Loopback::Shared => assert_eq!(outcome, Ok(url)),
                Loopback::Alias(_) => {
                    let (code, message) = refused(platform, outcome);
                    assert_eq!(code, Code::Unsupported, "{message}");
                    assert!(message.starts_with(&prefix), "{message}");
                    assert!(message.contains("sslmode=verify-full"), "{message}");
                }
            }
        }
    }
}

#[test]
fn every_other_server_is_dialled_as_given_on_every_platform() {
    let scratch = Scratch::new("kit-reach-remote");
    let urls = [
        format!("postgres://lfs:{SECRET}@db.example.com:5432/layerfs?sslmode=verify-full"),
        format!("postgresql://lfs:{SECRET}@10.0.0.5/layerfs"),
        "postgres://[2001:db8::1]:5432/layerfs".to_owned(),
        // only the three loopback names are rewritten
        "postgres://u@127.0.0.2/db".to_owned(),
    ];
    for platform in Platform::ALL {
        let spec = reach_spec(&scratch, platform);
        for url in &urls {
            assert_eq!(machine::reach(&spec, url).as_ref(), Ok(url), "{platform:?}");
        }
    }
}

#[test]
fn a_url_reach_cannot_rewrite_is_refused_unless_the_loopback_is_shared() {
    let scratch = Scratch::new("kit-reach-unread");
    let cases = [
        (
            format!("host=localhost user=lfs password={SECRET}"),
            Code::InvalidInput,
        ),
        (
            format!("postgres://lfs:{SECRET}@h1:5432,h2:5433/db"),
            Code::InvalidInput,
        ),
        (
            format!("postgres://lfs:{SECRET}@/layerfs"),
            Code::Unsupported,
        ),
        (
            format!("postgres:///layerfs?host=/var/run/postgresql&password={SECRET}"),
            Code::Unsupported,
        ),
        (
            format!("postgres://lfs:{SECRET}@db/layerfs?hostaddr=127.0.0.1"),
            Code::Unsupported,
        ),
    ];
    for platform in Platform::ALL {
        let spec = reach_spec(&scratch, platform);
        for (url, code) in &cases {
            let outcome = machine::reach(&spec, url);
            if matches!(loopback(&spec), Loopback::Shared) {
                assert_eq!(outcome.as_ref(), Ok(url), "{platform:?}");
            } else {
                let (got, message) = refused(platform, outcome);
                assert_eq!(got, *code, "{platform:?}: {message}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Options and Spec
// ---------------------------------------------------------------------------

/// Which `Spec` field a `machine create` option sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    /// `cpus`, `memory`, `disk`: the VM's size.
    Size,
    /// `image`: `os_image`, the VM's own system.
    OsImage,
}

/// An option's kind and field. A new option fails here until the kit maps it.
fn field(name: &str) -> (OptKind, Field) {
    match name {
        "cpus" => (OptKind::Int, Field::Size),
        "memory" | "disk" => (OptKind::Size, Field::Size),
        "image" | "os_image" => (OptKind::Text, Field::OsImage),
        other => panic!("option {other} has no Spec field in the conformance kit; map it"),
    }
}

fn takes(platform: Platform, wanted: Field) -> bool {
    (platform.profile().options.iter()).any(|o| field(o.name).1 == wanted)
}

fn refused_spec(spec: &Spec) -> bool {
    match spec.validate() {
        Ok(()) => false,
        Err(error) => {
            assert_eq!(error.code, Code::InvalidInput, "{error}");
            assert!(!error.vm_exists);
            true
        }
    }
}

#[test]
fn each_declared_option_is_a_spec_field_of_its_kind() {
    for platform in Platform::ALL {
        let options = platform.profile().options;
        let names: BTreeSet<&str> = options.iter().map(|o| o.name).collect();
        assert_eq!(names.len(), options.len(), "{platform:?}: an option twice");
        for option in options {
            assert_eq!(option.kind, field(option.name).0, "{platform:?} {option:?}");
        }
        // a field is declared whole or not at all
        let size = ["cpus", "memory", "disk"];
        let count = size.iter().filter(|n| names.contains(*n)).count();
        assert!(count == 0 || count == size.len(), "{platform:?} size");
        // created machines are sized and given their own system
        assert_eq!(
            takes(platform, Field::Size),
            platform.created(),
            "{platform:?}"
        );
        assert_eq!(
            takes(platform, Field::OsImage),
            platform.created(),
            "{platform:?}"
        );
    }
}

#[test]
fn spec_accepts_exactly_the_fields_the_profile_declares() {
    let scratch = Scratch::new("kit-options");
    for platform in Platform::ALL {
        let base = valid_spec(&scratch, platform);
        let at = format!("{platform:?}");

        // sizes are checked where they are declared and ignored elsewhere
        let sized = takes(platform, Field::Size);
        let mut spec = base.clone();
        spec.cpus = 0;
        assert_eq!(refused_spec(&spec), sized, "{at} cpus");
        let mut spec = base.clone();
        spec.memory = 1 << 20;
        assert_eq!(refused_spec(&spec), sized, "{at} memory");
        let mut spec = base.clone();
        spec.disk = 1 << 20;
        assert_eq!(refused_spec(&spec), sized, "{at} disk");
    }
}

// ---------------------------------------------------------------------------
// forward
// ---------------------------------------------------------------------------

/// How the caller reaches a Sandbox daemon on `127.0.0.1:<guest port>` in
/// a platform's machine (#535 G4, `control/README.md`).
enum DaemonPort {
    /// The daemon is on the caller's loopback: the port is used as it is.
    Shared,
    /// A detached `ssh -L` through the platform's own tools carries it.
    Tunnel,
    /// Refused before any tool runs, with this code and reason.
    Refused(Code, &'static str),
}

fn daemon_port(spec: &Spec) -> DaemonPort {
    match spec.kind {
        Platform::Native => DaemonPort::Shared,
        Platform::Lima => DaemonPort::Tunnel,
        Platform::Docker => DaemonPort::Refused(Code::Unsupported, "Docker publishes a container's ports only when it starts, so a Sandbox daemon's port in the worker container cannot be published later; its Workspace calls go through the worker"),
        Platform::AppleContainer => DaemonPort::Refused(Code::Unsupported, "the container CLI publishes a port only when the machine's container is created, so a Sandbox daemon's port cannot be published later; its Workspace calls go through the worker"),
    }
}

#[test]
fn each_platform_forwards_a_daemon_port_as_it_declares() {
    const GUEST: u16 = 41234;
    for platform in Platform::ALL {
        let scratch = Scratch::new("kit-forward");
        refusing_tools(&scratch);
        // the system's `sh` and `ps` after the stand-ins: a forward starts detached
        let mut spec = valid_spec(&scratch, platform);
        spec.tools.path.push(":/bin:/usr/bin");
        // the stand-in `ssh` runs once first: macOS assesses a new script at
        // its first run, which would otherwise outlast the forward's settle
        let _ = std::process::Command::new(scratch.bin().join("ssh")).output();
        let _ = std::fs::remove_file(scratch.log_file());
        let _ = std::fs::remove_file(scratch.path("tools.log"));
        let own: BTreeSet<&str> = platform.tool().into_iter().chain(TRANSPORTS).collect();
        let outcome = machine::forward(&spec, GUEST);
        let log = scratch.log();
        let tools = standin::read(&scratch.path("tools.log"));
        let at = format!("{platform:?}: {outcome:?}\n{log}");
        match daemon_port(&spec) {
            DaemonPort::Shared => {
                let shared = Forward {
                    address: ([127, 0, 0, 1], GUEST).into(),
                    pid: None,
                };
                assert_eq!(outcome, Ok(shared), "{at}");
                assert!(log.is_empty(), "{at}");
            }
            DaemonPort::Refused(code, why) => {
                let error = outcome.expect_err(&at);
                assert_eq!(error.code, code, "{at}");
                assert_eq!(
                    error.message,
                    format!(
                        "the {} machine dev forwards no Sandbox daemon port: {why}",
                        platform.name()
                    ),
                    "{at}"
                );
                assert!(!error.vm_exists, "{at}");
                assert!(log.is_empty(), "refused after an effect: {at}");
            }
            DaemonPort::Tunnel => {
                // the stand-ins refuse, so the forward ends at once (or its
                // machine is not observed running) and nothing is left
                let error = outcome.expect_err(&at);
                assert!(!error.vm_exists, "{at}");
                assert!(!log.is_empty(), "ran no tool: {at}");
                for tool in tools.lines() {
                    assert!(own.contains(tool), "drove another platform's {tool}: {at}");
                }
                let ssh: Vec<&str> = log.lines().filter(|l| l.starts_with("ssh ")).collect();
                assert!(ssh.len() <= 1, "one attempt: {at}");
                for call in ssh {
                    assert!(
                        call.starts_with("ssh -N -o ExitOnForwardFailure=yes "),
                        "{at}"
                    );
                    let target = format!(":127.0.0.1:{GUEST} ");
                    let local = call.split(" -L 127.0.0.1:").nth(1).expect(&at);
                    let (port, rest) = local.split_once(':').expect(&at);
                    assert_ne!(port.parse::<u16>().expect(&at), GUEST, "{at}");
                    assert!(format!(":{rest}").starts_with(&target), "{at}");
                }
            }
        }
    }
}

#[test]
fn forward_close_of_a_forward_without_a_pid_runs_nothing() {
    for platform in Platform::ALL {
        let scratch = Scratch::new("kit-forward-close");
        refusing_tools(&scratch);
        let spec = valid_spec(&scratch, platform);
        let forward = Forward {
            address: ([127, 0, 0, 1], 41235).into(),
            pid: None,
        };
        machine::forward_close(&spec, &forward);
        assert!(scratch.log().is_empty(), "{platform:?}: {}", scratch.log());
    }
}

// ---------------------------------------------------------------------------
// Push and pull (#536 §7.3): the snapshot, send, verify, install and retire
// steps per platform arrive with #536 step I; their checks join this kit
// then, run over `Platform::ALL` like the sections above.
// ---------------------------------------------------------------------------
