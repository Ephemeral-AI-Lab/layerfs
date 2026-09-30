//! Exact replay admission and one fixed raw-occurrence prefetch producer.
use super::binding::{Bindings, Headers};
use super::{ValidationState, ValidationWork, ALLOCATION_CHECK_BATCH};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::inode::read::InodeTable;
use crate::filesystem::rows::PreparedBindingRows;
use crate::object::AuthenticatedObjects;

const REPLAY_DOMAIN: &[u8] = b"layerfs/validation-prefetch-replay/v1\0";

/// Actual initial-demand work, distinct from logical demands and physical pages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValidationPrefetchWork {
    /// Eligible raw occurrences consumed by the replay producer.
    pub examined_occurrences: u64,
    /// Occurrences already answered by a positive or absence memo fact.
    pub memo_hits: u64,
    /// Not-memo occurrences compacted inside their current raw wave.
    pub local_duplicates: u64,
    /// Unique serials submitted, including unsuccessful lookup attempts.
    pub submitted_serials: u64,
    /// Nonempty lookup_many attempts issued, including unsuccessful attempts.
    pub lookup_calls: u64,
    /// Largest actual raw pending prefix, independent of deduplication.
    pub max_pending: u64,
    /// Largest actual missing prefix submitted to lookup_many.
    pub max_missing: u64,
    /// Largest actual successfully returned answer vector length.
    pub max_answers: u64,
}

#[derive(Eq, PartialEq)]
struct Completed {
    directories: u64,
    bindings: u64,
    demands: u64,
    transcript: [u8; 32],
}

pub(super) fn read(
    reader: &dyn AuthenticatedObjects,
    input: &dyn PreparedBindingRows,
    table: InodeTable,
    state: &mut ValidationState,
    declared: usize,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    // The count/EOF hasher is gone before replay. Only fixed totals and32 bytes
    // survive; no serial, name or row population enters this handoff.
    let admitted = visit(input, declared, |_| Ok(()))?;
    let mut wave = Wave::new();
    let replayed = visit(input, declared, |serial| {
        wave.push(serial, reader, table, state, work)
    })?;
    if replayed != admitted {
        return Err(ContentError::InvalidRecord("prefetch source replay"));
    }
    // Full waves may already have read data. A changed replay never buys the
    // final tail, reaches semantic mutation, or adopts a new source.
    wave.flush(reader, table, state, work)
}

fn visit(
    input: &dyn PreparedBindingRows,
    declared: usize,
    mut demand: impl FnMut(u64) -> ContentResult<()>,
) -> ContentResult<Completed> {
    let mut digest = blake3::Hasher::new();
    digest.update(REPLAY_DOMAIN);
    digest.update(input.scope().object().as_bytes());
    digest.update(&input.root_serial().to_be_bytes());
    let limit = declared
        .checked_mul(2)
        .ok_or(ContentError::LengthOverflow)?;
    let mut directories = 0u64;
    let mut names = 0usize;
    let mut demands = 0usize;
    let mut headers = Headers::new(input)?;
    while let Some(header) = headers.next()? {
        names = names
            .checked_add(header.binding_count() as usize)
            .ok_or(ContentError::LengthOverflow)?;
        if names > declared {
            return Err(ContentError::ObjectLimitExceeded {
                limit: declared,
                actual: names,
            });
        }
        directories = directories
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        let eligible = header.parent() != input.root_serial() && !input.is_new(header.parent())?;
        digest.update(&header.parent().to_be_bytes());
        digest.update(&header.ordinal().to_be_bytes());
        digest.update(&header.binding_count().to_be_bytes());
        digest.update(&header.wire_name_bytes().to_be_bytes());
        digest.update(&[u8::from(eligible)]);
        if eligible {
            demands = demands.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            demand(header.parent())?;
        }
        let mut bindings = Bindings::new(input, header)?;
        while let Some((name, child)) = bindings.next()? {
            digest.update(&(name.as_bytes().len() as u16).to_be_bytes());
            digest.update(name.as_bytes());
            digest.update(&child.unwrap_or(0).to_be_bytes());
            if let Some(serial) = child.filter(|serial| *serial != input.root_serial()) {
                demands = demands.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                demand(serial)?;
            }
        }
        // Bindings confirms exact EOF and the provider's issued completion.
        digest.update(&[1]);
    }
    if demands > limit {
        return Err(ContentError::ObjectLimitExceeded {
            limit,
            actual: demands,
        });
    }
    let bindings = u64::try_from(names).map_err(|_| ContentError::LengthOverflow)?;
    let demands = u64::try_from(demands).map_err(|_| ContentError::LengthOverflow)?;
    digest.update(&directories.to_be_bytes());
    digest.update(&bindings.to_be_bytes());
    digest.update(&demands.to_be_bytes());
    Ok(Completed {
        directories,
        bindings,
        demands,
        transcript: *digest.finalize().as_bytes(),
    })
}

struct Wave {
    pending: [u64; ALLOCATION_CHECK_BATCH],
    missing: [u64; ALLOCATION_CHECK_BATCH],
    used: usize,
}
impl Wave {
    fn new() -> Self {
        Self {
            pending: [0; ALLOCATION_CHECK_BATCH],
            missing: [0; ALLOCATION_CHECK_BATCH],
            used: 0,
        }
    }
    fn push(
        &mut self,
        serial: u64,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        state: &mut ValidationState,
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        if self.used == ALLOCATION_CHECK_BATCH {
            return Err(ContentError::InvalidRecord("prefetch pending bound"));
        }
        self.pending[self.used] = serial;
        self.used += 1;
        work.prefetch.examined_occurrences = work.prefetch.examined_occurrences.saturating_add(1);
        work.prefetch.max_pending = work.prefetch.max_pending.max(self.used as u64);
        if self.used == ALLOCATION_CHECK_BATCH {
            self.flush(reader, table, state, work)?;
        }
        Ok(())
    }
    fn flush(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        state: &mut ValidationState,
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        let mut count = 0;
        for &serial in &self.pending[..self.used] {
            if state.known(serial) {
                work.prefetch.memo_hits = work.prefetch.memo_hits.saturating_add(1);
            } else {
                self.missing[count] = serial;
                count += 1;
            }
        }
        self.missing[..count].sort_unstable();
        let mut unique = 0;
        for index in 0..count {
            if unique == 0 || self.missing[index] != self.missing[unique - 1] {
                self.missing[unique] = self.missing[index];
                unique += 1;
            }
        }
        work.prefetch.local_duplicates = work
            .prefetch
            .local_duplicates
            .saturating_add((count - unique) as u64);
        state.prefetch_wave(reader, table, &self.missing[..unique], work)?;
        self.used = 0;
        Ok(())
    }
}
