use blake3::hazmat::HasherExt;
use layerfs_storage::format::{
    ValidatedComponent, ValidatedSymlinkTarget, ROOT_DIRECTORY_MODE_SENTINEL_V1,
};
use layerfs_storage::identity::{
    derive_file_node_v1, derive_logical_chunk_v1, derive_logical_file_v1,
    derive_physical_chunk_id_v1, derive_physical_file_id_v1, derive_physical_symlink_id_v1,
    derive_symlink_node_v1, LogicalChunkRefV1,
};
use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1, OPERATION_SLOT_BYTES};
use layerfs_storage::object::{
    decode_physical_object_v1, DiscardStrongEdgesV1, PhysicalObjectPayloadV1, TreeRecordV1,
};
use layerfs_storage::profile::ProfileSpecV1;
use layerfs_storage::tree::{
    add_directory_entry_cow_v1, build_canonical_directory_v1, preflight_canonical_tree_v1,
    remove_directory_entry_cow_v1, replace_directory_entry_cow_v1,
    AuthenticatedTreeMutationEvidenceV1, AuthenticatedTreeReplacementEvidenceV1,
    CanonicalDirectoryTreeV1, CanonicalTreeChildV1, CanonicalTreeEntryV1,
    CanonicalTreeMutationSourceV1, DirectoryBuildModeV1, DirectoryHashProofV1,
    DirectoryHashSubtreeV1, DirectoryLogicalIdentityV1, DirectoryMutationHashProofV1,
    PreparedTreeSinkV1, TreeMutationSourceErrorV1, TreeObjectDispositionV1, TreePageBoundaryV1,
    TreePageSummaryV1, TreeSinkErrorV1, MAX_COW_TREE_PAGE_SUMMARIES,
    MAX_DIRECTORY_HASH_PROOF_NODES, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES,
};
use layerfs_storage::{CoreError, CoreResult};

fn object(kind: u8, payload: &[u8]) -> Vec<u8> {
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

fn symlink_child(target: &[u8]) -> CanonicalTreeChildV1 {
    let target = ValidatedSymlinkTarget::new(target).unwrap();
    let logical = derive_symlink_node_v1(target).unwrap();
    let mut payload = Vec::new();
    payload.extend_from_slice(&(target.as_bytes().len() as u32).to_be_bytes());
    payload.extend_from_slice(target.as_bytes());
    let physical = derive_physical_symlink_id_v1(&object(4, &payload)).unwrap();
    CanonicalTreeChildV1::Symlink { logical, physical }
}

fn fixed_names(count: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|index| format!("n{index:07}").into_bytes())
        .collect()
}

fn entries_for<'a>(
    names: &'a [Vec<u8>],
    child: CanonicalTreeChildV1,
) -> Vec<CanonicalTreeEntryV1<'a>> {
    names
        .iter()
        .map(|name| CanonicalTreeEntryV1::new(ValidatedComponent::new(name).unwrap(), child))
        .collect()
}

#[derive(Default)]
struct VecTreeSink {
    committed: Vec<(layerfs_storage::identity::PhysicalTreeIdV1, Vec<u8>)>,
    pending: Vec<(layerfs_storage::identity::PhysicalTreeIdV1, Vec<u8>)>,
    expected: usize,
    root: Option<layerfs_storage::identity::PhysicalTreeIdV1>,
    begins: usize,
    finishes: usize,
    aborts: usize,
    resident_bytes: u64,
}

impl VecTreeSink {
    fn bytes(&self, id: layerfs_storage::identity::PhysicalTreeIdV1) -> Option<&[u8]> {
        self.committed
            .iter()
            .find_map(|(candidate, bytes)| (*candidate == id).then_some(bytes.as_slice()))
    }
}

impl PreparedTreeSinkV1 for VecTreeSink {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(self.resident_bytes)
    }

    fn begin_private_tree_set(&mut self, maximum_objects: u32) -> Result<(), TreeSinkErrorV1> {
        if !self.pending.is_empty() {
            return Err(TreeSinkErrorV1::Failure);
        }
        self.expected = maximum_objects as usize;
        self.begins += 1;
        Ok(())
    }

    fn admit_private_tree(
        &mut self,
        id: layerfs_storage::identity::PhysicalTreeIdV1,
        canonical_bytes: &[u8],
    ) -> Result<TreeObjectDispositionV1, TreeSinkErrorV1> {
        if let Some((_, occupied)) = self
            .committed
            .iter()
            .chain(&self.pending)
            .find(|(candidate, _)| *candidate == id)
        {
            return if occupied == canonical_bytes {
                Ok(TreeObjectDispositionV1::Reused)
            } else {
                Err(TreeSinkErrorV1::Failure)
            };
        }
        if self.pending.len() >= self.expected {
            return Err(TreeSinkErrorV1::Failure);
        }
        self.pending.push((id, canonical_bytes.to_vec()));
        Ok(TreeObjectDispositionV1::Created)
    }

    fn finish_private_tree_set(
        &mut self,
        root: layerfs_storage::identity::PhysicalTreeIdV1,
    ) -> Result<(), TreeSinkErrorV1> {
        if self.pending.len() > self.expected {
            return Err(TreeSinkErrorV1::Failure);
        }
        self.root = Some(root);
        self.committed.append(&mut self.pending);
        self.finishes += 1;
        Ok(())
    }

    fn abort_private_tree_set(&mut self) {
        self.pending.clear();
        self.aborts += 1;
    }
}

