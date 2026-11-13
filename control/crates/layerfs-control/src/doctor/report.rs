//! The report: `{ ok, findings: [{ severity, entity: { kind, name },
//! message, fix }] }`, `ok` false when any finding is an error.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{detect, Layout};
use layerfs_home::json;
use layerfs_home::outcome::Outcome;
use std::path::{Path, PathBuf};

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
        });
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
            SdkValue::map([
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
            ])
        });
        SdkValue::map([
            ("ok", SdkValue::Bool(ok)),
            ("findings", SdkValue::List(findings.collect())),
        ])
    }
}

/// `layerfs doctor` on `home`: the report, which changes nothing in the
/// home or on any machine. A home that does not exist yet is `ok` with no
/// findings.
pub fn doctor(home: &Path) -> Outcome {
    let mut findings = Findings::default();
    let shown = home.display().to_string();
    let entity = ("home", shown.as_str());
    match detect(home) {
        Err(refusal) => findings.error(
            entity,
            format!("layerfs.json cannot be read: {}", refusal.message),
            format!(
                "no call reads this home: repair {} by hand, or use the LayerFS build that wrote it",
                home.join("layerfs.json").display()
            ),
        ),
        Ok(Layout::Empty) => {}
        Ok(Layout::Old) => findings.warning(
            entity,
            "a LayerFS 1.1.0 home, not converted yet".to_owned(),
            "`layerfs import-env` records this environment's settings; the first call that writes converts the home".to_owned(),
        ),
        Ok(Layout::Previous(_)) => super::convert::check(home, &mut findings),
        Ok(Layout::Current(_)) => {
            let scanned = super::scan::scan(home, &mut findings)?;
            super::checks::check(home, &scanned, &mut findings)?;
            super::capacity::check(home, &scanned, &mut findings);
            super::workers::check(home, &scanned, &mut findings);
        }
    }
    Ok(findings.to_sdk())
}

/// [`doctor`] as the Node and Python bindings answer it: JSON text
/// `{ value }` or `{ refused: { code, message, detail } }`, for the home the
/// caller resolved (or its refusal).
pub fn doctor_json(home: Result<PathBuf, SdkRefusal>) -> String {
    let answer = match home.and_then(|home| doctor(&home)) {
        Ok(value) => SdkValue::map([("value", value)]),
        Err(refusal) => SdkValue::map([(
            "refused",
            SdkValue::map([
                ("code", SdkValue::Text(refusal.code)),
                ("message", SdkValue::Text(refusal.message)),
                ("detail", refusal.detail),
            ]),
        )]),
    };
    json::print(&answer)
}
