//! Workspace-scoped unmount ownership; Sandbox discovery is held only at
//! admission and shared-record cleanup, never during the daemon's helper wait.
//! Mount paths are unique within one daemon lifetime; Sandbox removal clears
//! the terminal records before that directory can be reused.
use crate::{
    discovery,
    job::{failed, refused, Outcome},
    registry::SandboxState,
};
use layerfs_bridge::control::{SdkRefusal, SdkValue, WorkspaceToken};
use std::{
    fs::{File, OpenOptions},
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_SH: i32 = 1;
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK_UN: i32 = 8;
fn millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
fn record(state: &SandboxState, token: WorkspaceToken) -> PathBuf {
    discovery::workspace_file(state, token).with_extension("unmount")
}
struct Lease(File);
impl Drop for Lease {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}
fn lease(path: &std::path::Path, operation: i32) -> Result<Option<Lease>, SdkRefusal> {
    #[cfg(target_os = "macos")]
    let no_follow = 0x100;
    #[cfg(target_os = "linux")]
    let no_follow = 0x20000;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(no_follow)
        .open(path)
        .map_err(failed)?;
    discovery::private(path, false)?;
    if unsafe { flock(file.as_raw_fd(), operation | LOCK_NB) } == 0 {
        return Ok(Some(Lease(file)));
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(failed(error))
    }
}
/// Explicit unlock matters when a concurrent fork inherited the descriptor.
pub(crate) struct Attempt {
    _lease: Lease,
    path: PathBuf,
    started: i64,
}
fn result(value: &SdkValue) -> Outcome {
    if value.get_text("state") == Some("Unmounted") {
        return Ok(SdkValue::map([("kind", SdkValue::text("Unmounted"))]));
    }
    let detail = SdkValue::map([
        ("state", SdkValue::text("Retained")),
        (
            "elapsedMs",
            SdkValue::Int(
                millis()
                    .saturating_sub(value.get_int("startedMs").unwrap_or(millis()))
                    .max(0),
            ),
        ),
        ("original", value.get("detail").cloned().unwrap_or_default()),
    ]);
    Err(SdkRefusal {
        code: value.get_text("code").unwrap_or("Unknown").into(),
        message: value
            .get_text("message")
            .unwrap_or("unmount outcome retained")
            .into(),
        detail,
    })
}
fn progress(value: &SdkValue, active: bool) -> SdkRefusal {
    let elapsed = millis()
        .saturating_sub(value.get_int("startedMs").unwrap_or(millis()))
        .max(0);
    SdkRefusal {
        code: if active { "Busy" } else { "Unknown" }.into(),
        message: if active {
            format!("Workspace is Unmounting; waited {elapsed}ms")
        } else {
            format!("unmount caller ended without publishing its result after {elapsed}ms; inspect daemon custody, do not resend")
        },
        detail: SdkValue::map([
            (
                "state",
                SdkValue::text(if active { "Unmounting" } else { "Retained" }),
            ),
            ("elapsedMs", SdkValue::Int(elapsed)),
        ]),
    }
}
fn observed(path: &std::path::Path) -> Result<Option<SdkValue>, SdkRefusal> {
    match discovery::load(path) {
        Ok(value) => Ok(Some(value)),
        Err(_) if !path.exists() => Ok(None),
        Err(error) => Err(error),
    }
}
/// Consulted before token lookup, including when another process removed
/// the Workspace record. Does not acquire the Sandbox guard or dial daemon.
fn inspect(path: &std::path::Path) -> Result<Option<SdkValue>, SdkRefusal> {
    let Some(value) = observed(path)? else {
        return Ok(None);
    };
    if value.get_text("state") != Some("Unmounting") {
        return Ok(Some(value));
    }
    let available = lease(&path.with_extension("unmount-lock"), LOCK_SH)?;
    // Publication may have completed between our read and lease acquisition.
    let latest = observed(path)?;
    match latest {
        Some(latest) if latest.get_text("state") != Some("Unmounting") => Ok(Some(latest)),
        None => Ok(None),
        Some(_) if available.is_none() => Err(progress(&value, true)),
        Some(_) => {
            let error = progress(&value, false);
            Ok(Some(SdkValue::map([
                ("state", SdkValue::text("Retained")),
                (
                    "startedMs",
                    value.get("startedMs").cloned().unwrap_or_default(),
                ),
                ("path", value.get("path").cloned().unwrap_or_default()),
                ("code", SdkValue::text(error.code)),
                ("message", SdkValue::text(error.message)),
                ("detail", error.detail),
            ])))
        }
    }
}
pub(crate) fn check(state: &SandboxState, path: &str) -> Result<(), SdkRefusal> {
    if let Some(value) = find(state, path)? {
        if value.get_text("state") == Some("Unmounted") {
            return Err(refused("NotMounted", "Workspace already unmounted"));
        }
        return result(&value).map(drop);
    }
    Ok(())
}
pub(crate) fn find(state: &SandboxState, path: &str) -> Result<Option<SdkValue>, SdkRefusal> {
    // Steady-state token/directory lookups already have a token: avoid a
    // directory scan proportional to the lifetime's completed unmounts.
    let known = crate::registry::lock(&state.mounts)?
        .get(path)
        .map(|mount| mount.workspace.bound.token);
    if let Some(token) = known {
        return inspect(&record(state, token));
    }
    for entry in std::fs::read_dir(&state.directory).map_err(failed)? {
        let entry = entry.map_err(failed)?;
        if entry.path().extension().is_some_and(|ext| ext == "unmount") {
            if let Some(value) = observed(&entry.path())? {
                if value.get_text("path") == Some(path) {
                    return inspect(&entry.path());
                }
            }
        }
    }
    Ok(None)
}
/// Closing a running Sandbox may not race a registered terminal operation.
pub(crate) fn check_all(state: &SandboxState) -> Result<Vec<String>, SdkRefusal> {
    let mut recoverable = Vec::new();
    for entry in std::fs::read_dir(&state.directory).map_err(failed)? {
        let entry = entry.map_err(failed)?;
        if entry.path().extension().is_some_and(|ext| ext == "unmount") {
            // Even a published result may still be cleaning its Workspace
            // record. Close must not delete/reuse the Sandbox directory in
            // that short interval while the original lease is held.
            let probe = lease(&entry.path().with_extension("unmount-lock"), LOCK_SH)?;
            if probe.is_none() {
                if let Some(value) = observed(&entry.path())? {
                    return Err(progress(&value, true));
                }
            }
            if let Some(value) = inspect(&entry.path())? {
                if value.get_text("state") == Some("Retained") {
                    recoverable.push(crate::registry::text(&value, "path")?.to_owned());
                }
            }
        }
    }
    Ok(recoverable)
}
/// Called under the Sandbox guard, after all before-effect checks.
pub(crate) fn begin(
    state: &SandboxState,
    token: WorkspaceToken,
    path: &str,
) -> Result<Attempt, SdkRefusal> {
    let path_record = record(state, token);
    let file = lease(&path_record.with_extension("unmount-lock"), LOCK_EX)?
        .ok_or_else(|| refused("Busy", "Workspace is Unmounting"))?;
    let attempt = Attempt {
        _lease: file,
        path: path_record,
        started: millis(),
    };
    discovery::save(
        &attempt.path,
        SdkValue::map([
            ("state", SdkValue::text("Unmounting")),
            ("startedMs", SdkValue::Int(attempt.started)),
            ("path", SdkValue::text(path)),
        ]),
    )?;
    Ok(attempt)
}
/// Publish while the Workspace lease prevents another terminal request.
/// No Sandbox guard is needed to replace this attempt's own result file.
pub(crate) fn finish(attempt: &Attempt, path: &str, outcome: &Outcome) -> Result<(), SdkRefusal> {
    match outcome {
        Err(error) if matches!(error.code.as_str(), "Busy" | "Dirty") => {
            std::fs::remove_file(&attempt.path).map_err(failed)?;
        }
        _ => {
            let (state, code, message, detail) = match outcome {
                Ok(_) => ("Unmounted", "", "", SdkValue::Null),
                Err(error) => (
                    "Retained",
                    error.code.as_str(),
                    error.message.as_str(),
                    error.detail.clone(),
                ),
            };
            discovery::save(
                &attempt.path,
                SdkValue::map([
                    ("state", SdkValue::text(state)),
                    ("startedMs", SdkValue::Int(attempt.started)),
                    ("path", SdkValue::text(path)),
                    ("code", SdkValue::text(code)),
                    ("message", SdkValue::text(message)),
                    ("detail", detail),
                ]),
            )?;
        }
    }
    Ok(())
}
pub(crate) fn cancel(attempt: &Attempt) -> Result<(), SdkRefusal> {
    std::fs::remove_file(&attempt.path).map_err(failed)
}
pub(crate) fn cleanup_failure(mut outcome: Outcome, error: SdkRefusal) -> Outcome {
    // Do not replace a known daemon success with a cleanup Busy/Failed.
    match &mut outcome {
        Ok(SdkValue::Map(fields)) => {
            fields.push(("cleanupError".into(), SdkValue::text(error.message)));
        }
        Err(original) => {
            original.detail = SdkValue::map([
                ("original", original.detail.clone()),
                ("cleanupError", SdkValue::text(error.message)),
            ]);
        }
        _ => {}
    }
    outcome
}
pub(crate) fn repeat(value: &SdkValue) -> Outcome {
    result(value)
}