struct MutationSource<'a> {
    base: &'a [CanonicalTreeEntryV1<'a>],
    result: &'a [CanonicalTreeEntryV1<'a>],
    resident_bytes: u64,
    base_reads: usize,
    result_reads: usize,
}

impl<'a> MutationSource<'a> {
    fn new(base: &'a [CanonicalTreeEntryV1<'a>], result: &'a [CanonicalTreeEntryV1<'a>]) -> Self {
        Self {
            base,
            result,
            resident_bytes: 0,
            base_reads: 0,
            result_reads: 0,
        }
    }
}

impl CanonicalTreeMutationSourceV1 for MutationSource<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(self.resident_bytes)
    }

    fn declared_base_entry_count(&self) -> CoreResult<u32> {
        u32::try_from(self.base.len()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn declared_result_entry_count(&self) -> CoreResult<u32> {
        u32::try_from(self.result.len()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn read_base_entry(
        &mut self,
        ordinal: u32,
    ) -> Result<CanonicalTreeEntryV1<'_>, TreeMutationSourceErrorV1> {
        self.base_reads += 1;
        self.base
            .get(ordinal as usize)
            .copied()
            .ok_or(TreeMutationSourceErrorV1::Failure)
    }

    fn read_result_entry(
        &mut self,
        ordinal: u32,
    ) -> Result<CanonicalTreeEntryV1<'_>, TreeMutationSourceErrorV1> {
        self.result_reads += 1;
        self.result
            .get(ordinal as usize)
            .copied()
            .ok_or(TreeMutationSourceErrorV1::Failure)
    }
}

struct BuiltTree {
    directory: CanonicalDirectoryTreeV1,
    leaves: Vec<TreePageSummaryV1>,
    level_one: Vec<TreePageSummaryV1>,
    sink: VecTreeSink,
    counters: OperationCountersV1,
}

struct ReplacementEvidenceFixture<'a> {
    affected_leaf_index: u32,
    affected_entries: &'a [CanonicalTreeEntryV1<'a>],
    affected_leaf: TreePageSummaryV1,
    leaf_group: Vec<TreePageBoundaryV1<'a>>,
    level_one_group: Vec<TreePageBoundaryV1<'a>>,
    preimage_len: u64,
    window_offset: u64,
    old_window: Vec<u8>,
    prefix: Vec<DirectoryHashSubtreeV1>,
    suffix: Vec<DirectoryHashSubtreeV1>,
    affected_leaf_offset: u64,
}

#[derive(Clone)]
struct MutationEvidenceFixture<'a> {
    affected_leaf_index: Option<u32>,
    affected_leaf: Option<TreePageSummaryV1>,
    leaf_group: Vec<TreePageBoundaryV1<'a>>,
    level_one_group: Vec<TreePageBoundaryV1<'a>>,
    old_preimage_len: u64,
    stream_start_index: u32,
    stream_entry_offset: u64,
    old_head: Vec<u8>,
    middle: Vec<DirectoryHashSubtreeV1>,
    old_tail_prefix: Vec<u8>,
}

impl MutationEvidenceFixture<'_> {
    fn evidence<'a>(
        &'a self,
        base: CanonicalDirectoryTreeV1,
    ) -> AuthenticatedTreeMutationEvidenceV1<'a> {
        AuthenticatedTreeMutationEvidenceV1::new(
            base,
            self.affected_leaf_index,
            self.affected_leaf,
            &self.leaf_group,
            &self.level_one_group,
            DirectoryMutationHashProofV1::new(
                self.old_preimage_len,
                self.stream_start_index,
                self.stream_entry_offset,
                &self.old_head,
                &self.middle,
                &self.old_tail_prefix,
            ),
        )
    }
}

impl<'a> ReplacementEvidenceFixture<'a> {
    fn evidence<'b>(
        &'b self,
        base: CanonicalDirectoryTreeV1,
    ) -> AuthenticatedTreeReplacementEvidenceV1<'b>
    where
        'a: 'b,
    {
        AuthenticatedTreeReplacementEvidenceV1::new(
            base,
            self.affected_leaf_index,
            self.affected_entries,
            self.affected_leaf,
            &self.leaf_group,
            &self.level_one_group,
            DirectoryHashProofV1::new(
                self.preimage_len,
                self.window_offset,
                &self.old_window,
                &self.prefix,
                &self.suffix,
                self.affected_leaf_offset,
            ),
        )
    }

    fn proof_node_count(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }
}

fn logical_directory_preimage(
    mode: DirectoryBuildModeV1,
    entries: &[CanonicalTreeEntryV1<'_>],
) -> (Vec<u8>, Vec<usize>) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"ESV2-DNODE\0");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(
        &match mode {
            DirectoryBuildModeV1::ImplicitRoot => ROOT_DIRECTORY_MODE_SENTINEL_V1,
            DirectoryBuildModeV1::Explicit(mode) => mode,
        }
        .to_le_bytes(),
    );
    bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    let mut offsets = Vec::with_capacity(entries.len() + 1);
    for entry in entries {
        offsets.push(bytes.len());
        let name = entry.name().as_bytes();
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name);
        let (kind, id) = match entry.child() {
            CanonicalTreeChildV1::File { logical, .. } => (0x01, *logical.as_bytes()),
            CanonicalTreeChildV1::Directory { logical, .. } => (0x02, *logical.id().as_bytes()),
            CanonicalTreeChildV1::Symlink { logical, .. } => (0x03, *logical.as_bytes()),
        };
        bytes.push(kind);
        bytes.extend_from_slice(&id);
    }
    offsets.push(bytes.len());
    (bytes, offsets)
}

