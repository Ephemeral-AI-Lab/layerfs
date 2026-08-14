mod support;

#[cfg(feature = "operation-polymorphism")]
mod cas_admission_owner {
    use core::cmp::Ordering;
    use std::fs;

    use crate::support::temp_fs_cas::TempFsCas;
    use layerfs_storage::format::{ValidatedComponent, ValidatedSymlinkTarget};
    use layerfs_storage::identity::{
        derive_file_node_v1, derive_implicit_root_directory_v1, derive_logical_chunk_v1,
        derive_logical_file_v1, derive_physical_chunk_id_v1, derive_physical_file_id_v1,
        derive_physical_symlink_id_v1, derive_physical_tree_id_v1,
        derive_physical_version_record_id_v1, derive_symlink_node_v1, derive_version_v1,
        LogicalChildIdV1, LogicalChunkRefV1, LogicalDirectoryEntryV1,
        DEFERRED_COUNT_LOGICAL_FILE_HASHER_BYTES_V1,
    };
    use layerfs_storage::object::TypedPhysicalObjectIdV1;
    use layerfs_storage::profile::{ChunkerSpecV1, DigestSpecV1, ProfileSpecV1};
    use layerfs_storage::qualification::cas::semantic::{
        admit_v1, atomic_catalog_classification_v1, atomic_catalog_malformed_occupant_v1,
        cancel_after_locator_publication_v1, catalog_publication_io_failure_v1,
        closure_capability_binding_v1, closure_capability_failure_v1,
        closure_fence_counter_overflow_v1, closure_fence_io_failure_v1,
        closure_read_counter_overflow_v1, closure_validation_failure_v1,
        complete_carrier_backed_closure_v1, every_fresh_admission_boundary_v1,
        every_incumbent_boundary_v1, existing_catalog_classification_v1,
        malformed_carrier_directory_v1, malformed_object_locator_v1, transfer_pack_then_reopen_v1,
        unequal_incumbent_bytes_v1, valid_locator_binding_mismatches_v1, AdmissionRequestV1,
        AtomicCatalogCaseV1, ClosureCapabilityFailureCaseV1, ExistingCatalogCaseV1,
        PublicationErrorV1, PublicationRequestV1,
    };
    #[cfg(unix)]
    use layerfs_storage::qualification::cas::semantic::{
        post_comparison_locator_replacement_v1, symlinked_parent_namespace_creation_v1,
    };
    use layerfs_storage::qualification::pack::semantic::{
        build_v1, operation_slot_bytes, validate_v1, PackRequestV1, ValidationRequestV1,
    };
    use layerfs_storage::CoreError;

