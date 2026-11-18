//! `machine.files`: a machine's files: its LayerFS folders, or a folder
//! or file of it.
use super::stats::{at, field, rows};
use crate::calls::table::Kind::{Int, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::inspect::{machine_files, FilesRequest};

/// The entries a folder page holds when `--limit` is not given.
const LIMIT: i64 = 100;

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
    Ok(shown(&read))
}

/// A folder page or a file window as the row names it.
fn shown(read: &SdkValue) -> SdkValue {
    if read.get_text("kind") == Some("file") {
        let binary = read.get_bool("binary");
        let content = if binary {
            SdkValue::Null
        } else {
            field(read, "content")
        };
        return SdkValue::map([
            ("text", content),
            ("binary", SdkValue::Bool(binary)),
            (
                "size",
                if binary {
                    field(read, "size")
                } else {
                    SdkValue::Null
                },
            ),
            ("next_byte_offset", field(read, "next")),
            ("type", SdkValue::text("file")),
        ]);
    }
    let more = read.get_bool("has_more");
    SdkValue::map([
        ("items", rows(read.get("entries"), entry)),
        (
            "next_cursor",
            if more {
                field(read, "next")
            } else {
                SdkValue::Null
            },
        ),
        ("type", SdkValue::text("dir")),
    ])
}

/// One entry: its `type` (`file`, `dir`, `symlink`, `other`) and, for one
/// listed and not read, why (`system`, `mount`, `unreadable`,
/// `restricted`). An unreadable or restricted entry whose size was read is
/// a file, any other a folder.
fn entry(found: &SdkValue) -> SdkValue {
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
    SdkValue::map([
        ("path", field(found, "path")),
        ("type", SdkValue::text(kind)),
        ("size", field(found, "size")),
        ("not_read", SdkValue::opt_text(not_read)),
        ("role", field(found, "role")),
        ("depth", field(found, "depth")),
        ("mode", mode.unwrap_or_default()),
        ("modified_at", at(field(found, "mtime"))),
        ("target", field(found, "target")),
        ("fs", field(found, "fs")),
        ("workspace", field(found, "workspace")),
        ("disk_free_bytes", field(found, "free_bytes")),
    ])
}