fn directory_hash_subtrees(
    preimage: &[u8],
    mut offset: usize,
    end: usize,
) -> Vec<DirectoryHashSubtreeV1> {
    assert!(offset <= end && end <= preimage.len());
    assert!(offset == end || offset % blake3::CHUNK_LEN == 0);
    assert!(end == preimage.len() || end % blake3::CHUNK_LEN == 0);
    let mut result = Vec::new();
    while offset < end {
        let chunk_index = offset / blake3::CHUNK_LEN;
        let remaining_chunks = (end - offset).div_ceil(blake3::CHUNK_LEN);
        let mut subtree_chunks = 1_usize;
        while subtree_chunks <= remaining_chunks / 2
            && (chunk_index == 0 || chunk_index % (subtree_chunks * 2) == 0)
        {
            subtree_chunks *= 2;
        }
        let subtree_end = offset
            .saturating_add(subtree_chunks * blake3::CHUNK_LEN)
            .min(end);
        let mut hasher = blake3::Hasher::new();
        hasher.set_input_offset(offset as u64);
        hasher.update(&preimage[offset..subtree_end]);
        result.push(DirectoryHashSubtreeV1::new(
            offset as u64,
            (subtree_end - offset) as u64,
            hasher.finalize_non_root(),
        ));
        offset = subtree_end;
    }
    result
}

fn page_boundary<'a>(
    summary: TreePageSummaryV1,
    entries: &'a [CanonicalTreeEntryV1<'a>],
) -> TreePageBoundaryV1<'a> {
    TreePageBoundaryV1::new(
        summary,
        entries[summary.first_entry() as usize].name(),
        entries[summary.last_entry() as usize].name(),
    )
}

fn replacement_fixture<'a>(
    mode: DirectoryBuildModeV1,
    entries: &'a [CanonicalTreeEntryV1<'a>],
    built: &BuiltTree,
    replacement_index: usize,
) -> ReplacementEvidenceFixture<'a> {
    let affected_leaf_index = replacement_index / 192;
    let affected_leaf = built.leaves[affected_leaf_index];
    let affected_first = affected_leaf.first_entry() as usize;
    let affected_end = affected_leaf.last_entry() as usize + 1;
    let leaf_group = if built.directory.page_depth() == 0 {
        Vec::new()
    } else {
        let leaf_group_first = affected_leaf_index / 96 * 96;
        let leaf_group_end = (leaf_group_first + 96).min(built.leaves.len());
        built.leaves[leaf_group_first..leaf_group_end]
            .iter()
            .copied()
            .map(|summary| page_boundary(summary, entries))
            .collect()
    };
    let level_one_group = if built.directory.page_depth() == 2 {
        built
            .level_one
            .iter()
            .copied()
            .map(|summary| page_boundary(summary, entries))
            .collect()
    } else {
        Vec::new()
    };

    let (preimage, entry_offsets) = logical_directory_preimage(mode, entries);
    let affected_leaf_offset = entry_offsets[affected_first];
    let affected_leaf_end = entry_offsets[affected_end];
    let window_offset = affected_leaf_offset / blake3::CHUNK_LEN * blake3::CHUNK_LEN;
    let window_end = affected_leaf_end
        .div_ceil(blake3::CHUNK_LEN)
        .saturating_mul(blake3::CHUNK_LEN)
        .min(preimage.len());
    let prefix = directory_hash_subtrees(&preimage, 0, window_offset);
    let suffix = directory_hash_subtrees(&preimage, window_end, preimage.len());
    assert!(prefix.len() + suffix.len() <= MAX_DIRECTORY_HASH_PROOF_NODES);

    ReplacementEvidenceFixture {
        affected_leaf_index: affected_leaf_index as u32,
        affected_entries: &entries[affected_first..affected_end],
        affected_leaf,
        leaf_group,
        level_one_group,
        preimage_len: preimage.len() as u64,
        window_offset: window_offset as u64,
        old_window: preimage[window_offset..window_end].to_vec(),
        prefix,
        suffix,
        affected_leaf_offset: affected_leaf_offset as u64,
    }
}

fn mutation_fixture<'a>(
    mode: DirectoryBuildModeV1,
    entries: &'a [CanonicalTreeEntryV1<'a>],
    built: &BuiltTree,
    mutation_index: usize,
) -> MutationEvidenceFixture<'a> {
    let (preimage, entry_offsets) = logical_directory_preimage(mode, entries);
    let stream_start = mutation_index.saturating_sub(1);
    let stream_entry_offset = entry_offsets[stream_start];
    let tail_offset = stream_entry_offset / blake3::CHUNK_LEN * blake3::CHUNK_LEN;
    let (old_head, middle) = if tail_offset == 0 {
        (Vec::new(), Vec::new())
    } else {
        (
            preimage[..blake3::CHUNK_LEN].to_vec(),
            directory_hash_subtrees(&preimage, blake3::CHUNK_LEN, tail_offset),
        )
    };
    let old_tail_prefix = preimage[tail_offset..stream_entry_offset].to_vec();

    let (affected_leaf_index, affected_leaf, leaf_group, level_one_group) = if entries.is_empty() {
        (None, None, Vec::new(), Vec::new())
    } else {
        let affected_leaf_index = mutation_index.min(entries.len() - 1) / 192;
        let affected_leaf = built.leaves[affected_leaf_index];
        let leaf_group = if built.directory.page_depth() == 0 {
            Vec::new()
        } else {
            let leaf_group_first = affected_leaf_index / 96 * 96;
            let leaf_group_end = (leaf_group_first + 96).min(built.leaves.len());
            built.leaves[leaf_group_first..leaf_group_end]
                .iter()
                .copied()
                .map(|summary| page_boundary(summary, entries))
                .collect()
        };
        let level_one_group = if built.directory.page_depth() == 2 {
            built
                .level_one
                .iter()
                .copied()
                .map(|summary| page_boundary(summary, entries))
                .collect()
        } else {
            Vec::new()
        };
        (
            Some(affected_leaf_index as u32),
            Some(affected_leaf),
            leaf_group,
            level_one_group,
        )
    };

    MutationEvidenceFixture {
        affected_leaf_index,
        affected_leaf,
        leaf_group,
        level_one_group,
        old_preimage_len: preimage.len() as u64,
        stream_start_index: stream_start as u32,
        stream_entry_offset: stream_entry_offset as u64,
        old_head,
        middle,
        old_tail_prefix,
    }
}

