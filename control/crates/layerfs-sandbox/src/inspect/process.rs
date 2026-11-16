//! A Sandbox's daemon process as a monitoring read sees it
//! (layerfs-project#164), on the side whose kernel runs it: whether the
//! process its daemon record names still is that process ([`live`]: its
//! id, start time and boot, as a reopen checks them), when it started
//! ([`shown`]) and what its cgroup has used ([`usage`]). Everything is read
//! from the record and from `/proc` or the cgroup's own small files: no
//! daemon is asked, nothing is opened in a run folder and nothing is
//! written.
use crate::record::DaemonRecord;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

/// This kernel's unified cgroup v2 hierarchy, where the namespaces backend
/// makes one group per Sandbox (`layerfs-sandbox-backend`
/// `backend/namespaces/cgroup.rs`: `ROOT`).
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";
/// The folder under it that holds those groups, one per runtime id (the
/// same file's `GROUP`).
const GROUP: &str = "layerfs";

/// Why a Sandbox's `usage` is null: it runs as host processes, with no
/// cgroup of its own.
pub const HOST_PROCESS: &str = "host_process";
/// Why a Sandbox's `usage` is null: this kernel's cgroup hierarchy is not
/// the unified v2 one.
pub const CGROUP_V1: &str = "cgroup_v1";
/// Why a Sandbox's `usage` is null: its group is not there (it ended).
pub const NO_CGROUP: &str = "no_cgroup";
/// Why some values of a Sandbox's `usage` are null: its group has no
/// `memory.current` or no `pids.current`, because the group above it gives
/// its children no memory or pids controller.
pub const NO_CONTROLLER: &str = "no_controller";
/// Why a Sandbox's `daemon` and `usage` are null: the side that runs it
/// did not answer.
pub const NOT_OBSERVED: &str = "not_observed";
/// Why a Sandbox's `live` is null: its daemon record names no process.
pub const NO_PID: &str = "no_pid";

/// The names of the fields [`observed`] answers.
pub const FIELDS: [&str; 3] = ["daemon", "usage", "usage_reason"];

/// Whether the process `daemon` names is that process and runs: its id
/// with its start time and boot when the record holds them, as
/// `layerfs_home::process::alive_as` matches them. `Err` when the record
/// names no process (a record of the retired docker backend).
pub fn live(daemon: &DaemonRecord) -> Result<bool, &'static str> {
    let pid = (daemon.pid())
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or(NO_PID)?;
    Ok(layerfs_home::process::alive_as(
        pid,
        daemon.start().and_then(|start| start.parse().ok()),
        (daemon.boot()).and_then(|boot| u128::from_str_radix(boot, 16).ok()),
    ))
}

/// `sandbox.get`'s `daemon`: `{ pid, started_at }`. `pid` is the record's
/// (the daemon on `native`, the Sandbox's init process under namespaces),
/// in this machine's process ids. `started_at` is when that process
/// started, in milliseconds since the epoch ([`started_at`]); null when
/// the process is gone or its start cannot be told.
pub fn shown(daemon: &DaemonRecord) -> SdkValue {
    let started = match live(daemon) {
        Ok(true) => (daemon.start())
            .and_then(|start| start.parse().ok())
            .and_then(started_at),
        _ => None,
    };
    SdkValue::map([
        ("pid", daemon.pid().map_or(SdkValue::Null, SdkValue::Int)),
        ("started_at", started.map_or(SdkValue::Null, SdkValue::Int)),
    ])
}

/// A recorded process start (`layerfs_bridge::process::LocalProcess`
/// `start`) as milliseconds since the epoch. macOS records microseconds
/// since the epoch, so this is exact.
#[cfg(target_os = "macos")]
pub fn started_at(start: u64) -> Option<i64> {
    i64::try_from(start / 1000).ok()
}

/// A recorded process start (`layerfs_bridge::process::LocalProcess`
/// `start`) as milliseconds since the epoch. Linux records clock ticks
/// since boot (`/proc/<pid>/stat` field 22), so this is the boot time
/// (`btime` of `/proc/stat`, whole seconds) plus the ticks, as `ps` shows
/// a start: right to about a second.
#[cfg(target_os = "linux")]
pub fn started_at(start: u64) -> Option<i64> {
    /// `/proc` writes times in `USER_HZ` ticks, 100 a second on every
    /// architecture this builds for.
    const USER_HZ: u64 = 100;
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let boot: u64 = (stat.lines())
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    let ms = boot
        .checked_mul(1000)?
        .checked_add(start.checked_mul(1000)? / USER_HZ)?;
    i64::try_from(ms).ok()
}

