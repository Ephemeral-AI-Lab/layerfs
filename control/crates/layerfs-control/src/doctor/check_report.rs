//! Bounded per-check observations, independent outcomes and severity:
//! what today's target checks write, one row per observation. The frame
//! reads them into the checked thing's node ([`Checks::fill`]); a worker
//! sends its observations to the host as [`Checks::to_wire`].
use super::frame::CheckId;
use super::report::{Finding, Kind, NoVerdict, Node, NotChecked, Row, Severity, Thing, Verdict};
use layerfs_bridge::control::SdkValue;
use layerfs_home::state::BlockedBy;
use std::collections::BTreeMap;

const RETAINED_CHECKS: usize = 1024;

struct Check {
    id: String,
    outcome: &'static str,
    severity: Option<&'static str>,
    message: String,
    fix: String,
    evidence: SdkValue,
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
