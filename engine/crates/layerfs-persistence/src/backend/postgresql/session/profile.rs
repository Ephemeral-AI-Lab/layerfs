//! Per-transaction settings: schema, `synchronous_commit` and the lane wait.
//!
//! Every setting is applied with `SET LOCAL` inside the transaction it
//! governs, in the same round trip as `BEGIN`, so a transaction-pooling proxy
//! never carries one session's settings to another.
use crate::backend::error::BackendError;
use crate::{PostgresSelection, SynchronousCommit};

/// The checked settings of one PostgreSQL Store selection.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub(crate) schema: String,
    pub(crate) synchronous_commit: SynchronousCommit,
    pub(crate) lock_timeout_ms: u32,
    pub(crate) reader_idle_check_ms: u32,
}

impl Settings {
    pub(crate) fn new(selection: &PostgresSelection) -> Result<Self, BackendError> {
        let schema = &selection.schema;
        let valid = !schema.is_empty()
            && schema.len() <= 63
            && schema
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            && !schema.as_bytes()[0].is_ascii_digit()
            && !schema.starts_with("pg_");
        if !valid {
            return Err(BackendError::Integrity);
        }
        Ok(Self {
            schema: schema.clone(),
            synchronous_commit: selection.synchronous_commit,
            lock_timeout_ms: selection.lock_timeout_ms,
            reader_idle_check_ms: selection.reader_idle_check_ms,
        })
    }

    /// One read snapshot: every statement of the transaction sees one state.
    pub(crate) fn read_begin(&self) -> String {
        format!(
            "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL search_path TO {}",
            self.schema
        )
    }

    /// A write: READ COMMITTED, never SERIALIZABLE. With `lane`, it first
    /// waits for the Store's lane row, so writes of every process run one at
    /// a time; a write nested in this thread's held lane already owns it.
    pub(crate) fn write_begin(&self, lane: bool) -> String {
        let mut sql = format!(
            "BEGIN ISOLATION LEVEL READ COMMITTED; SET LOCAL search_path TO {}; \
             SET LOCAL synchronous_commit TO {}",
            self.schema,
            self.synchronous_commit.setting()
        );
        if lane {
            sql.push_str(&self.lane_lock());
        }
        sql
    }

    /// A writable repeatable-read snapshot that does **not** wait for the
    /// publication lane. The mark reads every history, locator and pack row
    /// from one fixed snapshot and writes only `kept_node`; publications into
    /// other tables never overlap it, and a second mark is the only writer
    /// contending for its rows.
    ///
    /// The server's JIT compiler is off for this transaction. The planner's
    /// estimate for the history closure crosses the JIT threshold on any
    /// Store, and compiling it then costs hundreds of milliseconds for a
    /// statement that runs in about ten.
    pub(crate) fn snapshot_begin(&self) -> String {
        format!(
            "BEGIN ISOLATION LEVEL REPEATABLE READ; SET LOCAL search_path TO {}; \
             SET LOCAL synchronous_commit TO {}; SET LOCAL jit TO off",
            self.schema,
            self.synchronous_commit.setting()
        )
    }

    /// A publication: like `write_begin(false)` with the lane's
    /// `lock_timeout`; the body takes the lane row itself, after its pack
    /// bodies are sent, with [`Self::lane_row`]. A mount's GC registration
    /// (`Mode::Admit`) begins the same way and never takes the lane row.
    pub(crate) fn publish_begin(&self) -> String {
        format!(
            "{}; SET LOCAL lock_timeout TO {}",
            self.write_begin(false),
            self.lock_timeout_ms
        )
    }

    /// The statements that wait for and lock the Store's lane row, then pass
    /// the lane barrier (see [`lane_barrier`]).
    pub(crate) fn lane_row(schema: &str) -> String {
        format!(
            "SELECT 1 FROM store_lane WHERE id = 1 FOR UPDATE; {}",
            lane_barrier(schema)
        )
    }

    /// Appended to the `BEGIN` of a write nested in this thread's held lane:
    /// the barrier key, held shared until the transaction ends, so a new lane
    /// holder's barrier waits for this write (see [`lane_barrier`]).
    pub(crate) fn in_lane(&self) -> String {
        let (class, key) = barrier_key(&self.schema);
        format!("; SELECT pg_advisory_xact_lock_shared({class}, {key})")
    }

    /// The held publication lane: one open transaction owning the lane row.
    pub(crate) fn lane_begin(&self) -> String {
        format!(
            "BEGIN ISOLATION LEVEL READ COMMITTED; SET LOCAL search_path TO {}{}",
            self.schema,
            self.lane_lock()
        )
    }

    fn lane_lock(&self) -> String {
        format!(
            "; SET LOCAL lock_timeout TO {}; {}",
            self.lock_timeout_ms,
            Self::lane_row(&self.schema)
        )
    }
}

/// The two-key advisory lock of one Store's lane barrier (finding #10): a
/// fixed class and a stable hash of the schema name. The two-key form is a
/// separate key space from the one-key GC advisory keys; two schemas whose
/// hashes collide only share a barrier, which can make one wait, never pass.
fn barrier_key(schema: &str) -> (i32, i32) {
    // FNV-1a, 32 bits: stable across processes, builds and machines.
    let hash = schema.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    (0x4c46_4c4e, hash as i32)
}

/// Run by every writer right after it locks the lane row, under the same
/// `lock_timeout`. Writes nested in a held lane commit on the session
/// connection while the row lock lives in the lane connection's transaction;
/// if that connection dies (an idle-in-transaction timeout, a keepalive or an
/// administrator), the row is free while such a write may still be running.
/// Each nested write holds the barrier key shared for its whole transaction
/// and checks before `COMMIT` that its lane transaction is still open; a new
/// holder takes the key exclusively and lets it go at once, so it waits for
/// any nested write of the previous holder still in flight before it reads
/// anything. Together they keep the head check and the `object_location`
/// existence probe exact without a unique key or a second database.
///
/// The take and the release are one statement (the subquery is an
/// optimization fence, so the lock is taken first): as two statements, an
/// error between them left the key held at session level, which `ROLLBACK`
/// does not end, on a connection that is then reused, and every later nested
/// write's shared key waited for it forever (#776 P6).
fn lane_barrier(schema: &str) -> String {
    let (class, key) = barrier_key(schema);
    format!(
        "SELECT pg_advisory_unlock({class}, {key}) \
           FROM (SELECT pg_advisory_lock({class}, {key}) OFFSET 0) AS barrier"
    )
}

/// Appended to the `ROLLBACK` of a transaction that may have run
/// [`lane_barrier`]: a statement cancelled after its lock and before its
/// release leaves the key held at session level, and no barrier is ever held
/// past its own statement, so a granted exclusive barrier of this backend is
/// such a leftover and is released. It matches nothing otherwise.
pub(crate) fn barrier_leftover(schema: &str) -> String {
    let (class, key) = barrier_key(schema);
    let unsigned = |value: i32| u32::from_ne_bytes(value.to_ne_bytes());
    format!(
        "; SELECT pg_advisory_unlock({class}, {key}) FROM pg_locks \
          WHERE locktype = 'advisory' AND pid = pg_backend_pid() AND granted \
            AND mode = 'ExclusiveLock' AND objsubid = 2 \
            AND classid = {} AND objid = {}",
        unsigned(class),
        unsigned(key)
    )
}
