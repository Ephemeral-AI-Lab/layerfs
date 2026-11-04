//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs the command as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. A local Sandbox
//! (`layerfs_sandbox::Sandbox::HostProcess`) runs the same commands as
//! processes of this kernel.
//!
//! A foreground command writes a private record in the Sandbox's `/tmp`
//! naming its Workspace, its pid and that pid's start time before it runs, so
//! a later call can find and stop an orphan whose first client is gone. The
//! executor owns command launch and cancellation (AGENTS.md), so the record
//! is written where the command runs, by the same runtime owner: `bash -c`
//! under util-linux `setsid --wait`, and `/proc` read inside the Sandbox. A
//! backend without `/proc` (the macOS host process) runs the command plainly
//! and leaves no record; nothing else changes.
//!
//! This module is the orchestration: the two entrypoints and the result they
//! shape. [`record`] owns the foreground record and the id it carries,
//! [`holders`] finds and stops the processes a record names, and [`runtime`]
//! adapts this Sandbox's runtime to the run and capture calls they make.
use crate::common::outcome::Outcome;
use crate::sandbox::registry::{sandbox, text};
use layerfs_bridge::control::SdkValue;

mod holders;
mod record;
mod runtime;

pub(crate) use holders::busy;

/// `workspace.exec(ws, command, { spool? })`.
///
/// A foreground result is `kind`, `exitCode`, `stdout`, `stderr`, `message`
/// and `truncated` (always false: no output is cut). A stream over the
/// inline window (1 MiB), or any non-empty stream with `spool`, is null
/// here and spooled; the result then also names its `output`, read with
/// `workspace.exec_output` and closed with `sandbox.exec_release`, and the
/// `stdoutBytes` and `stderrBytes` sizes. A `directory` from a caller that
/// keeps the Workspace's record (#535 G4) is the one the command runs in.
pub(crate) fn exec(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    let command = text(args, "command")?;
    // a caller that reaches the Sandbox's daemon directly keeps its
    // Workspace records and sends the directory it checked there (#535 G4)
    let directory = match args.get_text("directory") {
        Some(directory) => directory.to_owned(),
        None => state.directory(path)?,
    };
    let arguments = record::foreground_arguments(&state, command)?;
    let (code, stdout, stderr) =
        runtime::run(&state, &directory, arguments, args.get_bool("spool"))?;
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

/// `workspace.holders(ws)`: the foreground execs still running in a
/// Workspace, read, or cancelled first with `kill`. This op runs where the
/// Sandbox's runtime is (its machine's worker, or this process), so a caller
/// that reaches only its daemon's forwarded port can still cancel an orphan.
/// Answers `{ processes }`; with `kill`, each own process group gets TERM and
/// then KILL, and a group that reaches outside the Workspace answers `Busy`.
pub(crate) fn holders(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    if args.get_bool("kill") {
        holders::cancel(&state, path)?;
    }
    let records = record::foreground_in(&state, path)?;
    Ok(SdkValue::map([
        ("path", SdkValue::text(path)),
        (
            "processes",
            SdkValue::List(
                records
                    .into_iter()
                    .map(|record| SdkValue::Text(record.id))
                    .collect(),
            ),
        ),
    ]))
}