fn build(
    mode: DirectoryBuildModeV1,
    entries: &[CanonicalTreeEntryV1<'_>],
) -> CoreResult<BuiltTree> {
    let mut sink = VecTreeSink::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut object_scratch = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut page_scratch = vec![None; MAX_TREE_PAGE_SUMMARIES];
    let directory = build_canonical_directory_v1(
        mode,
        entries,
        &mut sink,
        &ledger,
        &mut counters,
        &mut object_scratch,
        &mut page_scratch,
    )?;
    assert_eq!(ledger.admitted_slots(), 0);
    let leaf_count = directory.leaf_count() as usize;
    let level_one_count = directory.level_one_count() as usize;
    let leaves = page_scratch[..leaf_count]
        .iter()
        .copied()
        .map(Option::unwrap)
        .collect();
    let level_one = page_scratch[leaf_count..leaf_count + level_one_count]
        .iter()
        .copied()
        .map(Option::unwrap)
        .collect();
    Ok(BuiltTree {
        directory,
        leaves,
        level_one,
        sink,
        counters,
    })
}

#[test]
fn exact_shape_preflight_covers_empty_fanout_splits_depth_and_one_over_limit() {
    let cases = [
        (0, 0, 0, 0, 0, 1),
        (1, 0, 1, 0, 1, 2),
        (192, 0, 1, 0, 1, 2),
        (193, 1, 2, 1, 3, 4),
        (18_432, 1, 96, 1, 97, 98),
        (18_433, 2, 97, 2, 100, 101),
        (1_000_000, 2, 5_209, 55, 5_265, 5_266),
    ];
    for (entries, depth, leaves, level_one, summaries, objects) in cases {
        let shape = preflight_canonical_tree_v1(entries).unwrap();
        assert_eq!(shape.entry_count(), entries as u32);
        assert_eq!(shape.page_depth(), depth);
        assert_eq!(shape.leaf_count(), leaves);
        assert_eq!(shape.level_one_count(), level_one);
        assert_eq!(shape.page_summary_count(), summaries);
        assert_eq!(shape.tree_object_count(), objects);
    }
    assert_eq!(
        preflight_canonical_tree_v1(1_000_001),
        Err(CoreError::CountCap)
    );
}

#[test]
fn empty_root_and_leaf_boundaries_emit_exact_canonical_tree_records() {
    let empty = build(DirectoryBuildModeV1::ImplicitRoot, &[]).unwrap();
    assert_eq!(empty.directory.entry_count(), 0);
    assert_eq!(empty.directory.page_depth(), 0);
    assert_eq!(empty.directory.tree_object_count(), 1);
    let bytes = empty.sink.bytes(empty.directory.physical()).unwrap();
    let decoded = decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1).unwrap();
    let PhysicalObjectPayloadV1::Tree(TreeRecordV1::Directory(directory)) = decoded.payload()
    else {
        panic!("expected directory wrapper");
    };
    assert_eq!(directory.mode, ROOT_DIRECTORY_MODE_SENTINEL_V1);
    assert_eq!(directory.entry_count, 0);
    assert_eq!(directory.root_page_id, None);

    let child = symlink_child(b"target");
    for (count, expected_leaf_counts) in [(1, vec![1]), (192, vec![192]), (193, vec![192, 1])] {
        let names = fixed_names(count);
        let entries = entries_for(&names, child);
        let built = build(DirectoryBuildModeV1::Explicit(0o755), &entries).unwrap();
        let mut leaf_counts = built
            .sink
            .committed
            .iter()
            .filter_map(|(_, bytes)| {
                let decoded = decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1).unwrap();
                match decoded.payload() {
                    PhysicalObjectPayloadV1::Tree(TreeRecordV1::Leaf(leaf)) => Some(leaf.count),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        leaf_counts.sort_unstable_by(|left, right| right.cmp(left));
        assert_eq!(leaf_counts, expected_leaf_counts);
    }
}

#[test]
fn canonical_order_is_permutation_invariant_only_after_sorting_and_rejects_duplicates() {
    let child = symlink_child(b"target");
    let names = fixed_names(3);
    let canonical = entries_for(&names, child);
    let expected = build(DirectoryBuildModeV1::Explicit(0o755), &canonical)
        .unwrap()
        .directory;

    let mut permuted = canonical.clone();
    permuted.swap(0, 2);
    let mut sink = VecTreeSink::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut object_scratch = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut page_scratch = vec![None; MAX_TREE_PAGE_SUMMARIES];
    assert_eq!(
        build_canonical_directory_v1(
            DirectoryBuildModeV1::Explicit(0o755),
            &permuted,
            &mut sink,
            &ledger,
            &mut counters,
            &mut object_scratch,
            &mut page_scratch,
        ),
        Err(CoreError::NonCanonicalOrder)
    );
    assert_eq!(sink.begins, 0);
    permuted.sort_by_key(|entry| entry.name().as_bytes());
    assert_eq!(
        build(DirectoryBuildModeV1::Explicit(0o755), &permuted)
            .unwrap()
            .directory,
        expected
    );

    let duplicate = [canonical[0], canonical[0]];
    assert!(matches!(
        build(DirectoryBuildModeV1::Explicit(0o755), &duplicate),
        Err(CoreError::NonCanonicalOrder)
    ));
}

#[test]
fn index_split_at_depth_two_is_canonical() {
    let names = fixed_names(18_433);
    let entries = entries_for(&names, symlink_child(b"target"));
    let built = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    assert_eq!(built.directory.page_depth(), 2);
    assert_eq!(built.directory.leaf_count(), 97);
    assert_eq!(built.directory.level_one_count(), 2);
    assert_eq!(built.directory.tree_object_count(), 101);
    assert_eq!(built.leaves[96].subtree_entry_count(), 1);
    assert_eq!(built.level_one[0].subtree_entry_count(), 18_432);
    assert_eq!(built.level_one[1].subtree_entry_count(), 1);
    assert_eq!(built.sink.committed.len(), 101);
    assert_eq!(built.counters.tree_nodes_created, 101);
    assert_eq!(built.counters.memory_high_water, 12_582_912);
}

#[test]
fn same_name_replacement_copies_only_the_affected_spine_and_matches_full_rebuild() {
    let names = fixed_names(300);
    let entries = entries_for(&names, symlink_child(b"old-target"));
    let base_build = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let base = base_build.directory;
    let replacement = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&names[200]).unwrap(),
        symlink_child(b"new-target"),
    );
    let mut sink = VecTreeSink::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut object_scratch = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut logical_scratch = [0_u8; 65_536];
    let evidence = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &entries,
        &base_build,
        200,
    );
    let cow = replace_directory_entry_cow_v1(
        base,
        evidence.evidence(base),
        200,
        replacement,
        &mut sink,
        &ledger,
        &mut counters,
        &mut object_scratch,
        &mut logical_scratch,
    )
    .unwrap();
    assert_eq!(cow.changed_leaf_index(), 1);
    assert_eq!(sink.committed.len(), 3);
    assert_eq!(counters.tree_nodes_created, 3);
    assert_eq!(counters.tree_nodes_reused, 1);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_ne!(cow.directory().physical(), base.physical());
    assert_ne!(cow.directory().logical(), base.logical());

    let mut mutated = entries.clone();
    mutated[200] = replacement;
    let rebuilt = build(DirectoryBuildModeV1::ImplicitRoot, &mutated)
        .unwrap()
        .directory;
    assert_eq!(cow.directory().logical(), rebuilt.logical());
    assert_eq!(cow.directory().physical(), rebuilt.physical());
    assert_ne!(cow.changed_leaf().id(), base_build.leaves[1].id());
}

