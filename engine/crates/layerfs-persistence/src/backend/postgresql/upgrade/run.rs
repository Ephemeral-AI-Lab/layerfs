//! `store upgrade` on a PostgreSQL Store (#804 §5.2): a raw client from the
//! connector, simple-protocol text only, connect and exactly two messages.
//! Message 1 reads; message 2 is one text the server runs as one
//! transaction (locks, asserts, the chain, the checks, COMMIT) without
//! waiting for the client, so no lock is held across a round trip.
use super::check::{self, Check, LIVE_COLUMNS};
use super::frozen::{self, VERSION_CHECKS};
use super::step::{self, literal, Context, FLOOR};
use crate::backend::postgresql::session::connection::{Connector, Dial};
use crate::upgrade::{
    run as run_chain, Format, FormatEvidence, FormatStatus, UpgradeError, UpgradeOutcome,
    UpgradeReport, UpgradeRequest, UpgradeWork, POSTGRESQL_LATEST,
};
use crate::PostgresSelection;
use layerfs_history::CatalogId;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long message 2 waits for a lock it does not take with `NOWAIT`.
const LOCK_TIMEOUT_MS: u64 = 2000;
/// The SQLSTATE of message 2's assert: the Store changed after message 1.
const CHANGED: &str = "LFU01";
/// The SQLSTATE of a failed check before COMMIT (§7).
const VERIFY: &str = "LFU02";
/// The oldest server with `sha256()` (11).
const LEAST_SERVER: i64 = 110_000;

fn connect(url: &str, schema: &str) -> Result<(Connector, postgres::Client), UpgradeError> {
    if !check::plain_schema(schema) {
        return Err(UpgradeError::Failed(format!(
            "schema name {schema:?} is not a Store schema name"
        )));
    }
    let connector = Connector::new(&PostgresSelection::new(url, schema))
        .map_err(|_| UpgradeError::Failed("the PostgreSQL URL does not parse".to_owned()))?;
    let client = connector.connect_raw().map_err(|dial| match dial {
        Dial::Driver(e) => UpgradeError::Failed(format!("connecting to PostgreSQL: {e}")),
        Dial::Silent(limit) => UpgradeError::Failed(format!(
            "connecting to PostgreSQL: no answer within {} ms",
            limit.as_millis()
        )),
    })?;
    Ok((connector, client))
}

/// Message 1 on `client`.
fn read(client: &mut postgres::Client, schema: &str) -> Result<Check, UpgradeError> {
    match client.simple_query(&check::message(schema)) {
        Ok(messages) => check::parse(messages),
        Err(e) => Err(match e.code().map(|c| c.code()) {
            Some("42P01" | "3F000" | "42703") => UpgradeError::Unsupported(format!(
                "schema {schema} holds no LayerFS PostgreSQL Store: {}",
                server_words(&e)
            )),
            Some(_) => UpgradeError::Failed(format!("message 1: {}", server_words(&e))),
            None => UpgradeError::Unknown(format!("message 1: {e}")),
        }),
    }
}

fn format_of(version: i64) -> Format {
    let status = match version {
        4 => FormatStatus::Current,
        3 => FormatStatus::Outdated,
        v if v > POSTGRESQL_LATEST => FormatStatus::Newer,
        _ => FormatStatus::Unsupported,
    };
    Format {
        version: (1..=i64::MAX).contains(&version).then_some(version),
        latest: POSTGRESQL_LATEST,
        status,
    }
}

/// [`crate::upgrade::postgresql_format_evidence`]: one connect, message 1.
pub(crate) fn evidence(url: &str, schema: &str) -> Result<FormatEvidence, UpgradeError> {
    let (_, mut client) = connect(url, schema)?;
    let checked = read(&mut client, schema);
    let _ = client.close();
    let checked = match checked {
        Ok(checked) => checked,
        Err(UpgradeError::Unsupported(_)) => {
            return Ok(FormatEvidence {
                format: Format {
                    version: None,
                    latest: POSTGRESQL_LATEST,
                    status: FormatStatus::Unsupported,
                },
                raw: 0,
                variant: None,
            })
        }
        Err(other) => return Err(other),
    };
    let mut format = format_of(checked.meta_version);
    if checked.policy_version != Some(checked.meta_version) {
        format = Format {
            version: None,
            latest: POSTGRESQL_LATEST,
            status: FormatStatus::Unsupported,
        };
    }
    let variant = (checked.meta_version == 3
        && [true, false]
            .iter()
            .any(|&a| checked.source_sha256 == frozen::sha256_hex(&frozen::source_v3(a))))
    .then(|| frozen::VARIANT.to_owned());
    Ok(FormatEvidence {
        format,
        raw: checked.meta_version,
        variant,
    })
}

