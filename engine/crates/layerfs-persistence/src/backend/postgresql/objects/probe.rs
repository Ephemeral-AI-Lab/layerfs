//! A test driver for the engine reads a mark makes inside one snapshot
//! transaction. Nothing in the product calls it; `internals` re-exports it so
//! an integration test can count the statements each read sends.
use super::super::session::transaction::Transaction;
use super::{self, super::metadata};
use crate::PostgresHandles;
use layerfs_storage::{
    location::{InodeValueRow, PackInfo},
    port::{AcquiredPackRead, PackRange, PackReadChoice, PackReadPlan, PersistenceError},
};

/// One read of [`reads`].
pub enum Read {
    /// Announces the packs the following scoped reads will ask for.
    Expect(Vec<i64>),
    /// One scoped read of a whole pack.
    Pack(i64),
    /// Ranges of several packs in one call.
    Ranges(Vec<(i64, Vec<PackRange>)>),
    /// Inode values of ascending ordinals, with a read-ahead allowance.
    Values(Vec<u32>, u32),
}

/// What one [`Read`] sent and returned.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Statements the read sent.
    pub statements: u64,
    /// A pack's body, or each range's bytes in request order.
    pub bytes: Vec<Vec<u8>>,
    /// The inode values handed back.
    pub values: Vec<InodeValueRow>,
}

struct Whole;

impl PackReadPlan for Whole {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Ok(PackReadChoice::Whole)
    }
}

fn read(tx: &Transaction<'_>, step: &Read) -> Result<Outcome, PersistenceError> {
    let mut outcome = Outcome::default();
    match step {
        Read::Expect(ids) => super::read::expect(tx, ids)?,
        Read::Pack(id) => match super::read::scoped(tx, *id, &mut Whole)? {
            AcquiredPackRead::Whole(pack) => outcome.bytes.push(pack.into_parts().1),
            AcquiredPackRead::Units(_) => return Err(PersistenceError::Malformed),
        },
        Read::Ranges(requests) => {
            let mut found = Vec::new();
            super::read::ranges(tx, requests, &mut found)?;
            outcome.bytes = found.into_iter().flatten().collect();
        }
        Read::Values(ordinals, ahead) => {
            metadata::reads::inode_values_ahead(tx, ordinals, *ahead, &mut outcome.values)?
        }
    }
    Ok(outcome)
}

/// Runs `steps` in order in one snapshot transaction of `handles`' session.
pub fn reads(handles: &PostgresHandles, steps: &[Read]) -> Result<Vec<Outcome>, PersistenceError> {
    handles.storage.session.run_snapshot(|tx| {
        let mut outcomes = Vec::with_capacity(steps.len());
        for step in steps {
            let before = tx.statements();
            let mut outcome = read(tx, step)?;
            outcome.statements = tx.statements() - before;
            outcomes.push(outcome);
        }
        Ok(outcomes)
    })
}
