//! Message 1 (#804 §5.2 step 2): one `SELECT`, no transaction, no locks,
//! naming everything the Rust side matches against the frozen list and
//! message 2 asserts unchanged.
use crate::upgrade::UpgradeError;
use postgres::SimpleQueryMessage;

/// What message 1 read.
#[derive(Clone, Debug)]
pub(crate) struct Check {
    pub(crate) server_version_num: i64,
    pub(crate) meta_version: i64,
    pub(crate) policy_version: Option<i64>,
    pub(crate) source_sha256: String,
    pub(crate) recorded_definition: String,
    pub(crate) live_definition: String,
    pub(crate) catalog_id: String,
    pub(crate) binding_key: String,
    pub(crate) incarnation: i64,
    /// `gc_op` rows, `-1` when the schema has no `gc_op` table.
    pub(crate) gc_ops: i64,
    pub(crate) attempt_index: bool,
    /// Version CHECKs with their generated names and the recorded version.
    pub(crate) version_checks: i64,
}

/// The live column definition of the current schema, as one text value
/// (`schema.rs` `definition`, aggregated on the server).
pub(crate) const LIVE_COLUMNS: &str = "(SELECT coalesce(string_agg(table_name::text||' '||\
column_name::text||' '||data_type::text||' '||is_nullable::text||chr(10),'' \
ORDER BY table_name, ordinal_position),'') FROM information_schema.columns \
WHERE table_schema = current_schema())";

/// Message 1's text for `schema` (already checked as a plain identifier).
pub(crate) fn message(schema: &str) -> String {
    let live = LIVE_COLUMNS.replace("current_schema()", &format!("'{schema}'"));
    format!(
        "SELECT current_setting('server_version_num'),h.schema_version,\
         (SELECT p.schema_version FROM {schema}.store_policy p WHERE p.id=1),\
         encode(sha256(convert_to(h.schema_source,'UTF8')),'hex'),h.schema_definition,{live},\
         encode(h.catalog_id,'hex'),encode(h.binding_key,'hex'),h.catalog_incarnation,\
         CASE WHEN to_regclass('{schema}.gc_op') IS NULL THEN -1 ELSE \
         (xpath('/row/c/text()',query_to_xml('SELECT count(*) AS c FROM {schema}.gc_op',\
         false,true,'')))[1]::text::bigint END,\
         to_regclass('{schema}.commit_attempt') IS NOT NULL,\
         (SELECT count(*) FROM pg_constraint WHERE connamespace='{schema}'::regnamespace \
         AND conname IN ('history_meta_schema_version_check','store_policy_schema_version_check') \
         AND pg_get_constraintdef(oid)=format('CHECK ((schema_version = %s))',h.schema_version)) \
         FROM {schema}.history_meta h WHERE h.id=1"
    )
}

/// Parses message 1's reply.
pub(crate) fn parse(messages: Vec<SimpleQueryMessage>) -> Result<Check, UpgradeError> {
    let mut rows = messages.into_iter().filter_map(|m| match m {
        SimpleQueryMessage::Row(row) => Some(row),
        _ => None,
    });
    let (Some(row), None) = (rows.next(), rows.next()) else {
        return Err(UpgradeError::Unsupported(
            "the schema's history_meta does not hold exactly one row".to_owned(),
        ));
    };
    let bad = |what: &str| UpgradeError::Unsupported(format!("history_meta: unreadable {what}"));
    let text = |i: usize| row.get(i).map(str::to_owned);
    let int = |i: usize| text(i).and_then(|t| t.parse::<i64>().ok());
    Ok(Check {
        server_version_num: int(0).ok_or_else(|| bad("server version"))?,
        meta_version: int(1).ok_or_else(|| bad("schema_version"))?,
        policy_version: int(2),
        source_sha256: text(3).ok_or_else(|| bad("schema_source"))?,
        recorded_definition: text(4).ok_or_else(|| bad("schema_definition"))?,
        live_definition: text(5).ok_or_else(|| bad("live columns"))?,
        catalog_id: text(6).ok_or_else(|| bad("catalog_id"))?,
        binding_key: text(7).ok_or_else(|| bad("binding_key"))?,
        incarnation: int(8).ok_or_else(|| bad("catalog_incarnation"))?,
        gc_ops: int(9).ok_or_else(|| bad("gc_op"))?,
        attempt_index: text(10).as_deref() == Some("t"),
        version_checks: int(11).ok_or_else(|| bad("version checks"))?,
    })
}

/// Whether `schema` is a plain lowercase identifier, as a Store schema
/// name must be, so it can be written into the text unquoted.
pub(crate) fn plain_schema(schema: &str) -> bool {
    let mut bytes = schema.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
        && schema.len() <= 63
}
