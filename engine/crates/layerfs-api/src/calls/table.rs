//! The one table of calls (S §3, with the answers to O11, O13, O14 and O24):
//! nine families, 57 calls. The shell parser, the help, the Node and Python
//! front ends' functions and argument checking all come from this table.
//!
//! Positionals go from the outside in: owner, Branch, path or new name,
//! other inputs, an optional Commit last (D7). Options say how. A shell
//! option is the option's name with `-` for `_` (`--start-line`).

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

const fn a(name: &'static str, kind: Kind) -> Arg {
    Arg {
        name,
        kind,
        optional: false,
    }
}

const fn opt(name: &'static str, kind: Kind) -> Arg {
    Arg {
        name,
        kind,
        optional: true,
    }
}

use Kind::{Bool, Content, Function, Int, Json, Name, Project, Size, Text};

const PAGE: [Arg; 2] = [opt("page_size", Int), opt("page_pos", Int)];
const MESSAGE: Arg = opt("message", Text);
const PROGRESS: Arg = opt("on_progress", Function);
const DIFF: [Arg; 4] = [
    opt("paths_only", Bool),
    opt("page_size", Int),
    opt("page_pos", Int),
    opt("cursor", Text),
];

const fn call(
    family: &'static str,
    call: &'static str,
    args: &'static [Arg],
    options: &'static [Arg],
    help: &'static str,
) -> Call {
    Call {
        family,
        call,
        args,
        options,
        help,
    }
}

/// The nine families, in the order help lists them.
pub const FAMILIES: [&str; 9] = [
    "project",
    "branch",
    "commit",
    "file",
    "sandbox",
    "workspace",
    "machine",
    "store",
    "backup",
];