#[test]
fn cow_rejects_same_shape_evidence_that_is_not_bound_to_the_base_root() {
    let names = fixed_names(300);
    let base_entries = entries_for(&names, symlink_child(b"base-target"));
    let other_entries = entries_for(&names, symlink_child(b"other-target"));
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).unwrap();
    let other = build(DirectoryBuildModeV1::ImplicitRoot, &other_entries).unwrap();
    let replacement = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&names[200]).unwrap(),
        symlink_child(b"replacement"),
    );
    let mut sink = VecTreeSink::default();
    let mut evidence = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base,
        200,
    );
    evidence.affected_leaf = other.leaves[1];

    assert_eq!(
        replace_directory_entry_cow_v1(
            base.directory,
            evidence.evidence(base.directory),
            200,
            replacement,
            &mut sink,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut OperationCountersV1::default(),
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; 65_536],
        ),
        Err(CoreError::IdMismatch)
    );
    assert_eq!(sink.begins, 0);
    assert!(sink.committed.is_empty());
}

#[test]
fn cow_rejects_tampered_logical_window_before_private_tree_output() {
    let names = fixed_names(300);
    let entries = entries_for(&names, symlink_child(b"base-target"));
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let replacement = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&names[200]).unwrap(),
        symlink_child(b"replacement"),
    );
    let mut evidence =
        replacement_fixture(DirectoryBuildModeV1::ImplicitRoot, &entries, &base, 200);
    evidence.old_window[0] ^= 0x01;
    let mut sink = VecTreeSink::default();

    assert_eq!(
        replace_directory_entry_cow_v1(
            base.directory,
            evidence.evidence(base.directory),
            200,
            replacement,
            &mut sink,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut OperationCountersV1::default(),
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; 65_536],
        ),
        Err(CoreError::IdMismatch)
    );
    assert_eq!(sink.begins, 0);
    assert!(sink.committed.is_empty());
}

