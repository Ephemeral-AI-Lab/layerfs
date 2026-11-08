//! Private SQLite temporary backing, outside the diagnosed Store.
use layerfs_content::{
    ConstructionRecordApply as Apply, ConstructionRecordChange as Change,
    ConstructionRecordExpected as Expected, ConstructionRecordKey as Key, ContentError,
    ContentResult, IndexedConstructionBacking,
};
use rusqlite::{Connection, OptionalExtension};

pub(super) struct Scratch(pub Connection);
fn error(_: rusqlite::Error) -> ContentError {
    ContentError::ProviderFailure {
        what: "diagnostic scratch",
    }
}
impl Scratch {
    pub fn new() -> ContentResult<Self> {
        // An empty SQLite filename creates an independently owned on-disk
        // temporary database, removed by SQLite when this connection closes.
        let connection = Connection::open("").map_err(error)?;
        connection.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA cache_size=-2048; PRAGMA temp_store=FILE; CREATE TABLE records(kind INTEGER NOT NULL,k BLOB NOT NULL,v BLOB NOT NULL,PRIMARY KEY(kind,k)) WITHOUT ROWID;").map_err(error)?;
        Ok(Self(connection))
    }
    /// Keep scratch SQL cancellable, including statements which emit no rows.
    /// The scoped watcher owns only SQLite's interruption handle and is joined
    /// before this independently owned connection can be reused or closed.
    pub fn observe<T>(
        &mut self,
        options: super::Options<'_>,
        run: impl FnOnce(&mut Self) -> Result<T, crate::backend::records::BackendError>,
    ) -> Result<T, crate::backend::records::BackendError> {
        let interrupt = self.0.get_interrupt_handle();
        let (done, ended) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let watcher = (options.deadline.is_some() || options.cancelled.is_some()).then(|| {
                scope.spawn(move || loop {
                    if options.check().is_err() {
                        interrupt.interrupt();
                        break;
                    }
                    let wait =
                        options
                            .deadline
                            .map_or(std::time::Duration::from_millis(20), |deadline| {
                                deadline
                                    .saturating_duration_since(std::time::Instant::now())
                                    .min(std::time::Duration::from_millis(20))
                            });
                    match ended.recv_timeout(wait) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                })
            });
            let result = run(self);
            let _ = done.send(());
            if watcher.is_some_and(|thread| thread.join().is_err()) {
                return Err(crate::backend::records::BackendError::Cancelled);
            }
            // SQLite interruption itself is never evidence of data corruption.
            options.check()?;
            result
        })
    }
    pub fn clear(&mut self) -> ContentResult<()> {
        self.0.execute("DELETE FROM records", []).map_err(error)?;
        Ok(())
    }
}
impl IndexedConstructionBacking for Scratch {
    fn contains(&mut self, key: Key) -> ContentResult<bool> {
        Ok(self.get(key)?.is_some())
    }
    fn get(&mut self, key: Key) -> ContentResult<Option<Vec<u8>>> {
        self.0
            .query_row(
                "SELECT v FROM records WHERE kind=?1 AND k=?2",
                rusqlite::params![key.kind, key.key.as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(error)
    }
    fn apply(&mut self, changes: Vec<Change>) -> ContentResult<Apply> {
        let tx = self.0.transaction().map_err(error)?;
        for (index, change) in changes.iter().enumerate() {
            let actual: Option<Vec<u8>> = tx
                .query_row(
                    "SELECT v FROM records WHERE kind=?1 AND k=?2",
                    rusqlite::params![change.key.kind, change.key.key.as_slice()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(error)?;
            let matches = match &change.expected {
                Expected::Missing => actual.is_none(),
                Expected::ExactBytes(bytes) => actual.as_ref() == Some(bytes),
            };
            if !matches {
                return Ok(Apply::NotApplied {
                    index,
                    key: change.key,
                    actual,
                });
            }
        }
        for change in changes {
            match change.value {
                Some(value) => tx.execute("INSERT INTO records(kind,k,v) VALUES(?1,?2,?3) ON CONFLICT(kind,k) DO UPDATE SET v=excluded.v", rusqlite::params![change.key.kind, change.key.key.as_slice(), value]),
                None => tx.execute("DELETE FROM records WHERE kind=?1 AND k=?2", rusqlite::params![change.key.kind, change.key.key.as_slice()]),
            }.map_err(error)?;
        }
        tx.commit().map_err(error)?;
        Ok(Apply::Applied)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        let mut stmt = self
            .0
            .prepare(
                "SELECT k FROM records WHERE kind=?1 AND (?2 IS NULL OR k!=?2) ORDER BY k LIMIT 64",
            )
            .map_err(error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![kind, excluded.as_ref().map(|v| v.as_slice())],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .map_err(error)?;
        rows.map(|r| {
            r.map_err(error)?
                .try_into()
                .map_err(|_| ContentError::InvalidRecord("diagnostic scratch key"))
        })
        .collect()
    }
    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        let mut stmt = self
            .0
            .prepare(
                "SELECT k FROM records WHERE kind=?1 AND (?2 IS NULL OR k>?2) ORDER BY k LIMIT 64",
            )
            .map_err(error)?;
        let rows = stmt
            .query_map(
                rusqlite::params![kind, after.as_ref().map(|v| v.as_slice())],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .map_err(error)?;
        rows.map(|r| {
            r.map_err(error)?
                .try_into()
                .map_err(|_| ContentError::InvalidRecord("diagnostic scratch key"))
        })
        .collect()
    }
}
