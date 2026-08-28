use super::artifact::{command_output, display_error, io_error};
use super::receipt_model::{Phase, ResourceObservation, Unavailable};
use crate::stage1_fixture::EvalResult;
use std::fs;
use std::path::Path;
use std::process::Command;
pub(crate) fn unavailable_defaults() -> Vec<Unavailable> {
    vec![
        Unavailable {
            field: "native.sync_regular_calls".to_owned(),
            availability: "Unavailable",
            reason: "product exposes only aggregate sync_calls".to_owned(),
        },
        Unavailable {
            field: "native.sync_directory_calls".to_owned(),
            availability: "Unavailable",
            reason: "product exposes only aggregate sync_calls".to_owned(),
        },
        Unavailable {
            field: "storage.rollback_journal_bytes".to_owned(),
            availability: "Unavailable",
            reason: "not continuously observed".to_owned(),
        },
        Unavailable {
            field: "storage.temporary_file_bytes".to_owned(),
            availability: "Unavailable",
            reason: "product storage observation does not expose a continuous peak".to_owned(),
        },
    ]
}
pub(crate) fn row_residual(row_wall_ns: u128, phases: &[Phase]) -> EvalResult<u128> {
    let attributed = phases.iter().try_fold(0_u128, |total, phase| {
        total
            .checked_add(phase.wall_ns)
            .ok_or_else(|| "row phase sum overflow".to_owned())
    })?;
    row_wall_ns
        .checked_sub(attributed)
        .ok_or_else(|| "row phase sum exceeds row wall".to_owned())
}
pub(crate) fn observe_row_resources(
    residue_root: Option<&Path>,
    active_store_connections: u64,
) -> EvalResult<ResourceObservation> {
    Ok(ResourceObservation {
        rss_current_bytes: None,
        rss_peak_bytes: maximum_rss_bytes()?,
        fd_current: fd_count()?,
        active_store_connections,
        child_processes: 0,
        owned_temp_entries: None,
        residue_entries: residue_root.map(residue_count).transpose()?.unwrap_or(0),
    })
}
pub(crate) fn observe_external_resources(
    residue_root: Option<&Path>,
    store: Option<&Path>,
) -> EvalResult<ResourceObservation> {
    Ok(ResourceObservation {
        rss_current_bytes: Some(current_rss_bytes()?),
        rss_peak_bytes: maximum_rss_bytes()?,
        fd_current: fd_count()?,
        active_store_connections: open_store_connection_count(store)?,
        child_processes: child_process_count()?,
        owned_temp_entries: None,
        residue_entries: residue_root.map(residue_count).transpose()?.unwrap_or(0),
    })
}
pub(crate) fn fd_count() -> EvalResult<u64> {
    Ok(fs::read_dir("/dev/fd").map_err(io_error)?.count() as u64)
}
pub(crate) fn open_store_connection_count(store: Option<&Path>) -> EvalResult<u64> {
    let Some(store) = store else {
        return Ok(0);
    };
    let store = store
        .canonicalize()
        .unwrap_or_else(|_| store.to_path_buf())
        .display()
        .to_string();
    let pid = std::process::id().to_string();
    let output = command_output("/usr/sbin/lsof", &["-Fn", "-p", &pid])?;
    Ok(output
        .lines()
        .filter(|line| {
            line.starts_with('n')
                && line.strip_prefix('n').is_some_and(|path| {
                    path.starts_with(&store)
                        && path.contains("generation-")
                        && path.ends_with(".sqlite")
                })
        })
        .count() as u64)
}
pub(crate) fn current_rss_bytes() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = command_output("/bin/ps", &["-o", "rss=", "-p", &pid])?;
    output
        .trim()
        .parse::<u64>()
        .map_err(display_error)?
        .checked_mul(1_024)
        .ok_or_else(|| "RSS conversion overflow".to_owned())
}
#[cfg(target_os = "macos")]
pub(crate) fn maximum_rss_bytes() -> EvalResult<u64> {
    use std::ffi::c_int;
    #[repr(C)]
    #[derive(Default)]
    struct TimeVal {
        seconds: i64,
        microseconds: i64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct RUsage {
        user: TimeVal,
        system: TimeVal,
        maximum_resident_set_bytes: i64,
        remaining: [i64; 13],
    }
    unsafe extern "C" {
        fn getrusage(who: c_int, usage: *mut RUsage) -> c_int;
    }
    let mut usage = RUsage::default();
    // SAFETY: usage is a live Darwin-compatible rusage buffer for this call.
    if unsafe { getrusage(0, &mut usage) } != 0 || usage.maximum_resident_set_bytes < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(usage.maximum_resident_set_bytes as u64)
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn maximum_rss_bytes() -> EvalResult<u64> {
    current_rss_bytes()
}
pub(crate) fn child_process_count() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("/usr/bin/pgrep")
        .args(["-P", &pid])
        .output()
        .map_err(io_error)?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)
            .map_err(display_error)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u64)
    } else if output.status.code() == Some(1) {
        Ok(0)
    } else {
        Err(format!("pgrep exited {}", output.status))
    }
}
pub(crate) fn residue_count(root: &Path) -> EvalResult<u64> {
    if !root.exists() {
        return Ok(0);
    }
    let mut count = 0_u64;
    let mut stack = vec![root.to_owned()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with("-journal")
                || name.ends_with("-wal")
                || name.ends_with("-shm")
                || name == "CURRENT.tmp"
                || name.starts_with(".layerfs-")
            {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| "residue count overflow".to_owned())?;
            }
            if entry.file_type().map_err(io_error)?.is_dir() {
                stack.push(path);
            }
        }
    }
    Ok(count)
}
