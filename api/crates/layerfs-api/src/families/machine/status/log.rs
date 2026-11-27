//! `machine.log`: a machine's worker log, newest page first (`logs.machine`
//! before the call surface 2.0).
use crate::dispatch::table::Kind::{Int, Name, Text};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Paging};
use crate::families::sandbox::log::{self, FORM};
use layerfs_home::names::MachineName;
use layerfs_home::reply::outcome::Outcome;
use layerfs_inspect::{machine_log, Window};
use layerfs_protocol::sdk::SdkValue;

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
    Ok(log::page(&machine_log(&context.home, &machine, &window)?))
}