#[test]
fn depth_two_replacement_reads_one_bounded_window_and_copies_only_its_spine() {
    let names = fixed_names(18_433);
    let entries = entries_for(&names, symlink_child(b"old-target"));
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let replacement_index = 50 * 192 + 10;
    let replacement = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&names[replacement_index]).unwrap(),
        symlink_child(b"new-target"),
    );
    let evidence = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &entries,
        &base,
        replacement_index,
    );
    assert!(evidence.old_window.len() <= 65_536);
    assert!(evidence.proof_node_count() <= MAX_DIRECTORY_HASH_PROOF_NODES);
    assert_eq!(evidence.affected_entries.len(), 192);
    assert_eq!(evidence.leaf_group.len(), 96);
    assert_eq!(evidence.level_one_group.len(), 2);

    let mut sink = VecTreeSink::default();
    let mut counters = OperationCountersV1::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let cow = replace_directory_entry_cow_v1(
        base.directory,
        evidence.evidence(base.directory),
        replacement_index,
        replacement,
        &mut sink,
        &ledger,
        &mut counters,
        &mut [0_u8; MAX_TREE_OBJECT_BYTES],
        &mut [0_u8; 65_536],
    )
    .unwrap();

    assert_eq!(cow.changed_leaf_index(), 50);
    assert_eq!(sink.committed.len(), 4);
    assert_eq!(counters.tree_nodes_created, 4);
    assert_eq!(counters.tree_nodes_reused, 96);
    assert_eq!(counters.bytes_read, evidence.old_window.len() as u64);
    assert!(counters.bytes_read <= 65_536);
    assert_eq!(ledger.admitted_slots(), 0);

    let mut updated = entries.clone();
    updated[replacement_index] = replacement;
    let rebuilt = build(DirectoryBuildModeV1::ImplicitRoot, &updated)
        .unwrap()
        .directory;
    assert_eq!(cow.directory(), rebuilt);
}

#[test]
fn add_and_remove_across_the_leaf_split_reuse_the_unchanged_leaf_and_exact_identity() {
    let names = fixed_names(192);
    let entries = entries_for(&names, symlink_child(b"stable-target"));
    let base_build = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let added_name = b"n9999999".to_vec();
    let added = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&added_name).unwrap(),
        symlink_child(b"added-target"),
    );
    let mut with_added = entries.clone();
    with_added.push(added);

    let mut add_sink = VecTreeSink::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut add_counters = OperationCountersV1::default();
    let mut object_scratch = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut logical_scratch = [0_u8; 65_536];
    let mut add_pages = vec![None; MAX_COW_TREE_PAGE_SUMMARIES];
    let add_evidence = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &entries,
        &base_build,
        entries.len(),
    );
    let mut add_source = MutationSource::new(&entries, &with_added);
    let added_cow = add_directory_entry_cow_v1(
        base_build.directory,
        add_evidence.evidence(base_build.directory),
        entries.len(),
        added,
        &mut add_source,
        &mut add_sink,
        &ledger,
        &mut add_counters,
        &mut object_scratch,
        &mut logical_scratch,
        &mut add_pages,
    )
    .unwrap();
    assert_eq!(added_cow.directory().entry_count(), 193);
    assert_eq!(added_cow.directory().page_depth(), 1);
    assert_eq!(added_cow.first_changed_leaf(), 1);
    assert_eq!(added_cow.structurally_reused_leaves(), 1);
    assert_eq!(added_cow.structurally_reused_level_one(), 0);
    assert_eq!(added_cow.emitted_objects(), 3);
    assert_eq!(add_sink.committed.len(), 3);
    assert_eq!(add_counters.tree_nodes_created, 3);
    assert_eq!(add_counters.tree_nodes_reused, 1);
    assert_eq!(add_pages[0].unwrap().id(), base_build.leaves[0].id());
    let fully_added = build(DirectoryBuildModeV1::ImplicitRoot, &with_added)
        .unwrap()
        .directory;
    assert_eq!(added_cow.directory(), fully_added);

    let added_build = build(DirectoryBuildModeV1::ImplicitRoot, &with_added).unwrap();
    let mut remove_sink = VecTreeSink::default();
    let mut remove_counters = OperationCountersV1::default();
    let mut remove_pages = vec![None; MAX_COW_TREE_PAGE_SUMMARIES];
    let remove_evidence = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &with_added,
        &added_build,
        with_added.len() - 1,
    );
    let mut remove_source = MutationSource::new(&with_added, &entries);
    let removed_cow = remove_directory_entry_cow_v1(
        added_cow.directory(),
        remove_evidence.evidence(added_cow.directory()),
        with_added.len() - 1,
        added,
        &mut remove_source,
        &mut remove_sink,
        &ledger,
        &mut remove_counters,
        &mut object_scratch,
        &mut logical_scratch,
        &mut remove_pages,
    )
    .unwrap();
    assert_eq!(removed_cow.directory(), base_build.directory);
    assert_eq!(removed_cow.directory().page_depth(), 0);
    assert_eq!(removed_cow.structurally_reused_leaves(), 1);
    assert_eq!(removed_cow.emitted_objects(), 1);
    assert_eq!(remove_sink.committed.len(), 1);
    assert_eq!(remove_counters.tree_nodes_created, 1);
    assert_eq!(remove_counters.tree_nodes_reused, 1);
    assert_eq!(remove_pages[0].unwrap().id(), base_build.leaves[0].id());
    assert_eq!(ledger.admitted_slots(), 0);
}

#[test]
fn cow_add_rejects_duplicate_or_mismatched_result_before_staging() {
    let names = fixed_names(2);
    let entries = entries_for(&names, symlink_child(b"target"));
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let duplicate = entries[1];
    let invalid_result = [entries[0], entries[1], duplicate];
    let evidence = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &entries,
        &base,
        entries.len(),
    );
    let mut source = MutationSource::new(&entries, &invalid_result);
    let mut sink = VecTreeSink::default();
    assert_eq!(
        add_directory_entry_cow_v1(
            base.directory,
            evidence.evidence(base.directory),
            2,
            duplicate,
            &mut source,
            &mut sink,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut OperationCountersV1::default(),
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; 65_536],
            &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
        ),
        Err(CoreError::NonCanonicalOrder)
    );
    assert_eq!(sink.begins, 0);
}

