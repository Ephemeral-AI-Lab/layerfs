//! `machine.files`: a machine's files: its LayerFS folders, or a folder
//! or file of it.
use crate::calls::table::Kind::{Name, Text};
use crate::calls::table::{a, call, opt, Call, FILES};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
#[cfg(unix)]
use layerfs_sandbox::inspect::{machine_files, FilesRequest};

/// Its table entry. It is quiet: a UI reads a folder on every expand, and
/// the read writes no call-log line.
pub(crate) const CALL: Call = call(
    "machine",
    "files",
    &[a("machine", Name), opt("path", Text)],
    &FILES,
    "a machine's files: its LayerFS folders by role, or a folder or file of the machine",
    run,
)
.quiet();

/// Runs `machine.files`. With no `path`: `{ kind: "folder", entries, next,
/// has_more }`, the machine's roots, each with a `role`: its LayerFS
/// folders (`data`, `stores`, `sandbox_records`, `run_folders`, `images`,
/// `staging`, `logs`, `state`, `bundle`, `image`), then `/` (`machine`) on
/// a machine that is LayerFS's own. With the path of a folder: one page of
/// its entries in tree order, `{ path, depth, kind, size, mode, mtime,
/// role?, target?, fs?, workspace? }`, continued from `next` while
/// `has_more`. With the path of a file: `{ kind: "file", size, offset,
/// content, binary, next }`. On `native` only the LayerFS folders are
/// read. It is read where the machine's data is; it takes no lock or
/// lease, asks no daemon and writes nothing (layerfs-project#178).
#[cfg(unix)]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    machine_files(&context.home, &machine, &FilesRequest::of(args))
}

// Windows: the machine files read waits for the monitoring port.
#[cfg(windows)]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused("Unsupported", "machine.files reads on the unix face today (#178)"))
}