/// Every call.
pub static CALLS: &[Call] = &[
    // project (4)
    call(
        "project",
        "init",
        &[a("project", Project), a("folder", Text)],
        &[opt("timeout", Int), PROGRESS],
        "import a host folder as a new project into an existing Store",
    ),
    call(
        "project",
        "list",
        &[opt("store", Name)],
        &PAGE,
        "every project in the home, or one Store's",
    ),
    call(
        "project",
        "get",
        &[a("project", Project)],
        &[],
        "a project's record: its Store, kind and machine",
    ),
    call(
        "project",
        "remove",
        &[a("project", Project)],
        &[],
        "remove one project from its Store",
    ),
    // branch (6)
    call(
        "branch",
        "list",
        &[a("project", Project)],
        &PAGE,
        "a project's Branches",
    ),
    call(
        "branch",
        "get",
        &[a("project", Project), a("branch", Name)],
        &[],
        "one Branch and its head Commit",
    ),
    call(
        "branch",
        "fork",
        &[
            a("project", Project),
            a("name", Name),
            a("from_branch", Name),
            opt("from_commit", Name),
        ],
        &[],
        "a new Branch from another Branch's head or Commit",
    ),
    call(
        "branch",
        "merge",
        &[
            a("project", Project),
            a("name", Name),
            a("a", Name),
            a("b", Name),
        ],
        &[MESSAGE, opt("units", Json)],
        "merge two Branches into a new one",
    ),
    call(
        "branch",
        "publish",
        &[
            a("project", Project),
            a("branch", Name),
            opt("commit", Name),
        ],
        &[],
        "a Branch's head or Commit as main's next Layer",
    ),
    call(
        "branch",
        "diff",
        &[
            a("project", Project),
            a("from_branch", Name),
            a("to_branch", Name),
        ],
        &DIFF,
        "files that differ between two Branches' heads",
    ),
    // commit (3)
    call(
        "commit",
        "list",
        &[a("project", Project), a("branch", Name)],
        &PAGE,
        "a Branch's Commits, newest first",
    ),
    call(
        "commit",
        "get",
        &[a("project", Project), a("branch", Name), a("commit", Name)],
        &[],
        "one Commit of a Branch",
    ),
    call(
        "commit",
        "diff",
        &[
            a("project", Project),
            a("from_commit", Name),
            a("to_commit", Name),
        ],
        &DIFF,
        "files that differ between two Commits",
    ),
    // file (14, O13: was fs)
    call(
        "file",
        "read",
        &[
            a("project", Project),
            a("branch", Name),
            a("path", Text),
            opt("commit", Name),
        ],
        &[opt("start_line", Int), opt("end_line", Int)],
        "read a file",
    ),
    call(
        "file",
        "stat",
        &[
            a("project", Project),
            a("branch", Name),
            a("path", Text),
            opt("commit", Name),
        ],
        &[],
        "a path's kind, size and mode",
    ),
    call(
        "file",
        "list",
        &[
            a("project", Project),
            a("branch", Name),
            a("path", Text),
            opt("commit", Name),
        ],
        &PAGE,
        "a folder's entries",
    ),
    call(
        "file",
        "glob",
        &[
            a("project", Project),
            a("branch", Name),
            a("path", Text),
            a("pattern", Text),
            opt("commit", Name),
        ],
        &PAGE,
        "paths under `path` matching a glob",
    ),
    call(
        "file",
        "grep",
        &[
            a("project", Project),
            a("branch", Name),
            a("path", Text),
            a("pattern", Text),
            opt("commit", Name),
        ],
        &PAGE,
        "lines under `path` matching a regular expression",
    ),
    call(
        "file",
        "blame",
        &[
            a("project", Project),
            a("branch", Name),
            a("path", Text),
            opt("commit", Name),
        ],
        &[opt("from_line", Int)],
        "the Commit of each line",
    ),
    call(
        "file",
        "diff",
        &[
            a("project", Project),
            a("left_branch", Name),
            a("left_path", Text),
            a("right_branch", Name),
            a("right_path", Text),
        ],
        &[opt("left_commit", Name), opt("right_commit", Name)],
        "the difference between two files",
    ),
    call(
        "file",
        "write",
        &[
            a("project", Project),
            a("branch", Name),
            a("path", Text),
            a("content", Content),
        ],
        &[MESSAGE, opt("mode", Int)],
        "write a file; one Commit",
    ),
    call(
        "file",
        "edit",
        &[a("project", Project), a("branch", Name), a("path", Text)],
        &[opt("old", Text), opt("new", Text), MESSAGE],
        "replace one unique text in a file; one Commit",
    ),
    call(
        "file",
        "rm",
        &[a("project", Project), a("branch", Name), a("path", Text)],
        &[MESSAGE],
        "remove a path; one Commit",
    ),
    call(
        "file",
        "mkdir",
        &[a("project", Project), a("branch", Name), a("path", Text)],
        &[MESSAGE, opt("mode", Int)],
        "make a folder; one Commit",
    ),
    call(
        "file",
        "mv",
        &[
            a("project", Project),
            a("branch", Name),
            a("from_path", Text),
            a("to_path", Text),
        ],
        &[MESSAGE],
        "move a path; one Commit",
    ),
    call(
        "file",
        "cp",
        &[
            a("project", Project),
            a("branch", Name),
            a("from_path", Text),
            a("to_path", Text),
        ],
        &[MESSAGE],
        "copy a path; one Commit",
    ),
    call(
        "file",
        "apply",
        &[a("project", Project), a("branch", Name), a("changes", Json)],
        &[MESSAGE],
        "apply a list of changes; one Commit",
    ),
    // sandbox (4)
    call(
        "sandbox",
        "create",
        &[a("project", Project), a("name", Name), a("machine", Name)],
        &[opt("image", Text)],
        "start a named Sandbox of a project on a machine",
    ),
    call(
        "sandbox",
        "list",
        &[opt("project", Project)],
        &PAGE,
        "every Sandbox in the home, or one project's",
    ),
    call(
        "sandbox",
        "get",
        &[a("name", Name)],
        &PAGE,
        "a Sandbox and its Workspaces",
    ),
    call(
        "sandbox",
        "close",
        &[a("name", Name)],
        &[opt("force", Bool)],
        "stop a Sandbox",
    ),
    // workspace (5, O14: status)
    call(
        "workspace",
        "mount",
        &[a("sandbox", Name), a("branch", Name)],
        &[],
        "mount a Branch in a Sandbox",
    ),
    call(
        "workspace",
        "exec",
        &[a("ws", Text), a("command", Text)],
        &[
            MESSAGE,
            opt("stream", Bool),
            opt("stdout", Function),
            opt("stderr", Function),
        ],
        "run one bash command in a Workspace",
    ),
    call(
        "workspace",
        "commit",
        &[a("ws", Text)],
        &[MESSAGE, opt("force", Bool)],
        "commit a Workspace's changes to its Branch",
    ),
    call(
        "workspace",
        "unmount",
        &[a("ws", Text)],
        &[opt("discard", Bool), opt("force", Bool)],
        "end a Workspace",
    ),
    call(
        "workspace",
        "status",
        &[a("ws", Text)],
        &[],
        "a Workspace's changes since its base",
    ),
    // machine (8, O11: update)
    call(
        "machine",
        "list",
        &[],
        &[],
        "every machine, and the platforms that could be created here",
    ),
    call(
        "machine",
        "get",
        &[a("name", Name)],
        &[],
        "a machine's record",
    ),
    call(
        "machine",
        "status",
        &[a("name", Name)],
        &[],
        "a machine's live state, free disk and tool version",
    ),
    call(
        "machine",
        "create",
        &[a("name", Name), a("platform", Name)],
        &[
            opt("cpus", Int),
            opt("memory", Size),
            opt("disk", Size),
            opt("image", Text),
            opt("nested", Bool),
            opt("host", Name),
        ],
        "create a VM machine",
    ),
    call(
        "machine",
        "connect",
        &[a("name", Name), a("host", Text)],
        &[
            opt("user", Text),
            opt("port", Int),
            opt("identity", Text),
            opt("known_hosts", Text),
            opt("root", Text),
        ],
        "connect an ssh host as a machine",
    ),
    call(
        "machine",
        "remove",
        &[a("name", Name)],
        &[],
        "delete a VM machine and its disk",
    ),
    call(
        "machine",
        "disconnect",
        &[a("name", Name)],
        &[],
        "forget an ssh machine; its data root stays",
    ),
    call(
        "machine",
        "update",
        &[a("name", Name)],
        &[
            opt("cpus", Int),
            opt("memory", Size),
            opt("image", Text),
            opt("docker", Text),
            opt("wsl", Text),
        ],
        "resize a VM, or set a built-in machine's image and Docker socket",
    ),
    // store (8)
    call(
        "store",
        "list",
        &[],
        &[],
        "every Store, from the home's records",
    ),
    call("store", "get", &[a("name", Name)], &[], "a Store's record"),
    call(
        "store",
        "status",
        &[a("name", Name)],
        &[],
        "a Store's live state",
    ),
    call(
        "store",
        "create",
        &[a("name", Name)],
        &[],
        "an empty sqlite Store on native",
    ),
    call(
        "store",
        "connect",
        &[a("name", Name), a("url", Text)],
        &[opt("password", Text), opt("synchronous_commit", Text)],
        "attach a PostgreSQL database as a Store",
    ),
    call(
        "store",
        "move",
        &[a("name", Name), a("machine", Name)],
        &[PROGRESS],
        "move a sqlite Store to another machine",
    ),
    call(
        "store",
        "remove",
        &[a("name", Name)],
        &[],
        "delete a sqlite Store with every project in it",
    ),
    call(
        "store",
        "disconnect",
        &[a("name", Name)],
        &[],
        "forget a PostgreSQL Store; its data stays",
    ),
    // backup (5)
    call(
        "backup",
        "connect",
        &[a("name", Name), a("url", Text)],
        &[
            opt("key", Text),
            opt("region", Text),
            opt("part_size", Size),
        ],
        "connect an r2 or MinIO bucket",
    ),
    call(
        "backup",
        "disconnect",
        &[a("name", Name)],
        &[],
        "forget a bucket and this home's links to it",
    ),
    call(
        "backup",
        "list",
        &[opt("bucket", Name)],
        &[],
        "every Store file on the connected buckets",
    ),
    call(
        "backup",
        "create",
        &[a("store", Name), a("bucket", Name), opt("file", Name)],
        &[PROGRESS],
        "upload a sqlite Store to a bucket",
    ),
    call(
        "backup",
        "restore",
        &[a("store", Name), a("bucket", Name), opt("file", Name)],
        &[PROGRESS],
        "download a backup as a sqlite Store on native",
    ),
];

/// The call named `family.call`.
pub fn find(name: &str) -> Option<&'static Call> {
    let (family, call) = name.split_once('.')?;
    CALLS.iter().find(|c| c.family == family && c.call == call)
}

/// The whole table as JSON text, for front ends that build their functions
/// from it: `[{ family, call, args: [{ name, kind, optional }], options,
/// help }]`.
pub fn table_json() -> String {
    use layerfs_bridge::control::SdkValue;
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
    let calls = CALLS
        .iter()
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
    super::json::print(&SdkValue::List(calls))
}
