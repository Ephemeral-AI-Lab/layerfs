//! File-level Create source contracts and semantic input records.
//!
//! Complete root, manifest, version, and candidate composition belongs to
//! the lifecycle coordinator. This module only owns the bounded source port
//! and the caller-owned manifest record consumed by that coordinator.

use crate::content::ContentSourceV1;
use crate::{CoreError, CoreResult};

pub trait SourceSupplierV1 {
    type Source: ContentSourceV1;

    /// Side-effect-free bound queried only after the root grant is held.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn supply(self) -> CoreResult<Self::Source>;
}

/// One file in a bounded, canonically ordered private complete tree operation.
/// The source is retained by the caller and is not read until the complete
/// manifest has passed path, type, count, and memory preflight.
pub struct TreeFileV1<'path, S> {
    path: &'path [u8],
    mode: u16,
    declared_len: u64,
    supplier: Option<S>,
}

impl<'path, S> TreeFileV1<'path, S> {
    pub const fn new(path: &'path [u8], mode: u16, declared_len: u64, source: S) -> Self {
        Self {
            path,
            mode,
            declared_len,
            supplier: Some(source),
        }
    }

    pub(crate) const fn path(&self) -> &'path [u8] {
        self.path
    }

    pub(crate) const fn mode(&self) -> u16 {
        self.mode
    }

    pub(crate) const fn declared_len(&self) -> u64 {
        self.declared_len
    }

    pub(crate) fn supplier_ref(&self) -> Option<&S> {
        self.supplier.as_ref()
    }

    pub(crate) fn take_supplier(&mut self) -> CoreResult<S> {
        self.supplier.take().ok_or(CoreError::SourceFailure)
    }
}

