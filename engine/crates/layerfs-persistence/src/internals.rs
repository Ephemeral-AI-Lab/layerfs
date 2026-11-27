//! Test-only re-exports for unit checks that must live outside product source
//! (the product-boundary guard refuses test-only attributes in `src/`).
//!
//! Nothing in the product calls these; they exist so an integration test can
//! cover the same §7/§8 helpers the product source used to test in place.
pub use crate::backend::error::BackendError;
pub use crate::backend::postgresql::compact::{decode_kept_waste, encode_kept_waste, report};
pub use crate::backend::postgresql::compact::{
    effective_rank, standby_satisfied, sync_name, sync_rank, takes_rebuild_path, Replica,
};
pub use crate::backend::postgresql::objects::probe::{
    reads as postgresql_reads, Outcome as PostgresReadOutcome, Read as PostgresRead,
};
pub use crate::backend::postgresql::session::pipeline::{
    Refusal as PipelineRefusal, Reply as PipelineReply, Request as PipelineRequest, UnitStatements,
};
pub use crate::backend::postgresql::session::rows::sqlstate;
pub use crate::backend::sqlite::compact::{
    io_error, peak_temp_bytes, receipt, reserve, source_mmap, staged_windows, trim,
    verify_inode_blocks, windows, PackLengths, PackScan, Rewritten, StagedLocators, Window,
    GAP_BYTES, SRC_MMAP_BYTES, WINDOW_BYTES,
};
pub use crate::gc::compact::{cap_delta_references, DELTA_BASE_REFERENCES_CAP};
pub use crate::history::spill::{Space, BUDGET_BYTES, LEAST_BUDGET_BYTES};
pub use crate::upgrade::chain::{
    check_chain as upgrade_check_chain, plan as upgrade_plan, run as upgrade_run,
    Step as UpgradeStep,
};