    fn object_kind(kind: u8, payload: &[u8]) -> Vec<u8> {
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

    fn object(payload: &[u8]) -> Vec<u8> {
        object_kind(5, payload)
    }

    fn empty_closure() -> [Vec<u8>; 2] {
        let root = object_kind(2, &[1, 0x10, 0, 0, 0, 0, 0, 0, 0]);
        let root_id = derive_physical_tree_id_v1(&root).unwrap();
        let logical_root = derive_implicit_root_directory_v1(&[]).unwrap();
        let mut payload = Vec::with_capacity(184);
        payload.extend_from_slice(derive_version_v1(logical_root).as_bytes());
        payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
        payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
        payload.extend_from_slice(root_id.as_bytes());
        payload.extend_from_slice(&0_u64.to_be_bytes());
        payload.extend_from_slice(&0_u64.to_be_bytes());
        for count in [0_u32, 1, 0, 0, 0, 0, 0, 2] {
            payload.extend_from_slice(&count.to_be_bytes());
        }
        payload.extend_from_slice(&0_u64.to_be_bytes());
        assert_eq!(payload.len(), 184);
        [object_kind(1, &payload), root]
    }

    fn single_symlink_closure() -> [Vec<u8>; 4] {
        let target = b"destination";
        let mut symlink_payload = Vec::with_capacity(4 + target.len());
        symlink_payload.extend_from_slice(&(target.len() as u32).to_be_bytes());
        symlink_payload.extend_from_slice(target);
        let symlink = object_kind(4, &symlink_payload);
        let symlink_id = derive_physical_symlink_id_v1(&symlink).unwrap();

        let name = b"link";
        let mut leaf_payload = vec![2, 0];
        leaf_payload.extend_from_slice(&1_u16.to_be_bytes());
        leaf_payload.extend_from_slice(&(name.len() as u16).to_be_bytes());
        leaf_payload.extend_from_slice(name);
        leaf_payload.push(3);
        leaf_payload.extend_from_slice(symlink_id.as_bytes());
        let leaf = object_kind(2, &leaf_payload);
        let leaf_id = derive_physical_tree_id_v1(&leaf).unwrap();

        let mut root_payload = vec![1];
        root_payload.extend_from_slice(&0x1000_u16.to_be_bytes());
        root_payload.extend_from_slice(&1_u32.to_be_bytes());
        root_payload.push(0);
        root_payload.push(1);
        root_payload.extend_from_slice(leaf_id.as_bytes());
        let root = object_kind(2, &root_payload);
        let root_id = derive_physical_tree_id_v1(&root).unwrap();

        let logical_symlink =
            derive_symlink_node_v1(ValidatedSymlinkTarget::new(target).unwrap()).unwrap();
        let logical_root = derive_implicit_root_directory_v1(&[LogicalDirectoryEntryV1::new(
            ValidatedComponent::new(name).unwrap(),
            LogicalChildIdV1::Symlink(logical_symlink),
        )])
        .unwrap();
        let mut version_payload = Vec::with_capacity(184);
        version_payload.extend_from_slice(derive_version_v1(logical_root).as_bytes());
        version_payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
        version_payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
        version_payload.extend_from_slice(root_id.as_bytes());
        version_payload.extend_from_slice(&0_u64.to_be_bytes());
        version_payload.extend_from_slice(&0_u64.to_be_bytes());
        for count in [1_u32, 2, 0, 1, 0, 0, 0, 4] {
            version_payload.extend_from_slice(&count.to_be_bytes());
        }
        version_payload.extend_from_slice(&0_u64.to_be_bytes());
        [object_kind(1, &version_payload), root, leaf, symlink]
    }

    fn typed_id(bytes: &[u8]) -> TypedPhysicalObjectIdV1 {
        match bytes[10] {
            1 => TypedPhysicalObjectIdV1::VersionRecord(
                derive_physical_version_record_id_v1(bytes).unwrap(),
            ),
            2 => TypedPhysicalObjectIdV1::Tree(derive_physical_tree_id_v1(bytes).unwrap()),
            3 => TypedPhysicalObjectIdV1::File(derive_physical_file_id_v1(bytes).unwrap()),
            4 => TypedPhysicalObjectIdV1::Symlink(derive_physical_symlink_id_v1(bytes).unwrap()),
            5 => TypedPhysicalObjectIdV1::Chunk(derive_physical_chunk_id_v1(bytes).unwrap()),
            kind => panic!("unexpected physical object kind {kind}"),
        }
    }

    fn typed_rank(id: TypedPhysicalObjectIdV1) -> u8 {
        match id {
            TypedPhysicalObjectIdV1::VersionRecord(_) => 1,
            TypedPhysicalObjectIdV1::Tree(_) => 2,
            TypedPhysicalObjectIdV1::File(_) => 3,
            TypedPhysicalObjectIdV1::Symlink(_) => 4,
            TypedPhysicalObjectIdV1::Chunk(_) => 5,
        }
    }

    fn canonicalize(objects: &mut [(TypedPhysicalObjectIdV1, Vec<u8>)]) {
        objects.sort_by(|left, right| {
            typed_rank(left.0)
                .cmp(&typed_rank(right.0))
                .then_with(|| left.0.as_bytes().cmp(right.0.as_bytes()))
        });
    }

    fn rechunked_file_closure() -> Vec<(TypedPhysicalObjectIdV1, Vec<u8>)> {
        let source = b"logical bytes reconstructed across physical chunks";
        let split = 9;
        let mut objects = Vec::new();
        let mut chunk_ids = Vec::new();
        for payload in [&source[..split], &source[split..]] {
            let bytes = object_kind(5, payload);
            let id = derive_physical_chunk_id_v1(&bytes).unwrap();
            chunk_ids.push((id, payload.len()));
            objects.push((TypedPhysicalObjectIdV1::Chunk(id), bytes));
        }

        let mut file_payload = Vec::new();
        file_payload.extend_from_slice(&0o644_u16.to_be_bytes());
        file_payload.extend_from_slice(&(source.len() as u64).to_be_bytes());
        file_payload.extend_from_slice(&1_u32.to_be_bytes());
        file_payload.push(2);
        file_payload.extend_from_slice(&(source.len() as u64).to_be_bytes());
        file_payload.extend_from_slice(&2_u32.to_be_bytes());
        for (id, len) in &chunk_ids {
            file_payload.extend_from_slice(&(*len as u32).to_be_bytes());
            file_payload.extend_from_slice(id.as_bytes());
        }
        let file = object_kind(3, &file_payload);
        let file_id = derive_physical_file_id_v1(&file).unwrap();
        objects.push((TypedPhysicalObjectIdV1::File(file_id), file));

        let name = b"file";
        let mut leaf_payload = vec![2, 0];
        leaf_payload.extend_from_slice(&1_u16.to_be_bytes());
        leaf_payload.extend_from_slice(&(name.len() as u16).to_be_bytes());
        leaf_payload.extend_from_slice(name);
        leaf_payload.push(2);
        leaf_payload.extend_from_slice(file_id.as_bytes());
        let leaf = object_kind(2, &leaf_payload);
        let leaf_id = derive_physical_tree_id_v1(&leaf).unwrap();
        objects.push((TypedPhysicalObjectIdV1::Tree(leaf_id), leaf));

        let mut root_payload = vec![1];
        root_payload.extend_from_slice(&0x1000_u16.to_be_bytes());
        root_payload.extend_from_slice(&1_u32.to_be_bytes());
        root_payload.push(0);
        root_payload.push(1);
        root_payload.extend_from_slice(leaf_id.as_bytes());
        let root = object_kind(2, &root_payload);
        let root_id = derive_physical_tree_id_v1(&root).unwrap();
        objects.push((TypedPhysicalObjectIdV1::Tree(root_id), root));

        let logical_chunk = derive_logical_chunk_v1(source).unwrap();
        let logical_file = derive_logical_file_v1(
            source.len() as u64,
            &[LogicalChunkRefV1::from_identity(logical_chunk)],
        )
        .unwrap();
        let logical_root = derive_implicit_root_directory_v1(&[LogicalDirectoryEntryV1::new(
            ValidatedComponent::new(name).unwrap(),
            LogicalChildIdV1::File(derive_file_node_v1(0o644, logical_file).unwrap()),
        )])
        .unwrap();
        let mut version_payload = Vec::with_capacity(184);
        version_payload.extend_from_slice(derive_version_v1(logical_root).as_bytes());
        version_payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
        version_payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
        version_payload.extend_from_slice(root_id.as_bytes());
        version_payload.extend_from_slice(&0_u64.to_be_bytes());
        version_payload.extend_from_slice(&(source.len() as u64).to_be_bytes());
        for count in [1_u32, 2, 1, 0, 2, 1, 2, 6] {
            version_payload.extend_from_slice(&count.to_be_bytes());
        }
        version_payload.extend_from_slice(&(source.len() as u64).to_be_bytes());
        let version = object_kind(1, &version_payload);
        objects.push((typed_id(&version), version));
        canonicalize(&mut objects);
        objects
    }

    fn indexed_symlink_closure() -> Vec<(TypedPhysicalObjectIdV1, Vec<u8>)> {
        let target = b"shared-target";
        let mut symlink_payload = Vec::with_capacity(4 + target.len());
        symlink_payload.extend_from_slice(&(target.len() as u32).to_be_bytes());
        symlink_payload.extend_from_slice(target);
        let symlink = object_kind(4, &symlink_payload);
        let symlink_id = derive_physical_symlink_id_v1(&symlink).unwrap();
        let names: Vec<Vec<u8>> = (0..193)
            .map(|index| format!("n{index:03}").into_bytes())
            .collect();
        let mut leaves = Vec::new();
        for range in [0..192, 192..193] {
            let mut payload = vec![2, 0];
            payload.extend_from_slice(&(range.len() as u16).to_be_bytes());
            for name in &names[range.clone()] {
                payload.extend_from_slice(&(name.len() as u16).to_be_bytes());
                payload.extend_from_slice(name);
                payload.push(3);
                payload.extend_from_slice(symlink_id.as_bytes());
            }
            let bytes = object_kind(2, &payload);
            leaves.push((derive_physical_tree_id_v1(&bytes).unwrap(), bytes, range));
        }

        let mut index_payload = vec![3, 1];
        index_payload.extend_from_slice(&2_u16.to_be_bytes());
        for (id, _, range) in &leaves {
            let first = &names[range.start];
            let last = &names[range.end - 1];
            index_payload.extend_from_slice(&(range.len() as u32).to_be_bytes());
            index_payload.extend_from_slice(&(first.len() as u16).to_be_bytes());
            index_payload.extend_from_slice(first);
            index_payload.extend_from_slice(&(last.len() as u16).to_be_bytes());
            index_payload.extend_from_slice(last);
            index_payload.extend_from_slice(id.as_bytes());
        }
        let index = object_kind(2, &index_payload);
        let index_id = derive_physical_tree_id_v1(&index).unwrap();
        let mut root_payload = vec![1];
        root_payload.extend_from_slice(&0x1000_u16.to_be_bytes());
        root_payload.extend_from_slice(&193_u32.to_be_bytes());
        root_payload.push(1);
        root_payload.push(1);
        root_payload.extend_from_slice(index_id.as_bytes());
        let root = object_kind(2, &root_payload);
        let root_id = derive_physical_tree_id_v1(&root).unwrap();

        let logical_symlink =
            derive_symlink_node_v1(ValidatedSymlinkTarget::new(target).unwrap()).unwrap();
        let logical_entries: Vec<_> = names
            .iter()
            .map(|name| {
                LogicalDirectoryEntryV1::new(
                    ValidatedComponent::new(name).unwrap(),
                    LogicalChildIdV1::Symlink(logical_symlink),
                )
            })
            .collect();
        let logical_root = derive_implicit_root_directory_v1(&logical_entries).unwrap();
        let mut version_payload = Vec::with_capacity(184);
        version_payload.extend_from_slice(derive_version_v1(logical_root).as_bytes());
        version_payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
        version_payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
        version_payload.extend_from_slice(root_id.as_bytes());
        version_payload.extend_from_slice(&0_u64.to_be_bytes());
        version_payload.extend_from_slice(&0_u64.to_be_bytes());
        for count in [193_u32, 4, 0, 1, 0, 0, 0, 6] {
            version_payload.extend_from_slice(&count.to_be_bytes());
        }
        version_payload.extend_from_slice(&0_u64.to_be_bytes());
        let version = object_kind(1, &version_payload);
        let mut objects = vec![
            (typed_id(&version), version),
            (TypedPhysicalObjectIdV1::Tree(root_id), root),
            (TypedPhysicalObjectIdV1::Tree(index_id), index),
            (TypedPhysicalObjectIdV1::Symlink(symlink_id), symlink),
        ];
        objects.extend(
            leaves
                .into_iter()
                .map(|(id, bytes, _)| (TypedPhysicalObjectIdV1::Tree(id), bytes)),
        );
        canonicalize(&mut objects);
        objects
    }

    fn large_valid_file(reference_count: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0o644_u16.to_be_bytes());
        payload.extend_from_slice(&u64::from(reference_count).to_be_bytes());
        payload.extend_from_slice(&1_u32.to_be_bytes());
        payload.push(2);
        payload.extend_from_slice(&u64::from(reference_count).to_be_bytes());
        payload.extend_from_slice(&reference_count.to_be_bytes());
        for _ in 0..reference_count {
            payload.extend_from_slice(&1_u32.to_be_bytes());
            payload.extend_from_slice(&[0x55; 32]);
        }
        object_kind(3, &payload)
    }

