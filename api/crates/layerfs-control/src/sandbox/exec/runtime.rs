//! This Sandbox's runtime behind the two calls the exec modules make: `run`
//! for a command whose output is the result, `capture` for a short probe
//! whose whole stdout is its answer. Both run with the Sandbox's command
//! identity and the scratch directory as home, since that identity has no
//! account in a container.
use crate::sandbox::output::Spool;
use crate::sandbox::registry::SandboxState;
use layerfs_bridge::control::SdkRefusal;

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline.
pub(super) fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let runtime = state.runtime()?;
    crate::sandbox::runtime::run(
        runtime,
        state.identity,
        arguments,
        vec![format!(
            "HOME={}",
            crate::sandbox::runtime::scratch(runtime)
        )],
        directory,
        spool,
    )
}

/// [`run`] of a short probe whose whole stdout is its answer.
pub(super) fn capture(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
) -> Result<(i32, String), SdkRefusal> {
    let runtime = state.runtime()?;
    let (code, stdout, _) = crate::sandbox::runtime::capture(
        runtime,
        state.identity,
        arguments,
        vec![format!(
            "HOME={}",
            crate::sandbox::runtime::scratch(runtime)
        )],
        directory,
    )?;
    Ok((code, String::from_utf8_lossy(&stdout).into_owned()))
}
