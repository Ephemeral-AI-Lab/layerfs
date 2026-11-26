//! The one report the six doctors answer (call surface 2.0, `calls.json`
//! `doctor`): a tree of the things that were checked, each a [`Node`] with
//! its status, its coverage and one row per check, and the two flat lists
//! every front end reads first, `findings` and `not_checked`.
//!
//! `{ ok, findings, not_checked, status, counts, observed_at, tree }` is
//! the long form; the dispatcher makes the short form from it (`ok` alone,
//! or `findings` and `not_checked`). `ok` is true only when every check was
//! made and nothing was found, so a notice makes it false. A thing's
//! status is the worst outcome among its own checks; the answer's is the
//! worst thing in it; `counts` counts things, once each.
use layerfs_bridge::control::SdkValue;
use layerfs_home::state::BlockedBy;

/// A kind of thing a doctor checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The home itself.
    Home,
    /// A machine.
    Machine,
    /// A Store.
    Store,
    /// A project, named `<store>/<project>`.
    Project,
    /// A Sandbox.
    Sandbox,
    /// A Workspace, named `<sandbox>:<workspace>`.
    Workspace,
    /// A bucket.
    Bucket,
}

impl Kind {
    /// Every kind, in the order the specification lists them.
    pub const ALL: [Kind; 7] = [
        Kind::Home,
        Kind::Machine,
        Kind::Store,
        Kind::Project,
        Kind::Sandbox,
        Kind::Workspace,
        Kind::Bucket,
    ];

    /// The kind's own word: the key that names a thing of this kind in a
    /// node, a finding and a `not_checked` entry.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Home => "home",
            Kind::Machine => "machine",
            Kind::Store => "store",
            Kind::Project => "project",
            Kind::Sandbox => "sandbox",
            Kind::Workspace => "workspace",
            Kind::Bucket => "bucket",
        }
    }

    /// The kind whose own word is `word`.
    pub fn named(word: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.word() == word)
    }

    /// The key of a parent's list of things of this kind, which is also the
    /// progress phase while they are checked.
    pub fn list(self) -> &'static str {
        match self {
            Kind::Home => "homes",
            Kind::Machine => "machines",
            Kind::Store => "stores",
            Kind::Project => "projects",
            Kind::Sandbox => "sandboxes",
            Kind::Workspace => "workspaces",
            Kind::Bucket => "buckets",
        }
    }

    /// The kinds of thing directly on a thing of this kind, in the order
    /// its node lists them.
    pub fn children(self) -> &'static [Kind] {
        match self {
            Kind::Home => &[Kind::Machine, Kind::Store, Kind::Bucket],
            Kind::Machine => &[Kind::Store, Kind::Sandbox],
            Kind::Store => &[Kind::Project],
            Kind::Sandbox => &[Kind::Workspace],
            Kind::Project | Kind::Workspace | Kind::Bucket => &[],
        }
    }
}

/// One thing: its kind and its handle (a home's path, `<store>/<project>`,
/// `<sandbox>:<workspace>`, else its name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thing {
    /// Its kind.
    pub kind: Kind,
    /// Its handle.
    pub name: String,
}

impl Thing {
    /// The thing of `kind` named `name`.
    pub fn new(kind: Kind, name: impl Into<String>) -> Self {
        Thing {
            kind,
            name: name.into(),
        }
    }

    fn field(&self) -> (String, SdkValue) {
        (self.kind.word().to_owned(), SdkValue::text(&self.name))
    }
}

/// How bad a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The thing does not work until it is fixed.
    Error,
    /// It works and something is wrong.
    Warning,
    /// A normal state worth saying, which blocks nothing.
    Notice,
}

impl Severity {
    /// The severity as a finding shows it.
    pub fn word(self) -> &'static str {
        Verdict::from(self).outcome()
    }
}

