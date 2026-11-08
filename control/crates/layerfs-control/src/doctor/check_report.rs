//! Bounded per-check observations, independent outcomes and severity.
use layerfs_bridge::control::SdkValue;
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
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
fn number(value: u64) -> SdkValue {
    SdkValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
}
