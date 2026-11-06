//! What each kind needs on this host and what is missing (`machine list`),
//! as its adapter reports it: `tool:<cli>` for a command not on the search
//! path, `host:<fact>` for what the host lacks. A probe starts nothing.
use crate::adapter::adapter;
use crate::platform::{HostOs, Platform};
use crate::setup::tool;
use crate::spec::Tools;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// One kind's readiness on this host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub kind: Platform,
    pub ready: bool,
    pub missing: Vec<String>,
    /// The provider tool's version, when it answers.
    pub version: Option<String>,
}

const ANSWER: Duration = Duration::from_secs(10);

/// Probes `kind` on this host.
pub fn probe(kind: Platform, tools: &Tools) -> Probe {
    if let Some(reason) = layerfs_sandbox_backend::platform_policy::disabled_reason(kind.name()) {
        return Probe {
            kind,
            ready: false,
            missing: vec![reason.to_owned()],
            version: None,
        };
    }
    let mut missing = Vec::new();
    let version = adapter(kind).probe(tools, HostOs::current(), &mut missing);
    Probe {
        kind,
        ready: missing.is_empty(),
        missing,
        version,
    }
}

/// `native` on Linux needs a root caller, `/dev/fuse` and cgroup v2
/// (`linuxMissing` in today's `sandbox/platforms.js`); macOS needs nothing
/// LayerFS can check (Network Volumes access is asked by the system).
pub fn native_missing(host: HostOs) -> Vec<String> {
    match host {
        HostOs::Linux => {
            let mut missing = Vec::new();
            if !nix::unistd::geteuid().is_root() {
                missing.push("host:root".into());
            }
            if !Path::new("/dev/fuse").exists() {
                missing.push("host:/dev/fuse".into());
            }
            if !Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
                missing.push("host:cgroup2".into());
            }
            missing
        }
        HostOs::Macos => Vec::new(),
        HostOs::Windows | HostOs::Other => vec!["host:macos-or-linux".into()],
    }
}

/// Lima's `vz`, Apple Container and Firecracker's host are macOS only today.
pub(crate) fn macos_missing(host: HostOs) -> Vec<String> {
    if host == HostOs::Macos {
        Vec::new()
    } else {
        vec!["host:macos".into()]
    }
}

/// Nested virtualization under `vz`: macOS 15 or newer on an M3 or newer.
pub fn nested_missing(host: HostOs) -> Vec<String> {
    if host != HostOs::Macos {
        return Vec::new();
    }
    let mut missing = Vec::new();
    let release = first_line(
        tool::command(Path::new("/usr/bin/sw_vers"), ["-productVersion"]),
        false,
    );
    let major = release
        .as_deref()
        .and_then(|v| v.split('.').next())
        .and_then(|m| m.parse::<u32>().ok());
    if major.is_none_or(|m| m < 15) {
        missing.push("host:macos15".into());
    }
    let brand = first_line(
        tool::command(
            Path::new("/usr/sbin/sysctl"),
            ["-n", "machdep.cpu.brand_string"],
        ),
        false,
    );
    let generation = brand
        .as_deref()
        .and_then(|b| b.strip_prefix("Apple M"))
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse::<u32>().ok());
    if generation.is_none_or(|g| g < 3) {
        missing.push("host:apple-m3".into());
    }
    missing
}

pub(crate) fn tool_version(
    tools: &Tools,
    name: &str,
    args: &[&str],
    missing: &mut Vec<String>,
) -> Option<String> {
    match tool::lookup(tools, name) {
        Some(path) => first_line(tool::command(&path, args), false),
        None => {
            missing.push(format!("tool:{name}"));
            None
        }
    }
}

/// The first line a command prints (on stderr when `stderr`), if it succeeds.
pub(crate) fn first_line(command: Command, stderr: bool) -> Option<String> {
    let out = tool::run(command, None, ANSWER, "a version query").ok()?;
    let text = if stderr { &out.stderr } else { &out.stdout };
    let line = text.lines().next()?.trim();
    (out.ok() && !line.is_empty()).then(|| line.to_owned())
}
