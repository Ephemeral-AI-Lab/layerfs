use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use std::fs;
use std::process::Command;

pub(in crate::stage1_materialize) struct ProcessUsage {
    pub(in crate::stage1_materialize) user_ns: u64,
    pub(in crate::stage1_materialize) system_ns: u64,
    pub(in crate::stage1_materialize) maximum_rss_bytes: u64,
}

#[cfg(target_os = "macos")]
pub(in crate::stage1_materialize) fn process_usage() -> EvalResult<ProcessUsage> {
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
    Ok(ProcessUsage {
        user_ns: timeval_ns(usage.user.seconds, usage.user.microseconds)?,
        system_ns: timeval_ns(usage.system.seconds, usage.system.microseconds)?,
        maximum_rss_bytes: usage.maximum_resident_set_bytes as u64,
    })
}

#[cfg(target_os = "macos")]
pub(in crate::stage1_materialize) fn timeval_ns(
    seconds: i64,
    microseconds: i64,
) -> EvalResult<u64> {
    let seconds = u64::try_from(seconds).map_err(display_error)?;
    let microseconds = u64::try_from(microseconds).map_err(display_error)?;
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|total| total.checked_add(microseconds * 1_000))
        .ok_or_else(|| "CPU time overflow".to_owned())
}

#[cfg(not(target_os = "macos"))]
pub(in crate::stage1_materialize) fn process_usage() -> EvalResult<ProcessUsage> {
    Ok(ProcessUsage {
        user_ns: 0,
        system_ns: 0,
        maximum_rss_bytes: current_rss_bytes()?,
    })
}

pub(in crate::stage1_materialize) fn current_rss_bytes() -> EvalResult<u64> {
    let output = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("ps RSS observation failed".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .trim()
        .parse::<u64>()
        .map_err(display_error)?
        .checked_mul(1024)
        .ok_or_else(|| "RSS conversion overflow".to_owned())
}

pub(in crate::stage1_materialize) fn fd_count() -> EvalResult<u64> {
    u64::try_from(fs::read_dir("/dev/fd").map_err(io_error)?.count()).map_err(display_error)
}
