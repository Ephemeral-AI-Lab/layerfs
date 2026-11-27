//! `machine.files`: a machine's files: its LayerFS folders, or a folder
//! or file of it.
use super::stats::field;
use crate::dispatch::table::Kind::{Int, Name, Text};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form, Paging, Part};
use crate::families::sandbox::files::{self, LIMIT};
use layerfs_home::disk::with_field;
use layerfs_home::names::MachineName;
use layerfs_home::reply::outcome::Outcome;
use layerfs_inspect::{machine_files, FilesRequest};
use layerfs_protocol::sdk::SdkValue;

/// One entry of a folder page.
const ENTRY: Form = Form::of(
    &["path", "type"],
    &["size", "not_read"],
    &[
        "path",
        "type",
        "size",
        "not_read",
        "role",
        "depth",
        "mode",
        "modified_at",
        "target",
        "fs",
        "workspace",
        "disk_free_bytes",
    ],
);
const FORM: Form = Form::of(
    &[],
    &[
        "items",
        "next_cursor",
        "text",
        "binary",
        "size",
        "next_byte_offset",
    ],
    &[
        "items",
        "next_cursor",
        "text",
        "binary",
        "size",
        "next_byte_offset",
        "type",
    ],
)
.parts(&[("items", Part::Rows(&ENTRY))]);

/// Its table entry. It is quiet: a UI reads a folder on every expand, and
/// the read writes no call-log line.
pub(crate) const CALL: Call = call(
    "machine",
    "files",
    &[a("machine", Name), opt("path", Text).redact_path()],
    &[
        opt("depth", Int),
        opt("limit", Int),
        opt("cursor", Text).redact(),
        opt("byte_offset", Int),
        opt("byte_length", Int),
    ],
    "a machine's files: its LayerFS folders by role, or a folder or file of the machine",
    run,
)
.quiet()
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `machine.files`. With no `path`: the machine's roots, each with a
/// `role`: its LayerFS folders, then `/` on a machine that is LayerFS's
/// own. With the path of a folder: one page of its entries in tree order to
/// `depth` levels, continued from `next_cursor`. With the path of a file:
/// a window of it, `text` with `next_byte_offset` while more remains, or
/// `binary` with its `size`. On `native` only the LayerFS folders are
/// read. It is read where the machine's data is; it takes no lock or
/// lease, asks no daemon and writes nothing.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FilesRequest {
        path: text("path"),
        depth: args.get_int("depth"),
        cursor: text("cursor"),
        limit: Some(args.get_int("limit").unwrap_or(LIMIT)),
        offset: args.get_int("byte_offset"),
        length: args.get_int("byte_length"),
    };
    let read = machine_files(&context.home, &machine, &request)?;
    Ok(files::shown(&read, entry))
}

/// One entry as `sandbox.files` names it, with the free space of the
/// filesystem it is on.
fn entry(found: &SdkValue) -> SdkValue {
    with_field(
        files::entry(found),
        "disk_free_bytes",
        field(found, "free_bytes"),
    )
}
