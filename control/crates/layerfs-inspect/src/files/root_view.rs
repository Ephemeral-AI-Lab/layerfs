//! `sandbox.files`, Root view (layerfs-project#178 step 5): what a
//! namespaces Sandbox's commands see as `/`, read through the root of the
//! Sandbox's init process.
//!
//! The Sandbox's daemon record names that process (id, start time, boot).
//! Its root and its mount namespace's table are opened through one
//! descriptor of its `/proc` entry (`confined::root_of`), and the process
//! behind that descriptor must be the recorded one and still run, checked
//! after the root is open (security rule 4): a Sandbox that has stopped,
//! or whose process id now names another process, is `Unavailable`. Its
//! home stays readable then, as a folder on disk (`files::sandbox_page`).
//!
//! A path is then read as `machine.files` reads from `/`, with the rules
//! of a Sandbox: links resolve inside the root, the last name is never
//! followed, `/proc`, `/sys` and `/dev` are `system`, a mount is entered
//! only when its type is a local disk's or memory's (so the private `/tmp`
//! and `/.layerfs/runtime` are ordinary folders, and no Workspace mount is
//! entered), and what was opened must be in such a mount by its mount id.
//! There is no world-readable rule: the Sandbox's files are its own. Its
//! run folder and a bound Store's folder are in the root at their own
//! paths, where `roots::kept` restricts what it does everywhere, and where
//! the kernel says an open ended is judged too, so a link a command made
//! does not rename them. A file with several names is read unless it is in
//! the run folder and not the command user's: an image's own files keep
//! their links.
use super::roots;
use super::walk::{answer, refusal, FilesRequest, Rules, Scope, CALL};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::backend::confined::{self, Refused, View};
use layerfs_sandbox::inspect::process;
use layerfs_sandbox::record::DaemonRecord;
use std::path::Path;

/// The entries of a Sandbox's root that are the kernel's own.
const SYSTEM: [&str; 3] = ["proc", "sys", "dev"];

/// A Sandbox with a root of its own, on the side that runs it.
pub(super) struct Sandbox<'a> {
    pub(super) name: &'a str,
    /// Its run folder, which is in its root at this same path.
    pub(super) run: &'a Path,
    pub(super) daemon: &'a DaemonRecord,
}

/// One page of the folder, or one window of the file, at `path` in the
/// root of `sandbox`, on the side whose root is `root`.
pub(super) fn page(
    root: &Path,
    sandbox: &Sandbox<'_>,
    path: &Path,
    request: &FilesRequest,
    sent: Option<&SdkValue>,
    spent: &dyn Fn() -> bool,
) -> Outcome {
    let name = sandbox.name;
    let stopped = |why: &str| {
        refused(
            "Unavailable",
            format!(
                "{CALL}: Sandbox {name}'s root is not there: {why}. Its home is still read: {}",
                sandbox.run.join(roots::HOME).display()
            ),
        )
    };
    let pid = (sandbox.daemon.pid()).and_then(|pid| u32::try_from(pid).ok());
    let Some(pid) = pid.filter(|_| process::live(sandbox.daemon) == Ok(true)) else {
        return Err(stopped("its daemon is not running"));
    };
    let found = confined::root_of(pid).map_err(|why| match why {
        Refused::Missing => stopped("its daemon is not running"),
        other => refusal(CALL, path, other),
    })?;
    // the process whose root this is must be the one recorded, and run
    // still: its start time through the same descriptor, then its boot
    let recorded = sandbox.daemon.start().and_then(|start| start.parse().ok());
    if recorded.is_some_and(|start: u64| found.start != Some(start))
        || process::live(sandbox.daemon) != Ok(true)
    {
        return Err(stopped(
            "its daemon ended, and its process id names another process",
        ));
    }
    let rules = Rules {
        view: View::InRoot,
        system: &SYSTEM,
        ..Rules::beneath()
    };
    let slash = Path::new("/");
    let owner = Some(roots::uid_of(Some(sandbox.daemon)));
    let scope = Scope::over(
        CALL,
        root,
        slash,
        found.base,
        slash,
        found.mounts,
        owner,
        rules,
    )
    .owning(sandbox.run)
    .with_workspaces(sent);
    let relative = path.strip_prefix(slash).unwrap_or(Path::new(""));
    answer(&scope, relative, request, spent)
}