#[cfg(test)]
pub(crate) use crate::lifecycle::{run_create_tree_v1, run_create_v1};
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cas::{
        global_seen_hash_v1, FileGlobalSeenSpoolV1, FsCasBoundaryV1, FsCasControlV1, FsCasErrorV1,
        FsCasV1, FsOperationKindV1, GlobalSeenErrorV1, GlobalSeenRecordV1,
        GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1, GLOBAL_SEEN_RECORD_BYTES,
    };
    use crate::cdc::{CdcAlgorithmV1, CdcControlV1, MAXIMUM_CHUNK_BYTES};
    use crate::content::ContentSourceV1;
    use crate::cow::{TreePageSummaryV1, MAX_TREE_OBJECT_BYTES};
    use crate::format::PhysicalObjectKindV1;
    use crate::identity::COMPARISON_WINDOW_BYTES;
    use crate::lifecycle::{
        request_create_operation_v1, request_tree_operation_v1, OperationBuffersV1,
        OperationErrorV1,
    };
    use crate::limits::OperationCountersV1;
    use crate::object::TypedPhysicalObjectIdV1;
    use crate::pack::MAX_PACK_BYTES;
    use crate::{CoreError, CoreResult};
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let sequence = NEXT_TEST_ROOT.fetch_add(1, AtomicOrdering::Relaxed);
            let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp directory");
            Self(parent.join(format!(
                "layerfs-private-tree-test-{}-{sequence:016x}",
                std::process::id()
            )))
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct FragmentedSource<'a> {
        bytes: &'a [u8],
        offset: usize,
        maximum_read: usize,
    }

    impl ContentSourceV1 for FragmentedSource<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read(
            &mut self,
            destination: &mut [u8],
        ) -> Result<usize, crate::content::ContentSourceErrorV1> {
            let take = destination
                .len()
                .min(self.maximum_read)
                .min(self.bytes.len() - self.offset);
            destination[..take].copy_from_slice(&self.bytes[self.offset..self.offset + take]);
            self.offset += take;
            Ok(take)
        }
    }

    struct FragmentedSupplier<'a> {
        bytes: &'a [u8],
        maximum_read: usize,
        cas: &'a FsCasV1,
    }

    struct EmptyShapeSupplier<'a> {
        calls: &'a AtomicU64,
        cas: &'a FsCasV1,
    }

    impl<'a> SourceSupplierV1 for EmptyShapeSupplier<'a> {
        type Source = FragmentedSource<'static>;

        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<FragmentedSource<'static>>() as u64)
        }

        fn supply(self) -> CoreResult<Self::Source> {
            assert_eq!(self.cas.operation_admitted_slots_v1(), 1);
            self.calls.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(FragmentedSource {
                bytes: &[],
                offset: 0,
                maximum_read: 1,
            })
        }
    }

    impl<'a> SourceSupplierV1 for FragmentedSupplier<'a> {
        type Source = FragmentedSource<'a>;

        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<FragmentedSource<'_>>() as u64)
        }

        fn supply(self) -> CoreResult<Self::Source> {
            assert_eq!(self.cas.operation_admitted_slots_v1(), 1);
            Ok(FragmentedSource {
                bytes: self.bytes,
                offset: 0,
                maximum_read: self.maximum_read,
            })
        }
    }

    #[derive(Default)]
    struct ContinueControl;

    impl CdcControlV1 for ContinueControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for ContinueControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct PreparationNamespaceControlV1 {
        preparation: PathBuf,
        carrier_phases: Vec<Vec<String>>,
        marker_phases: Vec<Vec<String>>,
        receipt_spool_high_water: u64,
        receipt_spool_name: Option<String>,
        carrier_receipt_starts: Vec<u64>,
        carrier_receipt_high_waters: Vec<u64>,
    }

    const LOCATOR_RECEIPT_RECORD_BYTES_V1: u64 = 184;

    impl PreparationNamespaceControlV1 {
        fn new(root: &std::path::Path) -> Self {
            Self {
                preparation: root.join("preparation"),
                carrier_phases: Vec::new(),
                marker_phases: Vec::new(),
                receipt_spool_high_water: 0,
                receipt_spool_name: None,
                carrier_receipt_starts: Vec::new(),
                carrier_receipt_high_waters: Vec::new(),
            }
        }

        fn snapshot_v1(&mut self, carrier_start: bool) -> Vec<String> {
            let mut receipt_spool = None;
            let mut names = fs::read_dir(&self.preparation)
                .expect("preparation directory")
                .map(|entry| {
                    let entry = entry.expect("preparation entry");
                    let name = entry
                        .file_name()
                        .into_string()
                        .expect("ASCII preparation name");
                    if name.starts_with("locator-receipts-") {
                        let len = entry.metadata().expect("receipt metadata").len();
                        assert!(receipt_spool.replace((name.clone(), len)).is_none());
                        self.receipt_spool_high_water = self.receipt_spool_high_water.max(len);
                    }
                    name
                })
                .collect::<Vec<_>>();
            names.sort();
            let (receipt_spool_name, receipt_spool_len) =
                receipt_spool.expect("one operation-owned locator receipt spool");
            assert_eq!(
                receipt_spool_len % LOCATOR_RECEIPT_RECORD_BYTES_V1,
                0,
                "every physical receipt-spool observation is record-aligned"
            );
            match self.receipt_spool_name.as_deref() {
                Some(expected) => assert_eq!(receipt_spool_name, expected),
                None => self.receipt_spool_name = Some(receipt_spool_name),
            }
            if carrier_start {
                self.carrier_receipt_starts.push(receipt_spool_len);
                self.carrier_receipt_high_waters.push(receipt_spool_len);
            } else {
                let high_water = self
                    .carrier_receipt_high_waters
                    .last_mut()
                    .expect("carrier start precedes locator publication");
                *high_water = (*high_water).max(receipt_spool_len);
            }
            names
        }

        fn assert_exact_namespace_lifetime_v1(
            &self,
            expected_high_water: u64,
            counters: &OperationCountersV1,
        ) {
            assert_eq!(
                counters.storage_preparation_inodes_high_water, expected_high_water,
                "compatibility counter measures logical namespace entries"
            );
            assert!(!self.carrier_phases.is_empty());
            assert!(!self.marker_phases.is_empty());
            for carrier in &self.carrier_phases {
                assert_eq!(carrier.len() as u64, expected_high_water);
                for marker in &self.marker_phases {
                    assert_eq!(marker.len() as u64, expected_high_water);
                    assert_eq!(
                        carrier.iter().filter(|name| marker.contains(name)).count() as u64,
                        expected_high_water - 1,
                        "the private carrier and private marker names are phase-local"
                    );
                }
            }
            assert_eq!(
                self.receipt_spool_high_water,
                counters.locator_installs * LOCATOR_RECEIPT_RECORD_BYTES_V1,
                "one real file-backed receipt record per installed locator"
            );
        }

        fn assert_receipt_spool_reused_across_carriers_v1(&self, counters: &OperationCountersV1) {
            assert!(self.carrier_receipt_starts.len() >= 2);
            assert_eq!(
                self.carrier_receipt_starts.len(),
                self.carrier_receipt_high_waters.len()
            );
            assert_eq!(self.carrier_receipt_starts[0], 0);
            assert_eq!(
                self.carrier_receipt_starts[1], 0,
                "the same operation spool is reset before carrier two"
            );
            assert!(self.carrier_receipt_high_waters[0] > 0);
            assert!(self.carrier_receipt_high_waters[1] > 0);

            let maximum_carrier_receipts = self
                .carrier_receipt_high_waters
                .iter()
                .copied()
                .max()
                .unwrap()
                / LOCATOR_RECEIPT_RECORD_BYTES_V1;
            assert_eq!(
                self.receipt_spool_high_water,
                maximum_carrier_receipts * LOCATOR_RECEIPT_RECORD_BYTES_V1
            );
            let observed_operation_receipts = self
                .carrier_receipt_high_waters
                .iter()
                .map(|bytes| bytes / LOCATOR_RECEIPT_RECORD_BYTES_V1)
                .sum::<u64>();
            assert_eq!(observed_operation_receipts, counters.locator_installs);
            assert!(
                self.receipt_spool_high_water
                    < counters.locator_installs * LOCATOR_RECEIPT_RECORD_BYTES_V1,
                "physical receipt storage is one carrier maximum, not the operation total"
            );
        }
    }

    impl CdcControlV1 for PreparationNamespaceControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationNamespaceControlV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            match boundary {
                FsCasBoundaryV1::BeforeCarrierInstall => {
                    let snapshot = self.snapshot_v1(true);
                    self.carrier_phases.push(snapshot);
                }
                FsCasBoundaryV1::AfterObjectLocatorMarkerLink => {
                    let snapshot = self.snapshot_v1(false);
                    self.marker_phases.push(snapshot);
                }
                _ => {}
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Default)]
    struct CancelGlobalSeenControl;

    impl FsCasControlV1 for CancelGlobalSeenControl {
        fn cancellation_requested(&mut self) -> bool {
            true
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct CorruptFirstPublishedCarrierControl {
        objects: PathBuf,
        corrupted_locator_count: u64,
        corrupted: bool,
    }

    impl CdcControlV1 for CorruptFirstPublishedCarrierControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CorruptFirstPublishedCarrierControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary != FsCasBoundaryV1::AfterCatalogPublication || self.corrupted {
                return;
            }
            self.corrupted = true;
            for entry in fs::read_dir(&self.objects).expect("published object directory") {
                let path = entry.expect("published object locator").path();
                let mut bytes = fs::read(&path).expect("read published locator");
                assert_eq!(&bytes[..8], b"LFSOBJ01");
                bytes[..8].copy_from_slice(b"CORRUPT!");
                fs::remove_file(&path).expect("remove locator for injected replacement");
                let mut replacement = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .expect("create corrupt locator replacement");
                replacement
                    .write_all(&bytes)
                    .expect("write corrupt locator replacement");
                let mut permissions = replacement
                    .metadata()
                    .expect("corrupt locator metadata")
                    .permissions();
                permissions.set_readonly(true);
                replacement
                    .set_permissions(permissions)
                    .expect("make corrupt locator replacement immutable");
                self.corrupted_locator_count += 1;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[test]
    fn non_tree_operation_reaches_six_live_preparation_namespace_entries() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let bytes = b"namespace-envelope";
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = vec![None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = PreparationNamespaceControlV1::new(&fixture.0);
        let operation = request_create_operation_v1(&cas, 6, &mut counters, &mut control).unwrap();

        let handoff = run_create_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            b"file.bin",
            0o644,
            bytes.len() as u64,
            FragmentedSupplier {
                bytes,
                maximum_read: bytes.len(),
                cas: &cas,
            },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

        assert_eq!(handoff.carrier_count(), 1);
        control.assert_exact_namespace_lifetime_v1(6, &counters);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation")).unwrap().count(),
            0
        );
    }

    #[test]
    fn tree_operation_reaches_eight_live_preparation_namespace_entries() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let bytes = b"tree-namespace-envelope";
        let mut files = [TreeFileV1::new(
            b"file.bin",
            0o644,
            bytes.len() as u64,
            FragmentedSupplier {
                bytes,
                maximum_read: bytes.len(),
                cas: &cas,
            },
        )];
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = vec![None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = PreparationNamespaceControlV1::new(&fixture.0);
        let operation = request_tree_operation_v1(&cas, 8, &mut counters, &mut control).unwrap();

        let handoff = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

        assert_eq!(handoff.carrier_count(), 1);
        control.assert_exact_namespace_lifetime_v1(8, &counters);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation")).unwrap().count(),
            0
        );
    }

    #[test]
    fn global_seen_file_table_has_exact_bounded_collision_work() {
        const CAPACITY: u32 = 64;
        const COLLIDING_IDS: usize = 16;

        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let mut operation_control = ContinueControl;
        let mut operation_counters = OperationCountersV1::default();
        let operation = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                0,
                &mut operation_counters,
                &mut operation_control,
            )
            .expect("reserve the root-owned operation first");
        let mut table = FileGlobalSeenSpoolV1::new(
            cas.begin_operation_spool_v1("global-seen-test", &mut operation_control)
                .expect("create operation spool"),
        );
        table.initialize(CAPACITY).expect("initialize table");

        let mut ids = Vec::with_capacity(COLLIDING_IDS);
        let mut candidate = 0_u64;
        while ids.len() < COLLIDING_IDS {
            let mut digest = [0_u8; 32];
            digest[..8].copy_from_slice(&candidate.to_be_bytes());
            let id =
                TypedPhysicalObjectIdV1::from_kind_and_digest(PhysicalObjectKindV1::Chunk, digest);
            if global_seen_hash_v1(id) & u64::from(CAPACITY - 1) == 0 {
                ids.push(id);
            }
            candidate = candidate.checked_add(1).expect("bounded search");
        }

        let mut control = ContinueControl;
        for (ordinal, id) in ids.iter().copied().enumerate() {
            let lookup = table.lookup(id, &mut control).expect("vacant lookup");
            assert!(lookup.record.is_none());
            assert_eq!(lookup.vacant_slot, ordinal as u32);
            table
                .insert(
                    lookup.vacant_slot,
                    id,
                    GlobalSeenRecordV1 {
                        complete_len: 52,
                        private_payload_offset: ordinal as u64,
                        carrier_ordinal: 0,
                    },
                )
                .expect("insert");
        }
        for (ordinal, id) in ids.iter().copied().enumerate() {
            let lookup = table.lookup(id, &mut control).expect("occupied lookup");
            let record = lookup.record.expect("record");
            assert_eq!(record.complete_len, 52);
            assert_eq!(record.private_payload_offset, ordinal as u64);
            assert_eq!(record.carrier_ordinal, 0);
        }

        let triangular = (COLLIDING_IDS * (COLLIDING_IDS + 1) / 2) as u64;
        assert_eq!(
            table.work_observation(),
            (
                (COLLIDING_IDS * 2) as u64,
                triangular * 2,
                COLLIDING_IDS as u32,
                COLLIDING_IDS as u32
            )
        );
        assert_eq!(
            table.direct_storage_observation(),
            (
                triangular * 2 * GLOBAL_SEEN_RECORD_BYTES,
                triangular * 2,
                COLLIDING_IDS as u64 * GLOBAL_SEEN_RECORD_BYTES,
            )
        );
        assert_eq!(
            table.storage_bytes(),
            u64::from(CAPACITY) * GLOBAL_SEEN_RECORD_BYTES
        );
        table
            .cleanup_controlled_v1(&mut ContinueControl)
            .expect("controlled cleanup");
        drop(operation);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation"))
                .expect("preparation directory")
                .count(),
            0
        );
    }

    #[test]
    fn global_seen_adversarial_cluster_stops_at_frozen_probe_budget_and_polls_first() {
        const CAPACITY: u32 = 1_024;
        const COLLIDING_IDS: usize = GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1 as usize + 1;

        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let mut operation_control = ContinueControl;
        let mut operation_counters = OperationCountersV1::default();
        let operation = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                0,
                &mut operation_counters,
                &mut operation_control,
            )
            .expect("reserve the root-owned operation first");
        let mut table = FileGlobalSeenSpoolV1::new(
            cas.begin_operation_spool_v1("global-seen-budget-test", &mut operation_control)
                .expect("create operation spool"),
        );
        table.initialize(CAPACITY).expect("initialize table");

        let mut ids = Vec::with_capacity(COLLIDING_IDS);
        let mut candidate = 0_u64;
        while ids.len() < COLLIDING_IDS {
            let mut digest = [0_u8; 32];
            digest[..8].copy_from_slice(&candidate.to_be_bytes());
            let id =
                TypedPhysicalObjectIdV1::from_kind_and_digest(PhysicalObjectKindV1::Chunk, digest);
            if global_seen_hash_v1(id) & u64::from(CAPACITY - 1) == 0 {
                ids.push(id);
            }
            candidate = candidate.checked_add(1).expect("bounded search");
        }

        let mut control = ContinueControl;
        for (ordinal, id) in ids
            .iter()
            .copied()
            .take(GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1 as usize)
            .enumerate()
        {
            let lookup = table.lookup(id, &mut control).expect("bounded lookup");
            assert!(lookup.record.is_none());
            table
                .insert(
                    lookup.vacant_slot,
                    id,
                    GlobalSeenRecordV1 {
                        complete_len: 52,
                        private_payload_offset: ordinal as u64,
                        carrier_ordinal: 0,
                    },
                )
                .expect("bounded insert");
        }
        assert!(matches!(
            table.lookup(ids[COLLIDING_IDS - 1], &mut control),
            Err(GlobalSeenErrorV1::Core(CoreError::CountCap))
        ));
        assert_eq!(
            table.maximum_probe,
            GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1
        );

        let reads_before_cancel = table.direct_storage_observation();
        let mut cancelled = CancelGlobalSeenControl;
        assert!(matches!(
            table.lookup(ids[0], &mut cancelled),
            Err(GlobalSeenErrorV1::Core(CoreError::Cancelled))
        ));
        assert_eq!(table.direct_storage_observation(), reads_before_cancel);
        table
            .cleanup_controlled_v1(&mut ContinueControl)
            .expect("controlled cleanup");
        drop(operation);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
    }

    fn deterministic_bytes(len: usize, mut state: u64) -> Vec<u8> {
        let mut bytes = vec![0_u8; len];
        for destination in bytes.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            destination.copy_from_slice(&state.to_le_bytes()[..destination.len()]);
        }
        bytes
    }

    #[test]
    fn global_seen_reuses_equal_objects_across_real_carrier_rollover() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let shared = deterministic_bytes(256 * 1024 + 19, 0x61d3_75a4_9bf2_08ce);
        let large = deterministic_bytes(
            usize::try_from(MAX_PACK_BYTES + 2 * 1024 * 1024).expect("bounded fixture"),
            0x8a51_e349_c072_6dbf,
        );
        let mut files = [
            TreeFileV1::new(
                b"a-shared.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"b-large.bin",
                0o644,
                large.len() as u64,
                FragmentedSupplier {
                    bytes: &large,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"z-shared-again.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
        ];
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = PreparationNamespaceControlV1::new(&fixture.0);
        let operation = request_tree_operation_v1(&cas, 1, &mut counters, &mut control).unwrap();

        let result = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

        assert!(result.carrier_count() >= 2, "real rollover was required");
        control.assert_receipt_spool_reused_across_carriers_v1(&counters);
        assert_eq!(
            counters.carrier_rollovers,
            u64::from(result.carrier_rollovers())
        );
        assert!(counters.global_seen_cross_carrier_reuses > 0);
        assert_eq!(
            counters.global_seen_lookups,
            counters.physical_objects_created + counters.physical_objects_reused
        );
        assert_eq!(
            counters.global_seen_entries,
            counters.pack_local_objects_created
        );
        assert_eq!(
            counters.pack_local_objects_created + counters.pack_local_objects_reused,
            counters.physical_objects_created + counters.physical_objects_reused
        );
        assert_eq!(
            counters.physical_carrier_object_writes,
            counters.pack_local_objects_created
        );
        assert_eq!(
            counters.locator_installs,
            counters.physical_carrier_object_writes
        );
        assert_eq!(counters.locator_equal_incumbent_reuses, 0);
        assert_eq!(
            counters.version_objects_created
                + counters.tree_objects_created
                + counters.file_objects_created
                + counters.symlink_objects_created
                + counters.chunk_objects_created,
            counters.pack_local_objects_created
        );
        assert_eq!(
            counters.version_objects_reused
                + counters.tree_objects_reused
                + counters.file_objects_reused
                + counters.symlink_objects_reused
                + counters.chunk_objects_reused,
            counters.pack_local_objects_reused
        );
        assert!(counters.global_seen_probes >= counters.global_seen_lookups);
        assert!(counters.global_seen_maximum_probe > 0);
        assert!(counters.global_seen_metadata_read_calls > 0);
        assert_eq!(
            counters.global_seen_metadata_bytes_read,
            counters.global_seen_probes * GLOBAL_SEEN_RECORD_BYTES
        );
        assert_eq!(
            counters.global_seen_metadata_bytes_written,
            counters.global_seen_entries * GLOBAL_SEEN_RECORD_BYTES
        );
        assert!(counters.carrier_bytes_total > MAX_PACK_BYTES);
        assert!(counters.maximum_active_carrier_bytes <= MAX_PACK_BYTES);
        assert!(counters.final_carrier_bytes <= MAX_PACK_BYTES);
        assert_eq!(counters.closure_objects_missing, 0);
        assert_eq!(
            counters.closure_objects_occupied_validated,
            result.object_count()
        );
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters
                .storage_bytes_released
                .checked_add(counters.storage_bytes_committed)
                .and_then(|value| value.checked_add(counters.storage_bytes_retained))
                .unwrap()
        );
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters
                .storage_inodes_released
                .checked_add(counters.storage_inodes_committed)
                .and_then(|value| value.checked_add(counters.storage_inodes_retained))
                .unwrap()
        );
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation"))
                .expect("preparation directory")
                .count(),
            0
        );
    }

    #[test]
    fn cross_carrier_occupied_corruption_remains_a_typed_fscas_error() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let shared = deterministic_bytes(256 * 1024 + 19, 0x61d3_75a4_9bf2_08ce);
        let large = deterministic_bytes(
            usize::try_from(MAX_PACK_BYTES + 2 * 1024 * 1024).expect("bounded fixture"),
            0x8a51_e349_c072_6dbf,
        );
        let mut files = [
            TreeFileV1::new(
                b"a-shared.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"b-large.bin",
                0o644,
                large.len() as u64,
                FragmentedSupplier {
                    bytes: &large,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
            TreeFileV1::new(
                b"z-shared-again.bin",
                0o644,
                shared.len() as u64,
                FragmentedSupplier {
                    bytes: &shared,
                    maximum_read: MAXIMUM_CHUNK_BYTES,
                    cas: &cas,
                },
            ),
        ];
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut control = CorruptFirstPublishedCarrierControl {
            objects: fixture.0.join("objects"),
            corrupted_locator_count: 0,
            corrupted: false,
        };
        let operation = request_tree_operation_v1(&cas, 2, &mut counters, &mut control).unwrap();

        let result = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        );

        assert_eq!(
            result,
            Err(OperationErrorV1::FsCas(FsCasErrorV1::MalformedOccupant))
        );
        assert!(control.corrupted_locator_count > 0);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(
            fs::read_dir(fixture.0.join("preparation"))
                .expect("preparation directory")
                .count(),
            0
        );
        assert_eq!(
            fs::read_dir(fixture.0.join("closures"))
                .expect("closure directory")
                .count(),
            0
        );
    }

    #[test]
    fn complete_tree_shapes_cover_100_1024_10240_and_25600_files() {
        for file_count in [100_usize, 1_024, 10_240, 25_600] {
            let fixture = TestRoot::new();
            let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
            let supplier_calls = AtomicU64::new(0);
            let paths = (0..file_count)
                .map(|ordinal| format!("f-{ordinal:05}.empty").into_bytes())
                .collect::<Vec<_>>();
            let mut files = paths
                .iter()
                .map(|path| {
                    TreeFileV1::new(
                        path,
                        0o644,
                        0,
                        EmptyShapeSupplier {
                            calls: &supplier_calls,
                            cas: &cas,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let mut counters = OperationCountersV1::default();
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages =
                vec![None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = vec![0_u8; 64 * 1024];
            let mut control = ContinueControl;
            let operation =
                request_tree_operation_v1(&cas, 3, &mut counters, &mut control).unwrap();

            let result = run_create_tree_v1(
                operation,
                CdcAlgorithmV1::FastCdc,
                &mut files,
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            )
            .unwrap_or_else(|error| panic!("{file_count} files: {error:?}; {counters:#?}"));

            assert_eq!(
                supplier_calls.load(AtomicOrdering::Relaxed),
                file_count as u64
            );
            assert_eq!(result.carrier_count(), 1);
            assert_eq!(counters.closure_fences, 1);
            assert_eq!(counters.closure_objects_missing, 0);
            assert_eq!(
                counters.closure_objects_occupied_validated,
                result.object_count()
            );
            assert_eq!(
                counters.pack_local_objects_created + counters.pack_local_objects_reused,
                counters.physical_objects_created + counters.physical_objects_reused
            );
            assert_eq!(
                counters.physical_carrier_object_writes,
                counters.pack_local_objects_created
            );
            assert_eq!(
                counters.locator_installs,
                counters.physical_carrier_object_writes
            );
            assert_eq!(counters.locator_equal_incumbent_reuses, 0);
            assert_eq!(
                counters.version_objects_created
                    + counters.version_objects_reused
                    + counters.tree_objects_created
                    + counters.tree_objects_reused
                    + counters.file_objects_created
                    + counters.file_objects_reused
                    + counters.symlink_objects_created
                    + counters.symlink_objects_reused
                    + counters.chunk_objects_created
                    + counters.chunk_objects_reused,
                counters.pack_local_objects_created + counters.pack_local_objects_reused
            );
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(
                fs::read_dir(fixture.0.join("preparation"))
                    .expect("preparation directory")
                    .count(),
                0
            );
        }
    }

    #[test]
    fn private_multi_entry_candidate_root_is_complete_and_fragmentation_independent() {
        let fixture = TestRoot::new();
        let cas = FsCasV1::create_new(&fixture.0).expect("create FsCas");
        let alpha = vec![0x11; 17 * 1024 + 3];
        let beta = vec![0x22; 41 * 1024 + 5];
        let gamma = vec![0x33; 9 * 1024 + 7];
        let omega = vec![0x44; 65 * 1024 + 11];
        let mut expected_root = None;

        for maximum_read in [1, MAXIMUM_CHUNK_BYTES] {
            let mut counters = OperationCountersV1::default();
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages = [None::<TreePageSummaryV1>; crate::cow::MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = vec![0_u8; 64 * 1024];
            let mut control = ContinueControl;
            let mut files = [
                TreeFileV1::new(
                    b"alpha.bin",
                    0o644,
                    alpha.len() as u64,
                    FragmentedSupplier {
                        bytes: &alpha,
                        maximum_read,
                        cas: &cas,
                    },
                ),
                TreeFileV1::new(
                    b"dir/beta.bin",
                    0o600,
                    beta.len() as u64,
                    FragmentedSupplier {
                        bytes: &beta,
                        maximum_read,
                        cas: &cas,
                    },
                ),
                TreeFileV1::new(
                    b"dir/nested/gamma.bin",
                    0o644,
                    gamma.len() as u64,
                    FragmentedSupplier {
                        bytes: &gamma,
                        maximum_read,
                        cas: &cas,
                    },
                ),
                TreeFileV1::new(
                    b"omega.bin",
                    0o644,
                    omega.len() as u64,
                    FragmentedSupplier {
                        bytes: &omega,
                        maximum_read,
                        cas: &cas,
                    },
                ),
            ];
            let operation =
                request_tree_operation_v1(&cas, 4, &mut counters, &mut control).unwrap();
            let result = run_create_tree_v1(
                operation,
                CdcAlgorithmV1::FastCdc,
                &mut files,
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            )
            .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

            if let Some(root) = expected_root {
                assert_eq!(result.root_tree(), root);
                assert_eq!(result.carriers_reused(), result.carrier_count());
            } else {
                expected_root = Some(result.root_tree());
                assert_eq!(result.carriers_installed(), result.carrier_count());
            }
            assert!(result.object_count() >= 12);
            assert_eq!(
                counters.source_bytes_read,
                (alpha.len() + beta.len() + gamma.len() + omega.len()) as u64
            );
            assert_eq!(counters.closure_fences, 1);
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(
                fs::read_dir(fixture.0.join("preparation"))
                    .expect("preparation directory")
                    .count(),
                0
            );
        }
    }
}
