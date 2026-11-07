//! Cancel (compute-plane 2.0 §5.1, §5.4): `workspace.exec_cancel` on the
//! side that keeps the command's record ([`cancel`]), and what `sandbox.close`
//! and `workspace.unmount` do with the records of theirs ([`settle`]): refuse
//! `Busy` while one is live, or with `force` cancel each first. A record
//! whose owner is gone is stale: it is ended through its stop handle if its
//! command still runs, removed and logged ([`stale`]). Cancel is explicit
//! only: one attempt, no timeout, retry or replay. An outcome that cannot be
//! confirmed (a worker that does not answer) is `unknown`, and the record
//! stays until its owner ends or a close clears it.
use super::record::{self, ExecRecord, Route};
use crate::record::lock_in;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{log_line, now_ms, write_entity};
use layerfs_home::outcome::{failed, refused};
use layerfs_sandbox_backend::{Cancelled, Stop};
use std::path::Path;
use std::time::Duration;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `graceMs` when the caller sends none.
pub const GRACE_MS: i64 = 5000;

/// How one cancel ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Its command was ended, by this signal (`SIGTERM` or `SIGKILL`).
    Cancelled(&'static str),
    /// It had already finished.
    Ended,
    /// Not confirmed: nothing is known to have changed.
    Unknown,
}

impl Outcome {
    /// The call's answer: `{ outcome, signal? }`.
    pub fn answer(self) -> SdkValue {
        let (outcome, signal) = match self {
            Self::Cancelled(signal) => ("cancelled", Some(signal)),
            Self::Ended => ("ended", None),
            Self::Unknown => ("unknown", None),
        };
        let mut fields = vec![("outcome", SdkValue::text(outcome))];
        fields.extend(signal.map(|signal| ("signal", SdkValue::text(signal))));
        SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
    }

    /// The outcome an answer names (`unknown` for anything else).
    pub fn read(answer: &SdkValue) -> Self {
        match answer.get_text("outcome") {
            Some("cancelled") => Self::Cancelled(match answer.get_text("signal") {
                Some("SIGTERM") => "SIGTERM",
                _ => "SIGKILL",
            }),
            Some("ended") => Self::Ended,
            _ => Self::Unknown,
        }
    }
}

/// `graceMs` as sent: refused `InvalidInput` when negative.
pub fn grace(grace_ms: Option<i64>) -> Result<Duration> {
    let grace = grace_ms.unwrap_or(GRACE_MS);
    u64::try_from(grace)
        .map(Duration::from_millis)
        .map_err(|_| refused("InvalidInput", "grace_ms is not negative"))
}

/// Ends the command `stop` names here, once.
pub fn stop(stop: &Stop, grace: Duration) -> Result<Outcome> {
    match layerfs_sandbox_backend::stop::cancel(stop, grace).map_err(failed)? {
        Cancelled::Ended => Ok(Outcome::Ended),
        Cancelled::Signalled(signal) => Ok(Outcome::Cancelled(signal.as_str())),
    }
}

/// How a record whose route is a machine is cancelled: there, through its
/// worker (D-11), by the caller that keeps the record.
pub type Remote<'a> = dyn FnMut(&ExecRecord, Duration) -> Outcome + 'a;

/// Cancels the command recorded at `file` in Sandbox `sandbox`'s folder
/// `dir`: a record that is gone is `Ended`; a stale one is ended and
/// removed ([`stale`]); a live one is marked under the Sandbox lock, then
/// ended through its stop handle here, or through `remote` on a machine.
/// One whose stop handle is not known yet is ended by its owner at spawn.
pub fn cancel(
    dir: &Path,
    sandbox: &str,
    file: &Path,
    grace: Duration,
    remote: &mut Remote<'_>,
) -> Result<Outcome> {
    let record = {
        let _sandbox = lock_in(dir, sandbox)?;
        let Some(mut record) = record::read(file)? else {
            return Ok(Outcome::Ended);
        };
        if !record.owned() {
            return stale(dir, file, &record);
        }
        record.cancel = Some(now_ms());
        write_entity(file, &record.encode())?;
        record
    };
    match (&record.route, &record.stop) {
        (Route::Machine(_), _) => Ok(remote(&record, grace)),
        (Route::Local, Some(handle)) => stop(handle, grace),
        (Route::Local, None) => Ok(Outcome::Cancelled("SIGKILL")),
    }
}

