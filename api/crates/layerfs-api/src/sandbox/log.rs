//! `sandbox.log`: a Sandbox's daemon log, newest page first (`logs.sandbox`
//! before the call surface 2.0).
use super::get::field;
use crate::calls::table::Kind::{Int, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_home::outcome::Outcome;
use layerfs_inspect::{sandbox_log, Window};
use layerfs_protocol::sdk::SdkValue;

/// One line, or a piece of a line longer than a read.
const LINE: Form = Form::of(&["text"], &["continued"], &["text", "continued"]);
/// A page of lines, `machine.log`'s too.
pub(crate) const FORM: Form = Form::of(&["items"], &["next_cursor"], &["items", "next_cursor"])
    .parts(&[("items", Part::Rows(&LINE))]);

/// Its table entry: a read a UI polls, so it writes no log line.
pub(crate) const CALL: Call = call(
    "sandbox",
    "log",
    &[a("sandbox", Name)],
    &[opt("limit", Int), opt("cursor", Text).redact()],
    "a Sandbox's daemon log: its last lines, or the lines before a cursor",
    run,
)
.quiet()
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `sandbox.log`: the last `limit` lines (200 by default), oldest
/// first, or with `cursor` the lines before it, at most 64 KiB a call;
/// `next_cursor` continues with the lines before these, and is null at the
/// start of the log. It is read where the Sandbox's run folder is, with no
/// lock or lease, no daemon call and no write.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let sandbox = args.get_text("sandbox").unwrap_or("");
    let window = Window {
        cursor: None,
        before: args.get_text("cursor").map(str::to_owned),
        limit: args.get_int("limit"),
    };
    Ok(page(&sandbox_log(&context.home, sandbox, &window)?))
}

/// A log read (`{ lines, before }`) as [`FORM`] names it, `machine.log`'s
/// too.
pub(crate) fn page(read: &SdkValue) -> SdkValue {
    let items = match read.get("lines") {
        Some(SdkValue::List(lines)) => (lines.iter())
            .map(|line| {
                SdkValue::map([
                    ("text", field(line, "text")),
                    ("continued", SdkValue::Bool(line.get_bool("continued"))),
                ])
            })
            .collect(),
        _ => Vec::new(),
    };
    SdkValue::map([
        ("items", SdkValue::List(items)),
        ("next_cursor", field(read, "before")),
    ])
}