    fn refs(objects: &[Vec<u8>]) -> Vec<&[u8]> {
        objects.iter().map(Vec::as_slice).collect()
    }

    fn typed_refs(objects: &[(TypedPhysicalObjectIdV1, Vec<u8>)]) -> Vec<&[u8]> {
        objects.iter().map(|(_, bytes)| bytes.as_slice()).collect()
    }

    fn be_u64(bytes: &[u8]) -> u64 {
        u64::from_be_bytes(bytes.try_into().unwrap())
    }

    fn reseal(pack: &mut [u8]) {
        let checksum_at = pack.len() - 32;
        let mut frame = [0_u8; 20];
        frame[..8].copy_from_slice(b"ELSHASH1");
        frame[8] = 0x20;
        frame[12..].copy_from_slice(&(checksum_at as u64).to_be_bytes());
        let mut hasher = blake3::Hasher::new();
        hasher.update(&frame);
        hasher.update(&pack[..checksum_at]);
        pack[checksum_at..].copy_from_slice(hasher.finalize().as_bytes());
    }

    fn framed_digest(tag: u8, bytes: &[u8]) -> [u8; 32] {
        let mut frame = [0_u8; 20];
        frame[..8].copy_from_slice(b"ELSHASH1");
        frame[8] = tag;
        frame[12..].copy_from_slice(&(bytes.len() as u64).to_be_bytes());
        let mut hasher = blake3::Hasher::new();
        hasher.update(&frame);
        hasher.update(bytes);
        *hasher.finalize().as_bytes()
    }

    fn independently_validate(bytes: &[u8]) -> Result<(), CoreError> {
        match validate_v1(ValidationRequestV1::new(bytes)).error() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn assert_build_custody(
        source_payload_bytes_read: u64,
        admitted_slots: u64,
        objects: &[&[u8]],
    ) {
        assert_eq!(
            source_payload_bytes_read,
            objects.iter().map(|value| value.len() as u64).sum::<u64>(),
            "each source object is copied into the private pack exactly once"
        );
        assert_eq!(admitted_slots, 0);
    }

    #[test]
    fn pack_is_transferred_once_then_reopened_through_committed_catalog() {
        let root = TempFsCas::new("pack-transfer");
        let objects = [object(b"first"), object(b"second")];
        let refs = refs(&objects);
        let observation =
            transfer_pack_then_reopen_v1(PublicationRequestV1::new(root.path(), &refs));

        assert!(observation.fixed_handles_within_budget());
        assert_eq!(observation.installed(), true);
        assert_eq!(observation.pack_len(), 432);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.bytes_written(), 0);
        assert!(observation.bytes_read() > 0);
        assert!(observation.read_calls() > 0);
        assert_eq!(observation.catalog_operations(), 1);
        assert_eq!(
            observation.installed_carrier_logical_bytes(),
            observation.pack_len()
        );
        assert!(observation.zero_forbidden_work());
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(observation.reopened_lengths_match(), true);
        assert_eq!(observation.reopened_bytes_match(), true);
        assert!(observation.reopened_read_calls() > 0);
        assert!(observation.reopened_bytes_read() >= observation.expected_object_bytes());
        assert_eq!(
            observation.source_payload_bytes_read(),
            observation.expected_source_payload_bytes()
        );
    }

    #[test]
    fn valid_locator_binding_mismatches_are_integrity_not_malformed_bytes() {
        let root = TempFsCas::new("locator-binding");
        let observation = valid_locator_binding_mismatches_v1(root.path());
        for case in observation.binding_cases() {
            assert_eq!(case.read_error(), Some(PublicationErrorV1::Integrity));
            assert_eq!(case.admission_error(), Some(PublicationErrorV1::Integrity));
            assert_eq!(case.admitted_slots(), 0);
            assert!(case.zero_forbidden_work());
        }
        assert_eq!(
            observation.reuse_error(),
            Some(PublicationErrorV1::Integrity)
        );
        assert_eq!(observation.reuse_admitted_slots(), 0);
        assert!(observation.reuse_zero_forbidden_work());
    }