#[test]
fn depth_two_tail_add_and_remove_use_only_bounded_evidence_and_suffix_reads() {
    let names = fixed_names(18_433);
    let entries = entries_for(&names, symlink_child(b"stable-target"));
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let added_name = b"n9999999".to_vec();
    let added = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&added_name).unwrap(),
        symlink_child(b"added-target"),
    );
    let mut with_added = entries.clone();
    with_added.push(added);

    let add_evidence = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &entries,
        &base,
        entries.len(),
    );
    assert!(add_evidence.leaf_group.len() <= 96);
    assert!(add_evidence.level_one_group.len() <= 55);
    assert!(add_evidence.middle.len() <= MAX_DIRECTORY_HASH_PROOF_NODES);
    let mut add_source = MutationSource::new(&entries, &with_added);
    let mut add_sink = VecTreeSink::default();
    let mut add_counters = OperationCountersV1::default();
    let mut object_scratch = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut logical_scratch = [0_u8; 65_536];
    let mut page_scratch = [None; MAX_COW_TREE_PAGE_SUMMARIES];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let added_cow = add_directory_entry_cow_v1(
        base.directory,
        add_evidence.evidence(base.directory),
        entries.len(),
        added,
        &mut add_source,
        &mut add_sink,
        &ledger,
        &mut add_counters,
        &mut object_scratch,
        &mut logical_scratch,
        &mut page_scratch,
    )
    .unwrap();
    assert_eq!(added_cow.emitted_objects(), 4);
    assert_eq!(add_sink.committed.len(), 4);
    assert_eq!(added_cow.structurally_reused_leaves(), 96);
    assert_eq!(added_cow.structurally_reused_level_one(), 1);
    assert!(add_source.base_reads < 32);
    assert!(add_source.result_reads < 32);
    let added_full = build(DirectoryBuildModeV1::ImplicitRoot, &with_added).unwrap();
    assert_eq!(added_cow.directory(), added_full.directory);

    let remove_evidence = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &with_added,
        &added_full,
        with_added.len() - 1,
    );
    let mut remove_source = MutationSource::new(&with_added, &entries);
    let mut remove_sink = VecTreeSink::default();
    let mut remove_counters = OperationCountersV1::default();
    let removed_cow = remove_directory_entry_cow_v1(
        added_cow.directory(),
        remove_evidence.evidence(added_cow.directory()),
        with_added.len() - 1,
        added,
        &mut remove_source,
        &mut remove_sink,
        &ledger,
        &mut remove_counters,
        &mut object_scratch,
        &mut logical_scratch,
        &mut page_scratch,
    )
    .unwrap();
    assert_eq!(removed_cow.directory(), base.directory);
    assert_eq!(removed_cow.emitted_objects(), 4);
    assert_eq!(remove_sink.committed.len(), 4);
    assert!(remove_source.base_reads < 32);
    assert!(remove_source.result_reads < 32);
    assert_eq!(ledger.admitted_slots(), 0);
}

#[test]
fn depth_two_remove_rejects_corruption_across_every_sparse_proof_region_before_staging() {
    let names = fixed_names(18_433);
    let entries = entries_for(&names, symlink_child(b"stable-target"));
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let removal_index = 10_001;
    let removed = entries[removal_index];
    let mut result = entries.clone();
    result.remove(removal_index);
    let evidence = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &entries,
        &base,
        removal_index,
    );
    assert!(!evidence.old_head.is_empty());
    assert!(evidence.middle.len() > 1);
    assert!(!evidence.old_tail_prefix.is_empty());
    assert!(evidence.leaf_group.len() > 1);
    assert!(evidence.level_one_group.len() > 1);

    let mut corrupt_head = evidence.clone();
    corrupt_head.old_head[0] ^= 1;
    let mut corrupt_middle = evidence.clone();
    corrupt_middle.middle.swap(0, 1);
    let mut corrupt_tail = evidence.clone();
    corrupt_tail.old_tail_prefix[0] ^= 1;
    let mut corrupt_leaf_siblings = evidence.clone();
    corrupt_leaf_siblings.leaf_group.swap(0, 1);
    let mut corrupt_level_one_siblings = evidence;
    corrupt_level_one_siblings.level_one_group.swap(0, 1);

    for (region, corrupted) in [
        ("logical head", corrupt_head),
        ("logical middle", corrupt_middle),
        ("logical tail", corrupt_tail),
        ("leaf sibling boundary", corrupt_leaf_siblings),
        ("level-one sibling summary", corrupt_level_one_siblings),
    ] {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source = MutationSource::new(&entries, &result);
        let mut sink = VecTreeSink::default();
        let outcome = remove_directory_entry_cow_v1(
            base.directory,
            corrupted.evidence(base.directory),
            removal_index,
            removed,
            &mut source,
            &mut sink,
            &ledger,
            &mut OperationCountersV1::default(),
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; 65_536],
            &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
        );
        assert!(outcome.is_err(), "accepted corrupted {region}");
        assert_eq!(sink.begins, 0, "staged output for corrupted {region}");
        assert_eq!(
            ledger.admitted_slots(),
            0,
            "leaked reservation for {region}"
        );
    }
}

