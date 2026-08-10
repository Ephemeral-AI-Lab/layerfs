use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use layerfs_storage::cas::{FsCasBoundaryV1, FsCasControlV1, FsCasV1};
use layerfs_storage::cdc::{C3CdcAlgorithmV1, CdcControlV1, FastCdcV1, MAXIMUM_CHUNK_BYTES};
use layerfs_storage::content::update::{
    AuthenticatedBaseByteReaderV1, BaseChunkEvidenceSourceV1, BaseChunkEvidenceV1, BaseReadErrorV1,
};
use layerfs_storage::content::{
    request_c3_tree_operation_v1, run_c3_create_tree_v1, C3OperationBuffersV1, C3SourceSupplierV1,
    C3TreeFileV1, ContentSourceErrorV1, ContentSourceV1, PreparedSinkErrorV1,
};
use layerfs_storage::cow::file::{AuthenticatedBaseFileV1, UpdateRangeV1};
use layerfs_storage::cow::{
    CanonicalTreeChildV1, CanonicalTreeEntryV1, DirectoryBuildModeV1, DirectoryLogicalIdentityV1,
    TreePageSummaryV1, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES,
};
use layerfs_storage::format::ValidatedComponent;
use layerfs_storage::identity::{
    derive_file_node_v1, derive_logical_chunk_v1, derive_logical_file_v1,
    derive_physical_chunk_id_v1, derive_physical_file_id_v1, LogicalChunkRefV1,
    LogicalFileIdentityV1, PhysicalFileIdV1, PhysicalTreeIdV1, PhysicalVersionRecordIdV1,
    COMPARISON_WINDOW_BYTES,
};
use layerfs_storage::lifecycle::{
    complete_cross_directory_move_operation_v1, run_c3_complete_add_v1,
    run_c3_complete_metadata_v1, run_c3_complete_move_v1, run_c3_complete_remove_v1,
    run_c3_complete_replace_v1, run_c3_complete_update_v1,
};
use layerfs_storage::limits::OperationCountersV1;
use layerfs_storage::profile::ProfileSpecV1;
use layerfs_storage::{CoreError, CoreResult};

use crate::l1_tree_tests::{build, mutation_fixture, replacement_fixture, MutationSource};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

struct TestRoot(PathBuf);

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temporary root");
        Self(parent.join(format!(
            "layerfs-c3-mutation-{label}-{}-{sequence:016x}",
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

#[derive(Default)]
struct ContinueControl {
    boundaries: Vec<FsCasBoundaryV1>,
}

impl CdcControlV1 for ContinueControl {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for ContinueControl {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        self.boundaries.push(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

struct SliceSource<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> SliceSource<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
}

impl ContentSourceV1 for SliceSource<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<Self>() as u64)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        let amount = destination.len().min(self.bytes.len() - self.offset);
        destination[..amount].copy_from_slice(&self.bytes[self.offset..self.offset + amount]);
        self.offset += amount;
        Ok(amount)
    }
}

struct SliceSupplier<'a> {
    bytes: &'a [u8],
}

impl<'a> C3SourceSupplierV1 for SliceSupplier<'a> {
    type Source = SliceSource<'a>;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        Ok(SliceSource::new(self.bytes))
    }
}

struct OperationScratch {
    source: Box<[u8; MAXIMUM_CHUNK_BYTES]>,
    cdc_ring: Box<[u8; MAXIMUM_CHUNK_BYTES]>,
    incoming: Box<[u8; COMPARISON_WINDOW_BYTES]>,
    occupied: Box<[u8; COMPARISON_WINDOW_BYTES]>,
    tree_object: Box<[u8; MAX_TREE_OBJECT_BYTES]>,
    tree_pages: Box<[Option<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES]>,
    traversal: Vec<u8>,
}