    #[test]
    fn nonexistent_objects_cannot_mint_a_closure_capability() {
        let root = TempFsCas::new("closure-nonexistent");
        let observation = closure_capability_failure_v1(
            root.path(),
            ClosureCapabilityFailureCaseV1::NonexistentObjects,
        );
        assert_eq!(observation.error(), Some(CoreError::SinkRefused));
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn spoofed_closure_bytes_cannot_mint_a_capability() {
        let root = TempFsCas::new("closure-spoofed");
        let observation = closure_capability_failure_v1(
            root.path(),
            ClosureCapabilityFailureCaseV1::SpoofedBytes,
        );
        assert_eq!(observation.error(), Some(CoreError::IdMismatch));
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn duplicate_typed_ids_cannot_enter_the_closure_transcript() {
        let root = TempFsCas::new("closure-duplicate-id");
        let observation = closure_capability_failure_v1(
            root.path(),
            ClosureCapabilityFailureCaseV1::DuplicateTypedIds,
        );
        assert_eq!(observation.error(), Some(CoreError::NonCanonicalOrder));
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn wrong_version_record_cannot_mint_a_closure_capability() {
        let root = TempFsCas::new("closure-wrong-version");
        let observation = closure_capability_failure_v1(
            root.path(),
            ClosureCapabilityFailureCaseV1::WrongVersionRecord,
        );
        assert_eq!(observation.error(), Some(CoreError::MissingClosureEdge));
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn forced_equal_typed_id_with_unequal_incumbent_bytes_fails_closed() {
        let root = TempFsCas::new("forced-object-id-collision");
        let observation = unequal_incumbent_bytes_v1(root.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::IdMismatch))
        );
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.object_entries(), 2);
        if !observation.loser_locator_absent() || !observation.incumbent_preserved() {
            panic!("loser locator survived or incumbent namespace changed");
        }
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn malformed_object_locator_fails_closed_without_publishing_the_loser() {
        let root = TempFsCas::new("malformed-object-locator");
        let observation = malformed_object_locator_v1(root.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::MalformedOccupant)
        );
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.residue_bytes(), 0);
        assert!(observation.invalidated());
        assert!(matches!(observation.owner_handle_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[cfg(unix)]
    #[test]
    fn post_comparison_locator_path_replacement_fails_before_catalog_publication() {
        let root = TempFsCas::new("post-comparison-locator-replacement");
        let observation = post_comparison_locator_replacement_v1(root.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::Integrity));
        assert!(observation.fault_injected());
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(
            observation.storage_bytes_request_matches_reservation(),
            true
        );
        assert_eq!(
            observation.storage_inodes_request_matches_reservation(),
            true
        );
        assert_eq!(
            observation.storage_bytes_terminal_sum_matches_reservation(),
            true
        );
        assert_eq!(
            observation.storage_inodes_terminal_sum_matches_reservation(),
            true
        );
        assert_eq!(observation.storage_bytes_retained(), 0);
        assert_eq!(observation.storage_inodes_retained(), 0);
        assert!(observation.zero_forbidden_work());
        assert!(matches!(observation.owner_handle_invalidated(), true));
        assert!(matches!(observation.stale_handle_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn atomic_catalog_no_replace_authenticates_a_racing_malformed_occupant() {
        let root = TempFsCas::new("atomic-catalog-race");
        let observation = atomic_catalog_malformed_occupant_v1(root.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::MalformedOccupant)
        );
        assert!(observation.fault_injected());
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.object_entries(), 0);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(
            fs::read(
                fs::read_dir(root.path().join("catalog"))
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path()
            )
            .unwrap(),
            [0_u8; 64]
        );
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn atomic_catalog_no_replace_classifies_valid_binding_and_unequal_incumbents() {
        let binding_root = TempFsCas::new("atomic-catalog-binding");
        let binding = atomic_catalog_classification_v1(
            binding_root.path(),
            AtomicCatalogCaseV1::BindingMismatch,
        );
        assert_eq!(binding.donor_installed(), true);
        assert_eq!(binding.incumbent_marker_has_canonical_size(), true);
        let unequal_root = TempFsCas::new("atomic-catalog-same-id-unequal");
        let unequal = atomic_catalog_classification_v1(
            unequal_root.path(),
            AtomicCatalogCaseV1::SameIdUnequal,
        );
        for (label, observation, expected) in [
            ("binding", binding, PublicationErrorV1::Integrity),
            (
                "same-id-unequal",
                unequal,
                PublicationErrorV1::UnequalOccupant,
            ),
        ] {
            assert_ne!(
                observation.incumbent_pack_len(),
                observation.candidate_pack_len()
            );
            assert_eq!(observation.error(), Some(expected), "{label}");
            assert!(observation.fault_injected(), "{label}");
            assert_eq!(observation.preparation_entries(), 0, "{label}");
            assert_eq!(observation.carrier_entries(), 0, "{label}");
            assert_eq!(observation.object_entries(), 0, "{label}");
            assert_eq!(observation.catalog_entries(), 1, "{label}");
            assert_eq!(observation.incumbent_preserved(), true, "{label}");
            assert_eq!(observation.residue_bytes(), 0, "{label}");
            assert_eq!(observation.admitted_slots(), 0, "{label}");
            assert!(observation.zero_forbidden_work(), "{label}");
        }
    }

    #[test]
    fn existing_catalog_classifies_valid_binding_and_unequal_incumbents() {
        let root = TempFsCas::new("existing-catalog");
        let object = object(b"existing canonical catalog candidate");
        let objects = [object.as_slice()];
        for (case, expected) in [
            (
                ExistingCatalogCaseV1::BindingMismatch,
                PublicationErrorV1::Integrity,
            ),
            (
                ExistingCatalogCaseV1::SameIdUnequal,
                PublicationErrorV1::UnequalOccupant,
            ),
        ] {
            let observation = existing_catalog_classification_v1(
                PublicationRequestV1::new(root.path(), &objects),
                case,
            );
            assert_eq!(observation.incumbent_installed(), true);
            assert_eq!(observation.error(), Some(expected));
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(observation.carrier_entries(), 1);
            assert_eq!(observation.object_entries(), 1);
            assert_eq!(observation.catalog_entries(), 1);
            assert_eq!(observation.marker_preserved(), true);
            assert_eq!(observation.unreachable_installed_residue_bytes(), 0);
            assert_eq!(observation.admitted_slots(), 0);
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn every_fresh_admission_boundary_cleans_or_counts_exact_residue() {
        let root = TempFsCas::new("fresh-admission-boundaries");
        let observation = every_fresh_admission_boundary_v1(root.path());
        for case in observation.cases() {
            assert_eq!(case.pack_len(), 296);
            assert_eq!(
                case.error(),
                Some(PublicationErrorV1::Core(CoreError::Cancelled))
            );
            assert_eq!(case.admitted_slots(), 0);
            assert_eq!(case.preparation_entries(), 0);
            assert_eq!(case.closure_entries(), 0);
            if case.after_catalog_publication() {
                assert_eq!(case.carrier_entries(), 1);
                assert_eq!(case.catalog_entries(), 1);
                assert_eq!(case.object_entries(), 1);
                assert_eq!(case.residue_bytes(), case.expected_residue_bytes());
                assert_eq!(case.bytes_written(), 0);
            } else {
                assert_eq!(case.carrier_entries(), 0);
                assert_eq!(case.catalog_entries(), 0);
                assert_eq!(case.object_entries(), 0);
                assert_eq!(case.residue_bytes(), 0);
            }
            assert_eq!(case.closure_fences(), 0);
            assert!(case.open_file_handles_high_water() <= 2);
            assert!(case.zero_forbidden_work());
        }
    }

    #[test]
    fn partial_multi_object_locator_publication_is_fully_rolled_back() {
        let root = TempFsCas::new("partial-locator-publication");
        let first = object(b"locator-a");
        let second = object(b"locator-b");
        let third = object(b"locator-c");
        let objects = [first.as_slice(), second.as_slice(), third.as_slice()];
        let observation = cancel_after_locator_publication_v1(
            PublicationRequestV1::new(root.path(), &objects)
                .with_cancel_after_locator_publication(3),
        );
        let object_entries = std::fs::read_dir(root.path().join("objects"))
            .expect("read object namespace after locator rollback")
            .count();
        if u64::try_from(object_entries).ok() != Some(observation.object_entries()) {
            panic!("locator rollback observation diverged from the object namespace");
        }

        assert_eq!(
            (
                observation.error(),
                observation.directories_observed(),
                observation.locator_publications(),
            ),
            (
                Some(PublicationErrorV1::Core(CoreError::Cancelled)),
                true,
                3
            )
        );
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.object_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.bytes_written(), 0);
        assert_eq!(
            observation.source_payload_bytes_read(),
            objects
                .iter()
                .map(|object| object.len() as u64)
                .sum::<u64>()
        );
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn every_incumbent_boundary_cleans_loser_without_changing_winner() {
        let root = TempFsCas::new("incumbent-boundaries");
        let observation = every_incumbent_boundary_v1(root.path());
        for case in observation.cases() {
            assert_eq!(
                case.error(),
                Some(PublicationErrorV1::Core(CoreError::Cancelled))
            );
            assert_eq!(case.admitted_slots(), 0);
            assert_eq!(case.preparation_entries(), 0);
            assert_eq!(case.incumbent_preserved(), true);
            assert_eq!(case.carrier_entries(), 1);
            assert_eq!(case.catalog_entries(), 1);
            assert_eq!(case.residue_bytes(), 0);
            assert_eq!(case.closure_fences(), 0);
            assert!(case.open_file_handles_high_water() <= 2);
            assert!(case.zero_forbidden_work());
        }
    }

    #[test]
    fn catalog_publication_io_fault_removes_validated_unpublished_carrier() {
        let root = TempFsCas::new("catalog-fault");
        let observation = catalog_publication_io_failure_v1(root.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::Filesystem));
        assert_eq!(observation.filesystem_write_failure(), true);
        assert!(observation.fault_injected());
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 0);
        assert!(observation.catalog_path_is_file());
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.closure_fences(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn malformed_root_owned_carrier_directory_fails_closed_without_fallback() {
        let root = TempFsCas::new("malformed-carrier-directory");
        let observation = malformed_carrier_directory_v1(root.path());
        assert_eq!(
            (observation.error(), observation.carrier_path_is_file()),
            (Some(PublicationErrorV1::MalformedOccupant), true)
        );
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert_eq!(observation.bytes_written(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn closure_catalog_is_visible_only_after_complete_carrier_backed_validation() {
        let root = TempFsCas::new("closure-fence");
        let observation = complete_carrier_backed_closure_v1(root.path());
        assert_eq!(observation.installed(), true);
        assert_eq!(observation.pack_len(), 616);
        assert_eq!(observation.invisible_before_validation(), true);
        assert_eq!(observation.version_record_matches(), true);
        assert_eq!(observation.object_count(), 2);
        assert_eq!(observation.created_count(), 0);
        assert_eq!(observation.reused_count(), 2);
        assert_eq!(observation.capability_version_matches(), true);
        assert_eq!(observation.capability_object_count(), 2);
        assert_eq!(observation.closure_entries(), 1);
        assert_eq!(observation.closure_fences(), 1);
        assert!(observation.bytes_read() > 0);
        assert!(observation.fscas_bytes_read_delta() > 0);
        assert!(observation.fscas_read_calls_delta() > 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn closure_counter_overflow_precedes_the_complete_fence() {
        let root = TempFsCas::new("closure-counter-overflow");
        let observation = closure_fence_counter_overflow_v1(root.path());
        assert_eq!(observation.error(), Some(CoreError::IntegerOverflow));
        assert_eq!(observation.closure_fences(), u64::MAX);
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn fscas_read_counter_overflow_precedes_the_complete_fence() {
        let root = TempFsCas::new("closure-read-counter-overflow");
        let observation = closure_read_counter_overflow_v1(root.path());
        assert_eq!(observation.error(), Some(CoreError::IntegerOverflow));
        assert_eq!(observation.fscas_bytes_read(), u64::MAX);
        assert_eq!(observation.fscas_read_calls(), 0);
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn closure_capability_rejects_cross_fscas_cross_operation_and_replay() {
        let root = TempFsCas::new("closure-capability-binding");
        let observation = closure_capability_binding_v1(root.path());
        assert_eq!(
            observation.cross_fscas_error(),
            Some(PublicationErrorV1::Integrity)
        );
        assert_eq!(
            observation.cross_operation_error(),
            Some(PublicationErrorV1::Integrity)
        );
        assert_eq!(
            observation.replay_error(),
            Some(PublicationErrorV1::Integrity)
        );
        assert_eq!(observation.primary_closure_entries(), 1);
        assert_eq!(observation.other_closure_entries(), 0);
        assert_eq!(observation.closure_fences(), 2);
        assert_eq!(observation.invalidation_terminal_failure(), true);
        assert_eq!(observation.invalidated_version_record(), true);
        assert_eq!(observation.invalidated_object_count(), true);
        assert_eq!(observation.invalidated_handoff(), true);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn closure_validation_failure_returns_no_closure_and_counts_installed_residue() {
        let root = TempFsCas::new("closure-validation-fault");
        let observation = closure_validation_failure_v1(root.path());
        assert_eq!(observation.error(), Some(CoreError::TypedEdge));
        assert_eq!(
            (observation.pack_len(), observation.record_count()),
            (616, 2)
        );
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(
            observation.residue_bytes(),
            observation.expected_residue_bytes()
        );
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn closure_fence_io_failure_returns_no_closure_or_publication() {
        let root = TempFsCas::new("closure-fence-fault");
        let observation = closure_fence_io_failure_v1(root.path());
        assert_eq!(observation.error(), Some(CoreError::SinkRefused));
        assert_eq!(
            (observation.pack_len(), observation.record_count()),
            (616, 2)
        );
        assert!(observation.bytes_read() > 0);
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.publication_authority_dispatches(), 0);
        assert!(observation.closure_path_is_file());
        assert_eq!(
            observation.residue_bytes(),
            observation.expected_residue_bytes()
        );
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_parent_is_typed_unsupported_before_namespace_creation() {
        let root = TempFsCas::new("symlink-parent");
        let observation = symlinked_parent_namespace_creation_v1(root.path());
        assert!(matches!(
            observation.error(),
            Some(PublicationErrorV1::Unsupported)
        ));
        if !observation.namespace_absent() {
            panic!("symlinked namespace was created");
        }
    }

    #[test]
    fn complete_empty_closure_is_visible_only_after_all_objects_validate() {
        let objects = empty_closure();
        let object_refs = refs(&objects);
        let observation = admit_v1(AdmissionRequestV1::new(&object_refs));
        assert_eq!((observation.error(), observation.object_count()), (None, 2));
        assert_eq!(observation.created_count(), 2);
        assert_eq!(observation.reused_count(), 0);
        assert_eq!(observation.sink_begun(), 2);
        assert_eq!(
            (
                observation.staged_count(),
                observation.staged_in_source_order()
            ),
            (2, true)
        );
        assert_eq!(observation.visible_expected(), true);
        assert_eq!(observation.sink_aborts(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(observation.physical_objects_created(), 0);
        assert_eq!(observation.closure_objects_missing(), 2);
        assert_eq!(observation.closure_objects_occupied_validated(), 0);
        assert_eq!(observation.publication_authority_dispatches(), 0);
    }

    #[test]
    fn admission_requires_canonical_typed_id_order_before_sink_use() {
        let [version, root] = empty_closure();
        let objects = [root.as_slice(), version.as_slice()];
        let observation = admit_v1(AdmissionRequestV1::new(&objects).with_version_ordinal(1));
        assert_eq!(observation.error(), Some(CoreError::NonCanonicalOrder));
        assert_eq!(observation.sink_begun(), 0);
        assert_eq!(observation.visible_expected(), false);
    }

    #[test]
    fn admission_counts_validation_staging_graph_and_version_reconstruction_reads() {
        let objects = empty_closure();
        let total_canonical_bytes = objects.iter().map(Vec::len).sum::<usize>() as u64;
        let object_refs = refs(&objects);
        let observation = admit_v1(AdmissionRequestV1::new(&object_refs));
        assert_eq!(
            (observation.error(), observation.bytes_read()),
            (None, 2 * total_canonical_bytes + 9)
        );
        assert_eq!(observation.bytes_copied(), total_canonical_bytes);
        assert_eq!(observation.bytes_written(), total_canonical_bytes);
    }

    #[test]
    fn nonempty_closure_reconstructs_its_logical_root_before_visibility() {
        let mut objects: Vec<_> = single_symlink_closure()
            .into_iter()
            .map(|bytes| (typed_id(&bytes), bytes))
            .collect();
        canonicalize(&mut objects);
        let object_refs = typed_refs(&objects);
        let observation = admit_v1(AdmissionRequestV1::new(&object_refs));
        assert_eq!((observation.error(), observation.object_count()), (None, 4));
        assert_eq!(observation.staged_in_source_order(), true);
        assert_eq!(observation.visible_expected(), true);
        assert!(observation.bytes_read() > 3 * (objects[0].1.len() + objects[1].1.len()) as u64);
        assert_eq!(observation.memory_high_water(), 12_582_912);
        let admission_traversal = 1_788_744;
        let planned = 2 * 65_536
            + 2 * 32_768
            + 1
            + admission_traversal
            + DEFERRED_COUNT_LOGICAL_FILE_HASHER_BYTES_V1 as usize;
        assert_eq!(observation.planned_high_water(), 8_388_608 + planned as u64);
    }

    #[test]
    fn admission_refuses_source_resident_memory_before_reading_object_bytes() {
        let objects = empty_closure();
        let object_refs = refs(&objects);
        let observation =
            admit_v1(AdmissionRequestV1::new(&object_refs).with_source_residency(4_000_000));
        assert_eq!(observation.error(), Some(CoreError::ResourceRefused));
        assert_eq!(observation.source_count_calls(), 0);
        assert_eq!(observation.source_reads(), 0);
        assert_eq!(observation.sink_begun(), 0);
    }

    #[test]
    fn admission_charges_occupied_and_sink_residency_before_any_port_operation() {
        let objects = empty_closure();
        let object_refs = refs(&objects);
        for oversized_occupied in [true, false] {
            let request = AdmissionRequestV1::new(&object_refs);
            let request = if oversized_occupied {
                request.with_occupied_residency(operation_slot_bytes())
            } else {
                request.with_sink_residency(operation_slot_bytes())
            };
            let observation = admit_v1(request);
            assert_eq!(observation.error(), Some(CoreError::ResourceRefused));
            assert_eq!(observation.source_count_calls(), 0);
            assert_eq!(observation.source_reads(), 0);
            assert_eq!(observation.occupied_lookups(), 0);
            assert_eq!(observation.sink_begun(), 0);
        }
    }

    #[test]
    fn admission_replays_logical_cdc_across_different_physical_chunking() {
        let logical_bytes = b"logical bytes reconstructed across physical chunks";
        let objects = rechunked_file_closure();
        let object_refs = typed_refs(&objects);
        let observation = admit_v1(AdmissionRequestV1::new(&object_refs));
        assert_eq!((observation.error(), observation.object_count()), (None, 6));
        assert_eq!(observation.visible_expected(), true);
        assert_eq!(observation.logical_reconstruction_cdc_passes(), 1);
        assert_eq!(
            observation.logical_reconstruction_logical_bytes(),
            logical_bytes.len() as u64
        );
        assert_eq!(observation.logical_reconstruction_payload_read_calls(), 2);
        assert_eq!(
            observation.logical_reconstruction_payload_bytes(),
            logical_bytes.len() as u64
        );
        assert!(observation.logical_reconstruction_control_polls() > 0);
        assert!(
            observation.logical_reconstruction_maximum_work_between_polls()
                <= layerfs_storage::cdc::MAXIMUM_CHUNK_BYTES as u64
        );
        assert_eq!(
            objects
                .iter()
                .filter(|(id, _)| matches!(id, TypedPhysicalObjectIdV1::Chunk(_)))
                .count(),
            2
        );
    }

    #[test]
    fn logical_reconstruction_borrowed_control_stops_before_visibility() {
        let objects = rechunked_file_closure();
        let object_refs = typed_refs(&objects);
        for failure in [CoreError::Cancelled, CoreError::Deadline] {
            let observation =
                admit_v1(AdmissionRequestV1::new(&object_refs).with_control_failure(failure));
            assert_eq!(observation.error(), Some(failure));
            assert_eq!(observation.logical_reconstruction_cdc_passes(), 1);
            assert!(observation.logical_reconstruction_control_polls() > 0);
            assert!(!observation.visible_expected());
            assert_eq!(observation.sink_aborts(), 1);
            assert_eq!(observation.admitted_slots(), 0);
        }
    }

    #[test]
    fn admission_reconstructs_an_indexed_directory_and_shared_child_once() {
        let objects = indexed_symlink_closure();
        let expected_is_version = matches!(
            objects.first().map(|(id, _)| id),
            Some(TypedPhysicalObjectIdV1::VersionRecord(_))
        );
        let object_refs = typed_refs(&objects);
        let observation = admit_v1(AdmissionRequestV1::new(&object_refs));
        assert_eq!(
            (
                observation.error(),
                observation.object_count(),
                expected_is_version
            ),
            (None, 6, true)
        );
        assert_eq!(observation.visible_expected(), true);
    }

    #[test]
    fn occupied_complete_equality_deduplicates_without_overwrite() {
        let objects = empty_closure();
        let object_refs = refs(&objects);
        let observation =
            admit_v1(AdmissionRequestV1::new(&object_refs).with_occupied(1, &objects[1]));
        assert_eq!(
            (observation.error(), observation.created_count()),
            (None, 1)
        );
        assert_eq!(observation.reused_count(), 1);
        assert_eq!(observation.staged_count(), 1);
        assert_eq!(observation.reused_occupied(), true);
        assert_eq!(observation.physical_objects_created(), 0);
        assert_eq!(observation.physical_objects_reused(), 0);
        assert_eq!(observation.closure_objects_missing(), 1);
        assert_eq!(observation.closure_objects_occupied_validated(), 1);
    }

    #[test]
    fn collision_and_malformed_occupant_fail_without_visibility_or_overwrite() {
        let objects = empty_closure();
        let object_refs = refs(&objects);
        let different_valid_tree = object_kind(2, &[1, 0, 0, 0, 0, 0, 0, 0, 0]);
        let observation =
            admit_v1(AdmissionRequestV1::new(&object_refs).with_occupied(1, &different_valid_tree));
        assert_eq!(
            observation.error(),
            Some(CoreError::OccupiedSameIdDifferentBytes)
        );
        assert_eq!(observation.visible_expected(), false);
        assert_eq!(observation.sink_aborts(), 1);

        let observation = admit_v1(
            AdmissionRequestV1::new(&object_refs)
                .with_occupied(1, &objects[1][..objects[1].len() - 1]),
        );
        assert_eq!(observation.error(), Some(CoreError::MalformedOccupant));
        assert_eq!(observation.visible_expected(), false);
        assert_eq!(observation.sink_aborts(), 1);

        let mut wrong_version = objects[1].clone();
        wrong_version[8..10].copy_from_slice(&2_u16.to_be_bytes());
        let mut wrong_profile = objects[1].clone();
        wrong_profile[12] ^= 1;
        let mut unknown_kind = objects[1].clone();
        unknown_kind[10] = 0xff;
        let wrong_typed_kind = object_kind(5, &[0x44]);
        for (label, occupied_bytes, expected) in [
            ("schema", wrong_version, CoreError::Schema),
            ("profile-domain", wrong_profile, CoreError::TypeDomain),
            ("unknown-kind", unknown_kind, CoreError::UnknownKind),
            ("typed-kind", wrong_typed_kind, CoreError::TypeDomain),
        ] {
            let observation =
                admit_v1(AdmissionRequestV1::new(&object_refs).with_occupied(1, &occupied_bytes));
            assert_eq!(observation.error(), Some(expected), "{label}");
            assert_eq!(observation.visible_expected(), false, "{label}");
            assert_eq!(observation.sink_aborts(), 1, "{label}");
        }
    }

    #[test]
    fn occupied_collision_reads_the_complete_large_object_in_bounded_windows() {
        let objects = rechunked_file_closure();
        let file_ordinal = objects
            .iter()
            .position(|(id, _)| matches!(id, TypedPhysicalObjectIdV1::File(_)))
            .unwrap();
        let object_refs = typed_refs(&objects);
        let occupied = large_valid_file(2_000);
        assert!(occupied.len() > 65_536);
        let observation =
            admit_v1(AdmissionRequestV1::new(&object_refs).with_occupied(file_ordinal, &occupied));
        assert_eq!(
            observation.error(),
            Some(CoreError::OccupiedSameIdDifferentBytes)
        );
        assert_eq!(observation.occupied_maximum_read(), 65_536);
        assert!(observation.occupied_reads() > 2);
        assert!(observation.bytes_read() > occupied.len() as u64);
        assert_eq!(observation.visible_expected(), false);
        assert_eq!(observation.sink_aborts(), 1);
    }

    #[test]
    fn missing_and_wrong_domain_edges_abort_the_private_closure() {
        let chunk = object_kind(5, b"x");
        let chunk_id = derive_physical_chunk_id_v1(&chunk).unwrap();
        let mut leaf_payload = vec![2, 0, 0, 1, 0, 1, b'x', 2];
        leaf_payload.extend_from_slice(chunk_id.as_bytes());
        let leaf = object_kind(2, &leaf_payload);
        let [version, root] = empty_closure();

        let mut missing = vec![
            (typed_id(&version), version.clone()),
            (typed_id(&root), root.clone()),
            (typed_id(&leaf), leaf.clone()),
        ];
        canonicalize(&mut missing);
        let missing_refs = typed_refs(&missing);
        let observation = admit_v1(AdmissionRequestV1::new(&missing_refs));
        assert_eq!(observation.error(), Some(CoreError::MissingClosureEdge));
        assert_eq!(observation.visible_expected(), false);

        let mut wrong_domain = missing;
        wrong_domain.push((TypedPhysicalObjectIdV1::Chunk(chunk_id), chunk));
        canonicalize(&mut wrong_domain);
        let wrong_domain_refs = typed_refs(&wrong_domain);
        let observation = admit_v1(AdmissionRequestV1::new(&wrong_domain_refs));
        assert_eq!(observation.error(), Some(CoreError::TypedEdge));
        assert_eq!(observation.visible_expected(), false);
    }

    #[test]
    fn identity_and_resource_failures_precede_visibility() {
        let expected = empty_closure();
        let mut mutated = expected.clone();
        *mutated[1].last_mut().unwrap() ^= 1;
        let source_refs = refs(&mutated);
        let expected_refs = refs(&expected);
        let observation =
            admit_v1(AdmissionRequestV1::new(&source_refs).with_expected_objects(&expected_refs));
        assert_eq!(observation.error(), Some(CoreError::TypedEdge));
        assert_eq!(observation.visible_expected(), false);

        let observation =
            admit_v1(AdmissionRequestV1::new(&expected_refs).with_ledger_budget(8_388_608));
        assert_eq!(observation.error(), Some(CoreError::ResourceRefused));
        assert_eq!(observation.sink_begun(), 0);
    }

    #[test]
    fn minimal_pack_is_exact_and_sealed_only_after_independent_validation() {
        let object = object(&[0]);
        let objects = [object.as_slice()];
        let pack = build_v1(PackRequestV1::new(&objects));
        assert_build_custody(
            pack.source_payload_bytes_read(),
            pack.admitted_slots(),
            &objects,
        );
        assert!(pack.sealed());
        assert!(!pack.aborted());
        assert_eq!(pack.bytes().len(), 288);
        assert_eq!(pack.pack_len(), 288);
        assert_eq!(pack.record_count(), 1);
        assert_eq!(pack.index_offset(), 128);
        assert_eq!(
            pack.pack_id(),
            crate::support::expected(
                "90bf9bf15f2d23614bc3fbd3807ba4ec114da4b882b707fadcc1020651097471"
            )
        );
        assert_eq!(&pack.bytes()[..8], b"ELSPACK1");
        assert_eq!(&pack.bytes()[128 + 80..128 + 88], b"ELSPEND1");
        assert_eq!(pack.spool_peak(), 1);
        assert_eq!(pack.pack_entries(), 1);
        assert_eq!(pack.pack_bytes(), 288);
        assert_eq!(pack.bytes_written(), 288);
        assert_eq!(pack.memory_high_water(), 12_582_912);
    }

    #[test]
    fn mixed_kind_records_keep_discovery_order_and_index_has_strict_typed_order() {
        let objects = [
            object_kind(5, &[7]),
            object_kind(4, &[0, 0, 0, 1, b'x']),
            object_kind(2, &[1, 0x10, 0, 0, 0, 0, 0, 0, 0]),
        ];
        let physical_ids: Vec<_> = objects.iter().map(|value| typed_id(value)).collect();
        let object_refs: Vec<_> = objects.iter().map(Vec::as_slice).collect();
        let pack = build_v1(PackRequestV1::new(&object_refs));
        assert_build_custody(
            pack.source_payload_bytes_read(),
            pack.admitted_slots(),
            &object_refs,
        );
        let mut offset = 64_usize;
        for (object, id) in objects.iter().zip(physical_ids) {
            assert_eq!(
                u32::from_be_bytes(pack.bytes()[offset..offset + 4].try_into().unwrap()) as usize,
                object.len()
            );
            assert_eq!(
                typed_id(&pack.bytes()[offset + 4..offset + 4 + object.len()]),
                id
            );
            offset += (4 + object.len() + 7) & !7;
        }
        assert_eq!(offset as u64, pack.index_offset());
        let mut previous: Option<(u8, [u8; 32])> = None;
        for entry in pack.bytes()[offset..offset + objects.len() * 80].chunks_exact(80) {
            let key = (entry[0], <[u8; 32]>::try_from(&entry[4..36]).unwrap());
            assert!(previous.is_none_or(|left| left.cmp(&key) == Ordering::Less));
            previous = Some(key);
        }
    }

    #[test]
    fn large_dense_pack_uses_one_bounded_window_and_metadata_only_spool() {
        let objects: Vec<_> = (0_u16..10_000)
            .map(|value| object(&value.to_be_bytes()))
            .collect();
        let object_refs: Vec<_> = objects.iter().map(Vec::as_slice).collect();
        let pack = build_v1(PackRequestV1::new(&object_refs));
        assert_build_custody(
            pack.source_payload_bytes_read(),
            pack.admitted_slots(),
            &object_refs,
        );
        assert!(pack.sealed());
        assert_eq!(pack.record_count(), 10_000);
        assert_eq!(pack.spool_peak(), 10_000);
        assert_eq!(pack.pack_entries(), 10_000);
        assert_eq!(pack.pack_bytes(), pack.bytes().len() as u64);
        assert_eq!(pack.memory_high_water(), 12_582_912);
        assert_eq!(
            pack.bytes_read(),
            objects.iter().map(|value| value.len() as u64).sum::<u64>()
                + pack.pack_bytes() * 2
                - 32,
            "construction reads each source once, then performs an independent full-pack hash pass and exact structural pass"
        );
    }

    #[test]
    fn oversized_index_residency_is_refused_before_pack_output() {
        let object = object(&[0]);
        let objects = [object.as_slice()];
        let pack =
            build_v1(PackRequestV1::new(&objects).with_index_residency(operation_slot_bytes()));
        assert_eq!(pack.error(), Some(CoreError::ResourceRefused));
        assert_eq!(pack.begins(), 0);
        assert!(pack.bytes().is_empty());
        assert_eq!(pack.source_metadata_reads(), 0);
        assert_eq!(pack.source_payload_bytes_read(), 0);
    }

    #[test]
    fn oversized_source_residency_is_refused_before_payload_read_or_pack_output() {
        let object = object(&[0]);
        let objects = [object.as_slice()];
        let pack =
            build_v1(PackRequestV1::new(&objects).with_source_residency(operation_slot_bytes()));
        assert_eq!(pack.error(), Some(CoreError::ResourceRefused));
        assert_eq!(pack.source_payload_bytes_read(), 0);
        assert_eq!(pack.source_metadata_reads(), 0);
        assert_eq!(pack.begins(), 0);
        assert!(pack.bytes().is_empty());
    }

    #[test]
    fn oversized_pack_port_residency_is_refused_before_source_preflight_or_output() {
        let object = object(&[0]);
        let objects = [object.as_slice()];
        let pack =
            build_v1(PackRequestV1::new(&objects).with_pack_residency(operation_slot_bytes()));
        assert_eq!(pack.error(), Some(CoreError::ResourceRefused));
        assert_eq!(pack.source_metadata_reads(), 0);
        assert_eq!(pack.source_payload_bytes_read(), 0);
        assert_eq!(pack.begins(), 0);
        assert_eq!(pack.len_calls(), 0);
        assert!(pack.bytes().is_empty());
    }

    #[test]
    fn validation_charges_pack_port_residency_before_length_or_payload_reads() {
        let object = object(&[0]);
        let objects = [object.as_slice()];
        let valid = build_v1(PackRequestV1::new(&objects));
        assert_build_custody(
            valid.source_payload_bytes_read(),
            valid.admitted_slots(),
            &objects,
        );
        let pack = validate_v1(
            ValidationRequestV1::new(valid.bytes()).with_pack_residency(operation_slot_bytes()),
        );
        assert_eq!(pack.error(), Some(CoreError::ResourceRefused));
        assert_eq!(pack.len_calls(), 0);
        assert_eq!(pack.bytes_read(), 0);
    }

    #[test]
    fn duplicate_key_and_sink_refusal_abort_without_sealing() {
        let object = object(&[1]);
        let duplicates = [object.as_slice(), object.as_slice()];
        let duplicate = build_v1(PackRequestV1::new(&duplicates));
        assert_eq!(duplicate.error(), Some(CoreError::NonCanonicalOrder));
        assert!(duplicate.aborted());
        assert!(!duplicate.sealed());

        let one_object = [object.as_slice()];
        let sink = build_v1(PackRequestV1::new(&one_object).with_sink_failure_after(64));
        assert_eq!(sink.error(), Some(CoreError::SinkRefused));
        assert!(sink.aborted());
        assert!(!sink.sealed());
        assert_eq!(sink.admitted_slots(), 0);
    }

    #[test]
    fn hostile_index_record_seal_truncation_overlap_and_trailing_bytes_fail_closed() {
        let objects = [object(&[0]), object(&[1])];
        let object_refs: Vec<_> = objects.iter().map(Vec::as_slice).collect();
        let valid = build_v1(PackRequestV1::new(&object_refs));
        assert_build_custody(
            valid.source_payload_bytes_read(),
            valid.admitted_slots(),
            &object_refs,
        );
        let index_offset = be_u64(&valid.bytes()[56..64]) as usize;

        let mut duplicate = valid.bytes().to_vec();
        duplicate.copy_within(index_offset + 4..index_offset + 36, index_offset + 84);
        reseal(&mut duplicate);
        assert_eq!(
            independently_validate(&duplicate),
            Err(CoreError::PackInvalid)
        );

        let mut overlap = valid.bytes().to_vec();
        overlap[index_offset + 80 + 36..index_offset + 80 + 44]
            .copy_from_slice(&64_u64.to_be_bytes());
        reseal(&mut overlap);
        assert_eq!(
            independently_validate(&overlap),
            Err(CoreError::PackInvalid)
        );

        let mut bad_object_checksum = valid.bytes().to_vec();
        bad_object_checksum[index_offset + 48] ^= 1;
        reseal(&mut bad_object_checksum);
        assert_eq!(
            independently_validate(&bad_object_checksum),
            Err(CoreError::IdMismatch)
        );

        let mut bad_pack_checksum = valid.bytes().to_vec();
        let last = bad_pack_checksum.len() - 1;
        bad_pack_checksum[last] ^= 1;
        assert_eq!(
            independently_validate(&bad_pack_checksum),
            Err(CoreError::IdMismatch)
        );

        let truncated = &valid.bytes()[..valid.bytes().len() - 1];
        assert_eq!(
            independently_validate(truncated),
            Err(CoreError::PackInvalid)
        );

        let mut trailing = valid.bytes().to_vec();
        trailing.push(0);
        assert_eq!(
            independently_validate(&trailing),
            Err(CoreError::PackInvalid)
        );
    }

    #[test]
    fn pack_validation_distinguishes_structure_schema_domain_kind_and_authentication() {
        let object = object(&[0x5a]);
        let objects = [object.as_slice()];
        let valid = build_v1(PackRequestV1::new(&objects));
        assert_build_custody(
            valid.source_payload_bytes_read(),
            valid.admitted_slots(),
            &objects,
        );
        let index_offset = be_u64(&valid.bytes()[56..64]) as usize;
        let object_offset = 64 + 4;

        let mut structural = valid.bytes().to_vec();
        structural[0] ^= 1;
        reseal(&mut structural);
        assert_eq!(
            independently_validate(&structural),
            Err(CoreError::PackInvalid)
        );

        let mut header_version = valid.bytes().to_vec();
        header_version[8..10].copy_from_slice(&2_u16.to_be_bytes());
        reseal(&mut header_version);
        assert_eq!(
            independently_validate(&header_version),
            Err(CoreError::Schema)
        );

        let mut trailer_version = valid.bytes().to_vec();
        let trailer_offset = trailer_version.len() - 80;
        trailer_version[trailer_offset + 8..trailer_offset + 10]
            .copy_from_slice(&2_u16.to_be_bytes());
        reseal(&mut trailer_version);
        assert_eq!(
            independently_validate(&trailer_version),
            Err(CoreError::Schema)
        );

        let mut header_profile = valid.bytes().to_vec();
        header_profile[16] ^= 1;
        reseal(&mut header_profile);
        assert_eq!(
            independently_validate(&header_profile),
            Err(CoreError::TypeDomain)
        );

        let mut object_profile = valid.bytes().to_vec();
        object_profile[object_offset + 12] ^= 1;
        reseal(&mut object_profile);
        assert_eq!(
            independently_validate(&object_profile),
            Err(CoreError::TypeDomain)
        );

        let mut object_kind = valid.bytes().to_vec();
        object_kind[object_offset + 10] = 0xff;
        reseal(&mut object_kind);
        assert_eq!(
            independently_validate(&object_kind),
            Err(CoreError::UnknownKind)
        );

        let mut index_kind = valid.bytes().to_vec();
        index_kind[index_offset] = 0xff;
        reseal(&mut index_kind);
        assert_eq!(
            independently_validate(&index_kind),
            Err(CoreError::UnknownKind)
        );

        let mut object_bytes = valid.bytes().to_vec();
        object_bytes[object_offset + 52] ^= 1;
        reseal(&mut object_bytes);
        assert_eq!(
            independently_validate(&object_bytes),
            Err(CoreError::IdMismatch)
        );
    }

    #[test]
    fn independent_validation_reparses_canonical_payload_not_just_consistent_hashes() {
        let mut leaf = vec![2, 0, 0, 2];
        for (name, id) in [(b'a', [0x11; 32]), (b'b', [0x12; 32])] {
            leaf.extend_from_slice(&1_u16.to_be_bytes());
            leaf.push(name);
            leaf.push(1);
            leaf.extend_from_slice(&id);
        }
        let object = object_kind(2, &leaf);
        let objects = [object.as_slice()];
        let valid = build_v1(PackRequestV1::new(&objects));
        assert_build_custody(
            valid.source_payload_bytes_read(),
            valid.admitted_slots(),
            &objects,
        );
        let mut malformed = valid.bytes().to_vec();

        let object_offset = 64 + 4;
        let object_len = u32::from_be_bytes(malformed[64..68].try_into().unwrap()) as usize;
        malformed[object_offset + 52 + 42] = b'a';
        let object_bytes = &malformed[object_offset..object_offset + object_len];
        let object_id = framed_digest(0x12, object_bytes);
        let object_checksum = framed_digest(0x21, object_bytes);
        let index_offset = be_u64(&malformed[56..64]) as usize;
        malformed[index_offset + 4..index_offset + 36].copy_from_slice(&object_id);
        malformed[index_offset + 48..index_offset + 80].copy_from_slice(&object_checksum);
        reseal(&mut malformed);

        assert_eq!(
            independently_validate(&malformed),
            Err(CoreError::PackInvalid),
            "self-consistent hashes cannot authenticate non-canonical payload bytes"
        );
    }

    #[test]
    fn empty_pack_is_refused_before_output_or_resource_reservation() {
        let empty: [&[u8]; 0] = [];
        let pack = build_v1(PackRequestV1::new(&empty));
        assert_eq!(pack.error(), Some(CoreError::CountCap));
        assert_eq!(pack.begins(), 0);
        assert_eq!(pack.admitted_slots(), 0);
        assert_eq!(pack.ledger_high_water(), 8_388_608);
    }

    #[test]
    fn pack_read_failures_remain_source_failures() {
        let object = object(&[0]);
        let objects = [object.as_slice()];
        let valid = build_v1(PackRequestV1::new(&objects));
        assert_build_custody(
            valid.source_payload_bytes_read(),
            valid.admitted_slots(),
            &objects,
        );
        assert_eq!(
            validate_v1(ValidationRequestV1::new(valid.bytes()).with_fail_reads(true)).error(),
            Some(CoreError::SourceFailure)
        );
    }
}