/// Why message 1's Store is not upgraded, by name; `None` when the chain
/// takes it or it is current.
fn refusal(checked: &Check) -> Option<UpgradeError> {
    let version = checked.meta_version;
    let text = match version {
        3 | POSTGRESQL_LATEST if checked.policy_version == Some(version) => return None,
        _ if checked.policy_version != Some(version) => format!(
            "the Store's version cells disagree (history_meta {version}, store_policy {:?})",
            checked.policy_version
        ),
        v if v > POSTGRESQL_LATEST => format!(
            "PostgreSQL Store schema version {v} is newer than this build, which writes \
             schema version {POSTGRESQL_LATEST}"
        ),
        2 => "PostgreSQL Store schema version 2 (before 2.0's ancestor lists and pack \
              directories, #758): this build has no upgrade for it"
            .to_owned(),
        1 => "PostgreSQL Store schema version 1: this build has no upgrade for it".to_owned(),
        v => format!("PostgreSQL Store schema version {v} is not a format this build recognizes"),
    };
    Some(UpgradeError::Unsupported(text))
}

fn report(
    outcome: UpgradeOutcome,
    before: i64,
    steps: Vec<String>,
    work: UpgradeWork,
) -> UpgradeReport {
    UpgradeReport {
        outcome,
        engine: "postgresql",
        format_before: before,
        format_after: POSTGRESQL_LATEST,
        steps,
        warnings: Vec::new(),
        work,
    }
}

/// [`crate::upgrade::upgrade_postgresql`].
pub(crate) fn upgrade(
    url: &str,
    schema: &str,
    request: &UpgradeRequest<'_>,
    phase: &mut dyn FnMut(&'static str),
) -> Result<UpgradeReport, UpgradeError> {
    let (connector, mut client) = connect(url, schema)?;
    let mut work = UpgradeWork {
        connects: 1,
        ..UpgradeWork::default()
    };
    phase("check");
    work.round_trips += 1;
    let checked = match read(&mut client, schema) {
        Ok(checked) => checked,
        Err(e) => {
            let _ = client.close();
            return Err(e);
        }
    };
    let prepared = prepare(checked, schema, request.cancel);
    let (text, steps) = match prepared {
        Ok(Some(prepared)) => prepared,
        Ok(None) => {
            let _ = client.close();
            return Ok(report(
                UpgradeOutcome::Current,
                POSTGRESQL_LATEST,
                Vec::new(),
                work,
            ));
        }
        Err(e) => {
            let _ = client.close();
            return Err(e);
        }
    };
    phase("convert");
    work.round_trips += 1;
    let token = client.cancel_token();
    let done = AtomicBool::new(false);
    let answer = std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                if request.cancel.load(Ordering::SeqCst) {
                    // One cancel request; the answer still follows the reply.
                    let _ = connector.cancel(&token);
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let answer = client.batch_execute(&text);
        done.store(true, Ordering::SeqCst);
        answer
    });
    // A failed text left an aborted transaction: close rather than spend a
    // round trip on ROLLBACK.
    let _ = client.close();
    match answer {
        Ok(()) => {
            work.write_transactions = 1;
            Ok(report(UpgradeOutcome::Upgraded, FLOOR, steps, work))
        }
        Err(e) => Err(outcome(&e)),
    }
}

