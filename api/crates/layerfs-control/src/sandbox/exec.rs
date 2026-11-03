//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs `bash -c <command>` as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. A local Sandbox
//! (`layerfs_sandbox::Sandbox::HostProcess`) runs the same commands as
//! processes of this kernel.
use crate::common::outcome::Outcome;
use crate::sandbox::output::Spool;
use crate::sandbox::registry::{sandbox, text, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline. Its home is
/// the Sandbox's scratch directory (`/tmp` in a container), since the
/// command identity has no account there.
fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    crate::sandbox::runtime::run(
        &state.runtime,
        state.identity,
        arguments,
        vec![format!(
            "HOME={}",
            crate::sandbox::runtime::scratch(&state.runtime)
        )],
        directory,
        spool,
    )
}

/// `workspace.exec(ws, command, { spool? })`.
///
/// A foreground result is `kind`, `exitCode`, `stdout`, `stderr`, `message`
/// and `truncated` (always false: no output is cut). A stream over the
/// inline window (1 MiB), or any non-empty stream with `spool`, is null
/// here and spooled; the result then also names its `output`, read with
/// `workspace.exec_output` and closed with `sandbox.exec_release`, and the
/// `stdoutBytes` and `stderrBytes` sizes.
pub(crate) fn exec(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    let command = text(args, "command")?;
    let directory = state.directory(path)?;
    let (code, stdout, stderr) = run(
        &state,
        &directory,
        vec!["bash".into(), "-c".into(), command.into()],
        args.get_bool("spool"),
    )?;
    let mut streams = crate::sandbox::output::fields(stdout, stderr)?.into_iter();
    let mut result = vec![
        ("kind", SdkValue::Text(format!("exit:{code}"))),
        ("exitCode", SdkValue::Int(i64::from(code))),
    ];
    result.extend(streams.by_ref().take(2));
    result.push(("message", SdkValue::opt_text(args.get_text("message"))));
    result.push(("truncated", SdkValue::Bool(false)));
    result.extend(streams);
    Ok(SdkValue::Map(
        result
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}
