//! The platform conformance kit (#536 §11 step 9): checks every machine
//! platform passes, run over `Platform::ALL`, so a new platform is covered
//! as soon as it is a variant. Nothing real is touched: provider tools are
//! the stand-ins of `support/standin.rs`, which log their calls and refuse.
//!
//! Per platform it checks:
//! - its profile against the one support table (`support`,
//!   `sandbox_allowed`) on every host, and against the committed
//!   `connect/SUPPORT.json`, row by row;
//! - that each lifecycle call (create or connect, start, stop, status,
//!   remove or disconnect, host identity) either runs through the
//!   platform's provider tool (here, its stand-in) or is refused before any
//!   effect with the fixed refusal its `How` declares;
//! - `reach` against the §7.3 rules: a loopback host is shared, rewritten to
//!   the platform's alias or refused; a rewrite is refused for
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
#[path = "../../layerfs-sandbox/tests/support/support_json.rs"]
mod support_json;
use layerfs_machine::{
    self as machine, platforms, sandbox_allowed, support, Backend, CallSite, Code, ExecutorKind,
    Forward, HostOs, How, KernelRoute, MachineError, OptKind, Platform, Route, Spec, SshSpec,
    StoreKind,
};
use layerfs_sandbox::MountDriver;
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
            let on_host = profile.hosts.contains(&host);
            if on_host {
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
    Connect,
    Start,
    Stop,
    Status,
    Remove,
    HostIdentity,
}

const CALLS: [Call; 7] = [
    Call::Create,
    Call::Connect,
    Call::Start,
    Call::Stop,
    Call::Status,
    Call::Remove,
    Call::HostIdentity,
];

/// The fixed refusal a platform's `How` declares for `call`, or `None`
/// when a platform of that kind does it.
fn refusal(how: How, call: Call) -> Option<&'static str> {
    match (call, how) {
        (Call::Create, How::BuiltIn) => Some("is built in and never created"),
        (Call::Create, How::Connected) => Some("is connected, not created"),
        (Call::Connect, How::BuiltIn | How::Created) => Some("is created, not connected"),
        (Call::Remove, How::BuiltIn) => Some("is built in and never removed"),
        (Call::HostIdentity, How::BuiltIn | How::Created) => Some("has no ssh host"),
        _ => None,
    }
}

/// Every refusal text above, so a call a platform does is never answered
/// with one.
const REFUSALS: [&str; 5] = [
    "is built in and never created",
    "is connected, not created",
    "is created, not connected",
    "is built in and never removed",
    "has no ssh host",
];

/// The guest transports any platform may drive beside its own tool.
const TRANSPORTS: [&str; 3] = ["ssh", "ssh-keygen", "tar"];

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

/// Whether `platform` declares the option `name`.
fn declares(platform: Platform, name: &str) -> bool {
    platform.profile().options.iter().any(|o| o.name == name)
}

/// The platform whose machines are nested hosts (the one that declares
/// `nested`); a `host` option names one of them.
fn nested_host_kind() -> Platform {
    let mut hosts = Platform::ALL.into_iter().filter(|p| declares(*p, "nested"));
    let kind = hosts.next().expect("a platform declares `nested`");
    assert!(hosts.next().is_none(), "one platform declares `nested`");
    kind
}

