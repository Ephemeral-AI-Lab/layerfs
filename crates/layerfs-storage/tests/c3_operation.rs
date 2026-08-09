use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use layerfs_storage::c3::{
    run_c3_create_v1, C3OperationBuffersV1, C3SourceSupplierV1, FileChunkReferenceSpoolV1,
    FilePackIndexSpoolV1,
};
use layerfs_storage::cdc::algorithms::C3CdcAlgorithmV1;
use layerfs_storage::cdc::{CdcControlV1, MAXIMUM_CHUNK_BYTES};
use layerfs_storage::content::{ContentSourceErrorV1, ContentSourceV1};
use layerfs_storage::fscas::{FsCasControlV1, FsCasV1, FsPackAdmissionOutcomeV1};
use layerfs_storage::identity::COMPARISON_WINDOW_BYTES;
use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1, MEMORY_PROFILE_32_MIB};
use layerfs_storage::tree::{TreePageSummaryV1, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES};
use layerfs_storage::CoreResult;

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

struct TestRoot(PathBuf);

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        Self(parent.join(format!(
            "layerfs-c3-operation-{label}-{}-{sequence:016x}",
            std::process::id()
        )))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct SliceSource<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl ContentSourceV1 for SliceSource<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<Self>() as u64)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        let take = destination.len().min(self.bytes.len() - self.offset);
        destination[..take].copy_from_slice(&self.bytes[self.offset..self.offset + take]);
        self.offset += take;
        Ok(take)
    }
}

struct CheckedSupplier<'a> {
    bytes: &'a [u8],
    ledger: &'a ResourceLedgerV1,
}

impl<'a> C3SourceSupplierV1 for CheckedSupplier<'a> {
    type Source = SliceSource<'a>;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        assert_eq!(self.ledger.admitted_slots(), 1);
        Ok(SliceSource {
            bytes: self.bytes,
            offset: 0,
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

#[test]
fn complete_algorithms_use_one_pre_supplier_slot_and_return_all_preparation_resources() {
    let mut input = vec![0_u8; 384 * 1024 + 73];
    let mut state = 0x9e37_79b9_u32;
    for (index, byte) in input.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = (state as u8) ^ (index as u8).wrapping_mul(17);
    }

    for algorithm in [C3CdcAlgorithmV1::FastCdc, C3CdcAlgorithmV1::SeqCdc] {
        let fixture = TestRoot::new("success");
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let references_path = fixture.path().join("reference-spool");
        let metadata_path = fixture.path().join("metadata-spool");
        let mut references = FileChunkReferenceSpoolV1::create(&references_path).unwrap();
        let mut metadata = FilePackIndexSpoolV1::create(&metadata_path).unwrap();
        let ledger = ResourceLedgerV1::new(MEMORY_PROFILE_32_MIB);
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = [0_u8; 64];
        let mut control = ContinueControl;

        let result = run_c3_create_v1(
            &cas,
            algorithm,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier {
                bytes: &input,
                ledger: &ledger,
            },
            &mut references,
            &mut metadata,
            C3OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &ledger,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{algorithm:?}: {error:?}; {counters:#?}"));

        assert_eq!(result.algorithm(), algorithm);
        assert_eq!(result.pack_outcome(), FsPackAdmissionOutcomeV1::Installed);
        assert!(result.object_count() >= 4);
        assert!(result.reference_spool_bytes().unwrap() > 0);
        assert!(result.index_spool_bytes().unwrap() > 0);
        assert_eq!(ledger.admitted_slots(), 0);
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(fs::metadata(references_path).unwrap().len(), 0);
        assert_eq!(fs::metadata(metadata_path).unwrap().len(), 0);
        assert!(counters.source_read_calls > 0);
        assert_eq!(counters.source_bytes_read, input.len() as u64);
        assert!(counters.fscas_read_calls > 0);
        assert!(counters.fscas_bytes_read > 0);
        assert!(counters.fscas_bytes_written > 0);
        assert_eq!(counters.closure_fences, 1);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.has_zero_forbidden_work());
    }
}
