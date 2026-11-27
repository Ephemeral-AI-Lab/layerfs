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
use layerfs_protocol::provision::install::SdkValue;
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

// Bounded per-check observations, independent outcomes and severity:
// what today's target checks write, one row per observation. The frame
// reads them into the checked thing's node ([`Checks::fill`]); a worker
// sends its observations to the host as [`Checks::to_wire`].
use super::runner::CheckId;
use std::collections::BTreeMap;

const RETAINED_CHECKS: usize = 1024;

struct Check {
    id: String,
    outcome: &'static str,
    severity: Option<&'static str>,
    message: String,
    fix: String,
    evidence: SdkValue,
    evidence_on_pass: bool,
    /// The thing it is about, when not the thing checked: `(kind, name)`.
    about: Option<(String, String)>,
    /// The machine it was made from, when not this computer.
    from: Option<String>,
    /// Why it gave no verdict, when the check said.
    reason: Option<NoVerdict>,
    /// What outside the thing kept it from being made.
    waits: Option<BlockedBy>,
}

/// One thing's bounded observations. Callers must redact evidence before
/// adding it.
#[derive(Default)]
pub struct Checks {
    checks: Vec<Check>,
    latest: BTreeMap<String, Option<usize>>,
    truncated: bool,
}
impl Checks {
    fn add(
        &mut self,
        id: &str,
        outcome: &'static str,
        severity: Option<&'static str>,
        message: String,
    ) {
        if self.checks.len() == RETAINED_CHECKS {
            self.truncated = true;
            // A dropped recurrence must not attach its evidence to an older
            // retained observation with the same stable check ID. Keep this
            // map bounded by IDs already present in retained rows.
            if let Some(latest) = self.latest.get_mut(id) {
                *latest = None;
            }
            return;
        }
        let message = if message.chars().count() > 4096 {
            self.truncated = true;
            message.chars().take(4096).collect()
        } else {
            message
        };
        self.latest.insert(id.to_owned(), Some(self.checks.len()));
        self.checks.push(Check {
            id: id.to_owned(),
            outcome,
            severity,
            message,
            fix: String::new(),
            evidence: SdkValue::Null,
            evidence_on_pass: false,
            about: None,
            from: None,
            reason: None,
            waits: None,
        });
    }
    /// The expected behavior was observed.
    pub fn pass(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "pass", None, message.into());
    }
    /// A passing observation whose evidence is visible in the tree.
    #[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
    pub fn pass_evidence(&mut self, id: &str, message: &str, evidence: SdkValue) {
        self.pass(id, message);
        self.evidence(id, evidence);
        if let Some(check) = self.retained(id) {
            check.evidence_on_pass = true;
        }
    }
    /// A concrete error was observed; a failure is not automatically corruption.
    pub fn fail(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "fail", Some("error"), message.into());
    }
    /// A concrete non-fatal problem was observed.
    pub fn warning(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "fail", Some("warning"), message.into());
    }
    /// A normal state worth saying, which blocks nothing.
    pub fn notice(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "fail", Some("notice"), message.into());
    }
    /// The check was tried and an operation held the thing.
    pub fn busy(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "unknown", None, message.into());
        if let Some(c) = self.retained(id) {
            c.reason = Some(NoVerdict::Busy);
        }
    }
    /// The check was not tried: `by`, outside the thing, is not ready.
    pub fn waits(&mut self, id: &str, message: impl Into<String>, by: BlockedBy) {
        self.add(id, "blocked", None, message.into());
        if let Some(c) = self.retained(id) {
            c.waits = Some(by);
        }
    }
    /// The last observation of `id` is about thing `kind` `name`, not the
    /// thing checked (a records check names what it read).
    pub fn about(&mut self, id: &str, kind: &str, name: &str) {
        if let Some(c) = self.retained(id) {
            c.about = Some((kind.to_owned(), name.to_owned()));
        }
    }
    /// The last observation of `id` was made from machine `machine`.
    pub fn from(&mut self, id: &str, machine: &str) {
        if let Some(c) = self.retained(id) {
            c.from = Some(machine.to_owned());
        }
    }
    /// The outcome and severity of the last observation of `id`.
    pub fn outcome(&self, id: &str) -> Option<(&'static str, Option<&'static str>)> {
        let index = self.latest.get(id).copied().flatten()?;
        self.checks.get(index).map(|c| (c.outcome, c.severity))
    }
    /// This requested check could not establish an answer, for no reason
    /// of the closed list: it is `not_checked`, blocked by its own thing
    /// `failed` (never `lost`, which only a reply that did not arrive is;
    /// conformance R4 F3).
    pub fn unknown(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "unknown", None, message.into());
    }
    /// The probe's reply did not arrive: `not_checked`, reason `lost`.
    pub fn lost(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "unknown", None, message.into());
        if let Some(c) = self.retained(id) {
            c.reason = Some(NoVerdict::Lost);
        }
    }
    /// The thing's time ran out before the check gave a verdict:
    /// `not_checked`, reason `timeout`.
    pub fn timeout(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "unknown", None, message.into());
        if let Some(c) = self.retained(id) {
            c.reason = Some(NoVerdict::Timeout);
        }
    }
    /// Attaches bounded, already redacted structured evidence to a retained check.
    pub fn evidence(&mut self, id: &str, evidence: SdkValue) {
        let evidence = if layerfs_home::json::print(&evidence).len() > 16 * 1024 {
            self.truncated = true;
            SdkValue::map([("truncated", SdkValue::Bool(true))])
        } else {
            evidence
        };
        if let Some(c) = self.retained(id) {
            c.evidence = evidence;
        }
    }
    /// Names a suggested action without performing it.
    pub fn fix(&mut self, id: &str, fix: impl Into<String>) {
        if let Some(c) = self.retained(id) {
            c.fix = fix.into();
        }
    }
    fn retained(&mut self, id: &str) -> Option<&mut Check> {
        let index = self.latest.get(id).copied().flatten()?;
        self.checks.get_mut(index)
    }
    /// The observations as a worker sends them to the host: `checks`, one
    /// row per retained observation (`id`, `outcome`, `severity`,
    /// `message`, `fix`, `evidence`), and whether any was left out
    /// (`truncated`).
    pub fn to_wire(&self) -> SdkValue {
        let rows = (self.checks.iter())
            .map(|c| {
                SdkValue::map([
                    ("id", SdkValue::text(&c.id)),
                    ("outcome", SdkValue::text(c.outcome)),
                    (
                        "severity",
                        c.severity.map_or(SdkValue::Null, SdkValue::text),
                    ),
                    ("message", SdkValue::text(&c.message)),
                    ("evidence", c.evidence.clone()),
                    ("fix", SdkValue::text(&c.fix)),
                ])
            })
            .collect();
        SdkValue::map([
            ("checks", SdkValue::List(rows)),
            ("truncated", SdkValue::Bool(self.truncated)),
        ])
    }
}
impl Checks {
    /// Reads the observations into `node`, the thing they checked: one row
    /// per check (and per machine it was made from), its outcome the worst
    /// observed; a failure is a finding, an answer that did not come is a
    /// `not_checked` entry (the reason the check gave: `busy`, `lost` or
    /// `timeout`; else `timeout` when `expired`, the thing's budget having
    /// run out; else blocked by its own thing `failed`), and a
    /// check kept from running by something outside the thing is one with
    /// `blocked_by`. A check id
    /// outside `list` is a bug in the kind's checks: it panics in a debug
    /// build.
    pub fn fill(&self, node: &mut Node, list: &[CheckId], expired: bool) {
        for check in &self.checks {
            let verdict = match (check.outcome, check.severity) {
                ("pass", _) => Verdict::Pass,
                ("fail", Some("error")) => Verdict::Error,
                ("fail", Some("notice")) => Verdict::Notice,
                ("fail", _) => Verdict::Warning,
                _ => Verdict::NotChecked,
            };
            if cfg!(debug_assertions) && !list.iter().any(|listed| listed.id == check.id) {
                panic!(
                    "doctor: check `{}` is not in the {} kind's list",
                    check.id,
                    node.thing.kind.word()
                );
            }
            let at =
                (node.rows.iter()).position(|row| row.check == check.id && row.from == check.from);
            let evidence = || check.evidence_on_pass.then(|| check.evidence.clone());
            match at {
                Some(at) => {
                    let row = &mut node.rows[at];
                    row.outcome = row.outcome.worse(verdict);
                    if let Some(evidence) = evidence() {
                        row.evidence = Some(evidence);
                    }
                }
                None => node.rows.push(Row {
                    check: check.id.clone(),
                    outcome: verdict,
                    from: check.from.clone(),
                    evidence: evidence(),
                }),
            }
            let severity = match verdict {
                Verdict::Error => Severity::Error,
                Verdict::Warning => Severity::Warning,
                Verdict::Notice => Severity::Notice,
                Verdict::Pass => continue,
                Verdict::NotChecked => {
                    // a check with no reason of the closed list is blocked
                    // by its own thing having failed it, not `lost`
                    let failed = || BlockedBy {
                        thing: node.thing.kind.word(),
                        handle: node.thing.name.clone(),
                        reason: "failed",
                    };
                    let (blocked_by, reason) = match (&check.waits, check.reason, expired) {
                        (Some(by), _, _) => (Some(by.clone()), None),
                        (None, Some(reason), _) => (None, Some(reason)),
                        (None, None, true) => (None, Some(NoVerdict::Timeout)),
                        (None, None, false) => (Some(failed()), None),
                    };
                    node.not_checked.push(NotChecked {
                        thing: node.thing.clone(),
                        check: Some(check.id.clone()),
                        blocked_by,
                        reason,
                    });
                    continue;
                }
            };
            node.findings.push(Finding {
                thing: about(check, &node.thing),
                check: check.id.clone(),
                severity,
                message: check.message.clone(),
                fix: Some(check.fix.clone()).filter(|fix| !fix.is_empty()),
                from: check.from.clone(),
                evidence: check.evidence.clone(),
            });
        }
    }
}

/// The thing a finding names: the one a records check read, by its own
/// word (a command record is its Workspace's), else the thing checked.
fn about(check: &Check, checked: &Thing) -> Thing {
    let Some((kind, name)) = &check.about else {
        return checked.clone();
    };
    match (kind.as_str(), Kind::named(kind)) {
        ("exec", _) => {
            let workspace = name.split_once('/').map_or(name.as_str(), |(at, _)| at);
            Thing::new(Kind::Workspace, workspace)
        }
        (_, Some(kind)) => Thing::new(kind, name.clone()),
        (_, None) => checked.clone(),
    }
}
