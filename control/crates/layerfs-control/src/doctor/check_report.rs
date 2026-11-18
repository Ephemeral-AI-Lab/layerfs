//! Bounded per-check observations, independent outcomes and severity:
//! what today's target checks write, one row per observation. The frame
//! reads them into the checked thing's node ([`Checks::fill`]); `to_sdk` is
//! the per-target report those checks' own tests still read.
use super::frame::CheckId;
use super::report::{Finding, Kind, NoVerdict, Node, NotChecked, Row, Severity, Thing, Verdict};
use layerfs_bridge::control::SdkValue;
use layerfs_home::state::BlockedBy;
use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const RETAINED_CHECKS: usize = 1024;

struct Check {
    id: String,
    outcome: &'static str,
    severity: Option<&'static str>,
    message: String,
    fix: String,
    evidence: SdkValue,
    blocked_by: Vec<String>,
    observed_ms: u64,
    duration_ms: Option<u64>,
    /// The thing it is about, when not the thing checked: `(kind, name)`.
    about: Option<(String, String)>,
    /// The machine it was made from, when not this computer.
    from: Option<String>,
    /// Why it gave no verdict, when the check said.
    reason: Option<NoVerdict>,
    /// What outside the thing kept it from being made.
    waits: Option<BlockedBy>,
}