/// No other system records a start this build can place in time.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn started_at(_: u64) -> Option<i64> {
    None
}

/// `sandbox.get`'s `usage` of the Sandbox `daemon` records, with why it is
/// null when it is: a namespaces Sandbox's cgroup under `root`
/// ([`usage_in`]); a host-process Sandbox has none.
pub fn usage(daemon: &DaemonRecord, root: &Path) -> (SdkValue, Option<&'static str>) {
    if daemon.runtime() != Some("namespaces") {
        return (SdkValue::Null, Some(HOST_PROCESS));
    }
    match daemon.container() {
        Some(id) => usage_in(root, id),
        None => (SdkValue::Null, Some(NO_CGROUP)),
    }
}

/// What the cgroup of the Sandbox with runtime id `id` has used, under
/// cgroup v2 hierarchy `root` ([`CGROUP_ROOT`]): `{ cpu_usec,
/// memory_current, memory_peak, pids }` from `cpu.stat` (`usage_usec`),
/// `memory.current`, `memory.peak` and `pids.current`, each null when its
/// file is not there (a controller the group does not have, or
/// `memory.peak` before Linux 5.19), and [`NO_CONTROLLER`] as the reason
/// when `memory.current` or `pids.current` is the one missing. Null with
/// [`CGROUP_V1`] when `root` is not a v2 hierarchy, and with [`NO_CGROUP`]
/// when the group is not there or `id` is no group's name.
pub fn usage_in(root: &Path, id: &str) -> (SdkValue, Option<&'static str>) {
    if !root.join("cgroup.controllers").is_file() {
        return (SdkValue::Null, Some(CGROUP_V1));
    }
    let plain = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    let group = root.join(GROUP).join(id);
    if id.is_empty() || !id.bytes().all(plain) || !group.is_dir() {
        return (SdkValue::Null, Some(NO_CGROUP));
    }
    let text = |file: &str| std::fs::read_to_string(group.join(file)).ok();
    let number = |text: Option<String>| {
        (text.and_then(|text| text.trim().parse::<i64>().ok()))
            .map_or(SdkValue::Null, SdkValue::Int)
    };
    let cpu = text("cpu.stat").and_then(|stat| {
        (stat.lines())
            .find_map(|line| line.strip_prefix("usage_usec "))
            .map(str::to_owned)
    });
    let (memory, pids) = (text("memory.current"), text("pids.current"));
    let reason = (memory.is_none() || pids.is_none()).then_some(NO_CONTROLLER);
    let usage = SdkValue::map([
        ("cpu_usec", number(cpu)),
        ("memory_current", number(memory)),
        ("memory_peak", number(text("memory.peak"))),
        ("pids", number(pids)),
    ]);
    (usage, reason)
}

/// What a monitoring read shows of the process `daemon` names: `{ daemon,
/// usage }`, with `usage_reason` when `usage` or one of its values is
/// null for a known cause. `sandbox.get`
/// shows these three fields as they are.
pub fn observed(daemon: &DaemonRecord) -> Vec<(String, SdkValue)> {
    let (usage, reason) = usage(daemon, Path::new(CGROUP_ROOT));
    fields(shown(daemon), usage, reason)
}

/// The same fields for a Sandbox whose side did not answer: both null,
/// with [`NOT_OBSERVED`].
pub fn not_observed() -> Vec<(String, SdkValue)> {
    fields(SdkValue::Null, SdkValue::Null, Some(NOT_OBSERVED))
}

fn fields(daemon: SdkValue, usage: SdkValue, reason: Option<&str>) -> Vec<(String, SdkValue)> {
    let mut fields = vec![("daemon".to_owned(), daemon), ("usage".to_owned(), usage)];
    fields.extend(reason.map(|reason| ("usage_reason".to_owned(), SdkValue::text(reason))));
    fields
}
