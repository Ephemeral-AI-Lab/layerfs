//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs `bash -c <command>` as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. A local Sandbox
//! (`layerfs_sandbox_backend::Sandbox::HostProcess`) runs the same commands as
//! processes of this kernel.
use crate::host::Host;
use crate::output::Spool;
use crate::registry::{sandbox, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers::required;
use layerfs_home::outcome::Outcome;

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline. Its home is
/// the Sandbox's scratch directory, since the
/// command identity has no account there.
fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let runtime = state.runtime()?;
    crate::runtime::run(
        runtime,
        state.identity,
        arguments,
        vec![format!("HOME={}", crate::runtime::scratch(runtime))],
        directory,
        spool,
    )
}

/// `workspace.exec(ws, command, { spool? })` on the Workspace at `path`
/// (a `directory` a caller checked, and the `command`).
///
/// A foreground result is `kind`, `exitCode`, `stdout`, `stderr`, `message`
/// and `truncated` (always false: no output is cut). A stream over the
/// inline window (1 MiB), or any non-empty stream with `spool`, is null
/// here and spooled; the result then also names its `output`, read with
/// `workspace.exec_output` and closed with `sandbox.exec_release`, and the
/// `stdoutBytes` and `stderrBytes` sizes. A `directory` from a caller that
/// keeps the Workspace's record (#535 G4) is the one the command runs in.
pub fn exec(
    host: &Host,
    path: Option<&str>,
    (directory, command): (Option<&str>, Option<&str>),
    spool: bool,
) -> Outcome {
    let state = sandbox(host)?;
    let path = required(path, "path")?;
    let command = required(command, "command")?;
    // a caller that reaches the Sandbox's daemon directly keeps its
    // Workspace records and sends the directory it checked there (#535 G4)
    let directory = match directory {
        Some(directory) => directory.to_owned(),
        None => state.directory(path)?,
    };
    let (code, stdout, stderr) = run(
        &state,
        &directory,
        vec!["bash".into(), "-c".into(), command.into()],
        spool,
    )?;
    let mut streams = crate::output::fields(stdout, stderr)?.into_iter();
    let mut result = vec![
        ("kind", SdkValue::Text(format!("exit:{code}"))),
        ("exitCode", SdkValue::Int(i64::from(code))),
    ];
    result.extend(streams.by_ref().take(2));
    result.push((
        "message",
        SdkValue::opt_text(host.message.text().map(str::to_owned)),
    ));
    result.push(("truncated", SdkValue::Bool(false)));
    result.extend(streams);
    Ok(SdkValue::Map(
        result
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}
