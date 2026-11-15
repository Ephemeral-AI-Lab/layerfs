//! `sandbox.files`: a Sandbox's roots, one page of a folder of its home
//! or one window of a file there.
use crate::calls::table::Kind::{Name, Text};
use crate::calls::table::{a, call, opt, Call, FILES};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::inspect::{sandbox_files, FilesRequest};

/// Its table entry. It is quiet: a UI reads a folder on every expand, and
/// the read writes no call-log line.
pub(crate) const CALL: Call = call(
    "sandbox",
    "files",
    &[a("sandbox", Name), opt("path", Text)],
    &FILES,
    "a Sandbox's files: its roots, or a folder or file under its home",
    run,
)
.quiet();

/// Runs `sandbox.files`. With no `path`: `{ kind: "folder", entries, next,
/// has_more }`, the Sandbox's roots, each with a `role` (`workspace`,
/// `home`, `log`). With the path of a folder under its home: one page of
/// its entries in tree order, `{ path, depth, kind, size, mode, mtime,
/// target? }`, continued from `next` while `has_more`. With the path of a
/// file: `{ kind: "file", size, offset, content, binary, next }`. It is
/// read where the Sandbox's run folder is; it takes no lock or lease, asks
/// no daemon and writes nothing (layerfs-project#178).
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let sandbox = args.get_text("sandbox").unwrap_or("");
    sandbox_files(&context.home, sandbox, &FilesRequest::of(args))
}
