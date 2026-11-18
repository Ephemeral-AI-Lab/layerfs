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
use super::form::{Form, Paging, Part, Profile, Section};
use super::platform_policy::Policy;
use super::render::Saves;
use super::scope::Rule;
use super::Context;
use layerfs_bridge::control::SdkValue;
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
    /// A Sandbox image: a `sha256:` digest, a Docker image Id or a Docker
    /// reference (`postgres:17-bookworm`); text in every front end.
    Image,
    /// File content: text or bytes; the shell reads stdin or `--from`.
    Content,
    /// A size: bytes as a number, or text with `K`, `M` or `G`.
    Size,
    /// A function the front end calls (`on_progress`, exec sinks); never
    /// crosses into the library.
    Function,
}

/// How the call log records an argument's value (#659 §4, #41 §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Redact {
    /// As a word, cut at 200 characters.
    No,
    /// Only as its byte count, never its value: free text, a credential, a
    /// URL that may embed one.
    Bytes,
    /// Only as the byte count of the path it names, one leading `/`
    /// stripped, so `/a` and `a` count the same and the root counts 0.
    Path,
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
    /// How the call log records its value. Content and JSON are always
    /// recorded as a byte count, whatever this says.
    pub redact: Redact,
    /// The shell flag that reads its value from stdin (`--password-stdin`),
    /// for a secret: the shell then takes the value no other way, and Node
    /// and Python pass it as the option.
    pub shell: Option<&'static str>,
    /// Whether the shell takes the option more than once: a JSON option
    /// given as `--env K=V` repeatedly is one object of names to text
    /// (call surface 2.0 `repeat`).
    pub repeat: bool,
}

impl Arg {
    /// This argument, logged only as its byte count.
    pub const fn redact(self) -> Arg {
        Arg {
            redact: Redact::Bytes,
            ..self
        }
    }

    /// This path argument, logged only as its normalized byte count.
    pub const fn redact_path(self) -> Arg {
        Arg {
            redact: Redact::Path,
            ..self
        }
    }

    /// This option, a secret the shell reads from stdin when given `flag`
    /// and takes no other way; it is logged only as its byte count.
    pub const fn from_stdin(self, flag: &'static str) -> Arg {
        Arg {
            shell: Some(flag),
            redact: Redact::Bytes,
            ..self
        }
    }

    /// This option, given more than once in the shell ([`Arg::repeat`]).
    pub const fn repeat(self) -> Arg {
        Arg {
            repeat: true,
            ..self
        }
    }

    /// Whether the call log leaves its value out.
    pub fn redacted(&self) -> bool {
        self.redact != Redact::No || matches!(self.kind, Kind::Content | Kind::Json)
    }
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
    /// Whether it writes no line to the home's call log: a read a UI
    /// polls (`sandbox.files`, `machine.files`), which would fill the log and writes
    /// nothing else either (layerfs-project#178, #179 B.1).
    pub quiet: bool,
    /// Whether the dispatcher opens (creates or converts) the home before
    /// it runs: every call but `home.init`, which opens it itself to say
    /// what it did, and `home.get` and `machine.platforms`, which read a
    /// home without changing it (and answer without one).
    pub opens: bool,
    /// The keys of its answer (call surface 2.0). None while the call is
    /// not migrated: its result then passes through as its run function
    /// made it.
    pub form: Option<&'static Form>,
    /// The sections `--with` can add to its answer.
    pub with: &'static [Section],
    /// How it pages.
    pub paging: Paging,
    /// Which profile lists it.
    pub profile: Profile,
    /// What a folder scope does with it.
    pub scope: Rule,
    /// Which of its arguments must not name a removed platform.
    pub policy: Policy,
    /// The saved version its call log line names.
    pub saves: Saves,
}

impl Call {
    /// `family.call`.
    pub fn name(&self) -> String {
        format!("{}.{}", self.family, self.call)
    }

    /// This call, writing no call-log line.
    pub const fn quiet(self) -> Self {
        Call {
            quiet: true,
            ..self
        }
    }

    /// This call, run without the dispatcher opening the home first.
    pub const fn unopened(self) -> Self {
        Call {
            opens: false,
            ..self
        }
    }

    /// This call, its answer shaped by `form`: the call is migrated.
    pub const fn form(self, form: &'static Form) -> Self {
        Call {
            form: Some(form),
            ..self
        }
    }

    /// This call, taking these `--with` sections.
    pub const fn with(self, with: &'static [Section]) -> Self {
        Call { with, ..self }
    }

    /// This call, paging as `paging` says.
    pub const fn paged(self, paging: Paging) -> Self {
        Call { paging, ..self }
    }

    /// This call, listed in the agent profile.
    pub const fn agent(self) -> Self {
        Call {
            profile: Profile::Agent,
            ..self
        }
    }

    /// This call, under a folder scope's `rule`.
    pub const fn scoped(self, scope: Rule) -> Self {
        Call { scope, ..self }
    }

    /// This call, refused when what `policy` names is on a removed platform.
    pub const fn policy(self, policy: Policy) -> Self {
        Call { policy, ..self }
    }

    /// This call, its call log line naming the version it saved.
    pub const fn saves(self, saves: Saves) -> Self {
        Call { saves, ..self }
    }

    /// Whether it reports progress events (it takes `on_progress`).
    pub fn progress(&self) -> bool {
        self.arg("on_progress").is_some()
    }

    /// The section `name` of its `--with`, when it declares one.
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.with.iter().find(|section| section.name == name)
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
        redact: Redact::No,
        shell: None,
        repeat: false,
    }
}

