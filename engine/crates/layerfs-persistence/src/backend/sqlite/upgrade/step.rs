//! The SQLite chain and its one step, 2 -> 3 (#804 §4, §5.1 step 4): add
//! `commit.attempt` and its partial unique index `commit_attempt`, purely
//! additive, keeping acquisition and layout.
use super::frozen::{self, Row};
use crate::upgrade::{Step, UpgradeError};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};

/// Env naming the Unix socket a reached failpoint reports to before it
/// blocks until the harness kills the process (as compaction's gate).
pub(crate) const CRASH_GATE_ENV: &str = "LAYERFS_UPGRADE_CRASH_GATE";

/// The open transaction every step works on.
pub(crate) struct Context<'c> {
    pub(crate) connection: &'c Connection,
    /// The `user_version` the transaction holds now.
    pub(crate) user_version: i64,
    /// The live-schema form the last recognition found (`"d"`, `"d+appended"`).
    pub(crate) form: Option<&'static str>,
    statements: u32,
    failpoint: Option<u32>,
    cancel: &'c AtomicBool,
}

impl<'c> Context<'c> {
    pub(crate) fn new(
        connection: &'c Connection,
        user_version: i64,
        failpoint: Option<u32>,
        cancel: &'c AtomicBool,
    ) -> Self {
        Self {
            connection,
            user_version,
            form: None,
            statements: 0,
            failpoint,
            cancel,
        }
    }

    /// Runs one writing statement, counted for the failpoint; refused
    /// `Cancelled` before it starts once a cancel is asked. Returns the
    /// rows it changed.
    pub(crate) fn exec(&mut self, sql: &str) -> Result<u64, UpgradeError> {
        self.cancelled()?;
        self.exec_past_cancel(sql)
    }

    /// [`Self::exec`] without the cancel check: the `COMMIT`, once the
    /// `commit` phase has started and cancel is no longer read.
    pub(crate) fn exec_past_cancel(&mut self, sql: &str) -> Result<u64, UpgradeError> {
        self.connection
            .execute_batch(sql)
            .map_err(|e| error(e, sql))?;
        self.statements += 1;
        if self.failpoint == Some(self.statements) {
            return Err(stop(self.statements));
        }
        Ok(self.connection.changes())
    }

    /// `Cancelled` once a cancel is asked.
    pub(crate) fn cancelled(&self) -> Result<(), UpgradeError> {
        match self.cancel.load(Ordering::SeqCst) {
            true => Err(UpgradeError::Cancelled),
            false => Ok(()),
        }
    }

    fn rows(&self) -> Result<Vec<Row>, UpgradeError> {
        frozen::rows(self.connection).map_err(|e| error(e, "reading the schema"))
    }
}

/// A rusqlite failure in words; an interrupted statement is the cancel.
pub(crate) fn error(e: rusqlite::Error, what: &str) -> UpgradeError {
    match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::OperationInterrupted) => UpgradeError::Cancelled,
        _ => UpgradeError::Failed(format!("{what}: {e}")),
    }
}

/// A reached test failpoint: with [`CRASH_GATE_ENV`] set it reports on that
/// socket with one line and blocks until the harness kills the process;
/// without it the run fails here and rolls back (the caller's failure path).
pub(crate) fn stop(statement: u32) -> UpgradeError {
    #[cfg(unix)]
    if let Some(gate) = std::env::var_os(CRASH_GATE_ENV) {
        use std::io::{Read, Write};
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(gate) {
            let _ = stream.write_all(b"R\n");
            let _ = stream.flush();
            // Never released: the harness SIGKILLs this process here.
            let mut release = [0u8; 1];
            let _ = stream.read(&mut release);
        }
    }
    UpgradeError::Failed(format!("failpoint {statement}"))
}

/// The SQLite chain, oldest first (rule 2). Schema 1 has no step (owner,
/// 2026-11-25), so the floor is 2.
pub(crate) fn steps<'c>() -> [Step<Context<'c>>; 1] {
    [Step {
        from: 2,
        to: 3,
        raw: &[(36, 56), (37, 57), (38, 58), (42, 62), (43, 63), (44, 64)],
        variants: &["d"],
        recognize: recognize_2,
        apply: apply_2,
    }]
}

/// The oldest SQLite format with a step path to the latest.
pub(crate) const FLOOR: i64 = 2;

/// Step 2 -> 3 accepts the Store exactly when it records source (d) for
/// its `user_version`, its recorded definition is its live one, and every
/// live row is the (d) row, fresh or position-migrated.
fn recognize_2(context: &mut Context<'_>) -> Result<Option<&'static str>, UpgradeError> {
    let user_version = context.user_version;
    if !matches!(user_version, 36..=38 | 42..=44) {
        return Ok(None);
    }
    let recorded: Option<(String, String)> = context
        .connection
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(error(e, "reading history_meta")),
        })?;
    let Some((source, recorded_definition)) = recorded else {
        return Ok(None);
    };
    let live = context.rows()?;
    if source != frozen::source_v2(user_version) || recorded_definition != frozen::definition(&live)
    {
        return Ok(None);
    }
    let (fresh, migrated) =
        frozen::rows_v2(user_version).map_err(|e| error(e, "building the frozen schema"))?;
    context.form = frozen::recognize_rows(&live, &fresh, &migrated);
    Ok(context.form.map(|_| "d"))
}

/// Step 2 -> 3's statements, in order (§5.1 step 4).
fn apply_2(context: &mut Context<'_>) -> Result<(), UpgradeError> {
    let old = context.user_version;
    let new = old + 20;
    context.exec(frozen::ADD_ATTEMPT)?;
    context.exec(frozen::ATTEMPT_INDEX)?;
    context.exec("PRAGMA ignore_check_constraints=ON")?;
    context.exec("PRAGMA writable_schema=ON")?;
    for table in ["store_policy", "history_meta"] {
        let changed = context.exec(&format!(
            "UPDATE sqlite_schema SET sql=replace(sql,'CHECK(schema_version={old})',\
             'CHECK(schema_version={new})') WHERE type='table' AND name='{table}'"
        ))?;
        if changed != 1 {
            return Err(UpgradeError::Failed(format!(
                "rewriting the {table} version CHECK changed {changed} rows"
            )));
        }
    }
    context.exec("PRAGMA writable_schema=RESET")?;
    context.exec(&format!("PRAGMA user_version={new}"))?;
    if context.exec(&format!(
        "UPDATE store_policy SET schema_version={new} WHERE id=1"
    ))? != 1
    {
        return Err(UpgradeError::Failed("store_policy has no row".to_owned()));
    }
    let definition = frozen::definition(&context.rows()?);
    let source = frozen::source_v3(new);
    let changed = context.exec(&format!(
        "UPDATE history_meta SET schema_version={new},schema_source={},schema_definition={} \
         WHERE id=1",
        quote(&source),
        quote(&definition)
    ))?;
    if changed != 1 {
        return Err(UpgradeError::Failed("history_meta has no row".to_owned()));
    }
    context.exec("PRAGMA ignore_check_constraints=OFF")?;
    context.user_version = new;
    Ok(())
}

/// A SQL string literal.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