/// One shipped upgrade step's data: `(from, to, raw encodings, variants)`.
pub type StepData = (i64, i64, &'static [(i64, i64)], &'static [&'static str]);

/// The SQLite chain's steps, oldest first, and its floor.
pub fn sqlite_upgrade_steps() -> (Vec<StepData>, i64) {
    let steps = crate::backend::sqlite::upgrade::step::steps();
    let data = steps
        .iter()
        .map(|s| (s.from, s.to, s.raw, s.variants))
        .collect();
    (data, crate::backend::sqlite::upgrade::step::FLOOR)
}

/// `(schema, acquisition * 3 + layout)` of a SQLite `user_version`.
pub fn sqlite_decode_user_version(user_version: i64) -> Option<(i64, i64)> {
    crate::backend::sqlite::upgrade::format::decode(user_version)
}

/// For a schema-3 `user_version`: the last step's frozen target source and
/// the source this build's schema constants write.
pub fn sqlite_target_and_current_source(user_version: i64) -> (String, String) {
    let selection = crate::backend::sqlite::upgrade::verify::selection(user_version);
    (
        crate::backend::sqlite::upgrade::frozen::source_v3(user_version),
        crate::backend::sqlite::schema::source(selection),
    )
}

/// The frozen schema-2 (d) source of a schema-2 `user_version`.
pub fn sqlite_v2_source(user_version: i64) -> String {
    crate::backend::sqlite::upgrade::frozen::source_v2(user_version)
}

/// One inode value block's values (`blocks::values::decode`).
pub fn decode_value_block(
    first: u32,
    last: u32,
    body: &[u8],
) -> Result<Vec<[u8; layerfs_content::inode_leaf::INODE_VALUE_BYTES]>, BackendError> {
    crate::storage::blocks::values::decode(first, last, body)
}

/// One binding hint block's hints (`blocks::hints::decode`).
pub fn decode_hint_block(
    group: i64,
    body: &[u8],
) -> Result<Vec<layerfs_history::records::BindingHint>, BackendError> {
    crate::storage::blocks::hints::decode(group, body)
}

/// One pipelined flight on the session of `handles`
/// (`postgresql::session::pipeline`): every request sent before any reply is
/// awaited, every reply returned in order. Stage 2 of #768 sends a Commit
/// this way; until then only the pipeline's own test calls it.
pub fn postgresql_pipeline(
    handles: &crate::PostgresHandles,
    requests: &[PipelineRequest<'_>],
) -> Result<Vec<Result<PipelineReply, PipelineRefusal>>, BackendError> {
    handles.storage.session.pipeline(requests)
}

/// The replies of one flight, and whether the session is still trusted.
pub type FlightReplies = (Vec<Result<PipelineReply, PipelineRefusal>>, bool);

/// One writer turn of the session of `handles` for a write unit (#818 I3b,
/// `DecidedTurn::prepare_unit` / `send_in_unit`): the test drives the
/// unit's flights through [`UnitTurn`]; the turn's end rolls back a unit
/// still open.
pub fn postgresql_unit<T>(
    handles: &crate::PostgresHandles,
    run: impl FnOnce(&mut UnitTurn<'_, '_>) -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    let session = &handles.storage.session;
    let begin = session.settings.write_begin(true);
    let release =
        crate::backend::postgresql::session::profile::barrier_leftover(&session.settings.schema);
    session.decided_turn(|turn| {
        turn.ready()?;
        run(&mut UnitTurn {
            turn,
            begin,
            release,
        })
    })
}

/// A writer turn [`postgresql_unit`] holds.
pub struct UnitTurn<'a, 's> {
    turn: &'a mut crate::backend::postgresql::session::pipeline::DecidedTurn<'s>,
    /// The unit's `BEGIN`: a write that takes the lane row and barrier.
    pub begin: String,
    /// The failure path's release of a leftover barrier key.
    pub release: String,
}

impl UnitTurn<'_, '_> {
    /// `DecidedTurn::prepare_unit`.
    pub fn prepare(&mut self, texts: &[&str]) -> Result<UnitStatements, BackendError> {
        self.turn.prepare_unit(texts)
    }
    /// `DecidedTurn::send_in_unit` with [`Self::release`]; the judge
    /// returns the replies and never answers unknown.
    pub fn send(
        &mut self,
        prepared: &UnitStatements,
        requests: &[PipelineRequest<'_>],
        open: bool,
    ) -> Result<FlightReplies, BackendError> {
        let release = self.release.clone();
        let judge = |replies, trusted| ((replies, trusted), false);
        self.turn
            .send_in_unit(prepared, requests, open, &release, judge)
    }
    /// `DecidedTurn::send`: an ordinary flight of the turn.
    pub fn send_plain(
        &mut self,
        requests: &[PipelineRequest<'_>],
    ) -> Result<FlightReplies, BackendError> {
        self.turn
            .send(requests, |replies, trusted| ((replies, trusted), false))
    }
    /// `DecidedTurn::count_unit`.
    pub fn count(&mut self, committed: bool) {
        self.turn.count_unit(committed);
    }
}

/// One write transaction on the session of `handles` whose body runs
/// `scripts` in order and ignores every refusal, then answers success: what
/// the session reports when a body leaves its transaction aborted (#768).
pub fn postgresql_write_ignoring(
    handles: &crate::PostgresHandles,
    scripts: &[&str],
) -> Result<(), BackendError> {
    use crate::backend::postgresql::session::transaction::Mode;
    handles
        .storage
        .session
        .run_as(Mode::Write, |tx| -> Result<(), BackendError> {
            for script in scripts {
                let _ = tx.batch(script);
            }
            Ok(())
        })
}

/// A fused Commit made at `time_ms` (`Committer::commit_at`): the
/// PostgreSQL identity is then the same for the same request, so a test can
/// make one that already exists.
pub fn fused_commit_at(
    committer: &crate::Committer,
    tail: Option<&layerfs_storage::port::Publication>,
    stack: layerfs_history::LayerStackId,
    request: &layerfs_history::records::CommitRequest,
    time_ms: i64,
) -> Result<layerfs_history::records::CommitOutcome, crate::Fused> {
    committer.commit_at(tail, stack, request, &[], Some(time_ms))
}

/// One PostgreSQL mount registration as the one message alone (#768 step
/// B): `true` when it admitted (the keys held with the op), `false` when it
/// refused with nothing written and no key held, where the registration
/// continues statement by statement.
pub fn mount_one_message(
    history: &crate::HistoryProvider,
    intent: &layerfs_history::records::GcIntent,
) -> layerfs_history::HistoryResult<bool> {
    crate::gc::admission::intents::one_message(history, intent, None).map(|read| read.is_some())
}

/// Whether the statement-by-statement registration's checks refuse
/// `intent` before settling anything (the Branch probe and the conflict
/// scan the one message encodes).
pub fn mount_separately_refuses(
    history: &crate::HistoryProvider,
    intent: &layerfs_history::records::GcIntent,
) -> layerfs_history::HistoryResult<bool> {
    crate::gc::admission::intents::separately_refuses(history, intent)
}

/// One registration statement by statement, as a refusal of the one
/// message continues.
pub fn register_separately(
    history: &crate::HistoryProvider,
    intent: &layerfs_history::records::GcIntent,
) -> layerfs_history::HistoryResult<()> {
    crate::gc::admission::intents::register_separately(history, intent)
}

/// The PostgreSQL chain's steps, oldest first, and its floor.
pub fn postgresql_upgrade_steps() -> (Vec<StepData>, i64) {
    let steps = crate::backend::postgresql::upgrade::step::steps();
    let data = steps
        .iter()
        .map(|s| (s.from, s.to, s.raw, s.variants))
        .collect();
    (data, crate::backend::postgresql::upgrade::step::FLOOR)
}

/// For PostgreSQL with or without the acquisition tables: the last step's
/// frozen target source and the source this build's schema constants write.
pub fn postgresql_target_and_current_source(acquisition: bool) -> (String, String) {
    (
        crate::backend::postgresql::upgrade::frozen::source_v4(acquisition),
        crate::backend::postgresql::schema::recorded_source(acquisition),
    )
}

/// The frozen PostgreSQL column definitions before and after step 3 -> 4.
pub fn postgresql_step_columns(acquisition: bool) -> (String, String) {
    use crate::backend::postgresql::upgrade::frozen;
    (
        frozen::columns_v3(acquisition),
        frozen::columns_v4(acquisition),
    )
}
