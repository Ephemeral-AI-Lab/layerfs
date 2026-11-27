//! `workspace.cancel` (compute-plane 2.0 §5.1): the call on the side that
//! runs the Workspace ([`call`]) and the cancel of one or every command
//! recorded for a Workspace ([`every`]). One command's cancel, a stale
//! record's end and what close and unmount do with running commands are
//! the Sandbox's (`layerfs_sandbox::exec::settle`).
use layerfs_home::outcome::refused;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::exec::id::ExecId;
use layerfs_sandbox::exec::settle::{cancel, grace, stop, unreached, Outcome, Remote, GRACE_MS};
use layerfs_sandbox::record::exec;
use std::path::Path;
use std::time::Duration;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `workspace.cancel(ws, { exec })` on the side that runs the Workspace at
/// `path`. With the Workspace's id (`workspace`) from a caller that keeps
/// the command's record (#535 G4), this side keeps none: the command's
/// group follows from the Sandbox and the ids, and the answer is that one
/// command's `{ outcome, signal? }`. Otherwise this side keeps the records:
/// the command `exec_id` names, or with none every command recorded for the
/// Workspace, is cancelled, and the answer is `{ commands: [{ exec,
/// outcome, signal }] }`. A Workspace that is not mounted is refused
/// `NotMounted`.
pub fn call(
    host: &layerfs_sandbox::route::machine::Host,
    path: Option<&str>,
    (exec_id, grace_ms): (Option<&str>, Option<i64>),
    workspace: Option<&str>,
) -> layerfs_home::outcome::Outcome {
    let id = exec_id.map(ExecId::parse).transpose()?;
    let grace = grace(grace_ms)?;
    let state = layerfs_sandbox::lifecycle::registry::sandbox(host)?;
    if let Some(workspace) = workspace {
        let id = id.ok_or_else(|| refused("InvalidInput", "a forwarded cancel names its exec"))?;
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
    let workspace = layerfs_protocol::sdk::hex(&token.workspace.to_bytes());
    let commands = every(
        &state.directory,
        &state.id,
        &workspace,
        id.as_ref(),
        &mut unreached,
    )?;
    Ok(listed_answer(&commands))
}

/// Cancels, in Sandbox `sandbox`'s folder `dir`, command `id` of Workspace
/// `workspace` or with none every command recorded for it, each once with
/// the default grace: each command's id and outcome, in record order.
pub fn every(
    dir: &Path,
    sandbox: &str,
    workspace: &str,
    id: Option<&ExecId>,
    remote: &mut Remote<'_>,
) -> Result<Vec<(String, Outcome)>> {
    let grace = Duration::from_millis(GRACE_MS as u64);
    if let Some(id) = id {
        let file = exec::file(dir, workspace, id.as_str());
        let outcome = cancel(dir, sandbox, &file, grace, remote)?;
        return Ok(vec![(id.to_string(), outcome)]);
    }
    let mut commands = Vec::new();
    for (file, found) in exec::listed(dir, Some(workspace))? {
        commands.push((
            found.id.clone(),
            cancel(dir, sandbox, &file, grace, remote)?,
        ));
    }
    Ok(commands)
}

/// `{ commands: [{ exec, outcome, signal }] }`.
pub fn listed_answer(commands: &[(String, Outcome)]) -> SdkValue {
    let rows = (commands.iter())
        .map(|(id, outcome)| {
            SdkValue::map([
                ("exec", SdkValue::text(id)),
                ("outcome", SdkValue::text(outcome.word())),
                ("signal", SdkValue::opt_text(outcome.signal())),
            ])
        })
        .collect();
    SdkValue::map([("commands", SdkValue::List(rows))])
}
