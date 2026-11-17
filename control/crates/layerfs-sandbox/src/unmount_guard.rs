//! Workspace-scoped unmount ownership, recorded beside the Workspace's
//! record (`workspace/<id>.unmount`); the Sandbox's guard is held only at
//! admission and shared-record cleanup, never during the daemon's helper wait.
//! Mount paths are unique within one daemon lifetime; Sandbox removal clears
//! the terminal records before that directory can be reused.
//!
//! The record is JSON in the version its `v` field names.
//!
//! - Version 0 (no `v`) has `state`, `startedMs`, `path`, `code`, `message`
//!   and `detail`, and nothing else. It predates any forced attempt.
//! - Version 1 ([`VERSION`]) adds `forced`, whether this attempt was started
//!   with `force`, and — in a published result — `stage`, the daemon's own
//!   retained teardown stage (`Null` when the failure never got a daemon
//!   answer). Both are extra fields: a version-0 reader reads the version-0
//!   fields unchanged and ignores them, which is what
//!   `tests/unmount_record.rs` checks.
//! - A record whose `v` is above [`VERSION`] is answered as it stands and is
//!   never forceable: the only step that could destroy unknown custody is the
//!   one this version does not take. Every reader keeps answering version-0
//!   records exactly as before.
use crate::files as discovery;
use crate::registry::SandboxState;
use layerfs_bridge::control::{SdkRefusal, SdkValue, WorkspaceToken};
use layerfs_home::outcome::{failed, refused, Outcome};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// The record version this build writes and the newest one it forces.
const VERSION: i64 = 1;

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
        layerfs_home::home::flock::unlock(&self.0);
    }
}
fn lease(path: &std::path::Path, exclusive: bool) -> Result<Option<Lease>, SdkRefusal> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(failed)?;
    layerfs_store::project::discovery::private(path, false)?;
    if layerfs_home::home::flock::try_lock(&file, exclusive).map_err(failed)? {
        Ok(Some(Lease(file)))
    } else {
        Ok(None)
    }
}
/// Explicit unlock matters when a concurrent fork inherited the descriptor.
pub struct Attempt {
    _lease: Lease,
    path: PathBuf,
    started: i64,
    forced: bool,
    /// The record this attempt replaced, put back when the attempt has no
    /// effect of its own (see [`restore`]).
    prior: Option<SdkValue>,
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
    match layerfs_store::project::discovery::load(path) {
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
    let available = lease(&path.with_extension("unmount-lock"), false)?;
    // Publication may have completed between our read and lease acquisition.
    let latest = observed(path)?;
    match latest {
        Some(latest) if latest.get_text("state") != Some("Unmounting") => Ok(Some(latest)),
        None => Ok(None),
        Some(_) if available.is_none() => Err(progress(&value, true)),
        Some(_) => {
            let error = progress(&value, false);
            Ok(Some(SdkValue::map([
                ("v", value.get("v").cloned().unwrap_or_default()),
                ("state", SdkValue::text("Retained")),
                (
                    "startedMs",
                    value.get("startedMs").cloned().unwrap_or_default(),
                ),
                ("path", value.get("path").cloned().unwrap_or_default()),
                (
                    "forced",
                    value
                        .get("forced")
                        .cloned()
                        .unwrap_or(SdkValue::Bool(false)),
                ),
                ("code", SdkValue::text(error.code)),
                ("message", SdkValue::text(error.message)),
                ("detail", error.detail),
            ])))
        }
    }
}
/// The entries of the Sandbox's `workspace/` folder; none before its first
/// Workspace.
fn unmounts(state: &SandboxState) -> Result<Vec<std::io::Result<std::fs::DirEntry>>, SdkRefusal> {
    unmounts_in(&state.directory)
}
/// [`unmounts`] of Sandbox folder `directory`.
fn unmounts_in(
    directory: &std::path::Path,
) -> Result<Vec<std::io::Result<std::fs::DirEntry>>, SdkRefusal> {
    match std::fs::read_dir(directory.join("workspace")) {
        Ok(entries) => Ok(entries.collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(failed(error)),
    }
}
pub fn check(state: &SandboxState, path: &str) -> Result<(), SdkRefusal> {
    if let Some(value) = find(state, path)? {
        if value.get_text("state") == Some("Unmounted") {
            return Err(refused("NotMounted", "Workspace already unmounted"));
        }
        return result(&value).map(drop);
    }
    Ok(())
}
pub fn find(state: &SandboxState, path: &str) -> Result<Option<SdkValue>, SdkRefusal> {
    // Steady-state token/directory lookups already have a token: avoid a
    // directory scan proportional to the lifetime's completed unmounts.
    let known = layerfs_home::helpers::lock(&state.mounts)?
        .get(path)
        .map(|mount| mount.workspace.bound.token);
    if let Some(token) = known {
        return inspect(&record(state, token));
    }
    find_in(&state.directory, path)
}
/// [`check`] in Sandbox folder `directory`, with no live Sandbox: a caller
/// that sends the Workspace's command to its machine (#535 G4).
pub fn check_in(directory: &std::path::Path, path: &str) -> Result<(), SdkRefusal> {
    if let Some(value) = find_in(directory, path)? {
        if value.get_text("state") == Some("Unmounted") {
            return Err(refused("NotMounted", "Workspace already unmounted"));
        }
        return result(&value).map(drop);
    }
    Ok(())
}
/// [`find`] by scanning Sandbox folder `directory`'s unmount records.
pub fn find_in(directory: &std::path::Path, path: &str) -> Result<Option<SdkValue>, SdkRefusal> {
    for entry in unmounts_in(directory)? {
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
pub fn check_all(state: &SandboxState) -> Result<Vec<String>, SdkRefusal> {
    let mut recoverable = Vec::new();
    for entry in unmounts(state)? {
        let entry = entry.map_err(failed)?;
        if entry.path().extension().is_some_and(|ext| ext == "unmount") {
            // Even a published result may still be cleaning its Workspace
            // record. Close must not delete/reuse the Sandbox directory in
            // that short interval while the original lease is held.
            let probe = lease(&entry.path().with_extension("unmount-lock"), false)?;
            if probe.is_none() {
                if let Some(value) = observed(&entry.path())? {
                    return Err(progress(&value, true));
                }
            }
            if let Some(value) = inspect(&entry.path())? {
                if value.get_text("state") == Some("Retained") {
                    recoverable.push(layerfs_home::helpers::text(&value, "path")?.to_owned());
                }
            }
        }
    }
    Ok(recoverable)
}
pub fn forceable(value: &SdkValue) -> bool {
    // Only a record the daemon itself answered `Retained` at `Detach` may be
    // replaced by `force`: it holds an export whose client unmount was not
    // established, and the one forced client unmount is what finishes it. The
    // daemon's own guard remains the real gate, and a refusal answers with
    // the record it holds. A record that is still `Unmounting` (its caller
    // ended without publishing), one the machine wrote after a send or
    // connect failure or a refusal before any effect (code `Failed` without
    // the daemon's `stage`), one kept at another stage, one already forced,
    // and a record written before this field existed keep their answer.
    value.get_text("state") == Some("Retained")
        && value.get_int("v") == Some(VERSION)
        && value.get("forced") == Some(&SdkValue::Bool(false))
        && value.get_text("code") == Some("Failed")
        && value.get_text("stage") == Some(FORCEABLE_STAGE)
}
/// The daemon's retained stage one forced client unmount can still finish:
/// an NFS export kept because its client unmount was not established
/// (`ExportKept::forceable` in the daemon). Any other stage keeps its answer.
const FORCEABLE_STAGE: &str = "Detach";

/// Called under the Sandbox guard, after all before-effect checks.
pub fn begin(
    state: &SandboxState,
    token: WorkspaceToken,
    path: &str,
    forced: bool,
) -> Result<Attempt, SdkRefusal> {
    begin_at(record(state, token), path, forced)
}
/// [`begin`] at the record file `path_record` (a Workspace's
/// `workspace/<id>.unmount`), holding its lease and replacing the record it
/// found with this attempt's `Unmounting` record.
pub fn begin_at(path_record: PathBuf, path: &str, forced: bool) -> Result<Attempt, SdkRefusal> {
    let file = lease(&path_record.with_extension("unmount-lock"), true)?
        .ok_or_else(|| refused("Busy", "Workspace is Unmounting"))?;
    let attempt = Attempt {
        _lease: file,
        prior: observed(&path_record)?,
        path: path_record,
        started: millis(),
        forced,
    };
    layerfs_store::project::discovery::save(
        &attempt.path,
        SdkValue::map([
            ("v", SdkValue::Int(VERSION)),
            ("state", SdkValue::text("Unmounting")),
            ("startedMs", SdkValue::Int(attempt.started)),
            ("path", SdkValue::text(path)),
            ("forced", SdkValue::Bool(attempt.forced)),
        ]),
    )?;
    Ok(attempt)
}
/// Undo this attempt's own record: what it replaced comes back, or the file
/// goes when there was none. A forced attempt admitted over an earlier record
/// that answers `Busy` or `Dirty` changed nothing, so the earlier record —
/// possibly one that says "do not resend" — must not be lost with it. A call
/// that sent nothing is the same case.
fn restore(attempt: &Attempt) -> Result<(), SdkRefusal> {
    match &attempt.prior {
        Some(prior) => layerfs_store::project::discovery::save(&attempt.path, prior.clone()),
        None => std::fs::remove_file(&attempt.path).map_err(failed),
    }
}
/// Publish while the Workspace lease prevents another terminal request.
/// No Sandbox guard is needed to replace this attempt's own result file.
pub fn finish(attempt: &Attempt, path: &str, outcome: &Outcome) -> Result<(), SdkRefusal> {
    match outcome {
        Err(error) if matches!(error.code.as_str(), "Busy" | "Dirty") => restore(attempt)?,
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
            // The daemon's own retained stage, when it answered one; `Null`
            // for a failure that never got that answer (see [`forceable`]).
            let stage = match outcome {
                Ok(_) => SdkValue::Null,
                Err(error) => error.detail.get("stage").cloned().unwrap_or(SdkValue::Null),
            };
            layerfs_store::project::discovery::save(
                &attempt.path,
                SdkValue::map([
                    ("v", SdkValue::Int(VERSION)),
                    ("state", SdkValue::text(state)),
                    ("startedMs", SdkValue::Int(attempt.started)),
                    ("path", SdkValue::text(path)),
                    ("forced", SdkValue::Bool(attempt.forced)),
                    ("stage", stage),
                    ("code", SdkValue::text(code)),
                    ("message", SdkValue::text(message)),
                    ("detail", detail),
                ]),
            )?;
        }
    }
    Ok(())
}
pub fn cancel(attempt: &Attempt) -> Result<(), SdkRefusal> {
    restore(attempt)
}
pub fn cleanup_failure(mut outcome: Outcome, error: SdkRefusal) -> Outcome {
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
pub fn repeat(value: &SdkValue) -> Outcome {
    result(value)
}
