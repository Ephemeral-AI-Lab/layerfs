//! Native FIFO pending and LIFO cursor release with fixed C1 listing windows.
use super::count_reduce::{known_base_wave, CountReducer};
use super::reduce::PendingState;
use super::release::ReleaseWork;
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::inode::read::InodeTable;
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::state::{
    BaseFact, CanonicalConstructionState, ReleaseFrame, ReleaseJob, ReleaseName, ZeroLedger,
    ZeroSeal,
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::AuthenticatedObjects;
use crate::{ContentError, ContentResult};
#[allow(clippy::too_many_arguments)]
pub(crate) fn release_with_state<S: CanonicalConstructionState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    reducer: &mut CountReducer,
    state: &mut S,
    seeds: &ZeroSeal,
    base_batch: usize,
    page_entries: usize,
    page_bytes: usize,
) -> ContentResult<ReleaseWork> {
    let scope = reducer.scope.jobs()?;
    state.release_begin(&scope, seeds)?;
    // Fixed control, page-child and raw demand windows precede any allocation.
    let _working = state
        .count_memory(&reducer.scope)?
        .reserve(canonical_release_working_bytes())?;
    let canonical = reducer.canonical();
    let mut ledger = ZeroLedger::new(seeds.counts.clone())?;
    let mut after = None;
    loop {
        let page = state.zero_page(seeds, after, base_batch.clamp(1, 64), 65536)?;
        if page.seal != *seeds {
            return Err(ContentError::InvalidOrderingRecord(
                "release selected seeds",
            ));
        }
        ledger.append(page.records())?;
        if page.last != ledger.last().or(after)
            || page.eof != (ledger.records() == seeds.records)
            || ledger.records() > seeds.records
        {
            return Err(ContentError::InvalidOrderingRecord(
                "release seed EOF/progress",
            ));
        }
        state.release_seed(&scope, page.records())?;
        after = page.last;
        if page.eof {
            break;
        }
    }
    if ledger.seal() != Ok(seeds.clone()) {
        return Err(ContentError::InvalidOrderingRecord(
            "release seed transcript",
        ));
    }
    state.release_close_seeds(&scope)?;
    let mut work = ReleaseWork::default();
    loop {
        if let Some(job) = state.release_take(&scope)? {
            let directory = match reducer.state(state, job.serial)? {
                Some(PendingState::New { value, .. }) => {
                    let value = value.ok_or(ContentError::InvalidRecord("released new inode"))?;
                    (value.kind == InodeKind::Directory).then_some(value.content_root)
                }
                Some(PendingState::Existing { value, .. }) => {
                    let base = job
                        .base
                        .ok_or(ContentError::InvalidRecord("released inode record"))?;
                    let kind = value.map_or(base.kind, |v| v.kind);
                    (kind == InodeKind::Directory)
                        .then_some(value.map_or(base.content_root, |v| v.content_root))
                }
                None => {
                    let base = job
                        .base
                        .ok_or(ContentError::InvalidRecord("released inode record"))?;
                    (base.kind == InodeKind::Directory).then_some(base.content_root)
                }
            };
            state.release_complete_job(&scope, &job, directory)?;
            continue;
        }
        let Some(frame) = state.release_frame(&scope)? else {
            break;
        };
        work.peak_depth = work
            .peak_depth
            .max(usize::try_from(frame.depth).map_err(|_| ContentError::LengthOverflow)?);
        if frame.finished {
            state.release_pop(&scope, &frame)?;
            continue;
        }
        let listing_after = frame
            .after
            .as_ref()
            .map(ReleaseName::to_path_name)
            .transpose()?;
        let page = list_after(
            reader,
            DirectoryRoot(frame.root),
            listing_after.as_ref(),
            page_entries,
            page_bytes,
            &mut DirectoryReadWork::default(),
        )?;
        if page.entries.len() > 64 {
            return Err(ContentError::ObjectLimitExceeded {
                limit: 64,
                actual: page.entries.len(),
            });
        }
        work.pages = work.pages.saturating_add(1);
        work.entries = work.entries.saturating_add(page.entries.len() as u64);
        let next = ReleaseFrame {
            after: page.continuation.as_ref().map(ReleaseName::from_path_name),
            finished: page.continuation.is_none() || page.entries.is_empty(),
            ..frame
        };
        frame.advances_to(next)?;
        let mut serials = [0; 64];
        for (i, (_, serial)) in page.entries.iter().enumerate() {
            serials[i] = *serial;
        }
        let wave = known_base_wave(
            state,
            &reducer.scope,
            reader,
            table,
            &serials[..page.entries.len()],
            &canonical,
        )?;
        work.base_records = work.base_records.saturating_add(wave.reads);
        let bases = wave.values;
        let mut children = [BaseFact {
            serial: 0,
            value: None,
        }; 64];
        let mut count = 0;
        for (serial, base) in serials[..page.entries.len()].iter().copied().zip(bases) {
            let base = base.ok_or(ContentError::InvalidRecord("released child"))?;
            reducer.removed(state, serial)?;
            work.released = work.released.saturating_add(1);
            let (kind, references) = match reducer.state(state, serial)? {
                Some(PendingState::New { value, count }) => {
                    (value.map_or(base.kind, |v| v.kind), count)
                }
                Some(PendingState::Existing { value, delta }) => (
                    value.map_or(base.kind, |v| v.kind),
                    u64::try_from(
                        (i128::from(base.namespace_ref_count) + i128::from(delta)).max(0),
                    )
                    .unwrap_or(0),
                ),
                None => (base.kind, base.namespace_ref_count),
            };
            if references == 0 && kind == InodeKind::Directory {
                children[count] = BaseFact {
                    serial,
                    value: Some(base),
                };
                count += 1;
            }
        }
        let acknowledged = state.release_advance(&scope, &frame, &next, &children[..count])?;
        if acknowledged != next {
            return Err(ContentError::InvalidOrderingRecord(
                "release acknowledged progress",
            ));
        }
        if !page.entries.is_empty() && work.released > 0 {
            work.traversed_directories = work.traversed_directories.saturating_add(1);
        }
    }
    let seal = state.release_seal(&scope)?;
    if seal.scope != scope {
        return Err(ContentError::InvalidOrderingRecord("release selected seal"));
    }
    state.release_retire(&seal)?;
    Ok(work)
}

/// Exact maximum fixed release control/base wave; no growing queue/stack is resident.
/// The inserted-fact wave returns before the child array is created; the latter
/// plus one answer vector is narrower than the simultaneous demand/read wave.
pub const fn canonical_release_working_bytes() -> usize {
    std::mem::size_of::<ZeroLedger>()
        + std::mem::size_of::<ReleaseJob>()
        + 2 * std::mem::size_of::<ReleaseFrame>()
        + std::mem::size_of::<Option<crate::filesystem::PathName>>()
        + 255
        + 64 * (std::mem::size_of::<BaseFact>()
            + 2 * std::mem::size_of::<u64>()
            + std::mem::size_of::<usize>()
            + 2 * std::mem::size_of::<Option<InodeValue>>())
}
