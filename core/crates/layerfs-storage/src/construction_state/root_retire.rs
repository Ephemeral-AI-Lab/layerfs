//! Bounded exact sealed-root deletion and acknowledged transcript custody.

use layerfs_content::filesystem::state::{
    GraphMemory, GraphMemoryLease, StateKey, StateLedger, StateRecord, StateSeal,
    STATE_APPEND_HEADER_BYTES, STATE_MAX_PAGE_BYTES,
};
use layerfs_content::ObjectId;
use rusqlite::Connection;

use super::{graph_index, index, profile};
use crate::error::{StorageError, StorageResult};

/// Known profile4 root completion state, separate from native release/refund.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootRetirementStage {
    /// Immutable terminal seal selected, before any deletion.
    Selected,
    /// One or more exact windows acknowledged; more rows remain.
    Retiring,
    /// Exact full replay seal and empty root projections acknowledged.
    Retired,
    /// Fixed owner reset proposed; not yet acknowledged.
    Resetting,
    /// Empty fixed rows and native observation acknowledged.
    Clean,
}

/// Bounded real progress observation; Unknown keeps separate proposed fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootRetirementProgress {
    /// Original immutable terminal root transcript.
    pub seal: StateSeal,
    /// Acknowledged root records deleted so far.
    pub retired_records: u64,
    /// Acknowledged exact framed root bytes deleted so far.
    pub retired_bytes: u64,
    /// Acknowledged complete last deleted key.
    pub after: Option<StateKey>,
    /// Records still owned by the root table.
    pub remaining_records: u64,
    /// Current known/proposed completion state.
    pub stage: RootRetirementStage,
    /// Proposed terminal count when a mutation acknowledgement is pending.
    pub proposed_records: Option<u64>,
    /// Proposed complete last key retained with an unacknowledged window.
    pub proposed_after: Option<StateKey>,
    /// Exact retained row-window count, at most128.
    pub pending_records: usize,
}