impl OperationScratch {
    fn new() -> Self {
        Self {
            source: boxed_zeroes(),
            cdc_ring: boxed_zeroes(),
            incoming: boxed_zeroes(),
            occupied: boxed_zeroes(),
            tree_object: boxed_zeroes(),
            tree_pages: vec![None; MAX_TREE_PAGE_SUMMARIES]
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!("exact tree-page scratch length")),
            traversal: vec![0; 4_096],
        }
    }

    fn borrow(&mut self) -> C3OperationBuffersV1<'_> {
        C3OperationBuffersV1 {
            source: &mut self.source,
            cdc_ring: &mut self.cdc_ring,
            incoming_comparison: &mut self.incoming,
            occupied_comparison: &mut self.occupied,
            tree_object: &mut self.tree_object,
            tree_pages: &mut self.tree_pages[..],
            traversal_state: &mut self.traversal,
        }
    }
}

fn boxed_zeroes<const N: usize>() -> Box<[u8; N]> {
    vec![0_u8; N]
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("exact boxed scratch length"))
}

#[derive(Clone)]
struct ExpectedFile {
    logical: LogicalFileIdentityV1,
    physical: PhysicalFileIdV1,
    chunks: Vec<BaseChunkEvidenceV1>,
}

impl ExpectedFile {
    fn child(&self, mode: u16) -> CanonicalTreeChildV1 {
        CanonicalTreeChildV1::File {
            logical: derive_file_node_v1(mode, self.logical).expect("canonical file node"),
            physical: self.physical,
        }
    }

    fn authenticated(&self, mode: u16) -> AuthenticatedBaseFileV1 {
        AuthenticatedBaseFileV1::new(self.logical, self.physical, mode, self.chunks.len() as u32)
    }

    fn evidence(&self) -> Evidence {
        Evidence {
            chunks: self.chunks.clone(),
            cursor: 0,
        }
    }
}

fn canonical_object(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(52 + payload.len());
    bytes.extend_from_slice(b"ELSOBJ01");
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.push(kind);
    bytes.push(0);
    bytes.extend_from_slice(ProfileSpecV1::frozen().id().as_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn expected_file(data: &[u8], mode: u16) -> ExpectedFile {
    let mut chunks = Vec::new();
    let mut logical_refs = Vec::new();
    let mut physical_refs = Vec::new();
    let mut offset = 0_usize;
    while offset < data.len() {
        let cut = FastCdcV1::new()
            .cut(&data[offset..])
            .expect("bounded FastCDC cut");
        let payload = &data[offset..offset + cut];
        let logical = derive_logical_chunk_v1(payload).expect("logical chunk");
        let physical =
            derive_physical_chunk_id_v1(&canonical_object(0x05, payload)).expect("physical chunk");
        chunks.push(BaseChunkEvidenceV1::new(
            offset as u64,
            logical.id(),
            physical,
            cut as u32,
        ));
        logical_refs.push(LogicalChunkRefV1::from_identity(logical));
        physical_refs.push((cut as u32, physical));
        offset += cut;
    }
    let logical =
        derive_logical_file_v1(data.len() as u64, &logical_refs).expect("logical file identity");
    let mut payload = Vec::new();
    payload.extend_from_slice(&mode.to_be_bytes());
    payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
    payload.extend_from_slice(&u32::from(!data.is_empty()).to_be_bytes());
    if !data.is_empty() {
        payload.push(0x02);
        payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
        payload.extend_from_slice(&(physical_refs.len() as u32).to_be_bytes());
        for (len, id) in physical_refs {
            payload.extend_from_slice(&len.to_be_bytes());
            payload.extend_from_slice(id.as_bytes());
        }
    }
    let physical = derive_physical_file_id_v1(&canonical_object(0x03, &payload))
        .expect("physical file identity");
    ExpectedFile {
        logical,
        physical,
        chunks,
    }
}

#[derive(Clone)]
struct Evidence {
    chunks: Vec<BaseChunkEvidenceV1>,
    cursor: usize,
}

impl Evidence {
    fn total_len(&self) -> u64 {
        self.chunks
            .last()
            .map(|chunk| chunk.end().expect("bounded chunk end"))
            .unwrap_or(0)
    }
}

impl BaseChunkEvidenceSourceV1 for Evidence {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<Self>() as u64)
    }

    fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
        let result = self.chunks.get(self.cursor).copied();
        self.cursor += usize::from(result.is_some());
        Ok(result)
    }

    fn containing(
        &mut self,
        offset: u64,
        include_end: bool,
    ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
        Ok(self.chunks.iter().copied().find(|chunk| {
            let end = chunk.end().expect("bounded chunk end");
            (chunk.start() <= offset && offset < end)
                || (include_end && offset == end && end == self.total_len())
        }))
    }

    fn at_start(
        &mut self,
        offset: u64,
    ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
        Ok(self
            .chunks
            .iter()
            .copied()
            .find(|chunk| chunk.start() == offset))
    }
}