/// One target's bounded report. Callers must redact evidence before adding it.
pub struct Checks {
    kind: String,
    name: String,
    started: Instant,
    observed_at_ms: u64,
    checks: Vec<Check>,
    latest: BTreeMap<String, Option<usize>>,
    counts: BTreeMap<&'static str, u64>,
    complete: bool,
    errors: u64,
    truncated: bool,
}
impl Checks {
    /// Begins observations for one entity.
    pub fn new(kind: &str, name: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            name: name.to_owned(),
            started: Instant::now(),
            observed_at_ms: millis(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default(),
            ),
            checks: Vec::new(),
            latest: BTreeMap::new(),
            counts: BTreeMap::new(),
            complete: true,
            errors: 0,
            truncated: false,
        }
    }
    fn add(
        &mut self,
        id: &str,
        outcome: &'static str,
        severity: Option<&'static str>,
        message: String,
        blocked_by: Vec<String>,
    ) {
        *self.counts.entry(outcome).or_default() += 1;
        if severity == Some("error") {
            self.errors += 1;
        }
        if matches!(outcome, "blocked" | "unknown") {
            self.complete = false;
        }
        if self.checks.len() == RETAINED_CHECKS {
            self.truncated = true;
            self.complete = false;
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
            self.complete = false;
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
            blocked_by,
            observed_ms: millis(self.started.elapsed()),
            duration_ms: None,
            about: None,
            from: None,
            reason: None,
            waits: None,
        });
    }
    /// The expected behavior was observed.
    pub fn pass(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "pass", None, message.into(), vec![]);
    }
    /// A concrete error was observed; a failure is not automatically corruption.
    pub fn fail(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "fail", Some("error"), message.into(), vec![]);
    }
    /// A concrete non-fatal problem was observed.
    pub fn warning(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "fail", Some("warning"), message.into(), vec![]);
    }
    /// A normal state worth saying, which blocks nothing.
    pub fn notice(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "fail", Some("notice"), message.into(), vec![]);
    }
    /// The check was tried and an operation held the thing.
    pub fn busy(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "unknown", None, message.into(), vec![]);
        if let Some(c) = self.retained(id) {
            c.reason = Some(NoVerdict::Busy);
        }
    }
    /// The check was not tried: `by`, outside the thing, is not ready.
    pub fn waits(&mut self, id: &str, message: impl Into<String>, by: BlockedBy) {
        self.add(id, "blocked", None, message.into(), vec![]);
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
    /// Forgets every observation of the checks `ids`: they do not apply.
    pub fn forget(&mut self, ids: &[&str]) {
        self.checks.retain(|c| !ids.contains(&c.id.as_str()));
        self.latest.clear();
        for (index, c) in self.checks.iter().enumerate() {
            self.latest.insert(c.id.clone(), Some(index));
        }
    }
    /// The ids of the observations a report shows (not a skipped check, or
    /// one a check of its own thing kept from running) that `list` does
    /// not hold, each once.
    pub fn unlisted(&self, list: &[CheckId]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.checks {
            let shown = match c.outcome {
                "skipped" => false,
                "blocked" => c.waits.is_some(),
                _ => true,
            };
            if shown && !list.iter().any(|listed| listed.id == c.id) && !out.contains(&c.id) {
                out.push(c.id.clone());
            }
        }
        out
    }
    /// The outcome and severity of the last observation of `id`.
    pub fn outcome(&self, id: &str) -> Option<(&'static str, Option<&'static str>)> {
        let index = self.latest.get(id).copied().flatten()?;
        self.checks.get(index).map(|c| (c.outcome, c.severity))
    }
    /// The evidence of the last observation of `id`.
    pub fn evidence_of(&self, id: &str) -> Option<&SdkValue> {
        let index = self.latest.get(id).copied().flatten()?;
        self.checks.get(index).map(|c| &c.evidence)
    }
    /// This requested check could not establish an answer.
    pub fn unknown(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "unknown", None, message.into(), vec![]);
    }
    /// Intentionally excluded or not applicable to the requested mode.
    pub fn skip(&mut self, id: &str, message: impl Into<String>) {
        self.add(id, "skipped", None, message.into(), vec![]);
    }
    /// A prerequisite prevented this check; the cause is recorded separately.
    pub fn blocked(&mut self, id: &str, message: impl Into<String>, dependency: &str) {
        self.add(
            id,
            "blocked",
            None,
            message.into(),
            vec![dependency.to_owned()],
        );
    }
    /// Attaches bounded, already redacted structured evidence to a retained check.
    pub fn evidence(&mut self, id: &str, evidence: SdkValue) {
        let evidence = if layerfs_home::json::print(&evidence).len() > 16 * 1024 {
            self.truncated = true;
            self.complete = false;
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
    /// Records measured time for a retained check.
    pub fn duration(&mut self, id: &str, elapsed: Duration) {
        if let Some(c) = self.retained(id) {
            c.duration_ms = Some(millis(elapsed));
        }
    }
    /// Explicitly marks missing coverage even if a stage already has a failure.
    pub fn incomplete(&mut self) {
        self.complete = false;
    }
    fn retained(&mut self, id: &str) -> Option<&mut Check> {
        let index = self.latest.get(id).copied().flatten()?;
        self.checks.get_mut(index)
    }
    /// Shapes an additive report retaining legacy ok/findings fields.
    pub fn to_sdk(&self) -> SdkValue {
        let entity = || {
            SdkValue::map([
                ("kind", SdkValue::text(&self.kind)),
                ("name", SdkValue::text(&self.name)),
            ])
        };
        let rows = self
            .checks
            .iter()
            .map(|c| {
                SdkValue::map([
                    ("id", SdkValue::text(&c.id)),
                    ("code", SdkValue::text(&c.id)),
                    ("outcome", SdkValue::text(c.outcome)),
                    (
                        "severity",
                        c.severity.map_or(SdkValue::Null, SdkValue::text),
                    ),
                    ("entity", entity()),
                    ("message", SdkValue::text(&c.message)),
                    ("evidence", c.evidence.clone()),
                    ("fix", SdkValue::text(&c.fix)),
                    (
                        "blocked_by",
                        SdkValue::List(c.blocked_by.iter().map(SdkValue::text).collect()),
                    ),
                    ("observed_ms", number(c.observed_ms)),
                    ("duration_ms", c.duration_ms.map_or(SdkValue::Null, number)),
                ])
            })
            .collect();
        let findings = self
            .checks
            .iter()
            .filter_map(|c| {
                c.severity.map(|severity| {
                    SdkValue::map([
                        ("severity", SdkValue::text(severity)),
                        ("entity", entity()),
                        ("message", SdkValue::text(&c.message)),
                        ("fix", SdkValue::text(&c.fix)),
                        ("check_id", SdkValue::text(&c.id)),
                    ])
                })
            })
            .collect();
        SdkValue::map([
            ("ok", SdkValue::Bool(self.errors == 0)),
            ("complete", SdkValue::Bool(self.complete)),
            ("target", entity()),
            ("checks", SdkValue::List(rows)),
            ("findings", SdkValue::List(findings)),
            ("truncated", SdkValue::Bool(self.truncated)),
            ("observed_at_ms", number(self.observed_at_ms)),
            ("duration_ms", number(millis(self.started.elapsed()))),
            (
                "counts",
                SdkValue::Map(
                    self.counts
                        .iter()
                        .map(|(k, v)| ((*k).to_owned(), number(*v)))
                        .collect(),
                ),
            ),
        ])
    }
}
impl Checks {
    /// Reads the observations into `node`, the thing they checked: one row
    /// per check (and per machine it was made from), its outcome the worst
    /// observed; a failure is a finding, an answer that did not come is a
    /// `not_checked` entry (`timeout` when `expired`, the thing's budget
    /// having run out, else `lost`, unless the check said `busy`), and a
    /// check kept from running by something outside the thing is one with
    /// `blocked_by`. A skipped check, or one a failed check of the same
    /// thing kept from running, does not apply and is left out. A check id
    /// outside `list` is a bug in the kind's checks: it panics in a debug
    /// build.
    pub fn fill(&self, node: &mut Node, list: &[CheckId], expired: bool) {
        for check in &self.checks {
            let verdict = match (check.outcome, check.severity) {
                ("pass", _) => Verdict::Pass,
                ("fail", Some("error")) => Verdict::Error,
                ("fail", Some("notice")) => Verdict::Notice,
                ("fail", _) => Verdict::Warning,
                ("unknown", _) => Verdict::NotChecked,
                ("blocked", _) if check.waits.is_some() => Verdict::NotChecked,
                _ => continue,
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
            match at {
                Some(at) => node.rows[at].outcome = node.rows[at].outcome.worse(verdict),
                None => node.rows.push(Row {
                    check: check.id.clone(),
                    outcome: verdict,
                    from: check.from.clone(),
                }),
            }
            let severity = match verdict {
                Verdict::Error => Severity::Error,
                Verdict::Warning => Severity::Warning,
                Verdict::Notice => Severity::Notice,
                Verdict::Pass => continue,
                Verdict::NotChecked => {
                    let reason = match (&check.waits, check.reason, expired) {
                        (Some(_), _, _) => None,
                        (None, Some(reason), _) => Some(reason),
                        (None, None, true) => Some(NoVerdict::Timeout),
                        (None, None, false) => Some(NoVerdict::Lost),
                    };
                    node.not_checked.push(NotChecked {
                        thing: node.thing.clone(),
                        check: Some(check.id.clone()),
                        blocked_by: check.waits.clone(),
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

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
fn number(value: u64) -> SdkValue {
    SdkValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
}
