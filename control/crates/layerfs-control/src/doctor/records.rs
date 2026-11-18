//! The collector of the records checks (`scan`, `checks`, `convert`) and of
//! the leftover walk (`leftover_report`): findings by the entity each is
//! about, in the order the checks made them. The home's doctor publishes
//! the records findings under its records check (`host_target`); the
//! leftover rows answer [`leftovers`], which no doctor reports.
//!
//! [`leftovers`] answers `{ ok, findings: [{ severity, entity: { kind,
//! name }, message, fix, leftover }], leftovers }`: each finding is
//! something an interrupted operation left, and `leftovers` lists each
//! machine whose folders were walked for them. It is the list `doctor.fix`
//! takes its ids from until `home.leftovers` replaces it.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{detect, Layout};
use layerfs_home::outcome::Outcome;
use std::path::Path;

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
    /// What a leftover's finding adds: its machine, kind, path, bytes, age
    /// and fix id.
    leftover: Option<SdkValue>,
}

/// The findings so far, in the order the checks made them, and the
/// machines walked for leftovers.
#[derive(Default)]
pub(super) struct Findings(Vec<Finding>, Vec<SdkValue>);

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
            leftover: None,
        });
    }

    /// A warning on entity `kind` `name` for something an interrupted
    /// operation left, with its `leftover` fields.
    pub(super) fn leftover(
        &mut self,
        entity: (&str, &str),
        message: String,
        fix: String,
        leftover: SdkValue,
    ) {
        self.warning(entity, message, fix);
        if let Some(finding) = self.0.last_mut() {
            finding.leftover = Some(leftover);
        }
    }

    /// One machine walked for leftovers: its row of the report's
    /// `leftovers`.
    pub(super) fn walked(&mut self, machine: SdkValue) {
        self.1.push(machine);
    }

    /// An error on entity `kind` `name`.
    pub(super) fn error(&mut self, entity: (&str, &str), message: String, fix: String) {
        self.add(Severity::Error, entity, message, fix);
    }

    /// A warning on entity `kind` `name`.
    pub(super) fn warning(&mut self, entity: (&str, &str), message: String, fix: String) {
        self.add(Severity::Warning, entity, message, fix);
    }

    pub(super) fn to_sdk(&self) -> SdkValue {
        let ok = !self.0.iter().any(|f| f.severity == Severity::Error);
        let findings = self.0.iter().map(|finding| {
            let severity = match finding.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            let mut fields = vec![
                ("severity", SdkValue::text(severity)),
                (
                    "entity",
                    SdkValue::map([
                        ("kind", SdkValue::text(&finding.kind)),
                        ("name", SdkValue::text(&finding.name)),
                    ]),
                ),
                ("message", SdkValue::text(&finding.message)),
                ("fix", SdkValue::text(&finding.fix)),
            ];
            fields.extend(finding.leftover.clone().map(|left| ("leftover", left)));
            SdkValue::Map(
                (fields.into_iter())
                    .map(|(field, value)| (field.to_owned(), value))
                    .collect(),
            )
        });
        let mut report = vec![
            ("ok".to_owned(), SdkValue::Bool(ok)),
            ("findings".to_owned(), SdkValue::List(findings.collect())),
        ];
        // only a home whose folders were walked has the rows: a home that
        // does not exist yet answers `{ ok, findings }` as before
        if !self.1.is_empty() {
            report.push(("leftovers".to_owned(), SdkValue::List(self.1.clone())));
        }
        SdkValue::Map(report)
    }
}

/// What interrupted operations left on `native` and on each running
/// machine of `home`, changing nothing: the walk `doctor.fix` removes one
/// entry of. A home that is not a current-layout home has none.
pub fn leftovers(home: &Path) -> Outcome {
    let mut left = Findings::default();
    if let Ok(Layout::Current(_)) = detect(home) {
        // the scan's own findings are the home doctor's records check
        let scanned = super::scan::scan(home, &mut Findings::default())?;
        super::leftover_report::check(home, &scanned, &mut left);
    }
    Ok(left.to_sdk())
}