struct BaseBytes<'a> {
    bytes: &'a [u8],
}

impl AuthenticatedBaseByteReaderV1 for BaseBytes<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<Self>() as u64)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), BaseReadErrorV1> {
        let start = usize::try_from(offset).map_err(|_| BaseReadErrorV1::Missing)?;
        let end = start
            .checked_add(destination.len())
            .ok_or(BaseReadErrorV1::Missing)?;
        destination.copy_from_slice(self.bytes.get(start..end).ok_or(BaseReadErrorV1::Missing)?);
        Ok(())
    }

    fn compare_exact_at(
        &mut self,
        offset: u64,
        first: &[u8],
        second: &[u8],
    ) -> Result<bool, BaseReadErrorV1> {
        let start = usize::try_from(offset).map_err(|_| BaseReadErrorV1::Missing)?;
        let first_end = start
            .checked_add(first.len())
            .ok_or(BaseReadErrorV1::Missing)?;
        let end = first_end
            .checked_add(second.len())
            .ok_or(BaseReadErrorV1::Missing)?;
        Ok(self.bytes.get(start..first_end) == Some(first)
            && self.bytes.get(first_end..end) == Some(second))
    }
}

fn entry<'a>(name: &'a [u8], file: &ExpectedFile, mode: u16) -> CanonicalTreeEntryV1<'a> {
    CanonicalTreeEntryV1::new(
        ValidatedComponent::new(name).expect("canonical component"),
        file.child(mode),
    )
}

fn directory_entry<'a>(
    name: &'a [u8],
    directory: layerfs_storage::cow::CanonicalDirectoryTreeV1,
) -> CanonicalTreeEntryV1<'a> {
    let DirectoryLogicalIdentityV1::Explicit(logical) = directory.logical() else {
        panic!("nested directory must have explicit logical identity");
    };
    CanonicalTreeEntryV1::new(
        ValidatedComponent::new(name).expect("canonical directory component"),
        CanonicalTreeChildV1::Directory {
            logical,
            physical: directory.physical(),
        },
    )
}

fn accept_files(
    cas: &FsCasV1,
    key: u64,
    files: &[(&[u8], u16, &[u8])],
) -> (PhysicalVersionRecordIdV1, PhysicalTreeIdV1) {
    let mut manifest: Vec<_> = files
        .iter()
        .map(|(path, mode, bytes)| {
            C3TreeFileV1::new(path, *mode, bytes.len() as u64, SliceSupplier { bytes })
        })
        .collect();
    let mut scratch = OperationScratch::new();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let operation =
        request_c3_tree_operation_v1(cas, key, &mut counters, &mut control).expect("root grant");
    let handoff = run_c3_create_tree_v1(
        operation,
        C3CdcAlgorithmV1::FastCdc,
        &mut manifest,
        scratch.borrow(),
        &mut control,
        &mut counters,
    )
    .expect("accepted base root");
    (handoff.version_record(), handoff.root_tree())
}

fn assert_clean_terminal(cas: &FsCasV1, root: &Path) {
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(
        fs::read_dir(root.join("preparation"))
            .expect("preparation directory")
            .count(),
        0
    );
}

fn assert_storage_terminal(counters: &OperationCountersV1) {
    assert!(counters.storage_bytes_requested > 0);
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained
    );
    assert!(
        counters.root_storage_active_reserved_bytes_lifetime_high_water
            >= counters.storage_bytes_reserved
    );
    assert!(
        counters.root_storage_active_reserved_inodes_lifetime_high_water
            >= counters.storage_inodes_reserved
    );
    assert!(counters.storage_bytes_committed > 0);
    assert!(counters.storage_inodes_committed > 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
}

