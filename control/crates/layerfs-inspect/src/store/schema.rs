//! A Store doctor's `store.schema` evidence (#804 §2): the format a
//! Store's shallow cut read, or the engine's words for one it could not.
use super::super::doctor::budget::Checks;
use layerfs_home::reply::values::map_of;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::admin::record::store::StoreRecord;

const SCHEMA: &str = super::doctor::SCHEMA;

/// `store.schema`'s evidence (#804 §2): `{ version, latest, status, raw,
/// variant }`, the format with the raw encoding (`user_version`, or
/// `history_meta.schema_version`) and the recorded-source variant.
pub(super) fn format_evidence(evidence: &layerfs_persistence::upgrade::FormatEvidence) -> SdkValue {
    let format = &evidence.format;
    SdkValue::map([
        (
            "version",
            format.version.map_or(SdkValue::Null, SdkValue::Int),
        ),
        ("latest", SdkValue::Int(format.latest)),
        ("status", SdkValue::text(format.status.word())),
        ("raw", SdkValue::Int(evidence.raw)),
        ("variant", SdkValue::opt_text(evidence.variant.clone())),
    ])
}

/// The engine's words for a format it could not read.
pub(super) fn upgrade_error(error: &layerfs_persistence::upgrade::UpgradeError) -> String {
    use layerfs_persistence::upgrade::UpgradeError;
    match error {
        UpgradeError::InUse(why)
        | UpgradeError::Busy(why)
        | UpgradeError::Unsupported(why)
        | UpgradeError::Failed(why)
        | UpgradeError::Unknown(why) => why.clone(),
        UpgradeError::Cancelled => layerfs_store::upgrade::CANCELLED.to_owned(),
    }
}

/// `fields` with the shallow cut's `format` (`{ version, latest, status,
/// raw, variant }` or `{ error }`) as `format` beside them.
pub(super) fn with_format(
    mut fields: Vec<(&str, SdkValue)>,
    format: Option<&SdkValue>,
) -> SdkValue {
    if let Some(format) = format {
        fields.push(("format", format.clone()));
    }
    map_of(fields)
}

/// PostgreSQL Store `record`'s format as `store.schema`'s evidence: one
/// connect and one `SELECT` (`postgresql_format_evidence`).
pub(super) fn postgresql_format(record: &StoreRecord, checks: &mut Checks) {
    let url = layerfs_store::admin::record::store::url(record);
    let schema = layerfs_store::upgrade::postgresql_schema(record);
    let format = match (url, schema) {
        (Ok(url), Ok(schema)) => {
            match layerfs_persistence::upgrade::postgresql_format_evidence(&url, &schema) {
                Ok(evidence) => format_evidence(&evidence),
                Err(error) => SdkValue::map([("error", SdkValue::text(upgrade_error(&error)))]),
            }
        }
        (Err(error), _) | (_, Err(error)) => {
            SdkValue::map([("error", SdkValue::text(error.message))])
        }
    };
    checks.evidence(SCHEMA, with_format(Vec::new(), Some(&format)));
}
