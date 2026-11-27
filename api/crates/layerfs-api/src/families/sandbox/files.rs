//! `sandbox.files`: a Sandbox's files: its roots, one page of a folder of
//! its home or of its root, or one byte window of a file there.
use super::get::field;
use crate::dispatch::table::Kind::{Int, Name, Text};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form, Paging, Part};
use layerfs_home::outcome::Outcome;
use layerfs_home::time::iso;
use layerfs_inspect::{sandbox_files, FilesRequest};
use layerfs_protocol::sdk::SdkValue;

/// The entries a folder page holds when `--limit` is not given.
pub(crate) const LIMIT: i64 = 100;

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
    "sandbox",
    "files",
    &[a("sandbox", Name), opt("path", Text).redact_path()],
    &[
        opt("depth", Int),
        opt("limit", Int),
        opt("cursor", Text).redact(),
        opt("byte_offset", Int),
        opt("byte_length", Int),
    ],
    "a Sandbox's files: its roots, one page of a folder, or one byte window of a file",
    run,
)
.quiet()
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `sandbox.files`. With no `path`: the Sandbox's roots, each with a
/// `role` (`workspace`, with the Workspace's handle; `home`; `log`; and
/// `root` for a namespaces Sandbox). With the path of a folder under its
/// home, or in its root when it has one: one page of its entries in tree
/// order to `depth` levels, continued from `next_cursor`. With the path of
/// a file: a window of it, `text` with `next_byte_offset` while more
/// remains, or `binary` with its `size`. It is read where the Sandbox's run
/// folder is; it takes no lock or lease, asks no daemon and writes nothing
/// (layerfs-project#178).
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let sandbox = args.get_text("sandbox").unwrap_or("");
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FilesRequest {
        path: text("path"),
        depth: args.get_int("depth"),
        cursor: text("cursor"),
        limit: Some(args.get_int("limit").unwrap_or(LIMIT)),
        offset: args.get_int("byte_offset"),
        length: args.get_int("byte_length"),
    };
    let read = sandbox_files(&context.home, sandbox, &request)?;
    Ok(shown(&read, entry))
}

/// A folder page or a file window as the row names it, each entry of a
/// page as `entry` names it (`machine.files` adds one key to [`entry`]).
pub(crate) fn shown(read: &SdkValue, entry: fn(&SdkValue) -> SdkValue) -> SdkValue {
    if read.get_text("kind") == Some("file") {
        let binary = read.get_bool("binary");
        let (text, size) = match binary {
            true => (SdkValue::Null, field(read, "size")),
            false => (field(read, "content"), SdkValue::Null),
        };
        return SdkValue::map([
            ("text", text),
            ("binary", SdkValue::Bool(binary)),
            ("size", size),
            ("next_byte_offset", field(read, "next")),
            ("type", SdkValue::text("file")),
        ]);
    }
    let next = match read.get_bool("has_more") {
        true => field(read, "next"),
        false => SdkValue::Null,
    };
    let entries = match read.get("entries") {
        Some(SdkValue::List(items)) => items.iter().map(entry).collect(),
        _ => Vec::new(),
    };
    SdkValue::map([
        ("items", SdkValue::List(entries)),
        ("next_cursor", next),
        ("type", SdkValue::text("dir")),
    ])
}

/// One entry: its `type` (`file`, `dir`, `symlink`, `other`) and, for one
/// listed and not read, why (`system`, `mount`, `unreadable`,
/// `restricted`). An unreadable or restricted entry whose size was read is
/// a file, any other a folder. Its `mode` is octal text.
pub(crate) fn entry(found: &SdkValue) -> SdkValue {
    let sized = !matches!(found.get("size"), None | Some(SdkValue::Null));
    let (kind, not_read) = match found.get_text("kind").unwrap_or("other") {
        "folder" => ("dir", None),
        "file" => ("file", None),
        "symlink" => ("symlink", None),
        why @ ("system" | "mount") => ("dir", Some(why)),
        why @ ("unreadable" | "restricted") => (if sized { "file" } else { "dir" }, Some(why)),
        _ => ("other", None),
    };
    let mode = found.get_int("mode");
    let mode = mode.map(|bits| SdkValue::Text(format!("{:04o}", bits & 0o7777)));
    let modified = found.get_int("mtime").map(|ms| SdkValue::Text(iso(ms)));
    SdkValue::map([
        ("path", field(found, "path")),
        ("type", SdkValue::text(kind)),
        ("size", field(found, "size")),
        ("not_read", SdkValue::opt_text(not_read)),
        ("role", field(found, "role")),
        ("depth", field(found, "depth")),
        ("mode", mode.unwrap_or_default()),
        ("modified_at", modified.unwrap_or_default()),
        ("target", field(found, "target")),
        ("fs", field(found, "fs")),
        ("workspace", field(found, "workspace")),
    ])
}
