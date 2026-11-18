//! The collector of the records checks (`scan`, `checks`, `convert`):
//! findings by the entity each is about, in the order the checks made
//! them. The home's doctor publishes them under its records check
//! (`host_target`).
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
            let fields = vec![
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
            SdkValue::Map(
                (fields.into_iter())
                    .map(|(field, value)| (field.to_owned(), value))
                    .collect(),
            )
        });
        SdkValue::map([
            ("ok", SdkValue::Bool(ok)),
            ("findings", SdkValue::List(findings.collect())),
        ])
    }
}