#[test]
fn complete_replace_and_metadata_reach_independently_derived_handoffs() {
    let fixture = TestRoot::new("replace-metadata");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let base_data: Vec<u8> = (0..48_123).map(|index| (index * 37) as u8).collect();
    let replacement_data: Vec<u8> = (0..57_321).map(|index| (index * 19 + 7) as u8).collect();
    let name = b"b.bin";
    let base_file = expected_file(&base_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x510, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());

    let replacement_file = expected_file(&replacement_data, 0o600);
    let replacement_entries = [entry(name, &replacement_file, 0o600)];
    let replacement_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &replacement_entries).expect("replacement tree");
    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );
    let mut source = SliceSource::new(&replacement_data);
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let replaced = run_c3_complete_replace_v1(
        &cas,
        0x511,
        C3CdcAlgorithmV1::FastCdc,
        base_version,
        base_tree.directory,
        replacement_proof.evidence(base_tree.directory),
        0,
        name,
        0o600,
        replacement_data.len() as u64,
        &mut source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete Replace handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(replaced.root_tree(), replacement_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());

    let metadata_file = expected_file(&replacement_data, 0o640);
    assert_ne!(metadata_file.physical, replacement_file.physical);
    let metadata_entries = [entry(name, &metadata_file, 0o640)];
    let metadata_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &metadata_entries).expect("metadata tree");
    let metadata_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &replacement_entries,
        &replacement_tree,
        0,
    );
    let mut evidence = replacement_file.evidence();
    let mut scratch = OperationScratch::new();
    let mut counters = OperationCountersV1::default();
    let metadata = run_c3_complete_metadata_v1(
        &cas,
        0x512,
        replaced.version_record(),
        replacement_tree.directory,
        metadata_proof.evidence(replacement_tree.directory),
        0,
        name,
        0o640,
        replacement_file.authenticated(0o600),
        &mut evidence,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .expect("complete metadata handoff");
    assert_eq!(metadata.root_tree(), metadata_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_add_move_and_remove_use_one_candidate_graph_each() {
    let fixture = TestRoot::new("add-move-remove");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let b_data: Vec<u8> = (0..24_321).map(|index| (index * 11) as u8).collect();
    let d_data: Vec<u8> = (0..31_777).map(|index| (index * 29 + 3) as u8).collect();
    let b_file = expected_file(&b_data, 0o644);
    let d_file = expected_file(&d_data, 0o600);
    let b_name = b"b.bin";
    let d_name = b"d.bin";
    let a_name = b"a.bin";

    let base_entries = [entry(b_name, &b_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x520, &[(b_name, 0o644, &b_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());

    let added_entries = [entry(b_name, &b_file, 0o644), entry(d_name, &d_file, 0o600)];
    let added_tree = build(DirectoryBuildModeV1::ImplicitRoot, &added_entries).expect("added tree");
    let add_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        1,
    );
    let mut tree_source = MutationSource::new(&base_entries, &added_entries);
    let mut source = SliceSource::new(&d_data);
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let added = run_c3_complete_add_v1(
        &cas,
        0x521,
        base_version,
        base_tree.directory,
        add_fixture.evidence(base_tree.directory),
        1,
        d_name,
        0o600,
        d_data.len() as u64,
        &mut source,
        &mut tree_source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete Add handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(added.root_tree(), added_tree.directory.physical());
    assert_storage_terminal(&counters);

    let moved_entries = [entry(a_name, &d_file, 0o600), entry(b_name, &b_file, 0o644)];
    let moved_tree = build(DirectoryBuildModeV1::ImplicitRoot, &moved_entries).expect("moved tree");
    let move_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &added_entries,
        &added_tree,
        0,
    );
    let mut tree_source = MutationSource::new(&added_entries, &moved_entries);
    let mut scratch = OperationScratch::new();
    let mut counters = OperationCountersV1::default();
    let moved = run_c3_complete_move_v1(
        &cas,
        0x522,
        added.version_record(),
        added_tree.directory,
        move_fixture.evidence(added_tree.directory),
        1,
        0,
        added_entries[1],
        a_name,
        &mut tree_source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete Move handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(moved.root_tree(), moved_tree.directory.physical());
    assert_storage_terminal(&counters);

    let removed_entries = [entry(b_name, &b_file, 0o644)];
    let removed_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &removed_entries).expect("removed tree");
    let remove_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &moved_entries,
        &moved_tree,
        0,
    );
    let mut tree_source = MutationSource::new(&moved_entries, &removed_entries);
    let mut scratch = OperationScratch::new();
    let mut counters = OperationCountersV1::default();
    let removed = run_c3_complete_remove_v1(
        &cas,
        0x523,
        moved.version_record(),
        moved_tree.directory,
        remove_fixture.evidence(moved_tree.directory),
        0,
        moved_entries[0],
        &mut tree_source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .expect("complete Remove handoff");
    assert_eq!(removed.root_tree(), removed_tree.directory.physical());
    assert_eq!(removed.root_tree(), base_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_cross_directory_move_detaches_and_attaches_in_one_handoff() {
    let fixture = TestRoot::new("cross-directory-move");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let moved_data: Vec<u8> = (0..28_417).map(|index| (index * 17 + 5) as u8).collect();
    let resident_data: Vec<u8> = (0..19_007).map(|index| (index * 31 + 9) as u8).collect();
    let moved_file = expected_file(&moved_data, 0o644);
    let resident_file = expected_file(&resident_data, 0o600);

    let source_base_entries = [entry(b"old.bin", &moved_file, 0o644)];
    let source_result_entries: [CanonicalTreeEntryV1<'_>; 0] = [];
    let destination_base_entries = [entry(b"z.bin", &resident_file, 0o600)];
    let destination_result_entries = [
        entry(b"moved.bin", &moved_file, 0o644),
        entry(b"z.bin", &resident_file, 0o600),
    ];
    let source_base = build(DirectoryBuildModeV1::Explicit(0o755), &source_base_entries)
        .expect("source base directory");
    let source_result = build(
        DirectoryBuildModeV1::Explicit(0o755),
        &source_result_entries,
    )
    .expect("source result directory");
    let destination_base = build(
        DirectoryBuildModeV1::Explicit(0o755),
        &destination_base_entries,
    )
    .expect("destination base directory");
    let destination_result = build(
        DirectoryBuildModeV1::Explicit(0o755),
        &destination_result_entries,
    )
    .expect("destination result directory");

    let root_base_entries = [
        directory_entry(b"left", source_base.directory),
        directory_entry(b"right", destination_base.directory),
    ];
    let root_result_entries = [
        directory_entry(b"left", source_result.directory),
        directory_entry(b"right", destination_result.directory),
    ];
    let root_base =
        build(DirectoryBuildModeV1::ImplicitRoot, &root_base_entries).expect("base root directory");
    let root_result = build(DirectoryBuildModeV1::ImplicitRoot, &root_result_entries)
        .expect("result root directory");
    let (base_version, accepted_root) = accept_files(
        &cas,
        0x524,
        &[
            (b"left/old.bin", 0o644, moved_data.as_slice()),
            (b"right/z.bin", 0o600, resident_data.as_slice()),
        ],
    );
    assert_eq!(accepted_root, root_base.directory.physical());

    let source_fixture = mutation_fixture(
        DirectoryBuildModeV1::Explicit(0o755),
        &source_base_entries,
        &source_base,
        0,
    );
    let destination_fixture = mutation_fixture(
        DirectoryBuildModeV1::Explicit(0o755),
        &destination_base_entries,
        &destination_base,
        0,
    );
    let root_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &root_base_entries,
        &root_base,
        0,
    );
    let mut source_view = MutationSource::new(&source_base_entries, &source_result_entries);
    let mut destination_view =
        MutationSource::new(&destination_base_entries, &destination_result_entries);
    let mut root_view = MutationSource::new(&root_base_entries, &root_result_entries);
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();

    let moved = complete_cross_directory_move_operation_v1(
        &cas,
        0x525,
        base_version,
        root_base.directory,
        root_fixture.evidence(root_base.directory),
        0,
        root_base_entries[0],
        source_base.directory,
        source_fixture.evidence(source_base.directory),
        0,
        source_base_entries[0],
        &mut source_view,
        1,
        root_base_entries[1],
        destination_base.directory,
        destination_fixture.evidence(destination_base.directory),
        0,
        b"moved.bin",
        &mut destination_view,
        &mut root_view,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete cross-directory Move handoff: {error:?}; boundaries={:?}; counters={counters:?}",
            control.boundaries,
        )
    });

    assert_eq!(moved.root_tree(), root_result.directory.physical());
    assert_eq!(
        control
            .boundaries
            .iter()
            .filter(|boundary| **boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff)
            .count(),
        1
    );
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_update_authenticates_and_rejoins_without_replace_fallback() {
    let fixture = TestRoot::new("update");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    // This deterministic counter-PRNG fixture and range are independently
    // proven by the bounded Update suite to establish an exact FastCDC
    // suffix rejoin. A non-rejoining edit must fail closed rather than being
    // silently widened into Replace.
    let base_data = crate::test_support::fastcdc_golden_input(300_000);
    let inserted = b"changed";
    let range = UpdateRangeV1::new(120_000, 120_010, base_data.len() as u64).expect("range");
    let mut result_data = Vec::new();
    result_data.extend_from_slice(&base_data[..range.start() as usize]);
    result_data.extend_from_slice(inserted);
    result_data.extend_from_slice(&base_data[range.end() as usize..]);

    let name = b"b.bin";
    let base_file = expected_file(&base_data, 0o644);
    let result_file = expected_file(&result_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let result_entries = [entry(name, &result_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let result_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &result_entries).expect("result tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x530, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());
    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );

    let mut inserted_source = SliceSource::new(inserted);
    let mut base_reader = BaseBytes { bytes: &base_data };
    let mut evidence = base_file.evidence();
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let updated = run_c3_complete_update_v1(
        &cas,
        0x531,
        base_version,
        base_tree.directory,
        replacement_proof.evidence(base_tree.directory),
        0,
        name,
        0o644,
        base_file.authenticated(0o644),
        range,
        inserted.len() as u64,
        &mut inserted_source,
        &mut base_reader,
        &mut evidence,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete Update handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(updated.algorithm(), C3CdcAlgorithmV1::FastCdc);
    assert_eq!(updated.root_tree(), result_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_mutation_rejects_an_unauthenticated_base_without_preparation() {
    let fixture = TestRoot::new("bad-base");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let data = b"authenticated base";
    let file = expected_file(data, 0o644);
    let entries = [entry(b"b.bin", &file, 0o644)];
    let tree = build(DirectoryBuildModeV1::ImplicitRoot, &entries).expect("base tree");
    let (version, accepted_root) = accept_files(&cas, 0x540, &[(b"b.bin", 0o644, data)]);
    assert_eq!(accepted_root, tree.directory.physical());
    let proof = replacement_fixture(DirectoryBuildModeV1::ImplicitRoot, &entries, &tree, 0);
    let mut source = SliceSource::new(b"replacement");
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let wrong_version = PhysicalVersionRecordIdV1::from_digest([0x5a; 32]);
    let error = run_c3_complete_replace_v1(
        &cas,
        0x541,
        C3CdcAlgorithmV1::FastCdc,
        wrong_version,
        tree.directory,
        proof.evidence(tree.directory),
        0,
        b"b.bin",
        0o644,
        11,
        &mut source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .expect_err("unaccepted version must fail closed");
    assert!(matches!(
        error,
        layerfs_storage::lifecycle::C3OperationErrorV1::FsCas(_)
            | layerfs_storage::lifecycle::C3OperationErrorV1::Core(CoreError::IdMismatch)
    ));
    assert_eq!(source.offset, 0);
    assert_clean_terminal(&cas, fixture.path());
    assert_ne!(version, wrong_version);
}