#[test]
fn cow_mutation_refuses_uncharged_source_or_sink_residency_before_reads_or_staging() {
    let names = fixed_names(2);
    let entries = entries_for(&names, symlink_child(b"target"));
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let added_name = b"n9999999".to_vec();
    let added = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&added_name).unwrap(),
        symlink_child(b"added"),
    );
    let mut result = entries.clone();
    result.push(added);
    let evidence = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &entries,
        &base,
        entries.len(),
    );
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);

    let mut oversized_source = MutationSource::new(&entries, &result);
    oversized_source.resident_bytes = OPERATION_SLOT_BYTES;
    let mut sink = VecTreeSink::default();
    assert_eq!(
        add_directory_entry_cow_v1(
            base.directory,
            evidence.evidence(base.directory),
            entries.len(),
            added,
            &mut oversized_source,
            &mut sink,
            &ledger,
            &mut OperationCountersV1::default(),
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; 65_536],
            &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
        ),
        Err(CoreError::ResourceRefused)
    );
    assert_eq!(
        oversized_source.base_reads + oversized_source.result_reads,
        0
    );
    assert_eq!(sink.begins, 0);

    let mut source = MutationSource::new(&entries, &result);
    let mut oversized_sink = VecTreeSink {
        resident_bytes: OPERATION_SLOT_BYTES,
        ..VecTreeSink::default()
    };
    assert_eq!(
        add_directory_entry_cow_v1(
            base.directory,
            evidence.evidence(base.directory),
            entries.len(),
            added,
            &mut source,
            &mut oversized_sink,
            &ledger,
            &mut OperationCountersV1::default(),
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; 65_536],
            &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
        ),
        Err(CoreError::ResourceRefused)
    );
    assert_eq!(source.base_reads + source.result_reads, 0);
    assert_eq!(oversized_sink.begins, 0);
    assert_eq!(ledger.admitted_slots(), 0);
}

fn file_child(data: &[u8], partition: &[&[u8]]) -> CanonicalTreeChildV1 {
    assert_eq!(partition.concat(), data);
    let logical_chunk = derive_logical_chunk_v1(data).unwrap();
    let logical_file = derive_logical_file_v1(
        data.len() as u64,
        &[LogicalChunkRefV1::from_identity(logical_chunk)],
    )
    .unwrap();
    let logical = derive_file_node_v1(0o644, logical_file).unwrap();
    let mut payload = Vec::new();
    payload.extend_from_slice(&0o644_u16.to_be_bytes());
    payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
    payload.extend_from_slice(&1_u32.to_be_bytes());
    payload.push(2);
    payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
    payload.extend_from_slice(&(partition.len() as u32).to_be_bytes());
    for bytes in partition {
        let chunk = object(5, bytes);
        let id = derive_physical_chunk_id_v1(&chunk).unwrap();
        payload.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        payload.extend_from_slice(id.as_bytes());
    }
    let physical = derive_physical_file_id_v1(&object(3, &payload)).unwrap();
    CanonicalTreeChildV1::File { logical, physical }
}

#[test]
fn logical_directory_identity_is_invariant_across_physical_rechunking() {
    let names = [b"file".to_vec()];
    let one_chunk = entries_for(&names, file_child(b"ab", &[b"ab"]));
    let two_chunks = entries_for(&names, file_child(b"ab", &[b"a", b"b"]));
    let first = build(DirectoryBuildModeV1::ImplicitRoot, &one_chunk)
        .unwrap()
        .directory;
    let second = build(DirectoryBuildModeV1::ImplicitRoot, &two_chunks)
        .unwrap()
        .directory;
    assert!(matches!(
        first.logical(),
        DirectoryLogicalIdentityV1::ImplicitRoot(_)
    ));
    assert_eq!(first.logical(), second.logical());
    assert_ne!(first.physical(), second.physical());
}

#[test]
fn file_update_reuses_unchanged_file_and_chunk_identity_outside_the_affected_spine() {
    let names = fixed_names(300);
    let mut entries = entries_for(&names, symlink_child(b"ordinary"));
    let stable_file = file_child(b"stable", &[b"stable"]);
    entries[0] =
        CanonicalTreeEntryV1::new(ValidatedComponent::new(&names[0]).unwrap(), stable_file);
    entries[200] = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&names[200]).unwrap(),
        file_child(b"old", &[b"old"]),
    );
    let base = build(DirectoryBuildModeV1::ImplicitRoot, &entries).unwrap();
    let replacement = CanonicalTreeEntryV1::new(
        ValidatedComponent::new(&names[200]).unwrap(),
        file_child(b"new", &[b"new"]),
    );

    let mut sink = VecTreeSink::default();
    let mut counters = OperationCountersV1::default();
    let evidence = replacement_fixture(DirectoryBuildModeV1::ImplicitRoot, &entries, &base, 200);
    let cow = replace_directory_entry_cow_v1(
        base.directory,
        evidence.evidence(base.directory),
        200,
        replacement,
        &mut sink,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut counters,
        &mut [0_u8; MAX_TREE_OBJECT_BYTES],
        &mut [0_u8; 65_536],
    )
    .unwrap();

    let mut updated = entries.clone();
    updated[200] = replacement;
    let rebuilt = build(DirectoryBuildModeV1::ImplicitRoot, &updated)
        .unwrap()
        .directory;
    assert_eq!(cow.directory(), rebuilt);
    assert_eq!(updated[0].child(), stable_file);
    assert_eq!(cow.changed_leaf_index(), 1);
    assert_eq!(counters.tree_nodes_created, 3);
    assert_eq!(counters.tree_nodes_reused, 1);
    assert_eq!(sink.committed.len(), 3);
    assert!(sink.committed.iter().all(|(_, bytes)| bytes[10] == 0x02));
}