/// Matches message 1 against the chain and builds message 2; `None` for a
/// current Store.
fn prepare(
    checked: Check,
    schema: &str,
    cancel: &AtomicBool,
) -> Result<Option<(String, Vec<String>)>, UpgradeError> {
    if let Some(refused) = refusal(&checked) {
        return Err(refused);
    }
    if checked.meta_version == POSTGRESQL_LATEST {
        return Ok(None);
    }
    if checked.server_version_num < LEAST_SERVER {
        return Err(UpgradeError::Unsupported(format!(
            "PostgreSQL server {} is older than 11",
            checked.server_version_num
        )));
    }
    if checked.gc_ops != 0 {
        return Err(UpgradeError::Busy(
            "an unsettled gc_op row (a compaction) names this Store".to_owned(),
        ));
    }
    let binding = unhex(&checked.binding_key)
        .ok_or_else(|| UpgradeError::Unsupported("history_meta.binding_key".to_owned()))?;
    let catalog = CatalogId::derive(&binding)
        .map_err(|_| UpgradeError::Unsupported("history_meta.binding_key".to_owned()))?;
    if unhex(&checked.catalog_id).as_deref() != Some(catalog.as_slice()) || checked.incarnation <= 0
    {
        return Err(UpgradeError::Unsupported(
            "history_meta's catalog identity does not match its binding".to_owned(),
        ));
    }
    let key = crate::history::admission::store_advisory_key(catalog);
    let mut context = Context {
        check: checked,
        version: 3,
        acquisition: false,
        text: String::new(),
    };
    let steps = step::steps();
    let names = run_chain(&mut context, &steps, FLOOR, POSTGRESQL_LATEST)?;
    if cancel.load(Ordering::SeqCst) {
        return Err(UpgradeError::Cancelled);
    }
    Ok(Some((message(schema, key, &context)?, names)))
}

