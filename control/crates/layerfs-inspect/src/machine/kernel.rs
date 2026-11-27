//! What the kernel a machine runs on says of its processors and memory
//! (layerfs-project#164), for the inspect kind `machine_stats`. In a
//! container (the docker machine's worker) these are the Engine host's;
//! in a VM the VM's own.
//!
//! Linux reads `/proc/meminfo`, and `/proc/stat` for the processor
//! counters (layerfs-project#182 L1). macOS has no such files: one run of
//! `/usr/sbin/sysctl` (the tool `layerfs_bridge::process` reads the boot
//! id with) prints the five memory numbers, with no new system call
//! binding; its processor ticks need `host_statistics`, an `unsafe` call
//! this crate forbids, so they are not read.
use layerfs_sandbox::inspect::process::KernelStat;

/// The processors this process may run on.
pub(super) fn cpus() -> Option<i64> {
    let cpus = std::thread::available_parallelism().ok()?;
    i64::try_from(cpus.get()).ok()
}

/// The machine's processor counters and boot time ([`KernelStat`]), from
/// one read of `/proc/stat`, with why the counters are null when they are:
/// `cpu_not_read` when the read failed, `cpu_unsupported` on a system with
/// no `/proc/stat` (macOS). In a container (the docker machine's worker)
/// `/proc/stat` covers the whole Engine host.
pub(super) fn cpu() -> (KernelStat, Option<&'static str>) {
    if cfg!(target_os = "linux") {
        let stat = KernelStat::read();
        (stat, stat.busy_usec.is_none().then_some("cpu_not_read"))
    } else {
        (KernelStat::default(), Some("cpu_unsupported"))
    }
}

/// Memory in bytes: all of it and what is in use, each `None` when it was
/// not read, with why.
pub(super) struct Memory {
    pub(super) total: Option<i64>,
    pub(super) used: Option<i64>,
    pub(super) reason: Option<&'static str>,
}

/// Why memory is null: the read of it failed.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const NOT_READ: &str = "memory_not_read";

/// `MemTotal` and `MemTotal - MemAvailable` of `/proc/meminfo`: what the
/// kernel estimates is in use and cannot be given back without swapping.
#[cfg(target_os = "linux")]
pub(super) fn memory() -> Memory {
    let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let bytes = |name: &str| -> Option<i64> {
        let line = text.lines().find_map(|line| line.strip_prefix(name))?;
        let kib: i64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
        kib.checked_mul(1024)
    };
    let total = bytes("MemTotal:");
    let used = total.and_then(|total| Some(total - bytes("MemAvailable:")?));
    Memory {
        total,
        used,
        reason: (total.is_none() || used.is_none()).then_some(NOT_READ),
    }
}

/// `hw.memsize`, and that less the pages macOS can give back at once
/// (free, file-backed and purgeable): an estimate of the same kind as
/// Linux's `MemTotal - MemAvailable`. One run of `sysctl`, read once.
#[cfg(target_os = "macos")]
pub(super) fn memory() -> Memory {
    const NAMES: [&str; 5] = [
        "hw.memsize",
        "vm.pagesize",
        "vm.page_free_count",
        "vm.page_pageable_external_count",
        "vm.page_purgeable_count",
    ];
    let read = || -> Option<(i64, i64)> {
        let out = std::process::Command::new("/usr/sbin/sysctl")
            .arg("-n")
            .args(NAMES)
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        let numbers: Vec<i64> = (text.lines())
            .map(|line| line.trim().parse().ok())
            .collect::<Option<_>>()?;
        let [total, page, free, file, purgeable] = numbers[..] else {
            return None;
        };
        if !out.status.success() || total <= 0 || page <= 0 {
            return None;
        }
        let available = free.checked_add(file)?.checked_add(purgeable)?;
        let used = total.checked_sub(available.checked_mul(page)?)?;
        Some((total, used.clamp(0, total)))
    };
    match read() {
        Some((total, used)) => Memory {
            total: Some(total),
            used: Some(used),
            reason: None,
        },
        None => Memory {
            total: None,
            used: None,
            reason: Some(NOT_READ),
        },
    }
}

/// No other system's memory is read.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn memory() -> Memory {
    Memory {
        total: None,
        used: None,
        reason: Some("memory_unsupported"),
    }
}