/// An option, or a positional that may be left off.
pub const fn opt(name: &'static str, kind: Kind) -> Arg {
    Arg {
        name,
        kind,
        optional: true,
        redact: Redact::No,
        shell: None,
        repeat: false,
    }
}

use Kind::{Bool, Function, Int, Text};

/// The paging options of a list call.
pub const PAGE: [Arg; 2] = [opt("page_size", Int), opt("page_pos", Int)];
/// The window options of a log read by cursor (`logs.machine`,
/// `logs.sandbox`).
pub const WINDOW: [Arg; 3] = [opt("cursor", Text), opt("before", Text), opt("limit", Int)];
/// The options of a files read (`sandbox.files`, `machine.files`): a
/// folder page's `depth`, `cursor` and `limit`, a file window's `offset`
/// and `length`.
pub const FILES: [Arg; 5] = [
    opt("depth", Int),
    opt("cursor", Text),
    opt("limit", Int),
    opt("offset", Int),
    opt("length", Int),
];
/// The paging options of a cursor-paged call (call surface 2.0): the most
/// rows in one page, and the `next_cursor` of the page before.
pub const CURSOR: [Arg; 2] = [opt("limit", Int), opt("cursor", Text)];
/// A Commit message.
pub const MESSAGE: Arg = opt("message", Text).redact();
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
        quiet: false,
        opens: true,
        form: None,
        with: &[],
        paging: Paging::None,
        profile: Profile::Full,
        scope: Rule::Outside,
        policy: Policy::None,
        saves: Saves::Nothing,
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

fn words(names: &[&str]) -> SdkValue {
    SdkValue::List(names.iter().map(|name| SdkValue::text(*name)).collect())
}

/// A form's keys: `{ short: { always, sometimes }, long }`, with `parts`
/// when a key has its own form (a plain-string list is `{ scalar }`).
fn form_value(form: &Form) -> SdkValue {
    let mut fields = vec![
        (
            "short".to_owned(),
            SdkValue::map([
                ("always", words(form.always)),
                ("sometimes", words(form.sometimes)),
            ]),
        ),
        ("long".to_owned(), words(form.long)),
    ];
    if !form.parts.is_empty() {
        let part = |(key, part): &(&str, Part)| {
            let value = match part {
                Part::Rows(form) => form_value(form),
                Part::Scalars(word) => SdkValue::map([("scalar", SdkValue::text(*word))]),
            };
            ((*key).to_owned(), value)
        };
        let parts = form.parts.iter().map(part).collect();
        fields.push(("parts".to_owned(), SdkValue::Map(parts)));
    }
    SdkValue::Map(fields)
}

/// A migrated call's declared answer: its form, `with` sections, paging,
/// progress, quiet and profile, as the specification's data spells them.
fn declared(call: &Call, form: &Form) -> SdkValue {
    let section = |section: &Section| {
        let mut fields = vec![
            ("where".to_owned(), SdkValue::text(section.place.word())),
            ("keys".to_owned(), words(section.keys)),
        ];
        if let Some(form) = section.form {
            fields.push(("form".to_owned(), form_value(form)));
        }
        (section.name.to_owned(), SdkValue::Map(fields))
    };
    let mut fields = match form_value(form) {
        SdkValue::Map(fields) => fields,
        _ => Vec::new(),
    };
    fields.extend([
        (
            "with".to_owned(),
            SdkValue::Map(call.with.iter().map(section).collect()),
        ),
        ("paging".to_owned(), SdkValue::opt_text(call.paging.word())),
        ("progress".to_owned(), SdkValue::Bool(call.progress())),
        ("quiet".to_owned(), SdkValue::Bool(call.quiet)),
        ("profile".to_owned(), SdkValue::text(call.profile.word())),
    ]);
    SdkValue::Map(fields)
}

/// The whole table as JSON text, for front ends that build their functions
/// from it: `[{ family, call, args: [{ name, kind, optional, shell? }],
/// options, help, form? }]` (`shell` is the flag of a secret option the
/// shell reads from stdin). A migrated call has `form`: `{ short, long, parts?,
/// with, paging, progress, quiet, profile }`.
pub fn table_json(families: &'static [Family]) -> String {
    let arg = |a: &Arg| {
        let mut fields = vec![
            ("name".to_owned(), SdkValue::text(a.name)),
            (
                "kind".to_owned(),
                SdkValue::text(format!("{:?}", a.kind).to_lowercase()),
            ),
            ("optional".to_owned(), SdkValue::Bool(a.optional)),
        ];
        if let Some(flag) = a.shell {
            fields.push(("shell".to_owned(), SdkValue::text(flag)));
        }
        if a.repeat {
            fields.push(("repeat".to_owned(), SdkValue::Bool(true)));
        }
        SdkValue::Map(fields)
    };
    let calls = calls(families)
        .map(|c| {
            let mut fields = vec![
                ("family".to_owned(), SdkValue::text(c.family)),
                ("call".to_owned(), SdkValue::text(c.call)),
                (
                    "args".to_owned(),
                    SdkValue::List(c.args.iter().map(arg).collect()),
                ),
                (
                    "options".to_owned(),
                    SdkValue::List(c.options.iter().map(arg).collect()),
                ),
                ("help".to_owned(), SdkValue::text(c.help)),
            ];
            if let Some(form) = c.form {
                fields.push(("form".to_owned(), declared(c, form)));
            }
            SdkValue::Map(fields)
        })
        .collect();
    layerfs_home::json::print(&SdkValue::List(calls))
}