/// A valid spec of `platform`, built only from what its profile declares:
/// an ssh host where it takes ssh options, a nested host machine where it
/// takes `host`. The docker worker container's image comes from the bundle
/// (#535 E7) and is passed as `os_image`.
fn valid_spec(scratch: &Scratch, platform: Platform) -> Spec {
    let mut spec = scratch.spec(platform);
    spec.ssh = declares(platform, "user")
        .then(|| SshSpec::new("example.test", scratch.path("known_hosts")));
    if declares(platform, "host") {
        let mut host = scratch.spec(nested_host_kind());
        host.name = "host".into();
        host.vm_name = "lfs-host".into();
        host.nested = true;
        spec.host = Some(Box::new(host));
    }
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
        Call::Connect => machine::connect(spec, &bundle).map(drop),
        Call::Start => machine::start(spec, &bundle).map(drop),
        Call::Stop => machine::stop(spec),
        Call::Status => machine::status(spec).map(drop),
        Call::Remove => machine::remove(spec),
        Call::HostIdentity => machine::host_identity(spec).map(drop),
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
/// `connect/README.md`). The one declaration this kit needs per platform.
enum Loopback {
    /// The daemon shares it: the URL is used as given.
    Shared,
    /// A loopback host becomes this host name or address.
    Alias(String),
    /// A loopback host is refused, `Unsupported`, with this reason.
    Refused(&'static str),
}

fn loopback(spec: &Spec) -> Loopback {
    match spec.kind {
        Platform::Native => Loopback::Shared,
        Platform::Docker => Loopback::Alias("host.docker.internal".into()),
        Platform::Lima => Loopback::Alias("host.lima.internal".into()),
        Platform::Firecracker => Loopback::Refused("a firecracker guest has no outbound NAT and its tap address is the nested lima host, not this host (G7)"),
        Platform::AppleContainer => Loopback::Refused("an apple-container guest has no host alias and cannot reach the host's loopback (G7)"),
        Platform::Microsandbox => Loopback::Refused("a microsandbox guest reaches the host only with --net host, which LayerFS does not set (G7)"),
        Platform::Ssh => Loopback::Refused("an ssh machine cannot reach the caller's loopback"),
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

/// Each platform's spec for `reach` with and without a recorded network
/// number (none reads it since the G7 receipt; both are kept).
fn reach_specs(scratch: &Scratch, platform: Platform) -> Vec<Spec> {
    let mut with_network = valid_spec(scratch, platform);
    with_network.state.network = Some(7);
    let mut without = with_network.clone();
    without.state.network = None;
    vec![with_network, without]
}

#[test]
fn each_platform_reaches_a_loopback_server_as_it_declares() {
    let scratch = Scratch::new("kit-reach");
    for platform in Platform::ALL {
        for spec in reach_specs(&scratch, platform) {
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
                        Loopback::Refused(why) => {
                            let (code, message) = refused(platform, outcome);
                            assert_eq!(code, Code::Unsupported, "{message}");
                            assert_eq!(message, format!("{prefix}{why}"));
                        }
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
                    Loopback::Refused(why) => {
                        let (_, message) = refused(platform, outcome);
                        assert_eq!(message, format!("{prefix}{why}"));
                    }
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
        for spec in reach_specs(&scratch, platform) {
            for url in &urls {
                assert_eq!(machine::reach(&spec, url).as_ref(), Ok(url), "{platform:?}");
            }
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
        for spec in reach_specs(&scratch, platform) {
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
}

#[test]
fn a_g7_loopback_refusal_is_the_support_tables_reason() {
    let scratch = Scratch::new("kit-reach-g7");
    for platform in Platform::ALL {
        let Loopback::Refused(why) = loopback(&valid_spec(&scratch, platform)) else {
            continue;
        };
        if !why.ends_with("(G7)") {
            continue;
        }
        for host in platform.profile().hosts {
            let row = support(platform, *host);
            if row.backend.is_some() {
                assert_eq!(row.postgresql.reason, Some(why), "{platform:?} on {host:?}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Options and Spec
// ---------------------------------------------------------------------------

/// Which `Spec` field a `machine create` / `machine connect` option sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    /// `cpus`, `memory`, `disk`: the VM's size.
    Size,
    /// `image`: `os_image`, the VM's own system.
    OsImage,
    /// `nested`.
    Nested,
    /// `host`: the nested machine it runs in.
    Host,
    /// `user`, `port`, `identity`, `known_hosts`, `root`: `ssh`.
    Ssh,
}

/// An option's kind and field. A new option fails here until the kit maps it.
fn field(name: &str) -> (OptKind, Field) {
    match name {
        "cpus" => (OptKind::Int, Field::Size),
        "memory" | "disk" => (OptKind::Size, Field::Size),
        "image" => (OptKind::Text, Field::OsImage),
        "nested" => (OptKind::Bool, Field::Nested),
        "host" => (OptKind::Name, Field::Host),
        "user" | "identity" | "known_hosts" | "root" => (OptKind::Text, Field::Ssh),
        "port" => (OptKind::Int, Field::Ssh),
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
        for (wanted, all) in [
            (Field::Size, &["cpus", "memory", "disk"][..]),
            (
                Field::Ssh,
                &["user", "port", "identity", "known_hosts", "root"][..],
            ),
        ] {
            let count = all.iter().filter(|n| names.contains(*n)).count();
            assert!(count == 0 || count == all.len(), "{platform:?} {wanted:?}");
        }
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

        let mut spec = base.clone();
        spec.nested = true;
        assert_eq!(refused_spec(&spec), !takes(platform, Field::Nested), "{at}");

        let mut without = base.clone();
        without.host = None;
        let mut with = base.clone();
        let mut host = scratch.spec(nested_host_kind());
        host.nested = true;
        with.host = Some(Box::new(host));
        assert_eq!(refused_spec(&with), !takes(platform, Field::Host), "{at}");
        assert_eq!(refused_spec(&without), takes(platform, Field::Host), "{at}");

        let mut with = base.clone();
        with.ssh = Some(SshSpec::new("example.test", scratch.path("known_hosts")));
        let mut without = base.clone();
        without.ssh = None;
        assert_eq!(refused_spec(&with), !takes(platform, Field::Ssh), "{at}");
        assert_eq!(refused_spec(&without), takes(platform, Field::Ssh), "{at}");

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
/// a platform's machine (#535 G4, `connect/README.md`).
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
        Platform::Ssh | Platform::Lima => DaemonPort::Tunnel,
        Platform::Docker => DaemonPort::Refused(Code::InvalidInput, "each docker Sandbox publishes its daemon's port on the host's loopback when it is created, and that published address is its endpoint"),
        Platform::AppleContainer => DaemonPort::Refused(Code::Unsupported, "the container CLI publishes a port only when the machine's container is created, so a Sandbox daemon's port cannot be published later; its Workspace calls go through the worker"),
        Platform::Microsandbox => DaemonPort::Refused(Code::Unsupported, "msb publishes a port only when the machine's sandbox is created, so a Sandbox daemon's port cannot be published later; its Workspace calls go through the worker"),
        Platform::Firecracker => DaemonPort::Refused(Code::Unsupported, "its Sandbox daemons listen on the microVM's loopback, which a forward through the Lima host cannot reach (it reaches only the microVM's tap address); its Workspace calls go through the worker"),
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