/// Message 2: the transaction around the chain's statements.
fn message(schema: &str, key: i64, context: &Context) -> Result<String, UpgradeError> {
    let check = &context.check;
    let acquisition = context.acquisition;
    let before_columns = literal(&frozen::columns_v3(acquisition))?;
    let after_columns = literal(&frozen::columns_v4(acquisition))?;
    let source_v4 = frozen::sha256_hex(&frozen::source_v4(acquisition));
    let g = check.incarnation;
    let before = "(SELECT count(*) FROM \"commit\")::text||':'||md5(\
coalesce((SELECT string_agg(encode(layer_stack_id,'hex')||':'||encode(head_layer_id,'hex'),',' \
ORDER BY layer_stack_id) FROM layer_stack),'')||'|'||\
coalesce((SELECT string_agg(encode(branch_id,'hex')||':'||coalesce(encode(head_commit_id,'hex'),'-')\
||':'||encode(base_layer_id,'hex'),',' ORDER BY branch_id) FROM branch),'')||'|'||\
(SELECT logical||','||canonical||','||stored||','||metadata FROM store_totals WHERE id=1)||'|'||\
(SELECT next_history_serial FROM history_meta WHERE id=1))";
    let mut checks = String::new();
    for (table, name) in VERSION_CHECKS {
        checks.push_str(&format!(
            "OR NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid='{table}'::regclass \
             AND conname='{name}' AND convalidated \
             AND pg_get_constraintdef(oid)='CHECK ((schema_version = 4))') "
        ));
    }
    Ok(format!(
        "BEGIN;\n\
SELECT set_config('search_path', '{schema}', true), set_config('lock_timeout', '{LOCK_TIMEOUT_MS}', true), \
set_config('synchronous_commit', 'on', true);\n\
DO $lfs_do$BEGIN IF NOT pg_try_advisory_xact_lock({key}) THEN RAISE EXCEPTION USING ERRCODE='55P03', \
MESSAGE='the Store-wide GC key is held: a mount or GC op uses the Store'; END IF; END$lfs_do$;\n\
LOCK TABLE \"commit\", history_meta, store_policy IN ACCESS EXCLUSIVE MODE NOWAIT;\n\
SELECT 1 FROM store_lane WHERE id = 1 FOR UPDATE NOWAIT;\n\
DO $lfs_do$BEGIN\n\
IF (SELECT schema_version FROM history_meta WHERE id=1) IS DISTINCT FROM 3 \
OR (SELECT schema_version FROM store_policy WHERE id=1) IS DISTINCT FROM 3 \
OR (SELECT encode(sha256(convert_to(schema_source,'UTF8')),'hex') FROM history_meta WHERE id=1) IS DISTINCT FROM '{source}' \
OR (SELECT encode(catalog_id,'hex') FROM history_meta WHERE id=1) IS DISTINCT FROM '{catalog}' \
OR (SELECT catalog_incarnation FROM history_meta WHERE id=1) IS DISTINCT FROM {g} \
OR (SELECT count(*) FROM gc_op) <> 0 \
OR {LIVE_COLUMNS} IS DISTINCT FROM {before_columns} THEN \
RAISE EXCEPTION USING ERRCODE='{CHANGED}', MESSAGE='the Store changed between the check and the conversion'; END IF;\n\
PERFORM set_config('layerfs.before', {before}, true);\n\
END$lfs_do$;\n\
{steps}\
DO $lfs_do$BEGIN\n\
IF {before} IS DISTINCT FROM current_setting('layerfs.before') THEN \
RAISE EXCEPTION USING ERRCODE='{VERIFY}', MESSAGE='verify: the heads, Commit count or totals changed'; END IF;\n\
IF {LIVE_COLUMNS} IS DISTINCT FROM {after_columns} THEN \
RAISE EXCEPTION USING ERRCODE='{VERIFY}', MESSAGE='verify: the live columns are not the frozen target definition'; END IF;\n\
IF (SELECT schema_version FROM history_meta WHERE id=1) IS DISTINCT FROM 4 \
OR (SELECT schema_version FROM store_policy WHERE id=1) IS DISTINCT FROM 4 \
OR (SELECT encode(sha256(convert_to(schema_source,'UTF8')),'hex') FROM history_meta WHERE id=1) IS DISTINCT FROM '{source_v4}' \
OR (SELECT schema_definition FROM history_meta WHERE id=1) IS DISTINCT FROM {after_columns} \
OR (SELECT catalog_incarnation FROM history_meta WHERE id=1) IS DISTINCT FROM {g} + 1 THEN \
RAISE EXCEPTION USING ERRCODE='{VERIFY}', MESSAGE='verify: history_meta or store_policy is not the schema-4 record'; END IF;\n\
IF NOT EXISTS (SELECT 1 FROM pg_index i WHERE i.indexrelid='commit_attempt'::regclass AND i.indisunique AND i.indisvalid \
AND i.indisready AND pg_get_indexdef(i.indexrelid)='CREATE UNIQUE INDEX commit_attempt ON '||quote_ident(current_schema())||'.commit USING btree (attempt) WHERE (attempt IS NOT NULL)') \
OR NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid='\"commit\"'::regclass AND conname='commit_attempt_check' \
AND convalidated AND pg_get_constraintdef(oid)='CHECK (((attempt IS NULL) OR (octet_length(attempt) = 16)))') \
{checks}THEN \
RAISE EXCEPTION USING ERRCODE='{VERIFY}', MESSAGE='verify: the commit_attempt index or a CHECK is not the schema-4 one'; END IF;\n\
END$lfs_do$;\n\
COMMIT",
        source = check.source_sha256,
        catalog = check.catalog_id,
        steps = context.text,
    ))
}

/// The answer a failed message 2 gives (§5.2 step 4). The server stopped
/// the text and rolled back whenever it sent a SQLSTATE, except for a
/// connection-class or shutdown error, whose COMMIT may have happened.
fn outcome(e: &postgres::Error) -> UpgradeError {
    let words = server_words(e);
    match e.code().map(|c| c.code()) {
        Some("55P03" | "40P01") => UpgradeError::Busy(words),
        Some(CHANGED) => UpgradeError::Busy(words),
        Some("57014") => UpgradeError::Cancelled,
        Some(VERIFY) => UpgradeError::Failed(words),
        Some(code) if code.starts_with("08") || code.starts_with("57P") => {
            UpgradeError::Unknown(words)
        }
        Some(_) => UpgradeError::Failed(words),
        None => UpgradeError::Unknown(format!("message 2: {e}")),
    }
}

fn server_words(e: &postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => format!("{} ({})", db.message(), db.code().code()),
        None => e.to_string(),
    }
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
