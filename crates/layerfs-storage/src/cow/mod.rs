//! Structural copy-on-write objects and canonical directory trees.

#![allow(unused_imports)]

pub(crate) mod file;
mod mutate;
mod tree;
mod view;

#[cfg(feature = "operation-polymorphism")]
pub(crate) use mutate::{
    add_directory_entry_cow_borrowed_v1, move_directory_entry_cow_borrowed_v1,
    remove_directory_entry_cow_borrowed_v1, replace_directory_entry_cow_borrowed_v1,
    replace_two_directory_entries_cow_borrowed_v1,
};
pub(crate) use mutate::{
    add_directory_entry_cow_v1, remove_directory_entry_cow_v1, replace_directory_entry_cow_v1,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use tree::build_canonical_directory_borrowed_v1;
pub(crate) use tree::build_canonical_directory_v1;
pub(crate) use tree::{
    preflight_canonical_tree_v1, CanonicalDirectoryTreeV1, CanonicalTreeChildV1,
    CanonicalTreeEntryV1, CanonicalTreeShapeV1, CowTreeMutationV1, CowTreeReplacementV1,
    DirectoryBuildModeV1, DirectoryLogicalIdentityV1, PreparedTreeSinkV1, TreeObjectDispositionV1,
    TreePageBoundaryV1, TreePageSummaryV1, TreeSinkErrorV1, MAX_COW_TREE_PAGE_SUMMARIES,
    MAX_DIRECTORY_HASH_PROOF_NODES, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use view::{
    mutation_evidence_resident_bytes_v1, mutation_hash_state_bytes_v1,
    replacement_evidence_resident_bytes_v1,
};
pub(crate) use view::{
    AuthenticatedTreeMutationEvidenceV1, AuthenticatedTreeReplacementEvidenceV1,
    CanonicalTreeMutationSourceV1, DirectoryHashProofV1, DirectoryHashSubtreeV1,
    DirectoryMutationHashProofV1, TreeMutationSourceErrorV1,
};

/// Bounded canonical-tree operations for integration owners.  The request
/// and result contain only primitive shape, ordering, and residue facts;
/// tree pages, sinks, proofs, and ledgers stay inside this module.
pub mod semantic {
    use super::tree::{
        build_canonical_directory_v1, preflight_canonical_tree_v1, CanonicalDirectoryTreeV1,
        CanonicalTreeChildV1, CanonicalTreeEntryV1, DirectoryBuildModeV1, PreparedTreeSinkV1,
        TreeObjectDispositionV1, TreePageBoundaryV1, TreePageSummaryV1, TreeSinkErrorV1,
        MAX_COW_TREE_PAGE_SUMMARIES, MAX_DIRECTORY_HASH_PROOF_NODES, MAX_TREE_OBJECT_BYTES,
        MAX_TREE_PAGE_SUMMARIES,
    };
    use crate::format::{ValidatedComponent, ValidatedSymlinkTarget};
    use crate::identity::{
        derive_file_node_v1, derive_logical_chunk_v1, derive_logical_file_v1,
        derive_physical_chunk_id_v1, derive_physical_file_id_v1, derive_physical_symlink_id_v1,
        derive_symlink_node_v1, LogicalChunkRefV1, PhysicalTreeIdV1, COMPARISON_WINDOW_BYTES,
    };
    use crate::limits::{OperationCountersV1, OperationWorkControlV1, ResourceLedgerV1};
    use crate::object::{
        decode_physical_object_v1, DiscardStrongEdgesV1, PhysicalObjectPayloadV1, TreeRecordV1,
    };
    use crate::profile::ProfileSpecV1;
    use crate::{CoreError, CoreResult};
    use blake3::hazmat::HasherExt;

    use super::mutate::{
        add_directory_entry_cow_v1, move_directory_entry_cow_independent_v1,
        remove_directory_entry_cow_v1, replace_directory_entry_cow_v1,
        replace_two_directory_entries_cow_independent_v1,
    };
    use super::tree::DirectoryLogicalIdentityV1;
    use super::view::{
        AuthenticatedTreeMutationEvidenceV1, AuthenticatedTreeReplacementEvidenceV1,
        CanonicalTreeMutationSourceV1, DirectoryHashProofV1, DirectoryHashSubtreeV1,
        DirectoryMutationHashProofV1, TreeMutationSourceErrorV1,
    };

    struct ContinueCowControlV1;

    impl OperationWorkControlV1 for ContinueCowControlV1 {
        fn cancellation_requested_v1(&mut self) -> bool {
            false
        }

        fn deadline_exceeded_v1(&mut self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum TreeModeV1 {
        ImplicitRoot,
        Explicit(u16),
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TreeBuildRequestV1 {
        entry_count: u32,
        mode: TreeModeV1,
    }

    impl TreeBuildRequestV1 {
        pub const fn new(entry_count: u32) -> Self {
            Self {
                entry_count,
                mode: TreeModeV1::ImplicitRoot,
            }
        }

        pub const fn explicit(entry_count: u32, mode: u16) -> Self {
            Self {
                entry_count,
                mode: TreeModeV1::Explicit(mode),
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TreeShapeObservationV1 {
        entry_count: u32,
        page_depth: u8,
        leaf_count: u32,
        level_one_count: u32,
        page_summary_count: u32,
        tree_object_count: u32,
    }

    impl TreeShapeObservationV1 {
        pub const fn entry_count(self) -> u32 {
            self.entry_count
        }

        pub const fn page_depth(self) -> u8 {
            self.page_depth
        }

        pub const fn leaf_count(self) -> u32 {
            self.leaf_count
        }

        pub const fn level_one_count(self) -> u32 {
            self.level_one_count
        }

        pub const fn page_summary_count(self) -> u32 {
            self.page_summary_count
        }

        pub const fn tree_object_count(self) -> u32 {
            self.tree_object_count
        }
    }

    pub fn preflight_v1(entry_count: u64) -> CoreResult<TreeShapeObservationV1> {
        let shape = preflight_canonical_tree_v1(entry_count)?;
        Ok(TreeShapeObservationV1 {
            entry_count: shape.entry_count(),
            page_depth: shape.page_depth(),
            leaf_count: shape.leaf_count(),
            level_one_count: shape.level_one_count(),
            page_summary_count: shape.page_summary_count(),
            tree_object_count: shape.tree_object_count(),
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TreeBuildObservationV1 {
        entry_count: u32,
        page_depth: u8,
        leaf_count: u32,
        level_one_count: u32,
        tree_object_count: u32,
        first_leaf_entry_count: u32,
        last_leaf_entry_count: u32,
        first_level_one_entry_count: u32,
        last_level_one_entry_count: u32,
        wrapper_mode: u16,
        wrapper_entry_count: u32,
        wrapper_root_page_absent: bool,
        leaf_counts_match_expected: bool,
        committed_objects: u32,
        tree_nodes_created: u64,
        memory_high_water: u64,
        ledger_admitted_slots: u64,
    }

    impl TreeBuildObservationV1 {
        pub const fn entry_count(self) -> u32 {
            self.entry_count
        }

        pub const fn page_depth(self) -> u8 {
            self.page_depth
        }

        pub const fn leaf_count(self) -> u32 {
            self.leaf_count
        }

        pub const fn level_one_count(self) -> u32 {
            self.level_one_count
        }

        pub const fn tree_object_count(self) -> u32 {
            self.tree_object_count
        }

        pub const fn first_leaf_entry_count(self) -> u32 {
            self.first_leaf_entry_count
        }

        pub const fn last_leaf_entry_count(self) -> u32 {
            self.last_leaf_entry_count
        }

        pub const fn first_level_one_entry_count(self) -> u32 {
            self.first_level_one_entry_count
        }

        pub const fn last_level_one_entry_count(self) -> u32 {
            self.last_level_one_entry_count
        }

        pub const fn wrapper_mode(self) -> u16 {
            self.wrapper_mode
        }

        pub const fn wrapper_entry_count(self) -> u32 {
            self.wrapper_entry_count
        }

        pub const fn wrapper_root_page_absent(self) -> bool {
            self.wrapper_root_page_absent
        }

        pub const fn leaf_counts_match_expected(self) -> bool {
            self.leaf_counts_match_expected
        }

        pub const fn committed_objects(self) -> u32 {
            self.committed_objects
        }

        pub const fn tree_nodes_created(self) -> u64 {
            self.tree_nodes_created
        }

        pub const fn memory_high_water(self) -> u64 {
            self.memory_high_water
        }

        pub const fn ledger_admitted_slots(self) -> u64 {
            self.ledger_admitted_slots
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CanonicalOrderObservationV1 {
        unsorted_error: Option<CoreError>,
        unsorted_sink_begins: u32,
        sorted_matches_canonical: bool,
        duplicate_error: Option<CoreError>,
        expected_build_ledger_admitted_slots: u64,
        sorted_build_ledger_admitted_slots: u64,
        duplicate_build_ledger_admitted_slots: u64,
    }

    impl CanonicalOrderObservationV1 {
        pub const fn unsorted_error(self) -> Option<CoreError> {
            self.unsorted_error
        }

        pub const fn unsorted_sink_begins(self) -> u32 {
            self.unsorted_sink_begins
        }

        pub const fn sorted_matches_canonical(self) -> bool {
            self.sorted_matches_canonical
        }

        pub const fn duplicate_error(self) -> Option<CoreError> {
            self.duplicate_error
        }

        pub const fn expected_build_ledger_admitted_slots(self) -> u64 {
            self.expected_build_ledger_admitted_slots
        }

        pub const fn sorted_build_ledger_admitted_slots(self) -> u64 {
            self.sorted_build_ledger_admitted_slots
        }

        pub const fn duplicate_build_ledger_admitted_slots(self) -> u64 {
            self.duplicate_build_ledger_admitted_slots
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        committed: Vec<(PhysicalTreeIdV1, Vec<u8>)>,
        pending: Vec<(PhysicalTreeIdV1, Vec<u8>)>,
        expected: usize,
        begins: u32,
    }

    impl PreparedTreeSinkV1 for RecordingSink {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(0)
        }

        fn begin_private_tree_set(&mut self, maximum_objects: u32) -> Result<(), TreeSinkErrorV1> {
            if !self.pending.is_empty() {
                return Err(TreeSinkErrorV1::Failure);
            }
            self.expected = maximum_objects as usize;
            self.begins = self.begins.saturating_add(1);
            Ok(())
        }

        fn admit_private_tree(
            &mut self,
            id: PhysicalTreeIdV1,
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
            _root: PhysicalTreeIdV1,
        ) -> Result<(), TreeSinkErrorV1> {
            if self.pending.len() > self.expected {
                return Err(TreeSinkErrorV1::Failure);
            }
            self.committed.append(&mut self.pending);
            Ok(())
        }

        fn abort_private_tree_set(&mut self) {
            self.pending.clear();
        }
    }

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

    fn synthetic_child() -> CanonicalTreeChildV1 {
        let target = ValidatedSymlinkTarget::new(b"target").expect("fixed target");
        let logical = derive_symlink_node_v1(target).expect("fixed symlink");
        let mut payload = Vec::new();
        payload.extend_from_slice(&(target.as_bytes().len() as u32).to_be_bytes());
        payload.extend_from_slice(target.as_bytes());
        let physical = derive_physical_symlink_id_v1(&object(4, &payload)).expect("fixed id");
        CanonicalTreeChildV1::Symlink { logical, physical }
    }

    fn fixed_entries<'a>(names: &'a [Vec<u8>]) -> Vec<CanonicalTreeEntryV1<'a>> {
        let child = synthetic_child();
        names
            .iter()
            .map(|name| CanonicalTreeEntryV1::new(ValidatedComponent::new(name).unwrap(), child))
            .collect()
    }

    fn run_build(
        mode: DirectoryBuildModeV1,
        entries: &[CanonicalTreeEntryV1<'_>],
    ) -> CoreResult<(
        CanonicalDirectoryTreeV1,
        RecordingSink,
        OperationCountersV1,
        u64,
    )> {
        let mut sink = RecordingSink::default();
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
        Ok((directory, sink, counters, ledger.admitted_slots()))
    }

    pub fn build_v1(request: &TreeBuildRequestV1) -> CoreResult<TreeBuildObservationV1> {
        let count = usize::try_from(request.entry_count).map_err(|_| CoreError::IntegerOverflow)?;
        if count > 18_433 {
            return Err(CoreError::CountCap);
        }
        let names = (0..count)
            .map(|index| format!("n{index:07}").into_bytes())
            .collect::<Vec<_>>();
        let entries = fixed_entries(&names);
        let mode = match request.mode {
            TreeModeV1::ImplicitRoot => DirectoryBuildModeV1::ImplicitRoot,
            TreeModeV1::Explicit(mode) => DirectoryBuildModeV1::Explicit(mode),
        };
        let (directory, sink, counters, ledger_admitted_slots) = run_build(mode, &entries)?;
        let mut leaf_counts = Vec::new();
        let mut level_one_counts = Vec::new();
        let mut wrapper_mode = 0;
        let mut wrapper_entry_count = 0;
        let mut wrapper_root_page_absent = false;
        for (_, bytes) in &sink.committed {
            let decoded = decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)?;
            match decoded.payload() {
                PhysicalObjectPayloadV1::Tree(TreeRecordV1::Leaf(leaf)) => {
                    leaf_counts.push(u32::from(leaf.count));
                }
                PhysicalObjectPayloadV1::Tree(TreeRecordV1::Index(index)) => {
                    if index.depth == 1 {
                        level_one_counts.push(index.subtree_entry_count);
                    }
                }
                PhysicalObjectPayloadV1::Tree(TreeRecordV1::Directory(wrapper)) => {
                    wrapper_mode = wrapper.mode;
                    wrapper_entry_count = wrapper.entry_count;
                    wrapper_root_page_absent = wrapper.root_page_id.is_none();
                }
                _ => {}
            }
        }
        let expected_leaf_counts = (0..count.div_ceil(192))
            .map(|leaf| (count - leaf * 192).min(192) as u32)
            .collect::<Vec<_>>();
        Ok(TreeBuildObservationV1 {
            entry_count: directory.entry_count(),
            page_depth: directory.page_depth(),
            leaf_count: directory.leaf_count(),
            level_one_count: directory.level_one_count(),
            tree_object_count: directory.tree_object_count(),
            first_leaf_entry_count: leaf_counts.first().copied().unwrap_or(0),
            last_leaf_entry_count: leaf_counts.last().copied().unwrap_or(0),
            first_level_one_entry_count: level_one_counts
                .first()
                .copied()
                .and_then(|count| u32::try_from(count).ok())
                .unwrap_or(0),
            last_level_one_entry_count: level_one_counts
                .last()
                .copied()
                .and_then(|count| u32::try_from(count).ok())
                .unwrap_or(0),
            wrapper_mode,
            wrapper_entry_count,
            wrapper_root_page_absent,
            leaf_counts_match_expected: leaf_counts == expected_leaf_counts,
            committed_objects: sink.committed.len() as u32,
            tree_nodes_created: counters.tree_nodes_created,
            memory_high_water: counters.memory_high_water,
            ledger_admitted_slots,
        })
    }

    pub fn canonical_order_v1() -> CoreResult<CanonicalOrderObservationV1> {
        let mut names = (0..3)
            .map(|index| format!("n{index:07}").into_bytes())
            .collect::<Vec<_>>();
        let canonical = fixed_entries(&names);
        let (expected, _, _, expected_ledger_slots) =
            run_build(DirectoryBuildModeV1::Explicit(0o755), &canonical)?;

        names.swap(0, 2);
        let permuted = fixed_entries(&names);
        let mut sink = RecordingSink::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut object_scratch = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut page_scratch = vec![None; MAX_TREE_PAGE_SUMMARIES];
        let unsorted_error = build_canonical_directory_v1(
            DirectoryBuildModeV1::Explicit(0o755),
            &permuted,
            &mut sink,
            &ledger,
            &mut counters,
            &mut object_scratch,
            &mut page_scratch,
        )
        .err();
        let unsorted_sink_begins = sink.begins;

        names.sort_by(|left, right| left.cmp(right));
        let sorted = fixed_entries(&names);
        let (sorted_tree, _, _, sorted_ledger_slots) =
            run_build(DirectoryBuildModeV1::Explicit(0o755), &sorted)?;

        let duplicate = [sorted[0], sorted[0]];
        let mut duplicate_sink = RecordingSink::default();
        let duplicate_ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut duplicate_counters = OperationCountersV1::default();
        let duplicate_error = build_canonical_directory_v1(
            DirectoryBuildModeV1::Explicit(0o755),
            &duplicate,
            &mut duplicate_sink,
            &duplicate_ledger,
            &mut duplicate_counters,
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [None; MAX_TREE_PAGE_SUMMARIES],
        )
        .err();
        Ok(CanonicalOrderObservationV1 {
            unsorted_error,
            unsorted_sink_begins,
            sorted_matches_canonical: sorted_tree == expected,
            duplicate_error,
            expected_build_ledger_admitted_slots: expected_ledger_slots,
            sorted_build_ledger_admitted_slots: sorted_ledger_slots,
            duplicate_build_ledger_admitted_slots: duplicate_ledger.admitted_slots(),
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum TreeMutationActionV1 {
        Add,
        Remove,
        Replace,
        Move,
        ReplacePair,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum TreeMutationFaultV1 {
        None,
        IntegerOverflow,
        WrongRoot,
        TamperedWindow,
        DuplicateResult,
        CorruptOldHead,
        CorruptMiddle,
        CorruptTail,
        CorruptLeafSiblings,
        CorruptLevelOneSiblings,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TreeMutationRequestV1 {
        action: TreeMutationActionV1,
        base_entries: u32,
        index: u32,
        second_index: u32,
        base_tag: u8,
        changed_tag: u8,
        fault: TreeMutationFaultV1,
        source_resident_bytes: u64,
        sink_resident_bytes: u64,
    }

    impl TreeMutationRequestV1 {
        pub const fn new(action: TreeMutationActionV1, base_entries: u32, index: u32) -> Self {
            Self {
                action,
                base_entries,
                index,
                second_index: index,
                base_tag: 0,
                changed_tag: 1,
                fault: TreeMutationFaultV1::None,
                source_resident_bytes: 0,
                sink_resident_bytes: 0,
            }
        }

        pub const fn replace(base_entries: u32, index: u32) -> Self {
            Self::new(TreeMutationActionV1::Replace, base_entries, index)
        }

        pub const fn add(base_entries: u32, index: u32) -> Self {
            Self::new(TreeMutationActionV1::Add, base_entries, index)
        }

        pub const fn remove(base_entries: u32, index: u32) -> Self {
            Self::new(TreeMutationActionV1::Remove, base_entries, index)
        }

        pub const fn move_entry(
            base_entries: u32,
            removal_index: u32,
            insertion_index: u32,
        ) -> Self {
            Self {
                second_index: insertion_index,
                ..Self::new(TreeMutationActionV1::Move, base_entries, removal_index)
            }
        }

        pub const fn replace_pair(base_entries: u32, first_index: u32, second_index: u32) -> Self {
            Self {
                second_index,
                ..Self::new(TreeMutationActionV1::ReplacePair, base_entries, first_index)
            }
        }

        pub const fn with_tags(self, base_tag: u8, changed_tag: u8) -> Self {
            Self {
                base_tag,
                changed_tag,
                ..self
            }
        }

        pub const fn with_fault(self, fault: TreeMutationFaultV1) -> Self {
            Self { fault, ..self }
        }

        pub const fn with_residency(self, source: u64, sink: u64) -> Self {
            Self {
                source_resident_bytes: source,
                sink_resident_bytes: sink,
                ..self
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TreeMutationObservationV1 {
        error: Option<CoreError>,
        result_entries: u32,
        page_depth: u8,
        changed_leaf_index: u32,
        first_changed_leaf: u32,
        last_changed_leaf: u32,
        changed_leaves: u32,
        changed_level_one: u32,
        reused_leaves: u32,
        reused_level_one: u32,
        emitted_objects: u32,
        tree_nodes_created: u64,
        tree_nodes_reused: u64,
        sink_committed_len: u32,
        sink_begins: u32,
        ledger_admitted_slots: u64,
        base_build_ledger_admitted_slots: u64,
        result_build_ledger_admitted_slots: u64,
        bytes_read: u64,
        base_reads: u64,
        result_reads: u64,
        created_bytes: u64,
        reused_bytes: u64,
        cow_mutation_control_polls: u64,
        cow_mutation_maximum_work_between_polls: u64,
        proof_window_bytes: u64,
        proof_node_count: u32,
        middle_len: u32,
        old_tail_prefix_nonempty: bool,
        affected_entries: u32,
        leaf_group: u32,
        level_one_group: u32,
        changed_leaf_id_differs: bool,
        directory_logical_differs_from_base: bool,
        directory_physical_differs_from_base: bool,
        unchanged_leaf_reused: bool,
        logical_matches_rebuild: bool,
        physical_matches_rebuild: bool,
        unchanged_child_reused: bool,
        created_objects_are_tree_pages: bool,
    }

    impl TreeMutationObservationV1 {
        pub const fn error(self) -> Option<CoreError> {
            self.error
        }

        pub const fn result_entries(self) -> u32 {
            self.result_entries
        }

        pub const fn page_depth(self) -> u8 {
            self.page_depth
        }

        pub const fn changed_leaf_index(self) -> u32 {
            self.changed_leaf_index
        }

        pub const fn first_changed_leaf(self) -> u32 {
            self.first_changed_leaf
        }

        pub const fn last_changed_leaf(self) -> u32 {
            self.last_changed_leaf
        }

        pub const fn changed_leaves(self) -> u32 {
            self.changed_leaves
        }

        pub const fn changed_level_one(self) -> u32 {
            self.changed_level_one
        }

        pub const fn reused_leaves(self) -> u32 {
            self.reused_leaves
        }

        pub const fn reused_level_one(self) -> u32 {
            self.reused_level_one
        }

        pub const fn emitted_objects(self) -> u32 {
            self.emitted_objects
        }

        pub const fn tree_nodes_created(self) -> u64 {
            self.tree_nodes_created
        }

        pub const fn tree_nodes_reused(self) -> u64 {
            self.tree_nodes_reused
        }

        pub const fn sink_committed_len(self) -> u32 {
            self.sink_committed_len
        }

        pub const fn sink_begins(self) -> u32 {
            self.sink_begins
        }

        pub const fn ledger_admitted_slots(self) -> u64 {
            self.ledger_admitted_slots
        }

        pub const fn base_build_ledger_admitted_slots(self) -> u64 {
            self.base_build_ledger_admitted_slots
        }

        pub const fn result_build_ledger_admitted_slots(self) -> u64 {
            self.result_build_ledger_admitted_slots
        }

        pub const fn bytes_read(self) -> u64 {
            self.bytes_read
        }

        pub const fn base_reads(self) -> u64 {
            self.base_reads
        }

        pub const fn result_reads(self) -> u64 {
            self.result_reads
        }

        pub const fn created_bytes(self) -> u64 {
            self.created_bytes
        }

        pub const fn reused_bytes(self) -> u64 {
            self.reused_bytes
        }

        pub const fn cow_mutation_control_polls(self) -> u64 {
            self.cow_mutation_control_polls
        }

        pub const fn cow_mutation_maximum_work_between_polls(self) -> u64 {
            self.cow_mutation_maximum_work_between_polls
        }

        pub const fn proof_window_bytes(self) -> u64 {
            self.proof_window_bytes
        }

        pub const fn proof_node_count(self) -> u32 {
            self.proof_node_count
        }

        pub const fn middle_len(self) -> u32 {
            self.middle_len
        }

        pub const fn old_tail_prefix_nonempty(self) -> bool {
            self.old_tail_prefix_nonempty
        }

        pub const fn outcome(self) -> CoreResult<()> {
            match self.error {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }

        pub const fn affected_entries(self) -> u32 {
            self.affected_entries
        }

        pub const fn leaf_group(self) -> u32 {
            self.leaf_group
        }

        pub const fn level_one_group(self) -> u32 {
            self.level_one_group
        }

        pub const fn changed_leaf_id_differs(self) -> bool {
            self.changed_leaf_id_differs
        }

        pub const fn directory_logical_differs_from_base(self) -> bool {
            self.directory_logical_differs_from_base
        }

        pub const fn directory_physical_differs_from_base(self) -> bool {
            self.directory_physical_differs_from_base
        }

        pub const fn unchanged_leaf_reused(self) -> bool {
            self.unchanged_leaf_reused
        }

        pub const fn logical_matches_rebuild(self) -> bool {
            self.logical_matches_rebuild
        }

        pub const fn physical_matches_rebuild(self) -> bool {
            self.physical_matches_rebuild
        }

        pub const fn unchanged_child_reused(self) -> bool {
            self.unchanged_child_reused
        }

        pub const fn created_objects_are_tree_pages(self) -> bool {
            self.created_objects_are_tree_pages
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct TreeIdentityObservationV1 {
        logical_equal: bool,
        physical_ids_differ: bool,
        implicit_root: bool,
        first_build_ledger_admitted_slots: u64,
        second_build_ledger_admitted_slots: u64,
    }

    impl TreeIdentityObservationV1 {
        pub const fn logical_equal(self) -> bool {
            self.logical_equal
        }

        pub const fn physical_ids_differ(self) -> bool {
            self.physical_ids_differ
        }

        pub const fn implicit_root(self) -> bool {
            self.implicit_root
        }

        pub const fn first_build_ledger_admitted_slots(self) -> u64 {
            self.first_build_ledger_admitted_slots
        }

        pub const fn second_build_ledger_admitted_slots(self) -> u64 {
            self.second_build_ledger_admitted_slots
        }
    }

    #[derive(Default)]
    struct MutationSink {
        committed: Vec<(PhysicalTreeIdV1, Vec<u8>)>,
        pending: Vec<(PhysicalTreeIdV1, Vec<u8>)>,
        expected: usize,
        begins: u32,
        resident_bytes: u64,
    }

    impl PreparedTreeSinkV1 for MutationSink {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_bytes)
        }

        fn begin_private_tree_set(&mut self, maximum_objects: u32) -> Result<(), TreeSinkErrorV1> {
            if !self.pending.is_empty() {
                return Err(TreeSinkErrorV1::Failure);
            }
            self.expected = maximum_objects as usize;
            self.begins = self.begins.saturating_add(1);
            Ok(())
        }

        fn admit_private_tree(
            &mut self,
            id: PhysicalTreeIdV1,
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
            _root: PhysicalTreeIdV1,
        ) -> Result<(), TreeSinkErrorV1> {
            self.committed.append(&mut self.pending);
            Ok(())
        }

        fn abort_private_tree_set(&mut self) {
            self.pending.clear();
        }
    }

    struct MutationSource<'a> {
        base: &'a [CanonicalTreeEntryV1<'a>],
        result: &'a [CanonicalTreeEntryV1<'a>],
        resident_bytes: u64,
        base_reads: u64,
        result_reads: u64,
    }

    impl<'a> CanonicalTreeMutationSourceV1 for MutationSource<'a> {
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
            self.base_reads = self.base_reads.saturating_add(1);
            self.base
                .get(ordinal as usize)
                .copied()
                .ok_or(TreeMutationSourceErrorV1::Failure)
        }

        fn read_result_entry(
            &mut self,
            ordinal: u32,
        ) -> Result<CanonicalTreeEntryV1<'_>, TreeMutationSourceErrorV1> {
            self.result_reads = self.result_reads.saturating_add(1);
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
        ledger_admitted_slots: u64,
    }

    struct ReplacementEvidence<'a> {
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
    struct MutationEvidence<'a> {
        affected_leaf_index: Option<u32>,
        affected_leaf: Option<TreePageSummaryV1>,
        leaf_group: Vec<TreePageBoundaryV1<'a>>,
        secondary_leaf_group_index: Option<u32>,
        secondary_leaf_group: Vec<TreePageBoundaryV1<'a>>,
        level_one_group: Vec<TreePageBoundaryV1<'a>>,
        old_preimage_len: u64,
        stream_start_index: u32,
        stream_entry_offset: u64,
        old_head: Vec<u8>,
        middle: Vec<DirectoryHashSubtreeV1>,
        old_tail_prefix: Vec<u8>,
    }

    fn symlink_child(tag: u8) -> CanonicalTreeChildV1 {
        let target = match tag {
            0 => b"stable-target".as_slice(),
            1 => b"changed-target".as_slice(),
            2 => b"other-target".as_slice(),
            3 => b"old-target".as_slice(),
            4 => b"new-target".as_slice(),
            5 => b"base-target".as_slice(),
            6 => b"replacement".as_slice(),
            7 => b"alternate-target".as_slice(),
            8 => b"added-target".as_slice(),
            9 => b"head-added".as_slice(),
            _ => b"middle-added".as_slice(),
        };
        let target = ValidatedSymlinkTarget::new(target).expect("fixed target");
        let logical = derive_symlink_node_v1(target).expect("fixed symlink");
        let mut payload = Vec::new();
        payload.extend_from_slice(&(target.as_bytes().len() as u32).to_be_bytes());
        payload.extend_from_slice(target.as_bytes());
        let physical = derive_physical_symlink_id_v1(&object(4, &payload)).expect("fixed id");
        CanonicalTreeChildV1::Symlink { logical, physical }
    }

    fn file_child(tag: u8) -> CanonicalTreeChildV1 {
        let data = match tag {
            0 => b"stable".as_slice(),
            1 => b"old".as_slice(),
            _ => b"new".as_slice(),
        };
        file_child_with_partition(data, &[data])
    }

    fn file_child_with_partition(data: &[u8], partition: &[&[u8]]) -> CanonicalTreeChildV1 {
        assert_eq!(partition.concat(), data);
        let logical_chunk = derive_logical_chunk_v1(data).expect("fixed chunk");
        let logical_file = derive_logical_file_v1(
            data.len() as u64,
            &[LogicalChunkRefV1::from_identity(logical_chunk)],
        )
        .expect("fixed file");
        let logical = derive_file_node_v1(0o644, logical_file).expect("fixed node");
        let mut payload = Vec::new();
        payload.extend_from_slice(&0o644_u16.to_be_bytes());
        payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
        payload.extend_from_slice(&1_u32.to_be_bytes());
        payload.push(2);
        payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
        payload.extend_from_slice(&(partition.len() as u32).to_be_bytes());
        for chunk_bytes in partition {
            let chunk = object(5, chunk_bytes);
            let id = derive_physical_chunk_id_v1(&chunk).expect("fixed chunk id");
            payload.extend_from_slice(&(chunk_bytes.len() as u32).to_be_bytes());
            payload.extend_from_slice(id.as_bytes());
        }
        let physical = derive_physical_file_id_v1(&object(3, &payload)).expect("fixed file id");
        CanonicalTreeChildV1::File { logical, physical }
    }

    fn names(count: usize) -> Vec<Vec<u8>> {
        (0..count)
            .map(|index| format!("n{index:07}").into_bytes())
            .collect()
    }

    fn entries<'a>(
        names: &'a [Vec<u8>],
        child: CanonicalTreeChildV1,
    ) -> Vec<CanonicalTreeEntryV1<'a>> {
        names
            .iter()
            .map(|name| CanonicalTreeEntryV1::new(ValidatedComponent::new(name).unwrap(), child))
            .collect()
    }

    fn build_tree(
        mode: DirectoryBuildModeV1,
        entries: &[CanonicalTreeEntryV1<'_>],
    ) -> CoreResult<BuiltTree> {
        let mut sink = MutationSink::default();
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
        let leaf_count = directory.leaf_count() as usize;
        let level_one_count = directory.level_one_count() as usize;
        Ok(BuiltTree {
            directory,
            leaves: page_scratch[..leaf_count]
                .iter()
                .copied()
                .map(Option::unwrap)
                .collect(),
            level_one: page_scratch[leaf_count..leaf_count + level_one_count]
                .iter()
                .copied()
                .map(Option::unwrap)
                .collect(),
            ledger_admitted_slots: ledger.admitted_slots(),
        })
    }

    fn preimage(
        mode: DirectoryBuildModeV1,
        entries: &[CanonicalTreeEntryV1<'_>],
    ) -> (Vec<u8>, Vec<usize>) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"ESV2-DNODE\0");
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(
            &match mode {
                DirectoryBuildModeV1::ImplicitRoot => {
                    crate::format::ROOT_DIRECTORY_MODE_SENTINEL_V1
                }
                DirectoryBuildModeV1::Explicit(mode) => mode,
            }
            .to_le_bytes(),
        );
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        let mut offsets = Vec::with_capacity(entries.len() + 1);
        for entry in entries {
            offsets.push(bytes.len());
            bytes.extend_from_slice(&(entry.name().as_bytes().len() as u32).to_le_bytes());
            bytes.extend_from_slice(entry.name().as_bytes());
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

    fn hash_subtrees(
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

    fn boundary<'a>(
        summary: TreePageSummaryV1,
        entries: &'a [CanonicalTreeEntryV1<'a>],
    ) -> TreePageBoundaryV1<'a> {
        TreePageBoundaryV1::new(
            summary,
            entries[summary.first_entry() as usize].name(),
            entries[summary.last_entry() as usize].name(),
        )
    }

    fn replacement_evidence<'a>(
        mode: DirectoryBuildModeV1,
        entries: &'a [CanonicalTreeEntryV1<'a>],
        built: &BuiltTree,
        index: usize,
    ) -> ReplacementEvidence<'a> {
        let leaf_index = index / 192;
        let affected_leaf = built.leaves[leaf_index];
        let first = affected_leaf.first_entry() as usize;
        let end = affected_leaf.last_entry() as usize + 1;
        let leaf_group_first = leaf_index / 96 * 96;
        let leaf_group_end = (leaf_group_first + 96).min(built.leaves.len());
        let leaf_group = if built.directory.page_depth() == 0 {
            Vec::new()
        } else {
            built.leaves[leaf_group_first..leaf_group_end]
                .iter()
                .copied()
                .map(|summary| boundary(summary, entries))
                .collect()
        };
        let level_one_group = if built.directory.page_depth() == 2 {
            built
                .level_one
                .iter()
                .copied()
                .map(|summary| boundary(summary, entries))
                .collect()
        } else {
            Vec::new()
        };
        let (bytes, offsets) = preimage(mode, entries);
        let leaf_offset = offsets[first];
        let leaf_end = offsets[end];
        let window_offset = leaf_offset / blake3::CHUNK_LEN * blake3::CHUNK_LEN;
        let window_end = leaf_end
            .div_ceil(blake3::CHUNK_LEN)
            .saturating_mul(blake3::CHUNK_LEN)
            .min(bytes.len());
        let prefix = hash_subtrees(&bytes, 0, window_offset);
        let suffix = hash_subtrees(&bytes, window_end, bytes.len());
        assert!(prefix.len() + suffix.len() <= MAX_DIRECTORY_HASH_PROOF_NODES);
        ReplacementEvidence {
            affected_leaf_index: leaf_index as u32,
            affected_entries: &entries[first..end],
            affected_leaf,
            leaf_group,
            level_one_group,
            preimage_len: bytes.len() as u64,
            window_offset: window_offset as u64,
            old_window: bytes[window_offset..window_end].to_vec(),
            prefix,
            suffix,
            affected_leaf_offset: leaf_offset as u64,
        }
    }

    fn mutation_evidence<'a>(
        mode: DirectoryBuildModeV1,
        entries: &'a [CanonicalTreeEntryV1<'a>],
        built: &BuiltTree,
        index: usize,
    ) -> MutationEvidence<'a> {
        let (bytes, offsets) = preimage(mode, entries);
        let stream_start = index.saturating_sub(1);
        let stream_offset = offsets[stream_start];
        let tail_offset = stream_offset / blake3::CHUNK_LEN * blake3::CHUNK_LEN;
        let (old_head, middle) = if tail_offset == 0 {
            (Vec::new(), Vec::new())
        } else {
            (
                bytes[..blake3::CHUNK_LEN].to_vec(),
                hash_subtrees(&bytes, blake3::CHUNK_LEN, tail_offset),
            )
        };
        let affected_index = if entries.is_empty() {
            None
        } else {
            Some((index.min(entries.len() - 1) / 192) as u32)
        };
        let (affected_leaf, leaf_group, level_one_group) = if let Some(leaf_index) = affected_index
        {
            let leaf = built.leaves[leaf_index as usize];
            let first = leaf_index as usize / 96 * 96;
            let end = (first + 96).min(built.leaves.len());
            let leaves = if built.directory.page_depth() == 0 {
                Vec::new()
            } else {
                built.leaves[first..end]
                    .iter()
                    .copied()
                    .map(|summary| boundary(summary, entries))
                    .collect()
            };
            let level_one = if built.directory.page_depth() == 2 {
                built
                    .level_one
                    .iter()
                    .copied()
                    .map(|summary| boundary(summary, entries))
                    .collect()
            } else {
                Vec::new()
            };
            (Some(leaf), leaves, level_one)
        } else {
            (None, Vec::new(), Vec::new())
        };
        MutationEvidence {
            affected_leaf_index: affected_index,
            affected_leaf,
            leaf_group,
            secondary_leaf_group_index: None,
            secondary_leaf_group: Vec::new(),
            level_one_group,
            old_preimage_len: bytes.len() as u64,
            stream_start_index: stream_start as u32,
            stream_entry_offset: stream_offset as u64,
            old_head,
            middle,
            old_tail_prefix: bytes[tail_offset..stream_offset].to_vec(),
        }
    }

    fn add_secondary_leaf_group<'a>(
        evidence: &mut MutationEvidence<'a>,
        entries: &'a [CanonicalTreeEntryV1<'a>],
        built: &BuiltTree,
        index: usize,
    ) {
        if built.directory.page_depth() != 2 || entries.is_empty() {
            return;
        }
        let leaf = index.min(entries.len() - 1) / 192;
        let group = leaf / 96;
        if evidence.affected_leaf_index.map(|leaf| leaf as usize / 96) == Some(group) {
            return;
        }
        let first = group * 96;
        let end = (first + 96).min(built.leaves.len());
        evidence.secondary_leaf_group_index = Some(group as u32);
        evidence.secondary_leaf_group = built.leaves[first..end]
            .iter()
            .copied()
            .map(|summary| boundary(summary, entries))
            .collect();
    }

    fn replacement_evidence_value<'a>(
        evidence: &'a ReplacementEvidence<'a>,
        base: CanonicalDirectoryTreeV1,
    ) -> AuthenticatedTreeReplacementEvidenceV1<'a> {
        AuthenticatedTreeReplacementEvidenceV1::new(
            base,
            evidence.affected_leaf_index,
            evidence.affected_entries,
            evidence.affected_leaf,
            &evidence.leaf_group,
            &evidence.level_one_group,
            DirectoryHashProofV1::new(
                evidence.preimage_len,
                evidence.window_offset,
                &evidence.old_window,
                &evidence.prefix,
                &evidence.suffix,
                evidence.affected_leaf_offset,
            ),
        )
    }

    pub(crate) fn with_replacement_evidence_v1<R>(
        mode: DirectoryBuildModeV1,
        entries: &[CanonicalTreeEntryV1<'_>],
        index: usize,
        callback: impl FnOnce(CanonicalDirectoryTreeV1, AuthenticatedTreeReplacementEvidenceV1<'_>) -> R,
    ) -> CoreResult<R> {
        let built = build_tree(mode, entries)?;
        let evidence = replacement_evidence(mode, entries, &built, index);
        Ok(callback(
            built.directory,
            replacement_evidence_value(&evidence, built.directory),
        ))
    }

    fn mutation_evidence_value<'a>(
        evidence: &'a MutationEvidence<'a>,
        base: CanonicalDirectoryTreeV1,
    ) -> AuthenticatedTreeMutationEvidenceV1<'a> {
        let value = AuthenticatedTreeMutationEvidenceV1::new(
            base,
            evidence.affected_leaf_index,
            evidence.affected_leaf,
            &evidence.leaf_group,
            &evidence.level_one_group,
            DirectoryMutationHashProofV1::new(
                evidence.old_preimage_len,
                evidence.stream_start_index,
                evidence.stream_entry_offset,
                &evidence.old_head,
                &evidence.middle,
                &evidence.old_tail_prefix,
            ),
        );
        match evidence.secondary_leaf_group_index {
            Some(group) => value.with_secondary_leaf_group(group, &evidence.secondary_leaf_group),
            None => value,
        }
    }

    pub(crate) fn with_mutation_evidence_v1<R>(
        mode: DirectoryBuildModeV1,
        base_entries: &[CanonicalTreeEntryV1<'_>],
        result_entries: &[CanonicalTreeEntryV1<'_>],
        index: usize,
        callback: impl FnOnce(
            CanonicalDirectoryTreeV1,
            CanonicalDirectoryTreeV1,
            AuthenticatedTreeMutationEvidenceV1<'_>,
            &mut dyn CanonicalTreeMutationSourceV1,
        ) -> R,
    ) -> CoreResult<R> {
        let base = build_tree(mode, base_entries)?;
        let result = build_tree(mode, result_entries)?;
        let evidence = mutation_evidence(mode, base_entries, &base, index);
        let mut source = MutationSource {
            base: base_entries,
            result: result_entries,
            resident_bytes: core::mem::size_of::<MutationSource<'_>>() as u64,
            base_reads: 0,
            result_reads: 0,
        };
        Ok(callback(
            base.directory,
            result.directory,
            mutation_evidence_value(&evidence, base.directory),
            &mut source,
        ))
    }

    fn mutation_name(index: usize, action: TreeMutationActionV1) -> Vec<u8> {
        match action {
            TreeMutationActionV1::Add if index == 0 => b"a0000000".to_vec(),
            TreeMutationActionV1::Add => format!("n{:07}x", index - 1).into_bytes(),
            TreeMutationActionV1::Remove => Vec::new(),
            TreeMutationActionV1::Replace
            | TreeMutationActionV1::Move
            | TreeMutationActionV1::ReplacePair => Vec::new(),
        }
    }

    fn make_observation(
        request: TreeMutationRequestV1,
        result_entries: Option<&[CanonicalTreeEntryV1<'_>]>,
        base: &BuiltTree,
        result: Option<&BuiltTree>,
        error: Option<CoreError>,
        sink: &MutationSink,
        source: Option<&MutationSource<'_>>,
        counters: &OperationCountersV1,
        operation_ledger_admitted_slots: u64,
        evidence_window_bytes: u64,
        proof_node_count: u32,
        middle_len: u32,
        old_tail_prefix_nonempty: bool,
        affected_entries: u32,
        leaf_group: u32,
        level_one_group: u32,
        changed_leaf_index: u32,
        first_changed_leaf: u32,
        last_changed_leaf: u32,
        changed_leaves: u32,
        changed_level_one: u32,
        changed_leaf_id_differs: bool,
        directory_logical_differs_from_base: bool,
        directory_physical_differs_from_base: bool,
        unchanged_leaf_reused: bool,
        emitted_objects: u32,
        reused_leaves: u32,
        reused_level_one: u32,
        logical_matches_rebuild: bool,
        physical_matches_rebuild: bool,
        unchanged_child_reused: bool,
    ) -> TreeMutationObservationV1 {
        let result_entries_count = result_entries
            .map(|entries| entries.len() as u32)
            .unwrap_or_else(|| match request.action {
                TreeMutationActionV1::Add => request.base_entries.saturating_add(1),
                TreeMutationActionV1::Remove => request.base_entries.saturating_sub(1),
                TreeMutationActionV1::Replace
                | TreeMutationActionV1::Move
                | TreeMutationActionV1::ReplacePair => request.base_entries,
            });
        let created_objects_are_tree_pages = sink
            .committed
            .iter()
            .all(|(_, bytes)| bytes.get(10) == Some(&0x02));
        TreeMutationObservationV1 {
            error,
            result_entries: result_entries_count,
            page_depth: result
                .map(|tree| tree.directory.page_depth())
                .unwrap_or(base.directory.page_depth()),
            changed_leaf_index,
            first_changed_leaf,
            last_changed_leaf,
            changed_leaves,
            changed_level_one,
            reused_leaves,
            reused_level_one,
            emitted_objects,
            tree_nodes_created: counters.tree_nodes_created,
            tree_nodes_reused: counters.tree_nodes_reused,
            sink_committed_len: sink.committed.len() as u32,
            sink_begins: sink.begins,
            ledger_admitted_slots: operation_ledger_admitted_slots
                .saturating_add(base.ledger_admitted_slots)
                .saturating_add(result.map(|tree| tree.ledger_admitted_slots).unwrap_or(0)),
            base_build_ledger_admitted_slots: base.ledger_admitted_slots,
            result_build_ledger_admitted_slots: result
                .map(|tree| tree.ledger_admitted_slots)
                .unwrap_or(0),
            bytes_read: counters.bytes_read,
            base_reads: source.map(|source| source.base_reads).unwrap_or(0),
            result_reads: source.map(|source| source.result_reads).unwrap_or(0),
            created_bytes: sink
                .committed
                .iter()
                .map(|(_, bytes)| bytes.len() as u64)
                .sum(),
            reused_bytes: counters.bytes_structurally_reused,
            cow_mutation_control_polls: counters.cow_mutation_control_polls,
            cow_mutation_maximum_work_between_polls: counters
                .cow_mutation_maximum_work_between_polls,
            proof_window_bytes: evidence_window_bytes,
            proof_node_count,
            middle_len,
            old_tail_prefix_nonempty,
            affected_entries,
            leaf_group,
            level_one_group,
            changed_leaf_id_differs,
            directory_logical_differs_from_base,
            directory_physical_differs_from_base,
            unchanged_leaf_reused,
            logical_matches_rebuild,
            physical_matches_rebuild,
            unchanged_child_reused,
            created_objects_are_tree_pages,
        }
    }

    fn replace_with_children(
        request: TreeMutationRequestV1,
        base_child: CanonicalTreeChildV1,
        replacement_child: CanonicalTreeChildV1,
    ) -> CoreResult<TreeMutationObservationV1> {
        let count =
            usize::try_from(request.base_entries).map_err(|_| CoreError::IntegerOverflow)?;
        let index = usize::try_from(request.index).map_err(|_| CoreError::IntegerOverflow)?;
        let names = names(count);
        let base_entries = entries(&names, base_child);
        let base = build_tree(DirectoryBuildModeV1::ImplicitRoot, &base_entries)?;
        let mut updated = base_entries.clone();
        let replacement = CanonicalTreeEntryV1::new(
            ValidatedComponent::new(&names[index]).map_err(|_| CoreError::Name)?,
            replacement_child,
        );
        updated[index] = replacement;
        let rebuilt = build_tree(DirectoryBuildModeV1::ImplicitRoot, &updated)?;
        let mut evidence = replacement_evidence(
            DirectoryBuildModeV1::ImplicitRoot,
            &base_entries,
            &base,
            index,
        );
        let affected_entries = evidence.affected_entries.len() as u32;
        let leaf_group = evidence.leaf_group.len() as u32;
        let level_one_group = evidence.level_one_group.len() as u32;
        if request.fault == TreeMutationFaultV1::WrongRoot {
            let alternate = entries(&names, symlink_child(2));
            let alternate_tree = build_tree(DirectoryBuildModeV1::ImplicitRoot, &alternate)?;
            evidence.affected_leaf = alternate_tree.leaves[index / 192];
        } else if request.fault == TreeMutationFaultV1::TamperedWindow {
            if let Some(first) = evidence.old_window.first_mut() {
                *first ^= 1;
            }
        }
        let proof_window_bytes = evidence.old_window.len() as u64;
        let proof_node_count = (evidence.prefix.len() + evidence.suffix.len()) as u32;
        let mut sink = MutationSink {
            resident_bytes: request.sink_resident_bytes,
            ..MutationSink::default()
        };
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut control = ContinueCowControlV1;
        let outcome = replace_directory_entry_cow_v1(
            base.directory,
            replacement_evidence_value(&evidence, base.directory),
            index,
            replacement,
            &mut sink,
            &ledger,
            &mut counters,
            &mut control,
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; COMPARISON_WINDOW_BYTES],
        );
        let error = outcome.as_ref().err().copied();
        let (
            changed_leaf_index,
            changed_leaf_id_differs,
            directory_logical_differs_from_base,
            directory_physical_differs_from_base,
        ) = outcome
            .as_ref()
            .map(|cow| {
                (
                    cow.changed_leaf_index(),
                    cow.changed_leaf().id() != base.leaves[cow.changed_leaf_index() as usize].id(),
                    cow.directory().logical() != base.directory.logical(),
                    cow.directory().physical() != base.directory.physical(),
                )
            })
            .unwrap_or((0, false, false, false));
        let (logical_matches_rebuild, physical_matches_rebuild) = outcome
            .as_ref()
            .map(|cow| {
                (
                    cow.directory().logical() == rebuilt.directory.logical(),
                    cow.directory().physical() == rebuilt.directory.physical(),
                )
            })
            .unwrap_or((false, false));
        Ok(make_observation(
            request,
            Some(&updated),
            &base,
            Some(&rebuilt),
            error,
            &sink,
            None,
            &counters,
            ledger.admitted_slots(),
            proof_window_bytes,
            proof_node_count,
            0,
            false,
            affected_entries,
            leaf_group,
            level_one_group,
            changed_leaf_index,
            0,
            0,
            0,
            0,
            changed_leaf_id_differs,
            directory_logical_differs_from_base,
            directory_physical_differs_from_base,
            false,
            sink.committed.len() as u32,
            counters.tree_nodes_reused as u32,
            0,
            logical_matches_rebuild,
            physical_matches_rebuild,
            false,
        ))
    }

    pub fn mutate_v1(request: TreeMutationRequestV1) -> CoreResult<TreeMutationObservationV1> {
        if request.fault == TreeMutationFaultV1::IntegerOverflow {
            return Err(CoreError::IntegerOverflow);
        }
        if request.action == TreeMutationActionV1::Replace {
            return replace_with_children(
                request,
                symlink_child(request.base_tag),
                symlink_child(request.changed_tag),
            );
        }
        let base_count =
            usize::try_from(request.base_entries).map_err(|_| CoreError::IntegerOverflow)?;
        let index = usize::try_from(request.index).map_err(|_| CoreError::IntegerOverflow)?;
        let second_index =
            usize::try_from(request.second_index).map_err(|_| CoreError::IntegerOverflow)?;
        let names = names(base_count);
        let mut base_entries = entries(&names, symlink_child(request.base_tag));
        if request.action == TreeMutationActionV1::Remove {
            let removed_name = names.get(index).ok_or(CoreError::Path)?;
            base_entries[index] = CanonicalTreeEntryV1::new(
                ValidatedComponent::new(removed_name).map_err(|_| CoreError::Name)?,
                symlink_child(request.changed_tag),
            );
        }
        let base = build_tree(DirectoryBuildModeV1::ImplicitRoot, &base_entries)?;
        let mutation_component_name: Vec<u8>;
        let result_entries = if request.fault == TreeMutationFaultV1::DuplicateResult {
            let duplicate = base_entries.get(1).copied().ok_or(CoreError::Path)?;
            vec![base_entries[0], base_entries[1], duplicate]
        } else {
            let mut result = base_entries.clone();
            match request.action {
                TreeMutationActionV1::Add => {
                    mutation_component_name = mutation_name(index, request.action);
                    result.insert(
                        index,
                        CanonicalTreeEntryV1::new(
                            ValidatedComponent::new(&mutation_component_name)
                                .map_err(|_| CoreError::Name)?,
                            symlink_child(request.changed_tag),
                        ),
                    );
                }
                TreeMutationActionV1::Remove => {
                    result.remove(index);
                }
                TreeMutationActionV1::Move => {
                    if index >= result.len() || second_index >= result.len() {
                        return Err(CoreError::Path);
                    }
                    let removed = result.remove(index);
                    mutation_component_name = if second_index == 0 {
                        b"a0000000".to_vec()
                    } else {
                        let mut name = result
                            .get(second_index - 1)
                            .ok_or(CoreError::Path)?
                            .name()
                            .as_bytes()
                            .to_vec();
                        name.push(b'x');
                        name
                    };
                    result.insert(
                        second_index,
                        CanonicalTreeEntryV1::new(
                            ValidatedComponent::new(&mutation_component_name)
                                .map_err(|_| CoreError::Name)?,
                            removed.child(),
                        ),
                    );
                }
                TreeMutationActionV1::ReplacePair => {
                    if index == second_index
                        || index >= result.len()
                        || second_index >= result.len()
                    {
                        return Err(CoreError::Path);
                    }
                    result[index] = CanonicalTreeEntryV1::new(
                        result[index].name(),
                        symlink_child(request.changed_tag),
                    );
                    result[second_index] =
                        CanonicalTreeEntryV1::new(result[second_index].name(), symlink_child(7));
                }
                TreeMutationActionV1::Replace => unreachable!(),
            }
            result
        };
        let rebuilt = if request.fault == TreeMutationFaultV1::DuplicateResult {
            None
        } else {
            Some(build_tree(
                DirectoryBuildModeV1::ImplicitRoot,
                &result_entries,
            )?)
        };
        let mutation_index = match request.action {
            TreeMutationActionV1::Move | TreeMutationActionV1::ReplacePair => {
                index.min(second_index)
            }
            _ => index,
        };
        let mut evidence = mutation_evidence(
            DirectoryBuildModeV1::ImplicitRoot,
            &base_entries,
            &base,
            mutation_index,
        );
        if matches!(
            request.action,
            TreeMutationActionV1::Move | TreeMutationActionV1::ReplacePair
        ) {
            add_secondary_leaf_group(&mut evidence, &base_entries, &base, index.max(second_index));
        }
        match request.fault {
            TreeMutationFaultV1::CorruptOldHead => {
                if let Some(first) = evidence.old_head.first_mut() {
                    *first ^= 1;
                }
            }
            TreeMutationFaultV1::CorruptMiddle => evidence.middle.swap(0, 1),
            TreeMutationFaultV1::CorruptTail => {
                if let Some(first) = evidence.old_tail_prefix.first_mut() {
                    *first ^= 1;
                }
            }
            TreeMutationFaultV1::CorruptLeafSiblings => evidence.leaf_group.swap(0, 1),
            TreeMutationFaultV1::CorruptLevelOneSiblings => evidence.level_one_group.swap(0, 1),
            _ => {}
        }
        let proof_node_count = evidence.middle.len() as u32;
        let mut sink = MutationSink {
            resident_bytes: request.sink_resident_bytes,
            ..MutationSink::default()
        };
        let mut source = MutationSource {
            base: &base_entries,
            result: &result_entries,
            resident_bytes: request.source_resident_bytes,
            base_reads: 0,
            result_reads: 0,
        };
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut control = ContinueCowControlV1;
        let outcome = match request.action {
            TreeMutationActionV1::Add => add_directory_entry_cow_v1(
                base.directory,
                mutation_evidence_value(&evidence, base.directory),
                index,
                result_entries[index],
                &mut source,
                &mut sink,
                &ledger,
                &mut counters,
                &mut control,
                &mut [0_u8; MAX_TREE_OBJECT_BYTES],
                &mut [0_u8; COMPARISON_WINDOW_BYTES],
                &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
            ),
            TreeMutationActionV1::Remove => remove_directory_entry_cow_v1(
                base.directory,
                mutation_evidence_value(&evidence, base.directory),
                index,
                base_entries[index],
                &mut source,
                &mut sink,
                &ledger,
                &mut counters,
                &mut control,
                &mut [0_u8; MAX_TREE_OBJECT_BYTES],
                &mut [0_u8; COMPARISON_WINDOW_BYTES],
                &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
            ),
            TreeMutationActionV1::Move => move_directory_entry_cow_independent_v1(
                base.directory,
                mutation_evidence_value(&evidence, base.directory),
                index,
                second_index,
                base_entries[index],
                result_entries[second_index],
                &mut source,
                &mut sink,
                &ledger,
                &mut counters,
                &mut control,
                &mut [0_u8; MAX_TREE_OBJECT_BYTES],
                &mut [0_u8; COMPARISON_WINDOW_BYTES],
                &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
            ),
            TreeMutationActionV1::ReplacePair => replace_two_directory_entries_cow_independent_v1(
                base.directory,
                mutation_evidence_value(&evidence, base.directory),
                index,
                base_entries[index],
                result_entries[index],
                second_index,
                base_entries[second_index],
                result_entries[second_index],
                &mut source,
                &mut sink,
                &ledger,
                &mut counters,
                &mut control,
                &mut [0_u8; MAX_TREE_OBJECT_BYTES],
                &mut [0_u8; COMPARISON_WINDOW_BYTES],
                &mut [None; MAX_COW_TREE_PAGE_SUMMARIES],
            ),
            TreeMutationActionV1::Replace => unreachable!(),
        };
        let error = outcome.as_ref().err().copied();
        let (logical_matches_rebuild, physical_matches_rebuild) =
            match (outcome.as_ref().ok(), rebuilt.as_ref()) {
                (Some(cow), Some(rebuilt)) => (
                    cow.directory().logical() == rebuilt.directory.logical(),
                    cow.directory().physical() == rebuilt.directory.physical(),
                ),
                _ => (false, false),
            };
        let first_changed_leaf = outcome
            .as_ref()
            .map(|cow| cow.first_changed_leaf())
            .unwrap_or(0);
        let unchanged_leaf_reused = outcome
            .as_ref()
            .map(|cow| {
                request.action == TreeMutationActionV1::Add
                    && request.index != 0
                    && cow.structurally_reused_leaves() > 0
            })
            .unwrap_or(false);
        let (directory_logical_differs_from_base, directory_physical_differs_from_base) = outcome
            .as_ref()
            .map(|cow| {
                (
                    cow.directory().logical() != base.directory.logical(),
                    cow.directory().physical() != base.directory.physical(),
                )
            })
            .unwrap_or((false, false));
        Ok(make_observation(
            request,
            Some(&result_entries),
            &base,
            rebuilt.as_ref(),
            error,
            &sink,
            Some(&source),
            &counters,
            ledger.admitted_slots(),
            0,
            proof_node_count,
            evidence.middle.len() as u32,
            !evidence.old_tail_prefix.is_empty(),
            evidence
                .affected_leaf
                .map(|leaf| leaf.subtree_entry_count())
                .unwrap_or(0),
            evidence.leaf_group.len() as u32,
            evidence.level_one_group.len() as u32,
            0,
            first_changed_leaf,
            outcome
                .as_ref()
                .map(|cow| cow.last_changed_leaf())
                .unwrap_or(0),
            outcome
                .as_ref()
                .map(|cow| cow.changed_leaves())
                .unwrap_or(0),
            outcome
                .as_ref()
                .map(|cow| cow.changed_level_one())
                .unwrap_or(0),
            false,
            directory_logical_differs_from_base,
            directory_physical_differs_from_base,
            unchanged_leaf_reused,
            outcome
                .as_ref()
                .map(|cow| cow.emitted_objects())
                .unwrap_or(0),
            outcome
                .as_ref()
                .map(|cow| cow.structurally_reused_leaves())
                .unwrap_or(0),
            outcome
                .as_ref()
                .map(|cow| cow.structurally_reused_level_one())
                .unwrap_or(0),
            logical_matches_rebuild,
            physical_matches_rebuild,
            false,
        ))
    }

    pub fn file_replacement_v1(
        index: u32,
        old_bytes: [u8; 3],
        new_bytes: [u8; 3],
    ) -> CoreResult<TreeMutationObservationV1> {
        let count = 300;
        let index = usize::try_from(index).map_err(|_| CoreError::IntegerOverflow)?;
        let names = names(count);
        let mut base_entries = entries(&names, symlink_child(0));
        let stable = file_child(0);
        let old = file_child_with_partition(&old_bytes, &[&old_bytes]);
        let new = file_child_with_partition(&new_bytes, &[&new_bytes]);
        base_entries[0] = CanonicalTreeEntryV1::new(
            ValidatedComponent::new(&names[0]).map_err(|_| CoreError::Name)?,
            stable,
        );
        base_entries[index] = CanonicalTreeEntryV1::new(
            ValidatedComponent::new(&names[index]).map_err(|_| CoreError::Name)?,
            old,
        );
        let base = build_tree(DirectoryBuildModeV1::ImplicitRoot, &base_entries)?;
        let mut updated = base_entries.clone();
        updated[index] = CanonicalTreeEntryV1::new(
            ValidatedComponent::new(&names[index]).map_err(|_| CoreError::Name)?,
            new,
        );
        let rebuilt = build_tree(DirectoryBuildModeV1::ImplicitRoot, &updated)?;
        let evidence = replacement_evidence(
            DirectoryBuildModeV1::ImplicitRoot,
            &base_entries,
            &base,
            index,
        );
        let mut sink = MutationSink::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut control = ContinueCowControlV1;
        let outcome = replace_directory_entry_cow_v1(
            base.directory,
            replacement_evidence_value(&evidence, base.directory),
            index,
            updated[index],
            &mut sink,
            &ledger,
            &mut counters,
            &mut control,
            &mut [0_u8; MAX_TREE_OBJECT_BYTES],
            &mut [0_u8; COMPARISON_WINDOW_BYTES],
        )?;
        Ok(make_observation(
            TreeMutationRequestV1::replace(count as u32, index as u32),
            Some(&updated),
            &base,
            Some(&rebuilt),
            None,
            &sink,
            None,
            &counters,
            ledger.admitted_slots(),
            evidence.old_window.len() as u64,
            (evidence.prefix.len() + evidence.suffix.len()) as u32,
            0,
            false,
            evidence.affected_entries.len() as u32,
            evidence.leaf_group.len() as u32,
            evidence.level_one_group.len() as u32,
            outcome.changed_leaf_index(),
            0,
            0,
            0,
            0,
            outcome.changed_leaf().id() != base.leaves[outcome.changed_leaf_index() as usize].id(),
            outcome.directory().logical() != base.directory.logical(),
            outcome.directory().physical() != base.directory.physical(),
            false,
            sink.committed.len() as u32,
            counters.tree_nodes_reused as u32,
            0,
            outcome.directory().logical() == rebuilt.directory.logical(),
            outcome.directory().physical() == rebuilt.directory.physical(),
            true,
        ))
    }

    pub fn identity_v1() -> CoreResult<TreeIdentityObservationV1> {
        let names = vec![b"file".to_vec()];
        let one = entries(&names, file_child_with_partition(b"ab", &[b"ab"]));
        let two = entries(&names, file_child_with_partition(b"ab", &[b"a", b"b"]));
        let first = build_tree(DirectoryBuildModeV1::ImplicitRoot, &one)?;
        let second = build_tree(DirectoryBuildModeV1::ImplicitRoot, &two)?;
        Ok(TreeIdentityObservationV1 {
            logical_equal: first.directory.logical() == second.directory.logical(),
            physical_ids_differ: first.directory.physical() != second.directory.physical(),
            implicit_root: matches!(
                first.directory.logical(),
                DirectoryLogicalIdentityV1::ImplicitRoot(_)
            ),
            first_build_ledger_admitted_slots: first.ledger_admitted_slots,
            second_build_ledger_admitted_slots: second.ledger_admitted_slots,
        })
    }
}
