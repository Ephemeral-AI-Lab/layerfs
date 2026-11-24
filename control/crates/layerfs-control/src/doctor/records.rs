//! The collector of the records checks (`scan`, `checks`): findings by the
//! entity each is about, in the order the checks made them. The home's
//! doctor publishes them under its records check (`host_target`).
use super::target::Checks;
use layerfs_bridge::control::SdkValue;

/// How bad a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Severity {
    /// A call on the entity is refused or does the wrong thing.
    Error,
    /// Left over or out of date; nothing is refused for it.
    Warning,
}

/// One finding.
struct Finding {
    severity: Severity,
    kind: String,
    name: String,
    message: String,
    fix: String,
    evidence: Option<SdkValue>,
}

/// The findings so far, in the order the checks made them.
#[derive(Default)]
pub(super) struct Findings(Vec<Finding>);

impl Findings {
    /// Adds a finding on entity `kind` `name`.
    pub(super) fn add(
        &mut self,
        severity: Severity,
        (kind, name): (&str, &str),
        message: String,
        fix: String,
    ) {
        self.0.push(Finding {
            severity,
            kind: kind.to_owned(),
            name: name.to_owned(),
            message,
            fix,
            evidence: None,
        });
    }

    /// Attaches `evidence` (observed against expected, redacted) to the
    /// finding added last.
    pub(super) fn evidence(&mut self, evidence: SdkValue) {
        if let Some(last) = self.0.last_mut() {
            last.evidence = Some(evidence);
        }
    }

    /// An error on entity `kind` `name`.
    pub(super) fn error(&mut self, entity: (&str, &str), message: String, fix: String) {
        self.add(Severity::Error, entity, message, fix);
    }

    /// A warning on entity `kind` `name`.
    pub(super) fn warning(&mut self, entity: (&str, &str), message: String, fix: String) {
        self.add(Severity::Warning, entity, message, fix);
    }

    /// Whether no check found anything.
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Each finding as a row of check `id`, on the thing it is about, in
    /// the order the checks made them.
    pub(super) fn publish(self, id: &'static str, checks: &mut Checks) {
        for finding in self.0 {
            match finding.severity {
                Severity::Error => checks.fail(id, finding.message),
                Severity::Warning => checks.warning(id, finding.message),
            }
            if !finding.fix.is_empty() {
                checks.fix(id, finding.fix);
            }
            // a finding without its own evidence names the thing it is about
            let evidence = finding.evidence.unwrap_or_else(|| {
                SdkValue::map([
                    ("kind", SdkValue::text(&finding.kind)),
                    ("name", SdkValue::text(&finding.name)),
                ])
            });
            checks.evidence(id, evidence);
            checks.about(id, &finding.kind, &finding.name);
        }
    }
}
