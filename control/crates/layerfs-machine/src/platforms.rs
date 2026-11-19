//! `home get --with platforms` (#535 §9a, D10, P2): the support table for this host
//! and build, the rows of `control/SUPPORT.json`, each with its live probe
//! (`ready`, `missing`, `version`), the bundle a worker platform needs
//! (`bundle:<dir>`), each option's default, and the docker row's socket and
//! the sockets the host names. It starts no machine (P4) and needs no home:
//! with one it reads the docker machine's recorded socket. Also the
//! per-machine `supports` of `machine.list` and `machine.get` (P3).
use super::spec as record;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_platform::{
    self as adapters, CallSite, DockerCandidate, DockerEnv, HostOs, Platform, Probe, SupportRow,
    Supports,
};
use std::path::{Path, PathBuf};

/// The platforms of this host: `{ host, items: [row] }`, the keys of
/// `home.get`'s `platforms` section.
pub fn platforms(home: &Path) -> Outcome {
    let host = HostOs::current();
    let env = DockerEnv::current();
    // a docker record this build cannot read (a damaged record that
    // `machine start` would refuse) leaves the host's choice
    let (socket, source) = record::docker_socket(home).unwrap_or_else(|_| {
        let chosen = adapters::docker_automatic(&env);
        let path = chosen
            .path
            .unwrap_or_else(|| PathBuf::from(adapters::DEFAULT_DOCKER_SOCKET));
        (path, chosen.source)
    });
    let mut tools = adapters::Tools::from_env();
    tools.docker_socket = socket.clone();
    // a platform that runs a worker needs this build's bundle
    let bundle = super::bundle::bundle()
        .err()
        .and_then(|_| super::bundle::bundle_dir("linux").ok())
        .map(|dir| format!("bundle:{}", dir.display()));
    let rows = adapters::platforms(host)
        .iter()
        .map(|row| {
            let mut probe = adapters::probe(row.platform, &tools);
            if row.sqlite == Some(CallSite::Worker) {
                probe.missing.extend(bundle.clone());
                probe.ready = probe.missing.is_empty();
            }
            // the probe's ping found the Engine answering when it gave a
            // version; else one more ping says why it did not
            let answering = probe.version.is_some();
            let mut value = row_value(row, probe);
            if row.platform == Platform::Docker {
                value = docker_fields(value, &socket, &source, answering, &env);
            }
            value
        })
        .collect();
    Ok(SdkValue::map([
        ("host", SdkValue::text(host.as_str())),
        ("items", SdkValue::List(rows)),
    ]))
}

/// The docker row's `socket` (the one its machine uses, where it comes
/// from, and what it does when asked) and `candidates` (every endpoint the
/// host names; one that is not `unix://` is listed with `usable: false`).
fn docker_fields(
    row: SdkValue,
    socket: &Path,
    source: &str,
    answering: bool,
    env: &DockerEnv,
) -> SdkValue {
    let state = match answering {
        true => adapters::SocketState::Answering,
        false => adapters::socket_state(socket, adapters::SOCKET_ANSWER),
    };
    let socket = SdkValue::map([
        ("path", SdkValue::text(socket.display().to_string())),
        ("source", SdkValue::text(source)),
        ("state", SdkValue::text(state.as_str())),
    ]);
    let candidates = adapters::docker_candidates(env)
        .iter()
        .map(candidate)
        .collect();
    let row = layerfs_home::home::with_field(row, "socket", socket);
    layerfs_home::home::with_field(row, "candidates", SdkValue::List(candidates))
}

fn candidate(found: &DockerCandidate) -> SdkValue {
    SdkValue::map([
        (
            "path",
            found
                .path
                .as_ref()
                .map_or(SdkValue::Null, |p| SdkValue::text(p.display().to_string())),
        ),
        ("host", SdkValue::text(&found.host)),
        ("source", SdkValue::text(&found.source)),
        ("usable", SdkValue::Bool(found.usable())),
    ])
}

/// What `machine.create` takes when an option is not given: the VMs' size.
fn default_of(name: &str) -> SdkValue {
    match name {
        "cpus" => SdkValue::Int(record::DEFAULT_CPUS),
        "memory" => SdkValue::Int(record::DEFAULT_MEMORY),
        "disk" => SdkValue::Int(record::DEFAULT_DISK),
        _ => SdkValue::Null,
    }
}

/// A machine's `supports`: what its platform does on this host, as words
/// (`sqlite`, `postgresql_sandbox`, `log`).
pub fn supports(platform: Platform) -> SdkValue {
    let Supports {
        sqlite,
        postgresql_sandbox,
        log,
        ..
    } = adapters::support(platform, HostOs::current()).supports();
    let does = [
        ("sqlite", sqlite),
        ("postgresql_sandbox", postgresql_sandbox),
        ("log", log),
    ];
    let words = does.into_iter().filter(|(_, does)| *does);
    SdkValue::List(words.map(|(word, _)| SdkValue::text(word)).collect())
}

fn row_value(row: &SupportRow, probe: Probe) -> SdkValue {
    let options = row
        .options
        .iter()
        .map(|option| {
            SdkValue::map([
                ("option", SdkValue::text(option.name)),
                ("kind", SdkValue::text(option.kind.as_str())),
                ("default", default_of(option.name)),
            ])
        })
        .collect();
    SdkValue::map([
        ("platform", SdkValue::text(row.platform.name())),
        ("how", SdkValue::text(row.how.as_str())),
        ("backend", optional(row.backend.map(|b| b.stored_name()))),
        ("route", optional(row.kernel.map(|k| k.as_str()))),
        ("sqlite", optional(row.sqlite.map(|s| s.as_str()))),
        (
            "postgresql",
            SdkValue::map([
                (
                    "store_calls",
                    SdkValue::text(row.postgresql.store_calls.as_str()),
                ),
                ("sandbox", SdkValue::Bool(row.postgresql.sandbox)),
                ("reason", optional(row.postgresql.reason)),
            ]),
        ),
        ("ready", SdkValue::Bool(probe.ready)),
        (
            "missing",
            SdkValue::List(probe.missing.into_iter().map(SdkValue::Text).collect()),
        ),
        (
            "version",
            probe.version.map(SdkValue::Text).unwrap_or_default(),
        ),
        ("options", SdkValue::List(options)),
        ("description", SdkValue::text(&row.description)),
    ])
}

fn optional(text: Option<&str>) -> SdkValue {
    text.map(SdkValue::text).unwrap_or_default()
}