/// One check's outcome, and by the same words (with `ok` for `pass`) a
/// thing's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The check passed; nothing was found.
    Pass,
    /// It found a notice.
    Notice,
    /// It could not be made.
    NotChecked,
    /// It found a warning.
    Warning,
    /// It found an error.
    Error,
}

impl From<Severity> for Verdict {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Error => Verdict::Error,
            Severity::Warning => Verdict::Warning,
            Severity::Notice => Verdict::Notice,
        }
    }
}

impl Verdict {
    /// Every verdict, the worst first: the order a status is chosen by.
    pub const WORST_FIRST: [Verdict; 5] = [
        Verdict::Error,
        Verdict::Warning,
        Verdict::NotChecked,
        Verdict::Notice,
        Verdict::Pass,
    ];

    /// Its place in [`Self::WORST_FIRST`], counted from the best.
    fn rank(self) -> usize {
        let worst = Self::WORST_FIRST.iter().position(|held| *held == self);
        Self::WORST_FIRST.len() - worst.unwrap_or(0)
    }

    /// The worse of two verdicts.
    pub fn worse(self, other: Verdict) -> Verdict {
        match self.rank() >= other.rank() {
            true => self,
            false => other,
        }
    }

    /// The word of a check's outcome.
    pub fn outcome(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            other => other.status(),
        }
    }

    /// The word of a thing's status, and of the whole answer's.
    pub fn status(self) -> &'static str {
        match self {
            Verdict::Pass => "ok",
            Verdict::Notice => "notice",
            Verdict::NotChecked => "not_checked",
            Verdict::Warning => "warning",
            Verdict::Error => "error",
        }
    }
}

/// How much of a thing was checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    /// Nothing: it could not be checked.
    None,
    /// Its own checks.
    Shallow,
    /// Its own checks and its Full check.
    Full,
}

impl Coverage {
    /// The coverage as a node shows it.
    pub fn word(self) -> &'static str {
        match self {
            Coverage::None => "none",
            Coverage::Shallow => "shallow",
            Coverage::Full => "full",
        }
    }
}

/// Why a check that was tried gave no verdict. Its own list, separate from
/// a state's reason codes, which appear in a report only inside
/// `blocked_by`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoVerdict {
    /// The thing's time budget ran out.
    Timeout,
    /// An operation holds the thing.
    Busy,
    /// The probe's reply did not arrive.
    Lost,
}

impl NoVerdict {
    /// Every reason, in the order the specification lists them.
    pub const ALL: [NoVerdict; 3] = [NoVerdict::Timeout, NoVerdict::Busy, NoVerdict::Lost];

    /// The reason as a `not_checked` entry shows it.
    pub fn word(self) -> &'static str {
        match self {
            NoVerdict::Timeout => "timeout",
            NoVerdict::Busy => "busy",
            NoVerdict::Lost => "lost",
        }
    }
}

/// Something a check found.
#[derive(Clone, Debug)]
pub struct Finding {
    /// The thing it is about: the thing checked, or for a records check any
    /// thing in the home.
    pub thing: Thing,
    /// The check that found it.
    pub check: String,
    /// How bad it is.
    pub severity: Severity,
    /// What is wrong, as a sentence.
    pub message: String,
    /// One runnable shell line that fixes it, when a command does.
    pub fix: Option<String>,
    /// The machine the check was made from, when it was not this computer.
    pub from: Option<String>,
    /// What the check observed; in the long form only.
    pub evidence: SdkValue,
}

impl Finding {
    /// `{ <thing>, check, severity, message, fix, from, evidence }`.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::Map(vec![
            self.thing.field(),
            ("check".to_owned(), SdkValue::text(&self.check)),
            ("severity".to_owned(), SdkValue::text(self.severity.word())),
            ("message".to_owned(), SdkValue::text(&self.message)),
            ("fix".to_owned(), SdkValue::opt_text(self.fix.as_deref())),
            ("from".to_owned(), SdkValue::opt_text(self.from.as_deref())),
            ("evidence".to_owned(), self.evidence.clone()),
        ])
    }
}