/// Ends stale record `found` at `file` (its owner is gone) through its stop
/// handle if its command still runs here, removes it and logs it in the
/// side's `log/exec.jsonl`. The caller holds the Sandbox lock. A command on
/// a machine is not reached from here: its record goes, and the log says
/// so.
pub fn stale(dir: &Path, file: &Path, found: &ExecRecord) -> Result<Outcome> {
    let outcome = match (&found.route, &found.stop) {
        (Route::Local, Some(handle)) => stop(handle, Duration::ZERO)?,
        (Route::Local, None) => Outcome::Ended,
        (Route::Machine(_), _) => Outcome::Unknown,
    };
    record::remove(file)?;
    if let Some(root) = dir.parent().and_then(Path::parent) {
        let sandbox = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let _ = log_line(
            root,
            "exec.jsonl",
            &SdkValue::map([
                ("at", SdkValue::Int(now_ms())),
                ("stale", SdkValue::Bool(true)),
                ("sandbox", SdkValue::text(sandbox)),
                ("workspace", SdkValue::text(&found.workspace)),
                ("exec", SdkValue::text(&found.id)),
                ("owner", found.owner.clone()),
                ("ended", outcome.answer()),
            ]),
        );
    }
    Ok(outcome)
}

/// What a `sandbox.close` (`workspace` `None`) or a `workspace.unmount` of
/// Workspace `workspace` does with the command records in Sandbox
/// `sandbox`'s folder `dir`: stale ones are removed ([`stale`]); live ones
/// refuse it `Busy`, or with `force` are each cancelled first, with the
/// default grace. A cancel that is not confirmed refuses it `Unknown`.
pub fn settle(
    dir: &Path,
    sandbox: &str,
    workspace: Option<&str>,
    force: bool,
    remote: &mut Remote<'_>,
) -> Result<()> {
    let mut live = Vec::new();
    for (file, found) in record::listed(dir, workspace)? {
        if found.owned() {
            live.push((file, found));
            continue;
        }
        let _sandbox = lock_in(dir, sandbox)?;
        match record::read(&file)? {
            Some(now) if !now.owned() => {
                stale(dir, &file, &now)?;
            }
            Some(now) => live.push((file, now)),
            None => {}
        }
    }
    if live.is_empty() {
        return Ok(());
    }
    let ids = || SdkValue::List(live.iter().map(|(_, r)| SdkValue::text(&r.id)).collect());
    let whose = match workspace {
        Some(_) => format!("Workspace {sandbox}:{}", live[0].1.path),
        None => format!("Sandbox {sandbox}"),
    };
    if !force {
        return Err(SdkRefusal {
            code: "Busy".to_owned(),
            message: format!(
                "{whose} still runs {} command(s): cancel them with workspace.exec_cancel, or \
                 pass force: true to cancel them first",
                live.len()
            ),
            detail: SdkValue::map([("execs", ids())]),
        });
    }
    let grace = Duration::from_millis(GRACE_MS as u64);
    let mut unconfirmed = Vec::new();
    for (file, found) in &live {
        if cancel(dir, sandbox, file, grace, remote)? == Outcome::Unknown {
            unconfirmed.push(SdkValue::text(&found.id));
        }
    }
    if unconfirmed.is_empty() {
        return Ok(());
    }
    Err(SdkRefusal {
        code: "Unknown".to_owned(),
        message: format!(
            "{whose}: the cancel of {} command(s) was not confirmed; nothing more was done",
            unconfirmed.len()
        ),
        detail: SdkValue::map([("execs", SdkValue::List(unconfirmed))]),
    })
}

/// [`Remote`] on a side that reaches no worker: never confirmed.
pub fn unreached(_: &ExecRecord, _: Duration) -> Outcome {
    Outcome::Unknown
}

/// `workspace.exec_cancel(ws, exec_id, { grace_ms })` on the side that runs
/// the Workspace at `path`: `{ outcome, signal? }`. With the Workspace's id
/// (`workspace`) from a caller that keeps the command's record (#535 G4),
/// this side keeps none: the command's group follows from the Sandbox and
/// the ids. A Workspace that is not mounted is refused `NotMounted`.
pub fn call(
    host: &crate::host::Host,
    path: Option<&str>,
    (exec_id, grace_ms): (Option<&str>, Option<i64>),
    workspace: Option<&str>,
) -> layerfs_home::outcome::Outcome {
    let id = super::id::ExecId::parse(layerfs_home::helpers::required(exec_id, "exec_id")?)?;
    let grace = grace(grace_ms)?;
    let state = crate::registry::sandbox(host)?;
    if let Some(workspace) = workspace {
        let handle = state.runtime()?.exec_stop(workspace, id.as_str());
        let handle = handle.ok_or_else(|| {
            refused(
                "Unsupported",
                "a command of this Sandbox is cancelled by the caller that keeps its record",
            )
        })?;
        return Ok(stop(&handle, grace)?.answer());
    }
    let path = layerfs_home::helpers::required(path, "path")?;
    let token = state.token(path)?;
    let workspace = layerfs_bridge::sdk_values::hex(&token.workspace.to_bytes());
    let file = record::file(&state.directory, &workspace, id.as_str());
    let outcome = cancel(&state.directory, &state.id, &file, grace, &mut unreached)?;
    Ok(outcome.answer())
}