pub(crate) struct RootOwner {
    value: Box<RootRetirement>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for RootOwner {
    type Target = RootRetirement;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl std::ops::DerefMut for RootOwner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
pub(crate) struct RootRetirement {
    pub(crate) seal: StateSeal,
    pub(crate) ledger: StateLedger,
    pub(crate) maximum: Option<StateKey>,
    pub(crate) stage: RootRetirementStage,
    pub(crate) attempt: Option<RootAttempt>,
}
pub(crate) struct RootAttempt {
    value: Box<RootAttemptData>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for RootAttempt {
    type Target = RootAttemptData;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
pub(crate) struct RootAttemptData {
    pub(crate) proposed: StateLedger,
    pub(crate) rows: Vec<StateRecord>,
}

impl RootRetirement {
    // Construction returns the funded owner so its allocation and lease stay inseparable.
    #[allow(clippy::new_ret_no_self)]
    pub(crate) fn new(
        memory: &GraphMemory,
        seal: StateSeal,
        maximum: Option<StateKey>,
    ) -> StorageResult<RootOwner> {
        let lease = memory.reserve(std::mem::size_of::<Self>())?;
        Ok(RootOwner {
            value: Box::new(Self {
                ledger: StateLedger::new(seal.scope().clone()),
                seal,
                maximum,
                stage: RootRetirementStage::Selected,
                attempt: None,
            }),
            _memory: lease,
        })
    }

    pub(crate) fn progress(&self) -> RootRetirementProgress {
        RootRetirementProgress {
            seal: self.seal.clone(),
            retired_records: self.ledger.records(),
            retired_bytes: self.ledger.encoded_bytes(),
            after: self.ledger.last(),
            remaining_records: self.seal.records() - self.ledger.records(),
            stage: self.stage,
            proposed_records: self.attempt.as_ref().map(|a| a.proposed.records()),
            proposed_after: self.attempt.as_ref().and_then(|a| a.proposed.last()),
            pending_records: self.attempt.as_ref().map_or(0, |a| a.rows.len()),
        }
    }
}

pub(crate) fn prepare(
    connection: &Connection,
    memory: &GraphMemory,
    state: &RootRetirement,
) -> StorageResult<RootAttempt> {
    let remaining = state.seal.records() - state.ledger.records();
    let count = remaining.min(128) as usize;
    if STATE_APPEND_HEADER_BYTES + count * 63 > STATE_MAX_PAGE_BYTES {
        return Err(StorageError::Integrity(
            "construction roots retirement window",
        ));
    }
    let lease = memory.reserve(
        std::mem::size_of::<RootAttemptData>() + count * std::mem::size_of::<StateRecord>(),
    )?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(count)
        .map_err(|_| StorageError::Integrity("construction roots retirement allocation"))?;
    if records.capacity() > count {
        return Err(StorageError::Integrity(
            "construction roots retirement capacity",
        ));
    }
    let after = state
        .ledger
        .last()
        .map(|key| *key.as_bytes())
        .unwrap_or([0; 25]);
    let mut statement = connection.prepare(
        "SELECT key,ordinal,root FROM directory_roots WHERE key>?1 ORDER BY key LIMIT ?2",
    )?;
    let mut rows = statement.query(rusqlite::params![after.as_slice(), count as i64])?;
    while let Some(row) = rows.next()? {
        if records.len() == count
            || row.get::<_, i64>(1)? != (state.ledger.records() + records.len() as u64 + 1) as i64
        {
            return Err(StorageError::Integrity(
                "construction roots retirement ordinal",
            ));
        }
        let key = StateKey::decode(state.seal.scope(), graph_index::blob(row.get_ref(0)?, 25)?)?;
        let root = ObjectId::from_bytes(graph_index::blob(row.get_ref(2)?, 32)?)?;
        records.push(StateRecord::new(key, root));
    }
    if records.len() != count {
        return Err(StorageError::Integrity(
            "construction roots retirement cardinality",
        ));
    }
    let mut proposed = state.ledger.clone();
    proposed.acknowledge(&records)?;
    if proposed.records() == state.seal.records()
        && (proposed.seal() != state.seal || proposed.last() != state.maximum)
    {
        return Err(StorageError::Integrity(
            "construction roots retirement exact terminal seal",
        ));
    }
    Ok(RootAttempt {
        value: Box::new(RootAttemptData {
            proposed,
            rows: records,
        }),
        _memory: lease,
    })
}

pub(crate) fn commit(
    connection: &Connection,
    header: &[u8],
    state: &RootRetirement,
    engine: Option<&'static crate::engine::EngineGuard>,
) -> StorageResult<()> {
    let attempt = state.attempt.as_ref().ok_or(StorageError::Integrity(
        "construction roots missing retirement attempt",
    ))?;
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        index::verify_header(connection, header, Some(&state.seal))?;
        let mut statement = connection
            .prepare("DELETE FROM directory_roots WHERE key=?1 AND ordinal=?2 AND root=?3")?;
        for (offset, row) in attempt.rows.iter().enumerate() {
            if statement.execute(rusqlite::params![
                row.key().as_bytes().as_slice(),
                (state.ledger.records() + offset as u64 + 1) as i64,
                row.root().as_bytes().as_slice()
            ])? != 1
            {
                return Err(StorageError::Integrity(
                    "construction roots retirement exact row",
                ));
            }
        }
        drop(statement);
        if attempt.proposed.records() == state.seal.records() {
            empty(connection)?;
        }
        Ok(())
    })();
    profile::finish_write_guarded(connection, result, engine)
}

pub(crate) fn empty(connection: &Connection) -> StorageResult<()> {
    for sql in [
        "SELECT key FROM directory_roots ORDER BY key LIMIT 1",
        "SELECT ordinal FROM directory_roots ORDER BY ordinal LIMIT 1",
    ] {
        if connection.prepare(sql)?.query([])?.next()?.is_some() {
            return Err(StorageError::Integrity(
                "construction roots retirement projections remain",
            ));
        }
    }
    Ok(())
}

pub(crate) fn acknowledge(state: &mut RootRetirement) {
    let RootAttempt { value, _memory } = state.attempt.take().unwrap();
    let RootAttemptData { proposed, rows } = *value;
    state.ledger = proposed;
    state.stage = if state.ledger.records() == state.seal.records() {
        RootRetirementStage::Retired
    } else {
        RootRetirementStage::Retiring
    };
    drop(rows);
    drop(_memory);
}