/// A check, or a whole thing, that could not be checked: by `blocked_by`
/// (what it needs is not ready, and nothing was tried) or by `reason` (it
/// was tried and gave no verdict), never both.
#[derive(Clone, Debug)]
pub struct NotChecked {
    /// The thing.
    pub thing: Thing,
    /// The check; none when it is the whole thing.
    pub check: Option<String>,
    /// The root of the block.
    pub blocked_by: Option<BlockedBy>,
    /// Why a check that was tried gave no verdict.
    pub reason: Option<NoVerdict>,
}

impl NotChecked {
    /// `{ <thing>, check, blocked_by, reason }`.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::Map(vec![
            self.thing.field(),
            (
                "check".to_owned(),
                SdkValue::opt_text(self.check.as_deref()),
            ),
            ("blocked_by".to_owned(), blocked_by(&self.blocked_by)),
            ("reason".to_owned(), reason(self.reason)),
        ])
    }
}

fn blocked_by(by: &Option<BlockedBy>) -> SdkValue {
    by.as_ref().map_or(SdkValue::Null, BlockedBy::to_sdk)
}

fn reason(reason: Option<NoVerdict>) -> SdkValue {
    SdkValue::opt_text(reason.map(NoVerdict::word))
}

/// One check of a thing: `{ check, outcome, from }` with optional evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The check's id.
    pub check: String,
    /// Its outcome: the worst of what it found.
    pub outcome: Verdict,
    /// The machine it was made from, when it was not this computer.
    pub from: Option<String>,
    /// Structured evidence from an observation that passed.
    pub evidence: Option<SdkValue>,
}

/// One thing in the tree, with what its checks said.
#[derive(Clone, Debug)]
pub struct Node {
    /// The thing.
    pub thing: Thing,
    /// How much of it was checked.
    pub coverage: Coverage,
    /// Its checks, in the order its kind lists them.
    pub rows: Vec<Row>,
    /// What its checks found.
    pub findings: Vec<Finding>,
    /// Its checks that could not be made, and itself when nothing was.
    pub not_checked: Vec<NotChecked>,
    /// The root of the block, when the whole thing was not checked for it.
    pub blocked_by: Option<BlockedBy>,
    /// Why the whole thing gave no report, when it was tried.
    pub reason: Option<NoVerdict>,
    /// What blocks the things on it; never shown, the runner reads it.
    pub gate: Option<BlockedBy>,
    /// The things directly on it, by kind; empty on a shallow child.
    pub children: Vec<Node>,
}

impl Node {
    /// A node with nothing checked yet.
    pub fn new(thing: Thing) -> Self {
        Node {
            thing,
            coverage: Coverage::None,
            rows: Vec::new(),
            findings: Vec::new(),
            not_checked: Vec::new(),
            blocked_by: None,
            reason: None,
            gate: None,
            children: Vec::new(),
        }
    }

    /// The node of a thing that was not checked because `by` is not ready:
    /// no check was tried, and what is on it waits on the same root.
    pub fn blocked(thing: Thing, by: BlockedBy) -> Self {
        let mut node = Node::new(thing.clone());
        node.not_checked.push(NotChecked {
            thing,
            check: None,
            blocked_by: Some(by.clone()),
            reason: None,
        });
        node.blocked_by = Some(by.clone());
        node.gate = Some(by);
        node
    }

    /// Its status: `ok` when every check passed, else the worst outcome
    /// among its checks; a thing that was not checked at all is
    /// `not_checked`.
    pub fn status(&self) -> Verdict {
        let whole = self.blocked_by.is_some() || self.reason.is_some();
        let start = match whole {
            true => Verdict::NotChecked,
            false => Verdict::Pass,
        };
        (self.rows.iter()).fold(start, |worst, row| worst.worse(row.outcome))
    }

