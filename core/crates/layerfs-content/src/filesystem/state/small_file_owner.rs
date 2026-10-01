//! Fixed concrete ordinary-file authority; no native owner or generic proof adoption.
use super::empty_owner::{bad, EmptyData};
use super::*;
use crate::filesystem::{references::*, rows::VerifiedSmallFileRows, InodeUpdate};
use crate::{ContentError, ContentResult};
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum SmallStage {
    Unbegun,
    Mutable,
    Effects,
    Resumed,
    Final,
    Retired,
}
pub(super) struct SmallStateData {
    pub(super) rows: [Option<InodeUpdate>; 8],
    pub(super) bases: [Option<BaseFact>; 8],
    pub(super) len: usize,
    pub(super) table: crate::filesystem::inode::read::InodeTable,
    pub(super) fact_scope: Option<FactScope>,
    pub(super) fact_seal: Option<FactSeal>,
    pub(super) fact_eof: bool,
    pub(super) counts: Option<CanonicalScope>,
    pub(super) stage: SmallStage,
    pub(super) records: [Option<CountRecord>; 8],
    pub(super) effects: Option<CountSeal>,
    pub(super) final_seal: Option<CountSeal>,
    pub(super) final_eof: bool,
    pub(super) zeros: [Option<BaseFact>; 8],
    pub(super) zero_len: usize,
    pub(super) zero_seal: Option<ZeroSeal>,
    pub(super) zero_eof: bool,
    pub(super) release_scope: Option<CanonicalScope>,
    pub(super) seed_len: usize,
    pub(super) seeds_closed: bool,
    pub(super) jobs: [Option<ReleaseJob>; 8],
    pub(super) next_job: usize,
    pub(super) current: Option<ReleaseJob>,
    pub(super) completed_jobs: u64,
    pub(super) release_eof: bool,
    pub(super) release_seal: Option<ReleaseSeal>,
    pub(super) release_retired: bool,
}
/// A separately verified <=8 existing-file owner on the ordinary Canonical8 producer.
pub struct VerifiedSmallFileState {
    pub(super) inner: VerifiedEmptyState,
    pub(super) data: Box<SmallStateData>,
    _memory: GraphMemoryLease,
}
impl VerifiedSmallFileState {
    /// Verify real source/table/kinds/EOF, then admit compiled simultaneous metadata shape.
    pub fn new(selector: [u8; 32], rows: &VerifiedSmallFileRows) -> ContentResult<Self> {
        let mut credit = rows.take_actor_credit(selector)?;
        let empty_credit = credit.split(std::mem::size_of::<EmptyData>() + 128)?;
        let mut inner =
            VerifiedEmptyState::new_with_credit(selector, rows.subject().clone(), empty_credit)?;
        inner.confirm_small_file(rows)?;
        let mut selected = [None; 8];
        let mut bases = [None; 8];
        for at in 0..rows.len() {
            let row = rows.row(at).ok_or(bad("small file row proof"))?;
            selected[at] = Some(row);
            bases[at] = rows.base(row.serial);
        }
        Ok(Self {
            inner,
            data: Box::new(SmallStateData {
                rows: selected,
                bases,
                len: rows.len(),
                table: rows.table(),
                fact_scope: None,
                fact_seal: None,
                fact_eof: false,
                counts: None,
                stage: SmallStage::Unbegun,
                records: [None; 8],
                effects: None,
                final_seal: None,
                final_eof: false,
                zeros: [None; 8],
                zero_len: 0,
                zero_seal: None,
                zero_eof: false,
                release_scope: None,
                seed_len: 0,
                seeds_closed: false,
                jobs: [None; 8],
                next_job: 0,
                current: None,
                completed_jobs: 0,
                release_eof: false,
                release_seal: None,
                release_retired: false,
            }),
            _memory: credit,
        })
    }
    /// Consume the exact pre-Save captured selection from verified rows once.
    pub fn from_rows(rows: &VerifiedSmallFileRows) -> ContentResult<Self> {
        Self::new(
            rows.captured_selector()
                .ok_or(bad("small file captured admission absent"))?,
            rows,
        )
    }
    pub(crate) const fn admitted_actor_bytes() -> usize {
        std::mem::size_of::<EmptyData>()
            + 128
            + std::mem::size_of::<SmallStateData>()
            + std::mem::size_of::<Self>()
    }
    pub(crate) fn admitted_window_bytes() -> usize {
        let count = std::mem::size_of::<CountPage>() + 8 * std::mem::size_of::<CountRecord>();
        let zero = std::mem::size_of::<ZeroPage>() + 8 * std::mem::size_of::<BaseFact>();
        let facts = std::mem::size_of::<FactPage<BaseFact>>() + 8 * std::mem::size_of::<BaseFact>();
        let roots_scan = std::mem::size_of::<StatePage>() + directory_root_cursor_working_bytes();
        let phase = (canonical_zero_working_bytes() + count)
            .max(canonical_release_working_bytes() + zero)
            .max(canonical_final_working_bytes() + count + directory_root_cursor_working_bytes())
            .max(roots_scan)
            .max(crate::filesystem::update::canonical_directory_working_bytes(0))
            + canonical_base_working_bytes()
            + 64 * std::mem::size_of::<Option<crate::object::inode_leaf::InodeValue>>();
        canonical_count_control_working_bytes()
            + crate::filesystem::update::canonical_reduction_working_bytes()
            + directory_roots_working_bytes(0)
            + phase.max(facts)
    }
    /// Exact issued logical context shared by all real narrow roles.
    pub fn scopes(&self) -> &GraphConstructionScopes {
        self.inner.scopes()
    }
    /// Selected operation identity; no native binding is fabricated.
    pub fn selection(&self) -> &StateSelection {
        self.inner.selection()
    }
    /// Captured source/base/namespace/root/logicalS context.
    pub fn subject(&self) -> &GraphSubject {
        self.inner.subject()
    }
    /// All required metadata/final consumer completions before root publication.
    pub fn completed(&self) -> bool {
        self.inner.completed()
            && self.data.stage == SmallStage::Retired
            && self.data.release_retired
    }
    /// Truthful zero native files/binding/allocation/cleanup effects.
    pub fn physical(&self) -> EmptyPhysical {
        self.inner.physical()
    }
    /// Actual source/state/helper/held-page working credits, without a process claim.
    pub fn working_bytes(&self) -> usize {
        self.inner.working_bytes()
    }
    /// Own-operation failure only; no retry, guessed refund or native cleanup.
    pub fn abandon(&mut self) {
        self.inner.abandon();
    }
    pub(super) fn selected(
        &self,
        scope: &CanonicalScope,
        table: StateTable,
        phase: u64,
    ) -> ContentResult<()> {
        self.inner.live()?;
        if scope.state().selection() != self.selection()
            || scope.subject().selected() != self.subject()
            || scope.subject().table() != Some(self.data.table)
            || scope.state().table() != table
            || scope.state().phase() != phase
        {
            return Err(bad("small file foreign canonical context"));
        }
        Ok(())
    }
    pub(super) fn counts_scope(&self, scope: &CanonicalScope) -> ContentResult<()> {
        self.selected(scope, StateTable::Counts, 6)?;
        if self.data.counts.as_ref() != Some(scope) || self.data.stage == SmallStage::Retired {
            return Err(bad("small file count phase"));
        }
        Ok(())
    }
    pub(super) fn position(&self, serial: u64) -> Option<usize> {
        self.data.rows[..self.data.len]
            .iter()
            .position(|row| row.unwrap().serial == serial)
    }
    pub(super) fn base(&self, serial: u64) -> ContentResult<BaseFact> {
        self.position(serial)
            .and_then(|at| self.data.bases[at])
            .ok_or(bad("small file unselected base"))
    }
    pub(super) fn capacity_class() -> ContentResult<CanonicalCapacity> {
        CanonicalCapacity::new(
            8,
            8,
            8,
            0,
            ALIAS_FIXED_BYTES
                + FACT_OWNER_ALLOWANCE
                + CANONICAL_OWNER_ALLOWANCE
                + 8 * (105 + 128 + 105 + 113),
        )
    }
    pub(super) fn occupancy(&self, counts: u64, zeros: u64, jobs: u64) -> ContentResult<()> {
        Self::capacity_class()?.check(CanonicalOccupancy {
            namespace: FactOccupancy {
                facts: if self.inner.data.facts_retired {
                    0
                } else {
                    self.data.len as u64
                },
                ..FactOccupancy::default()
            },
            counts,
            zeros,
            jobs,
            frames: 0,
        })
    }
    pub(super) fn page_limit(
        after: Option<u64>,
        maximum: Option<u64>,
        records: usize,
        bytes: usize,
        header: usize,
        width: usize,
    ) -> ContentResult<usize> {
        if records == 0
            || records > 128
            || bytes < header
            || bytes > 65536
            || after.is_some_and(|s| maximum.is_none_or(|m| s > m))
        {
            return Err(bad("small file page bounds"));
        }
        let n = records.min(8).min((bytes - header) / width);
        if n == 0 && maximum.is_some_and(|m| after.is_none_or(|s| s < m)) {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "small file page bytes",
                limit: bytes as u64,
                actual: (header + width) as u64,
            });
        }
        Ok(n)
    }
}
