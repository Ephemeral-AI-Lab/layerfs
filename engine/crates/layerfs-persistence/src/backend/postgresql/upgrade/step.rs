//! The PostgreSQL chain and its one step, 3 -> 4 (#804 §4, §5.2 step 3):
//! `commit.attempt` and its partial unique index, purely additive. A step
//! here appends its statements to message 2's text; the server runs the
//! whole text as one transaction.
use super::check::Check;
use super::frozen::{self, VERSION_CHECKS};
use crate::upgrade::{Step, UpgradeError};

/// Message 2 as the chain builds it.
pub(crate) struct Context {
    pub(crate) check: Check,
    /// The version the text has brought the Store to.
    pub(crate) version: i64,
    /// Whether the Store has Init's acquisition tables (its recorded variant).
    pub(crate) acquisition: bool,
    /// The statements so far.
    pub(crate) text: String,
}

/// The oldest PostgreSQL format with a step path (version 2 has none,
/// owner 2026-11-25).
pub(crate) const FLOOR: i64 = 3;

/// The PostgreSQL chain, oldest first.
pub(crate) fn steps() -> [Step<Context>; 1] {
    [Step {
        from: 3,
        to: 4,
        raw: &[(3, 4)],
        variants: &[frozen::VARIANT],
        recognize: recognize_3,
        apply: apply_3,
    }]
}

/// Step 3 -> 4 accepts the Store exactly when both version cells say 3, it
/// records the schema-3 source (with or without the acquisition tables),
/// its recorded definition is its live one and that is the schema-3
/// definition, the attempt index is absent and both version CHECKs carry
/// their generated names.
fn recognize_3(context: &mut Context) -> Result<Option<&'static str>, UpgradeError> {
    let check = &context.check;
    if context.version != 3 || check.meta_version != 3 || check.policy_version != Some(3) {
        return Ok(None);
    }
    let acquisition = if check.source_sha256 == frozen::sha256_hex(&frozen::source_v3(true)) {
        true
    } else if check.source_sha256 == frozen::sha256_hex(&frozen::source_v3(false)) {
        false
    } else {
        return Ok(None);
    };
    let exact = check.recorded_definition == check.live_definition
        && check.live_definition == frozen::columns_v3(acquisition)
        && !check.attempt_index
        && check.gc_ops == 0
        && check.version_checks == 2;
    context.acquisition = acquisition;
    Ok(exact.then_some(frozen::VARIANT))
}

/// Step 3 -> 4's statements. Each version CHECK is dropped before its row
/// is updated and added after, since adding `CHECK (schema_version = 4)`
/// validates the existing row.
fn apply_3(context: &mut Context) -> Result<(), UpgradeError> {
    let source = frozen::source_v4(context.acquisition);
    let columns = frozen::columns_v4(context.acquisition);
    let (meta, meta_check) = VERSION_CHECKS[0];
    let (policy, policy_check) = VERSION_CHECKS[1];
    context.text.push_str(&format!(
        "ALTER TABLE \"commit\" ADD COLUMN attempt bytea \
         CHECK (attempt IS NULL OR octet_length(attempt) = 16);\n\
         CREATE UNIQUE INDEX commit_attempt ON \"commit\" (attempt) WHERE attempt IS NOT NULL;\n\
         ALTER TABLE {meta} DROP CONSTRAINT {meta_check};\n\
         UPDATE {meta} SET schema_version = 4, schema_source = {}, schema_definition = {}, \
         catalog_incarnation = catalog_incarnation + 1 WHERE id = 1;\n\
         ALTER TABLE {meta} ADD CONSTRAINT {meta_check} CHECK (schema_version = 4);\n\
         ALTER TABLE {policy} DROP CONSTRAINT {policy_check};\n\
         UPDATE {policy} SET schema_version = 4 WHERE id = 1;\n\
         ALTER TABLE {policy} ADD CONSTRAINT {policy_check} CHECK (schema_version = 4);\n",
        literal(&source)?,
        literal(&columns)?
    ));
    context.version = 4;
    Ok(())
}

/// A dollar-quoted literal of `text` under a tag the text cannot hold.
pub(crate) fn literal(text: &str) -> Result<String, UpgradeError> {
    const TAG: &str = "$layerfs_upgrade$";
    match text.contains(TAG) {
        true => Err(UpgradeError::Failed(
            "a frozen text holds the quoting tag".to_owned(),
        )),
        false => Ok(format!("{TAG}{text}{TAG}")),
    }
}