    /// The node as the tree and a progress event show it: `{ <thing>,
    /// status, coverage, checks, blocked_by, reason }`, then with
    /// `children` one list per kind of thing on it.
    pub fn to_sdk(&self, children: bool) -> SdkValue {
        let row = |row: &Row| {
            let value = SdkValue::map([
                ("check", SdkValue::text(&row.check)),
                ("outcome", SdkValue::text(row.outcome.outcome())),
                ("from", SdkValue::opt_text(row.from.as_deref())),
            ]);
            match &row.evidence {
                Some(evidence) => {
                    layerfs_home::home::with_field(value, "evidence", evidence.clone())
                }
                None => value,
            }
        };
        let mut fields = vec![
            self.thing.field(),
            ("status".to_owned(), SdkValue::text(self.status().status())),
            ("coverage".to_owned(), SdkValue::text(self.coverage.word())),
            (
                "checks".to_owned(),
                SdkValue::List(self.rows.iter().map(row).collect()),
            ),
            ("blocked_by".to_owned(), blocked_by(&self.blocked_by)),
            ("reason".to_owned(), reason(self.reason)),
        ];
        if children {
            for kind in self.thing.kind.children() {
                let of = self.children.iter().filter(|on| on.thing.kind == *kind);
                let list = of.map(|on| on.to_sdk(false)).collect();
                fields.push((kind.list().to_owned(), SdkValue::List(list)));
            }
        }
        SdkValue::Map(fields)
    }
}

/// One doctor's report: its own thing's node, with the things directly on
/// it as that node's children.
#[derive(Clone, Debug)]
pub struct Report {
    /// The doctor's own thing.
    pub tree: Node,
    /// When the report was made, in milliseconds since 1970.
    pub observed_ms: i64,
}

impl Report {
    /// Every node: the doctor's own thing, then the things on it.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        std::iter::once(&self.tree).chain(&self.tree.children)
    }

    /// Every finding, in the order the things were checked.
    pub fn findings(&self) -> impl Iterator<Item = &Finding> {
        self.nodes().flat_map(|node| &node.findings)
    }

    /// Everything that could not be checked, in the same order.
    pub fn not_checked(&self) -> impl Iterator<Item = &NotChecked> {
        self.nodes().flat_map(|node| &node.not_checked)
    }

    /// Whether every check was made and nothing was found.
    pub fn ok(&self) -> bool {
        self.findings().next().is_none() && self.not_checked().next().is_none()
    }

    /// The answer's status: the worst thing in it.
    pub fn status(&self) -> Verdict {
        (self.nodes()).fold(Verdict::Pass, |worst, node| worst.worse(node.status()))
    }

    /// How many things have each status.
    pub fn count(&self, status: Verdict) -> usize {
        self.nodes().filter(|node| node.status() == status).count()
    }

    /// The long form: `{ ok, findings, not_checked, status, counts,
    /// observed_at, tree }`.
    pub fn to_sdk(&self) -> SdkValue {
        // the order `counts` lists the statuses in
        let counted = [
            Verdict::Pass,
            Verdict::Notice,
            Verdict::Warning,
            Verdict::Error,
            Verdict::NotChecked,
        ];
        let count = |status: &Verdict| {
            let things = i64::try_from(self.count(*status)).unwrap_or(i64::MAX);
            (status.status().to_owned(), SdkValue::Int(things))
        };
        SdkValue::map([
            ("ok", SdkValue::Bool(self.ok())),
            (
                "findings",
                SdkValue::List(self.findings().map(Finding::to_sdk).collect()),
            ),
            (
                "not_checked",
                SdkValue::List(self.not_checked().map(NotChecked::to_sdk).collect()),
            ),
            ("status", SdkValue::text(self.status().status())),
            ("counts", SdkValue::Map(counted.iter().map(count).collect())),
            (
                "observed_at",
                SdkValue::Text(layerfs_home::time::iso(self.observed_ms)),
            ),
            ("tree", self.tree.to_sdk(true)),
        ])
    }
}
