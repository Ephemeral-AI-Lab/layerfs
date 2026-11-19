//! `machine.log`: a machine's worker log, newest page first (`logs.machine`
//! before the call surface 2.0).
use super::stats::{field, rows};
use crate::calls::table::Kind::{Int, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::inspect::{machine_log, Window};

/// One line, or a piece of a line longer than a read.
const LINE: Form = Form::of(&["text"], &["continued"], &["text", "continued"]);
const FORM: Form = Form::of(&["items"], &["next_cursor"], &["items", "next_cursor"])
    .parts(&[("items", Part::Rows(&LINE))]);

/// Its table entry: a read a UI polls, so it writes no log line.
pub(crate) const CALL: Call = call(
    "machine",
    "log",
    &[a("machine", Name)],
    &[opt("limit", Int), opt("cursor", Text).redact()],
    "a machine's worker log: its last lines, then the lines before a cursor",
    run,
)
.quiet()
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `machine.log`: the last `limit` lines, oldest first, or with
/// `cursor` the lines before it; `next_cursor` continues with the lines
/// before these, and is null at the start of the log. It is read in the
/// machine's worker, which takes no lock or lease, starts nothing and
/// writes nothing.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let window = Window {
        cursor: None,
        before: args.get_text("cursor").map(str::to_owned),
        limit: args.get_int("limit"),
    };
    let read = machine_log(&context.home, &machine, &window)?;
    let items = rows(read.get("lines"), |line| {
        SdkValue::map([
            ("text", field(line, "text")),
            ("continued", SdkValue::Bool(line.get_bool("continued"))),
        ])
    });
    Ok(SdkValue::map([
        ("items", items),
        ("next_cursor", field(&read, "before")),
    ]))
}
