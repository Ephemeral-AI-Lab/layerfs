//! The shape of the call table (S §3, with the answers to O11, O13, O14 and
//! O24): what a call, its arguments and a family are. The entries
//! themselves live in `layerfs-api`, one file per call under its family,
//! each with the function that runs it; every function here takes that
//! list of families. The shell parser, the
//! help, the Node and Python front ends' functions and argument checking
//! all come from it.
//!
//! Positionals go from the outside in: owner, Branch, path or new name,
//! other inputs, an optional Commit last (D7). Options say how. A shell
//! option is the option's name with `-` for `_` (`--start-line`).
use layerfs_bridge::control::SdkValue;
use layerfs_home::context::Context;
use layerfs_home::outcome::Outcome;

/// What one argument accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A machine, Store, bucket, Branch or Sandbox name, or a Commit id.
    Name,
    /// A project, `<store>/<project>`.
    Project,
    /// A path or URL as text.
    Text,
    /// A whole number.
    Int,
    /// true or false; a shell flag.
    Bool,
    /// Any JSON value; the shell reads it as JSON text.
    Json,
    /// File content: text or bytes; the shell reads stdin or `--from`.
    Content,
    /// A size: bytes as a number, or text with `K`, `M` or `G`.
    Size,
    /// A function the front end calls (`on_progress`, exec sinks); never
    /// crosses into the library.
    Function,
}

/// One positional or option.
#[derive(Clone, Copy, Debug)]
pub struct Arg {
    /// Its snake_case name, the same in every front end.
    pub name: &'static str,
    /// What it accepts.
    pub kind: Kind,
    /// Whether a positional may be left off (options always may).
    pub optional: bool,
}

/// One call of the table.
#[derive(Clone, Copy, Debug)]
pub struct Call {
    /// Its family.
    pub family: &'static str,
    /// Its name in the family.
    pub call: &'static str,
    /// Its positionals, in order.
    pub args: &'static [Arg],
    /// Its options.
    pub options: &'static [Arg],
    /// One line of help.
    pub help: &'static str,
    /// Runs it with its checked arguments.
    pub run: Run,
}

impl Call {
    /// `family.call`.
    pub fn name(&self) -> String {
        format!("{}.{}", self.family, self.call)
    }

    /// The positional or option named `name`.
    pub fn arg(&self, name: &str) -> Option<&Arg> {
        self.args
            .iter()
            .chain(self.options)
            .find(|a| a.name == name)
    }
}

/// How a call runs: the running call and its checked arguments.
pub type Run = fn(&mut Context<'_>, &SdkValue) -> Outcome;

/// One family: its name and its calls in the order help lists them.
#[derive(Clone, Copy, Debug)]
pub struct Family {
    /// Its name.
    pub name: &'static str,
    /// Lists its calls.
    pub list: fn() -> Calls,
}

impl Family {
    /// Its calls, in the order help lists them.
    pub fn calls(&self) -> Calls {
        (self.list)()
    }
}

/// A family's calls, in the order help lists them.
pub type Calls = &'static [Call];
/// The families of the table, in the order help lists them.
pub type Families = &'static [Family];
/// One call of the table.
pub type CallRef = &'static Call;

/// A required positional.
pub const fn a(name: &'static str, kind: Kind) -> Arg {
    Arg {
        name,
        kind,
        optional: false,
    }
}

/// An option, or a positional that may be left off.
pub const fn opt(name: &'static str, kind: Kind) -> Arg {
    Arg {
        name,
        kind,
        optional: true,
    }
}

use Kind::{Bool, Function, Int, Text};

/// The paging options of a list call.
pub const PAGE: [Arg; 2] = [opt("page_size", Int), opt("page_pos", Int)];
/// A Commit message.
pub const MESSAGE: Arg = opt("message", Text);
/// The progress function of a long call.
pub const PROGRESS: Arg = opt("on_progress", Function);
/// The options of a diff call.
pub const DIFF: [Arg; 4] = [
    opt("paths_only", Bool),
    opt("page_size", Int),
    opt("page_pos", Int),
    opt("cursor", Text),
];

/// One table entry.
pub const fn call(
    family: &'static str,
    call: &'static str,
    args: &'static [Arg],
    options: &'static [Arg],
    help: &'static str,
    run: Run,
) -> Call {
    Call {
        family,
        call,
        args,
        options,
        help,
        run,
    }
}

/// Every call of `families`, in table order.
pub fn calls(families: &'static [Family]) -> impl Iterator<Item = &'static Call> {
    families.iter().flat_map(Family::calls)
}

/// The entry of the call named `family.call`.
pub fn find(families: &'static [Family], name: &str) -> Option<&'static Call> {
    let (family, call) = name.split_once('.')?;
    let family = families.iter().find(|f| f.name == family)?;
    family.calls().iter().find(|c| c.call == call)
}

/// The whole table as JSON text, for front ends that build their functions
/// from it: `[{ family, call, args: [{ name, kind, optional }], options,
/// help }]`.
pub fn table_json(families: &'static [Family]) -> String {
    let arg = |a: &Arg| {
        SdkValue::map([
            ("name", SdkValue::text(a.name)),
            (
                "kind",
                SdkValue::text(format!("{:?}", a.kind).to_lowercase()),
            ),
            ("optional", SdkValue::Bool(a.optional)),
        ])
    };
    let calls = calls(families)
        .map(|c| {
            SdkValue::map([
                ("family", SdkValue::text(c.family)),
                ("call", SdkValue::text(c.call)),
                ("args", SdkValue::List(c.args.iter().map(arg).collect())),
                (
                    "options",
                    SdkValue::List(c.options.iter().map(arg).collect()),
                ),
                ("help", SdkValue::text(c.help)),
            ])
        })
        .collect();
    layerfs_home::json::print(&SdkValue::List(calls))
}
