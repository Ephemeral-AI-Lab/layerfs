mod support;

#[path = "reference/naive_fastcdc.rs"]
mod naive_fastcdc;

mod l0_codec_vectors {
    use layerfs_storage::format::{
        checked_encoded_len, checked_u32, compare_paths_unsigned, compare_unsigned,
        require_at_most, require_nonzero_at_most, require_strictly_increasing,
        require_strictly_increasing_paths, validate_chunk_object_count,
        validate_chunk_reference_len, validate_chunk_refs_per_file,
        validate_chunk_refs_per_version, validate_component, validate_directory_mode,
        validate_directory_page_depth, validate_domain, validate_entry_count,
        validate_extents_per_file, validate_extents_per_version, validate_file_mode,
        validate_flags_zero, validate_index_page_depth, validate_leaf_page_depth,
        validate_logical_chunk_payload_len, validate_logical_length, validate_path,
        validate_physical_chunk_payload_len, validate_physical_object_len, validate_reserved_zero,
        validate_schema_v1, validate_symlink_target, validate_total_object_count,
        validate_tree_index_fanout, validate_tree_leaf_fanout, validate_tree_object_count,
        ByteSink, DirectoryModeContext, ExtentTagV1, LogicalChildKindV1, PathValidator,
        PhysicalObjectKindV1, PhysicalTreeChildKindV1, PresenceV1, SliceCursor, SliceSink,
        TreeSubtypeV1, ValidatedComponent, ValidatedPath, ValidatedSymlinkTarget, MAX_CHUNK_BYTES,
        MAX_CHUNK_OBJECTS, MAX_CHUNK_REFS_PER_FILE, MAX_CHUNK_REFS_PER_VERSION,
        MAX_COMPONENT_BYTES, MAX_ENTRIES, MAX_EXTENTS_PER_FILE, MAX_EXTENTS_PER_VERSION,
        MAX_LOGICAL_BYTES, MAX_PATH_BYTES, MAX_PATH_DEPTH, MAX_PHYSICAL_OBJECT_BYTES,
        MAX_SYMLINK_TARGET_BYTES, MAX_TOTAL_OBJECTS, MAX_TREE_INDEX_FANOUT, MAX_TREE_LEAF_FANOUT,
        MAX_TREE_OBJECTS, MAX_TREE_PAGE_DEPTH, ROOT_DIRECTORY_MODE_SENTINEL_V1,
    };
    use layerfs_storage::{CoreError, OutcomeCode};

    fn valid_path(bytes: &[u8]) -> ValidatedPath<'_> {
        ValidatedPath::new(bytes).unwrap()
    }

    type CountCase = (fn(u64) -> Result<(), CoreError>, u64);

    #[test]
    fn fixed_width_codec_is_explicit_and_exact_eof_is_required() {
        let mut bytes = [0_u8; 29];
        let mut sink = SliceSink::new(&mut bytes);
        sink.write_u8(0x5a).unwrap();
        sink.write_u16_le(0x1234).unwrap();
        sink.write_u16_be(0x1234).unwrap();
        sink.write_u32_le(0x1234_5678).unwrap();
        sink.write_u32_be(0x1234_5678).unwrap();
        sink.write_u64_le(0x0123_4567_89ab_cdef).unwrap();
        sink.write_u64_be(0x0123_4567_89ab_cdef).unwrap();
        assert_eq!(sink.remaining_capacity(), 0);
        assert_eq!(
            sink.written(),
            &[
                0x5a, 0x34, 0x12, 0x12, 0x34, 0x78, 0x56, 0x34, 0x12, 0x12, 0x34, 0x56, 0x78, 0xef,
                0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd,
                0xef,
            ]
        );

        let mut cursor = SliceCursor::new(sink.written());
        assert_eq!(cursor.read_u8(), Ok(0x5a));
        assert_eq!(cursor.read_u16_le(), Ok(0x1234));
        assert_eq!(cursor.read_u16_be(), Ok(0x1234));
        assert_eq!(cursor.read_u32_le(), Ok(0x1234_5678));
        assert_eq!(cursor.read_u32_be(), Ok(0x1234_5678));
        assert_eq!(cursor.read_u64_le(), Ok(0x0123_4567_89ab_cdef));
        assert_eq!(cursor.read_u64_be(), Ok(0x0123_4567_89ab_cdef));
        assert_eq!(cursor.finish(), Ok(()));

        let mut truncated = SliceCursor::new(&[1, 2, 3]);
        assert_eq!(truncated.read_u32_be(), Err(CoreError::Truncated));

        let mut trailing = SliceCursor::new(&[0]);
        assert_eq!(trailing.finish(), Err(CoreError::TrailingBytes));
        assert_eq!(trailing.read_u8(), Ok(0));
        assert_eq!(trailing.finish(), Ok(()));
    }

    #[test]
    fn sink_refusal_is_before_effect_and_bounds_are_fail_closed() {
        let mut output = [0xa5; 3];
        let mut sink = SliceSink::new(&mut output);
        assert_eq!(sink.write(&[1, 2, 3, 4]), Err(CoreError::SinkRefused));
        assert_eq!(sink.len(), 0);
        assert_eq!(sink.written(), &[]);
        sink.write(&[1, 2, 3]).unwrap();
        assert_eq!(sink.written(), &[1, 2, 3]);

        assert_eq!(require_at_most(32_768, 32_768, CoreError::ChunkCap), Ok(()));
        assert_eq!(
            require_at_most(32_769, 32_768, CoreError::ChunkCap),
            Err(CoreError::ChunkCap)
        );
        assert_eq!(
            require_nonzero_at_most(1, 32_768, CoreError::ChunkLength),
            Ok(())
        );
        assert_eq!(
            require_nonzero_at_most(0, 32_768, CoreError::ChunkLength),
            Err(CoreError::ChunkLength)
        );
        assert_eq!(checked_encoded_len(52, 192, 290), Ok(55_732));
        assert_eq!(
            checked_encoded_len(usize::MAX, 1, 1),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(
            checked_encoded_len(0, u64::MAX, usize::MAX),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(checked_u32(u32::MAX as u64), Ok(u32::MAX));
        assert_eq!(
            checked_u32(u32::MAX as u64 + 1),
            Err(CoreError::IntegerOverflow)
        );

        let mut cursor = SliceCursor::new(&[0xa5; 8]);
        assert_eq!(
            cursor.read_bounded_bytes(MAX_CHUNK_BYTES + 1, MAX_CHUNK_BYTES, CoreError::ChunkCap),
            Err(CoreError::ChunkCap)
        );
        assert_eq!(cursor.position(), 0);
        assert_eq!(
            cursor.read_nonzero_bounded_bytes(0, MAX_CHUNK_BYTES, CoreError::ChunkLength),
            Err(CoreError::ChunkLength)
        );
        assert_eq!(cursor.position(), 0);
        assert_eq!(
            cursor.read_bounded_bytes(8, MAX_CHUNK_BYTES, CoreError::ChunkCap),
            Ok(&[0xa5; 8][..])
        );
        assert!(cursor.is_eof());
    }

    #[test]
    fn contextual_caps_match_the_frozen_one_over_vectors() {
        assert_eq!(validate_logical_length(MAX_LOGICAL_BYTES), Ok(()));
        assert_eq!(
            validate_logical_length(MAX_LOGICAL_BYTES + 1),
            Err(CoreError::LogicalLength)
        );
        assert_eq!(validate_logical_chunk_payload_len(0), Ok(()));
        assert_eq!(validate_logical_chunk_payload_len(MAX_CHUNK_BYTES), Ok(()));
        assert_eq!(
            validate_logical_chunk_payload_len(MAX_CHUNK_BYTES + 1),
            Err(CoreError::ChunkCap)
        );
        assert_eq!(
            validate_physical_chunk_payload_len(0),
            Err(CoreError::ChunkCap)
        );
        assert_eq!(validate_physical_chunk_payload_len(1), Ok(()));
        assert_eq!(validate_physical_chunk_payload_len(MAX_CHUNK_BYTES), Ok(()));
        assert_eq!(validate_chunk_reference_len(0), Err(CoreError::ChunkLength));
        assert_eq!(validate_chunk_reference_len(MAX_CHUNK_BYTES), Ok(()));
        assert_eq!(
            validate_physical_object_len(MAX_PHYSICAL_OBJECT_BYTES),
            Ok(())
        );
        assert_eq!(
            validate_physical_object_len(MAX_PHYSICAL_OBJECT_BYTES + 1),
            Err(CoreError::PhysicalObjectCap)
        );

        let cases: [CountCase; 8] = [
            (validate_entry_count, MAX_ENTRIES),
            (validate_tree_object_count, MAX_TREE_OBJECTS),
            (validate_chunk_object_count, MAX_CHUNK_OBJECTS),
            (validate_total_object_count, MAX_TOTAL_OBJECTS),
            (validate_extents_per_file, MAX_EXTENTS_PER_FILE),
            (validate_chunk_refs_per_file, MAX_CHUNK_REFS_PER_FILE),
            (validate_extents_per_version, MAX_EXTENTS_PER_VERSION),
            (validate_chunk_refs_per_version, MAX_CHUNK_REFS_PER_VERSION),
        ];
        for (validate, maximum) in cases {
            assert_eq!(validate(maximum), Ok(()));
            assert_eq!(validate(maximum + 1), Err(CoreError::CountCap));
        }

        assert_eq!(validate_tree_leaf_fanout(0), Err(CoreError::CountCap));
        assert_eq!(validate_tree_leaf_fanout(MAX_TREE_LEAF_FANOUT), Ok(()));
        assert_eq!(
            validate_tree_leaf_fanout(MAX_TREE_LEAF_FANOUT + 1),
            Err(CoreError::CountCap)
        );
        assert_eq!(validate_tree_index_fanout(0), Err(CoreError::CountCap));
        assert_eq!(validate_tree_index_fanout(MAX_TREE_INDEX_FANOUT), Ok(()));
        assert_eq!(validate_directory_page_depth(MAX_TREE_PAGE_DEPTH), Ok(()));
        assert_eq!(
            validate_directory_page_depth(MAX_TREE_PAGE_DEPTH + 1),
            Err(CoreError::CountCap)
        );
        assert_eq!(validate_leaf_page_depth(0), Ok(()));
        assert_eq!(validate_leaf_page_depth(1), Err(CoreError::CountCap));
        assert_eq!(validate_index_page_depth(0), Err(CoreError::CountCap));
        assert_eq!(validate_index_page_depth(MAX_TREE_PAGE_DEPTH), Ok(()));
    }

    #[test]
    fn fixed_prefix_domains_modes_and_discriminators_are_typed() {
        assert_eq!(validate_domain(b"ELSOBJ01", b"ELSOBJ01"), Ok(()));
        assert_eq!(
            validate_domain(b"ELSOBJ00", b"ELSOBJ01"),
            Err(CoreError::TypeDomain)
        );
        assert_eq!(validate_schema_v1(1), Ok(()));
        assert_eq!(validate_schema_v1(0), Err(CoreError::Schema));
        assert_eq!(validate_schema_v1(2), Err(CoreError::Schema));
        assert_eq!(validate_flags_zero(0), Ok(()));
        assert_eq!(validate_flags_zero(1), Err(CoreError::Flags));
        assert_eq!(validate_reserved_zero(&[0, 0, 0]), Ok(()));
        assert_eq!(validate_reserved_zero(&[0, 1, 0]), Err(CoreError::Reserved));

        assert_eq!(validate_file_mode(0x0fff), Ok(()));
        assert_eq!(
            validate_file_mode(ROOT_DIRECTORY_MODE_SENTINEL_V1),
            Err(CoreError::FileMode)
        );
        assert_eq!(
            validate_directory_mode(0x0fff, DirectoryModeContext::Explicit),
            Ok(())
        );
        assert_eq!(
            validate_directory_mode(
                ROOT_DIRECTORY_MODE_SENTINEL_V1,
                DirectoryModeContext::Explicit
            ),
            Err(CoreError::ChildMode)
        );
        assert_eq!(
            validate_directory_mode(
                ROOT_DIRECTORY_MODE_SENTINEL_V1,
                DirectoryModeContext::ImplicitRoot
            ),
            Ok(())
        );
        assert_eq!(
            validate_directory_mode(0x0fff, DirectoryModeContext::ImplicitRoot),
            Err(CoreError::RootSentinel)
        );
        assert_eq!(
            validate_directory_mode(0x1001, DirectoryModeContext::ImplicitRoot),
            Err(CoreError::RootSentinel)
        );

        assert_eq!(
            LogicalChildKindV1::try_from(0x01),
            Ok(LogicalChildKindV1::File)
        );
        assert_eq!(
            LogicalChildKindV1::try_from(0x03),
            Ok(LogicalChildKindV1::Symlink)
        );
        assert_eq!(
            PhysicalObjectKindV1::try_from(0x05),
            Ok(PhysicalObjectKindV1::Chunk)
        );
        assert_eq!(TreeSubtypeV1::try_from(0x03), Ok(TreeSubtypeV1::Index));
        assert_eq!(
            PhysicalTreeChildKindV1::try_from(0x02),
            Ok(PhysicalTreeChildKindV1::File)
        );
        assert_eq!(ExtentTagV1::try_from(0x02), Ok(ExtentTagV1::Data));
        assert_eq!(PresenceV1::try_from(0x00), Ok(PresenceV1::Absent));
        assert_eq!(PresenceV1::try_from(0x01), Ok(PresenceV1::Present));
        for value in [0, u8::MAX] {
            assert_eq!(
                LogicalChildKindV1::try_from(value),
                Err(CoreError::UnknownKind)
            );
            assert_eq!(
                PhysicalObjectKindV1::try_from(value),
                Err(CoreError::UnknownKind)
            );
            assert_eq!(TreeSubtypeV1::try_from(value), Err(CoreError::UnknownKind));
            assert_eq!(
                PhysicalTreeChildKindV1::try_from(value),
                Err(CoreError::UnknownKind)
            );
            assert_eq!(ExtentTagV1::try_from(value), Err(CoreError::UnknownKind));
        }
    }

    #[test]
    fn od01_path_and_order_vectors_are_exact() {
        let maximum_component = vec![b'a'; MAX_COMPONENT_BYTES];
        assert_eq!(validate_component(&maximum_component), Ok(()));
        assert_eq!(
            validate_component(&vec![b'a'; MAX_COMPONENT_BYTES + 1]),
            Err(CoreError::Name)
        );
        for invalid in [
            b"".as_slice(),
            b".".as_slice(),
            b"..",
            b"a/b",
            b"a\0b",
            &[0xff],
        ] {
            assert_eq!(validate_component(invalid), Err(CoreError::Name));
        }
        assert_eq!(
            ValidatedComponent::new(b"name").unwrap().as_bytes(),
            b"name"
        );

        let maximum_depth = vec!["a"; MAX_PATH_DEPTH].join("/");
        assert_eq!(
            validate_path(maximum_depth.as_bytes()),
            Ok(MAX_PATH_DEPTH as u16)
        );
        assert_eq!(
            validate_path(vec!["a"; MAX_PATH_DEPTH + 1].join("/").as_bytes()),
            Err(CoreError::Path)
        );
        assert!(maximum_depth.len() <= MAX_PATH_BYTES);
        assert_eq!(validate_path(b"a//b"), Err(CoreError::Path));
        assert_eq!(validate_path(b"/a"), Err(CoreError::Path));
        assert_eq!(validate_path(b"a/"), Err(CoreError::Path));
        assert_eq!(validate_path(b"a/../b"), Err(CoreError::Path));

        let mut maximum_length_components = vec!["a".repeat(MAX_COMPONENT_BYTES); 15];
        maximum_length_components.push("b".repeat(MAX_COMPONENT_BYTES - 1));
        maximum_length_components.push("c".to_owned());
        let maximum_length_path = maximum_length_components.join("/");
        assert_eq!(maximum_length_path.len(), MAX_PATH_BYTES);
        assert_eq!(validate_path(maximum_length_path.as_bytes()), Ok(17));
        maximum_length_components[16].push('d');
        assert_eq!(
            validate_path(maximum_length_components.join("/").as_bytes()),
            Err(CoreError::Path)
        );

        let symlink = vec![b'a'; MAX_SYMLINK_TARGET_BYTES];
        assert_eq!(validate_symlink_target(&symlink), Ok(()));
        assert_eq!(
            validate_symlink_target(&vec![b'a'; MAX_SYMLINK_TARGET_BYTES + 1]),
            Err(CoreError::Target)
        );
        assert_eq!(validate_symlink_target(b"../target"), Ok(()));

        assert_eq!(
            compare_unsigned(&[0xff], &[0x00]),
            core::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_paths_unsigned(valid_path(b"a"), valid_path(b"a/b")),
            core::cmp::Ordering::Less
        );
        assert_eq!(
            compare_paths_unsigned(valid_path("é".as_bytes()), valid_path("水".as_bytes())),
            core::cmp::Ordering::Less
        );
        assert_eq!(require_strictly_increasing(b"a", b"b"), Ok(()));
        assert_eq!(
            require_strictly_increasing(b"b", b"a"),
            Err(CoreError::NonCanonicalOrder)
        );
        assert_eq!(
            require_strictly_increasing_paths(valid_path(b"a/b"), valid_path(b"a/b")),
            Err(CoreError::NonCanonicalOrder)
        );
        assert_eq!(
            require_strictly_increasing_paths(valid_path(b"a/b"), valid_path(b"a.")),
            Ok(())
        );

        let mut validator = PathValidator::new(3).unwrap();
        validator.push(b'a').unwrap();
        validator.push(b'/').unwrap();
        validator.push(b'b').unwrap();
        assert_eq!(validator.finish(), Ok(2));
        assert_eq!(ValidatedPath::new(b"a/b").unwrap().depth(), 2);
        assert_eq!(
            ValidatedSymlinkTarget::new(b"a/b").unwrap().as_bytes(),
            b"a/b"
        );
    }

    #[test]
    fn every_internal_error_maps_to_one_frozen_external_outcome() {
        let mappings = [
            (CoreError::Schema, "S_SCHEMA"),
            (CoreError::Truncated, "S_TRUNCATED"),
            (CoreError::TrailingBytes, "S_EXACT_EOF"),
            (CoreError::LogicalLength, "S_LOGICAL_LENGTH"),
            (CoreError::TypedEdge, "S_TYPED_EDGE"),
            (CoreError::FileMode, "S_FILE_MODE"),
            (CoreError::Target, "S_TARGET"),
            (CoreError::RootSentinel, "S_ROOT_SENTINEL"),
            (CoreError::ChildMode, "S_CHILD_MODE"),
            (CoreError::Name, "S_NAME"),
            (CoreError::UnknownKind, "S_UNKNOWN_KIND"),
            (CoreError::NonCanonicalOrder, "S_ORDER_DUPLICATE"),
            (CoreError::TypeDomain, "S_TYPE_DOMAIN"),
            (
                CoreError::OccupiedSameIdDifferentBytes,
                "S_OCCUPIED_SAME_ID_DIFFERENT_BYTES",
            ),
            (CoreError::IdMismatch, "S_ID_MISMATCH"),
            (CoreError::Flags, "S_FLAGS"),
            (CoreError::Reserved, "S_RESERVED"),
            (CoreError::Path, "S_PATH"),
            (CoreError::IntegerOverflow, "S_INTEGER_OVERFLOW"),
            (CoreError::ChunkCap, "S_CHUNK_CAP"),
            (CoreError::CountCap, "S_COUNT_CAP"),
            (CoreError::ChunkLength, "S_CHUNK_LENGTH"),
            (CoreError::PhysicalObjectCap, "S_PHYSICAL_OBJECT_CAP"),
            (CoreError::DigestUnavailable, "S_DIGEST_UNAVAILABLE"),
            (CoreError::DigestFailure, "S_DIGEST_FAILURE"),
            (CoreError::DigestWidth, "S_DIGEST_WIDTH"),
            (CoreError::DigestProtocol, "S_DIGEST_PROTOCOL"),
            (CoreError::SourceFailure, "S_SOURCE_FAILURE"),
            (CoreError::SinkRefused, "S_SINK_REFUSED"),
            (CoreError::ResourceRefused, "S_RESOURCE_REFUSED"),
            (CoreError::Cancelled, "S_CANCELLED"),
            (CoreError::Deadline, "S_DEADLINE"),
        ];
        assert_eq!(mappings.len(), 32);
        for (error, expected) in mappings {
            assert_eq!(error.outcome_code().as_str(), expected);
            assert_eq!(error.to_string(), expected);
        }
        assert_eq!(OutcomeCode::ExactEof.as_str(), "S_EXACT_EOF");
        assert_eq!(OutcomeCode::OrderDuplicate.as_str(), "S_ORDER_DUPLICATE");
    }
}

mod l0_identity_vectors {
    const STRUCTURAL_VECTORS: [(&str, &str, &str); 9] = [
    (
        "logical_chunk_abc",
        "455356322d4c4348554e4b0001000300000000000000616263",
        "1174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c164",
    ),
    (
        "logical_file_abc",
        "455356322d4c46494c450001000300000000000000010000001174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c1640300000000000000",
        "c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d32",
    ),
    (
        "file_node_0644_abc",
        "455356322d464e4f4445000100a401c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d320300000000000000",
        "82204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee",
    ),
    (
        "symlink_node_file_txt",
        "455356322d534e4f44450001000800000066696c652e747874",
        "b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236",
    ),
    (
        "directory_explicit_0755_nested_file",
        "455356322d444e4f4445000100ed010100000004000000646174610182204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee",
        "00768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466",
    ),
    (
        "directory_implicit_empty_root_1000",
        "455356322d444e4f4445000100001000000000",
        "b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09",
    ),
    (
        "version_empty_root",
        "455356322d56524f4f54000100b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09",
        "44b0eb7c80a93ffc3cb98e4ff16c90d4a8549b0c7c0e86e0d3ee2a857b300963",
    ),
    (
        "directory_implicit_composite_root_1000",
        "455356322d444e4f44450001000010030000000800000066696c652e7478740182204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee040000006c696e6b03b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236060000006e65737465640200768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466",
        "70ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26",
    ),
    (
        "version_composite",
        "455356322d56524f4f5400010070ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26",
        "f2dfceb5f1618031b99634897ddc5c760421fcd92b53f8715b776a127e40effa",
    ),
];

    fn decode_hex(input: &str) -> Vec<u8> {
        assert_eq!(input.len() % 2, 0, "hex has an odd number of nibbles");
        input
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).expect("high nibble");
                let low = (pair[1] as char).to_digit(16).expect("low nibble");
                ((high << 4) | low) as u8
            })
            .collect()
    }

    #[test]
    fn frozen_m60_structural_preimages_recompute_exactly() {
        for (name, preimage_hex, expected_id) in STRUCTURAL_VECTORS {
            let preimage = decode_hex(preimage_hex);
            let expected_id = decode_hex(expected_id);
            assert_eq!(expected_id.len(), 32, "{name} digest width");
            assert_eq!(preimage.len() * 2, preimage_hex.len(), "{name} byte count");
            assert_eq!(
                blake3::hash(&preimage).as_bytes(),
                expected_id.as_slice(),
                "{name}"
            );
        }
    }

    #[test]
    fn frozen_fixture_text_contains_the_authoritative_vector_and_receipt_markers() {
        let golden = include_str!("../../../tests/fixtures/frozen/m6-0/M6_0_GOLDEN_VECTORS.md");
        let change_receipt =
            include_str!("../../../tests/fixtures/frozen/m6-0/M6_0_CHANGE_RECEIPT_V1.md");
        let generator_readme =
            include_str!("../../../tests/fixtures/frozen/m6-0/m6-vectors/README.md");
        let receipt =
            include_str!("../../../tests/fixtures/frozen/m6-1-2/M6_1_2_VERIFICATION_RECEIPT.md");
        let spec = include_str!("../../../tests/fixtures/frozen/m6-1-2/M6_1_SPEC.md");

        for name in [
            "logical_chunk_abc",
            "logical_file_abc",
            "file_node_0644_abc",
            "symlink_node_file_txt",
            "directory_explicit_0755_nested_file",
            "directory_implicit_empty_root_1000",
            "version_empty_root",
            "directory_implicit_composite_root_1000",
            "version_composite",
        ] {
            assert!(golden.contains(name), "missing frozen vector {name}");
        }
        assert!(change_receipt.contains("CONTRACT_STATUS: FROZEN_M6_0_CONTRACT_COMPLETE"));
        assert!(generator_readme.contains("DOCUMENTATION-ONLY"));
        assert!(receipt.contains("RECEIPT_STATE: FINAL_PASS"));
        assert!(receipt.contains("M6_1_2_CRITERIA_PASS: 6_OF_6"));
        assert!(spec.contains("M6.1"));
    }
}

mod l1_cdc {
    use crate::support::{expected, fastcdc_golden_input, sha256};
    use layerfs_storage::cdc::{
        BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1,
        CdcSourceErrorV1, ChunkBoundaryV1, FastCdcV1, FastCdcV1Stream, MAXIMUM_CHUNK_BYTES,
    };
    use layerfs_storage::CoreError;

    #[derive(Default)]
    struct Control {
        cancelled: bool,
        deadline: bool,
        cancellation_calls: usize,
        deadline_calls: usize,
    }

    impl CdcControlV1 for Control {
        fn cancellation_requested(&mut self) -> bool {
            self.cancellation_calls += 1;
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.deadline_calls += 1;
            self.deadline
        }
    }

    #[derive(Default)]
    struct RecordingConsumer {
        calls: usize,
        refuse_call: Option<usize>,
        ends: Vec<u64>,
        ranges: Vec<(u64, u64)>,
        bytes: Vec<u8>,
        saw_wrap: bool,
    }

    impl BoundaryConsumerV1 for RecordingConsumer {
        fn accept(
            &mut self,
            boundary: ChunkBoundaryV1,
            chunk: BorrowedChunkV1<'_>,
        ) -> Result<(), CdcBoundaryConsumerErrorV1> {
            self.calls += 1;
            if self.refuse_call == Some(self.calls) {
                return Err(CdcBoundaryConsumerErrorV1::Refused);
            }
            assert_eq!(boundary.len(), chunk.len() as u64);
            assert!(boundary.start() < boundary.end());
            assert!(chunk.len() <= MAXIMUM_CHUNK_BYTES);
            self.ends.push(boundary.end());
            self.ranges.push((boundary.start(), boundary.end()));
            self.bytes.extend_from_slice(chunk.first());
            self.bytes.extend_from_slice(chunk.second());
            self.saw_wrap |= !chunk.second().is_empty();
            Ok(())
        }
    }

    #[derive(Default)]
    struct PauseAfterFirstBoundary {
        inner: RecordingConsumer,
    }

    impl BoundaryConsumerV1 for PauseAfterFirstBoundary {
        fn accept(
            &mut self,
            boundary: ChunkBoundaryV1,
            chunk: BorrowedChunkV1<'_>,
        ) -> Result<(), CdcBoundaryConsumerErrorV1> {
            self.inner.accept(boundary, chunk)
        }

        fn pause_after_accepted_boundary(&self) -> bool {
            self.inner.calls == 1
        }
    }

    fn one_shot(source: &[u8]) -> Vec<u64> {
        let chunker = FastCdcV1::new();
        let mut ends = Vec::new();
        let mut offset = 0;
        while offset < source.len() {
            let amount = chunker.cut(&source[offset..]).expect("exact cut");
            assert!((1..=MAXIMUM_CHUNK_BYTES).contains(&amount));
            offset += amount;
            ends.push(offset as u64);
        }
        ends
    }

    fn run_fragments(
        source: &[u8],
        pattern: &[usize],
        interleave_empty: bool,
    ) -> RecordingConsumer {
        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut control = Control::default();
        let mut consumer = RecordingConsumer::default();
        let mut stream = FastCdcV1::new()
            .stream(&mut ring, &mut control)
            .expect("exact ring");
        let mut offset = 0;
        let mut index = 0;
        while offset < source.len() {
            if interleave_empty {
                stream
                    .push(Ok(&[]), &mut control, &mut consumer)
                    .expect("empty fragment");
            }
            let requested = pattern[index % pattern.len()];
            let end = (offset + requested).min(source.len());
            stream
                .push(Ok(&source[offset..end]), &mut control, &mut consumer)
                .expect("positive fragment");
            offset = end;
            index += 1;
        }
        stream
            .finish(&mut control, &mut consumer)
            .expect("finish stream");
        consumer
    }

    fn assert_coverage(ranges: &[(u64, u64)], source_len: usize) {
        let mut expected_start = 0;
        for &(start, end) in ranges {
            assert_eq!(start, expected_start);
            assert!(start < end);
            assert!(end - start <= MAXIMUM_CHUNK_BYTES as u64);
            expected_start = end;
        }
        assert_eq!(expected_start, source_len as u64);
    }

    #[test]
    fn exact_golden_boundaries_and_hostile_eof_edges_match() {
        let generated_32k = fastcdc_golden_input(32_768);
        let generated_100k = fastcdc_golden_input(100_000);
        assert_eq!(
            sha256(&generated_32k),
            expected("9d3dbe8a478f75fc9e66754267da822d5f7b20ece70bfdf03953d92a8c427363")
        );
        assert_eq!(
            sha256(&generated_100k),
            expected("ae185ec52770d5c67076421abd6c5579afb2598327824db5df6b6e9bbc5c96de")
        );
        assert_eq!(one_shot(&generated_32k), [16_688, 32_768]);
        assert_eq!(
            one_shot(&generated_100k),
            [16_688, 34_949, 52_688, 70_914, 90_807, 100_000]
        );
        assert_eq!(crate::naive_fastcdc::ends(&generated_32k), [16_688, 32_768]);
        assert_eq!(
            crate::naive_fastcdc::ends(&generated_100k),
            [16_688, 34_949, 52_688, 70_914, 90_807, 100_000]
        );
        for (len, expected_ends) in [
            (0, vec![]),
            (1, vec![1]),
            (8_191, vec![8_191]),
            (8_192, vec![8_192]),
            (8_193, vec![8_193]),
            (16_383, vec![16_383]),
            (16_384, vec![16_384]),
            (16_385, vec![16_385]),
            (32_767, vec![32_767]),
            (32_768, vec![32_768]),
            (32_769, vec![32_768, 32_769]),
            (65_537, vec![32_768, 65_536, 65_537]),
        ] {
            assert_eq!(one_shot(&vec![0; len]), expected_ends, "zero length {len}");
        }
    }

    #[test]
    fn seventeen_fragmentation_schedules_equal_one_shot_and_exercise_wrap() {
        const ODD_PRIMES: &[usize] = &[1, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31];
        const EDGE_CYCLE: &[usize] = &[
            1, 8_191, 8_192, 8_193, 16_383, 16_384, 16_385, 32_767, 32_768,
        ];
        let schedules: [(&[usize], bool); 17] = [
            (&[100_000], false),
            (&[1], false),
            (&[2], false),
            (ODD_PRIMES, false),
            (&[8_191], false),
            (&[8_192], false),
            (&[8_193], false),
            (&[16_383], false),
            (&[16_384], false),
            (&[16_385], false),
            (&[32_767], false),
            (&[32_768], false),
            (&[32_767, 1], false),
            (&[1, 32_767], false),
            (EDGE_CYCLE, false),
            (&[8_191], true),
            (&[31_337, 997, 17], false),
        ];
        let source = fastcdc_golden_input(100_000);
        let expected_ends = one_shot(&source);
        let mut saw_wrap = false;
        for (pattern, interleave_empty) in schedules {
            let captured = run_fragments(&source, pattern, interleave_empty);
            assert_eq!(captured.ends, expected_ends);
            assert_eq!(captured.bytes, source);
            assert_coverage(&captured.ranges, source.len());
            saw_wrap |= captured.saw_wrap;
        }
        assert!(
            saw_wrap,
            "at least one schedule must exercise both ring spans"
        );
    }

    #[test]
    fn lifecycle_errors_poison_terminally_and_finish_is_idempotent() {
        let mut invalid_ring = [0_u8; MAXIMUM_CHUNK_BYTES - 1];
        let mut control = Control::default();
        assert_eq!(
            FastCdcV1::new()
                .stream(&mut invalid_ring, &mut control)
                .err(),
            Some(CoreError::ResourceRefused)
        );

        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut control = Control::default();
        let mut consumer = RecordingConsumer::default();
        let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
        stream
            .push(Ok(b"final suffix"), &mut control, &mut consumer)
            .unwrap();
        stream.finish(&mut control, &mut consumer).unwrap();
        assert_eq!(consumer.ends, [12]);
        let calls = consumer.calls;
        stream.finish(&mut control, &mut consumer).unwrap();
        assert_eq!(consumer.calls, calls, "repeated finish emits nothing");
        assert_eq!(
            stream.push(Ok(b"late"), &mut control, &mut consumer),
            Err(CoreError::TrailingBytes)
        );

        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut control = Control::default();
        let mut consumer = RecordingConsumer::default();
        let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
        assert_eq!(
            stream.push(Err(CdcSourceErrorV1::Failure), &mut control, &mut consumer),
            Err(CoreError::SourceFailure)
        );
        control.cancelled = true;
        assert_eq!(
            stream.finish(&mut control, &mut consumer),
            Err(CoreError::SourceFailure)
        );

        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut control = Control::default();
        let mut consumer = RecordingConsumer {
            refuse_call: Some(1),
            ..RecordingConsumer::default()
        };
        let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
        assert_eq!(
            stream.push(Ok(&[0; MAXIMUM_CHUNK_BYTES]), &mut control, &mut consumer),
            Err(CoreError::SinkRefused)
        );
        assert!(
            consumer.ends.is_empty(),
            "refused boundary is not published"
        );
        let calls = consumer.calls;
        assert_eq!(
            stream.finish(&mut control, &mut consumer),
            Err(CoreError::SinkRefused)
        );
        assert_eq!(consumer.calls, calls, "poisoned state does not retry");
    }

    #[test]
    fn bounded_fragment_can_pause_at_an_accepted_boundary_without_extra_publication() {
        let source = fastcdc_golden_input(100_000);
        let first_end = one_shot(&source)[0] as usize;
        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut control = Control::default();
        let mut consumer = PauseAfterFirstBoundary::default();
        let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
        let consumed = stream
            .push_until_consumer_pause(Ok(&source), &mut control, &mut consumer)
            .unwrap();
        assert_eq!(consumer.inner.calls, 1);
        assert_eq!(consumer.inner.ends, [first_end as u64]);
        assert_eq!(consumer.inner.bytes, source[..first_end]);
        assert!((first_end..=first_end + 2).contains(&consumed));
        assert!(consumed < source.len());
        stream.finish_at_accepted_boundary(&mut control).unwrap();
        stream.finish_at_accepted_boundary(&mut control).unwrap();
    }

    #[test]
    fn cancellation_and_deadline_precede_source_and_are_terminal() {
        let mut invalid_ring = [0_u8; MAXIMUM_CHUNK_BYTES - 1];
        let mut control = Control {
            cancelled: true,
            deadline: true,
            ..Control::default()
        };
        assert_eq!(
            FastCdcV1::new()
                .stream(&mut invalid_ring, &mut control)
                .err(),
            Some(CoreError::Cancelled)
        );
        assert_eq!((control.cancellation_calls, control.deadline_calls), (1, 0));

        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut control = Control::default();
        let mut consumer = RecordingConsumer::default();
        let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
        control.deadline = true;
        assert_eq!(
            stream.push(Err(CdcSourceErrorV1::Failure), &mut control, &mut consumer),
            Err(CoreError::Deadline)
        );
        control.deadline = false;
        assert_eq!(
            stream.push(Ok(b"ignored"), &mut control, &mut consumer),
            Err(CoreError::Deadline)
        );
        assert_eq!(consumer.calls, 0);
    }

    #[derive(Default)]
    struct CountingConsumer {
        next_start: u64,
        chunks: u64,
        max_chunk: usize,
    }

    impl BoundaryConsumerV1 for CountingConsumer {
        fn accept(
            &mut self,
            boundary: ChunkBoundaryV1,
            chunk: BorrowedChunkV1<'_>,
        ) -> Result<(), CdcBoundaryConsumerErrorV1> {
            assert_eq!(boundary.start(), self.next_start);
            assert_eq!(boundary.len(), chunk.len() as u64);
            self.next_start = boundary.end();
            self.chunks += 1;
            self.max_chunk = self.max_chunk.max(chunk.len());
            Ok(())
        }
    }

    #[test]
    fn long_stream_retains_only_ring_reference_and_scalar_state() {
        let state_bytes = core::mem::size_of::<FastCdcV1Stream<'static>>();
        assert!(state_bytes <= 128);
        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut control = Control::default();
        let mut consumer = CountingConsumer::default();
        let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
        let mut fragment = [0_u8; 4_093];
        for (index, byte) in fragment.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(29).wrapping_add(17);
        }
        for _ in 0..4_100 {
            stream
                .push(Ok(&fragment), &mut control, &mut consumer)
                .unwrap();
        }
        stream.finish(&mut control, &mut consumer).unwrap();
        assert_eq!(consumer.next_start, 4_100 * fragment.len() as u64);
        assert!(consumer.chunks > 500);
        assert!(consumer.max_chunk <= MAXIMUM_CHUNK_BYTES);
        assert_eq!(
            state_bytes,
            core::mem::size_of::<FastCdcV1Stream<'static>>()
        );
    }
}

mod l1_identity {
    use layerfs_storage::format::{
        ValidatedComponent, ValidatedSymlinkTarget, ROOT_DIRECTORY_MODE_SENTINEL_V1,
    };
    use layerfs_storage::identity::{
        derive_explicit_directory_v1, derive_file_node_v1, derive_implicit_root_directory_v1,
        derive_logical_chunk_v1, derive_logical_file_v1, derive_physical_chunk_id_v1,
        derive_physical_file_id_v1, derive_symlink_node_v1, derive_version_v1, LogicalChildIdV1,
        LogicalChunkRefV1, LogicalDirectoryEntryV1,
    };
    use layerfs_storage::CoreError;
    use std::process::Command;

    fn expected(hex: &str) -> [u8; 32] {
        assert_eq!(hex.len(), 64);
        let mut bytes = [0_u8; 32];
        for (slot, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            let high = (pair[0] as char).to_digit(16).expect("high nibble");
            let low = (pair[1] as char).to_digit(16).expect("low nibble");
            *slot = ((high << 4) | low) as u8;
        }
        bytes
    }

    #[test]
    fn all_frozen_m60_logical_vectors_are_exact() {
        let chunk = derive_logical_chunk_v1(b"abc").expect("logical chunk");
        assert_eq!(
            chunk.id().as_bytes(),
            &expected("1174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c164")
        );
        let file = derive_logical_file_v1(3, &[LogicalChunkRefV1::from_identity(chunk)])
            .expect("logical file");
        assert_eq!(
            file.id().as_bytes(),
            &expected("c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d32")
        );
        let file_node = derive_file_node_v1(0o644, file).expect("file node");
        assert_eq!(
            file_node.as_bytes(),
            &expected("82204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee")
        );

        let target = ValidatedSymlinkTarget::new(b"file.txt").expect("target");
        let symlink = derive_symlink_node_v1(target).expect("symlink node");
        assert_eq!(
            symlink.as_bytes(),
            &expected("b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236")
        );

        let data = ValidatedComponent::new(b"data").expect("component");
        let nested = derive_explicit_directory_v1(
            0o755,
            &[LogicalDirectoryEntryV1::new(
                data,
                LogicalChildIdV1::File(file_node),
            )],
        )
        .expect("nested directory");
        assert_eq!(
            nested.id().as_bytes(),
            &expected("00768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466")
        );

        let empty_root = derive_implicit_root_directory_v1(&[]).expect("empty implicit root");
        assert_eq!(
            empty_root.id().as_bytes(),
            &expected("b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09")
        );
        let empty_version = derive_version_v1(empty_root);
        assert_eq!(
            empty_version.as_bytes(),
            &expected("44b0eb7c80a93ffc3cb98e4ff16c90d4a8549b0c7c0e86e0d3ee2a857b300963")
        );

        let composite_root = derive_implicit_root_directory_v1(&[
            LogicalDirectoryEntryV1::new(
                ValidatedComponent::new(b"file.txt").expect("component"),
                LogicalChildIdV1::File(file_node),
            ),
            LogicalDirectoryEntryV1::new(
                ValidatedComponent::new(b"link").expect("component"),
                LogicalChildIdV1::Symlink(symlink),
            ),
            LogicalDirectoryEntryV1::new(
                ValidatedComponent::new(b"nested").expect("component"),
                LogicalChildIdV1::Directory(nested),
            ),
        ])
        .expect("composite root");
        assert_eq!(
            composite_root.id().as_bytes(),
            &expected("70ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26")
        );
        assert_eq!(
            derive_version_v1(composite_root).as_bytes(),
            &expected("f2dfceb5f1618031b99634897ddc5c760421fcd92b53f8715b776a127e40effa")
        );
    }

    #[test]
    fn root_sentinel_and_explicit_modes_are_separate_domains() {
        assert_eq!(ROOT_DIRECTORY_MODE_SENTINEL_V1, 0x1000);
        assert_eq!(
            derive_explicit_directory_v1(ROOT_DIRECTORY_MODE_SENTINEL_V1, &[]),
            Err(CoreError::ChildMode)
        );
        let explicit = derive_explicit_directory_v1(0, &[]).expect("explicit empty directory");
        let implicit = derive_implicit_root_directory_v1(&[]).expect("implicit empty root");
        assert_ne!(explicit.id(), implicit.id());
    }

    #[test]
    fn ordering_lengths_endian_and_domains_change_or_reject_identity() {
        let empty = derive_logical_chunk_v1(&[]).expect("empty logical chunk");
        let one = derive_logical_chunk_v1(&[0]).expect("one-byte logical chunk");
        assert_ne!(empty.id(), one.id(), "length is in the canonical preimage");

        let file = derive_logical_file_v1(1, &[LogicalChunkRefV1::from_identity(one)])
            .expect("logical file");
        let file_node = derive_file_node_v1(0o644, file).expect("file node");
        assert_ne!(
            file.id().as_bytes(),
            file_node.as_bytes(),
            "logical-file and file-node domain separators differ"
        );

        let unordered = [
            LogicalDirectoryEntryV1::new(
                ValidatedComponent::new(b"z").expect("z"),
                LogicalChildIdV1::File(file_node),
            ),
            LogicalDirectoryEntryV1::new(
                ValidatedComponent::new(b"a").expect("a"),
                LogicalChildIdV1::File(file_node),
            ),
        ];
        assert_eq!(
            derive_implicit_root_directory_v1(&unordered),
            Err(CoreError::NonCanonicalOrder)
        );
    }

    #[test]
    fn physical_domains_are_separate_and_repacking_does_not_change_logical_identity() {
        let logical = derive_logical_chunk_v1(b"abc").expect("logical chunk");
        let physical_chunk =
            derive_physical_chunk_id_v1(b"same canonical envelope").expect("physical chunk digest");
        let physical_file =
            derive_physical_file_id_v1(b"same canonical envelope").expect("physical file digest");
        assert_ne!(physical_chunk.as_bytes(), physical_file.as_bytes());
        assert_ne!(logical.id().as_bytes(), physical_chunk.as_bytes());

        let before = derive_logical_file_v1(3, &[LogicalChunkRefV1::from_identity(logical)])
            .expect("logical file before repack");
        let _first_pack_placement = derive_physical_chunk_id_v1(b"physical encoding A").unwrap();
        let _second_pack_placement = derive_physical_chunk_id_v1(b"physical encoding B").unwrap();
        let after = derive_logical_file_v1(3, &[LogicalChunkRefV1::from_identity(logical)])
            .expect("logical file after repack");
        assert_eq!(
            before, after,
            "pack placement is absent from logical identity"
        );
    }

    #[test]
    fn repeated_derivation_is_process_stable_and_unkeyed() {
        let expected = expected("1174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c164");
        if std::env::var_os("LAYERFS_IDENTITY_REPEAT_CHILD").is_none() {
            let executable = std::env::current_exe().expect("current test executable");
            for _ in 0..2 {
                let output = Command::new(&executable)
                    .arg("--exact")
                    .arg("l1_identity::repeated_derivation_is_process_stable_and_unkeyed")
                    .arg("--nocapture")
                    .env("LAYERFS_IDENTITY_REPEAT_CHILD", "1")
                    .output()
                    .expect("spawn repeatability child");
                assert!(
                    output.status.success(),
                    "repeatability child failed: {}",
                    output.status
                );
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let sentinel_count = stdout
                    .matches("LAYERFS_IDENTITY_REPEAT_CHILD_OK_V1")
                    .count()
                    + stderr
                        .matches("LAYERFS_IDENTITY_REPEAT_CHILD_OK_V1")
                        .count();
                assert_eq!(
                    sentinel_count, 1,
                    "identity selector did not execute exactly one child"
                );
            }
        } else {
            println!("LAYERFS_IDENTITY_REPEAT_CHILD_OK_V1");
        }
        for _ in 0..1_024 {
            assert_eq!(
                derive_logical_chunk_v1(b"abc").unwrap().id().as_bytes(),
                &expected
            );
        }
    }
}

mod l1_profiles {
    use layerfs_storage::profile::{
        ChunkerSpecV1, DigestSpecV1, ProfileSpecV1, CHUNKER_SPEC_BYTES, DIGEST_SPEC_BYTES,
        PROFILE_SPEC_BYTES,
    };
    use layerfs_storage::CoreError;

    fn expected(hex: &str) -> [u8; 32] {
        assert_eq!(hex.len(), 64);
        let mut bytes = [0_u8; 32];
        for (slot, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            let high = (pair[0] as char).to_digit(16).expect("high nibble");
            let low = (pair[1] as char).to_digit(16).expect("low nibble");
            *slot = ((high << 4) | low) as u8;
        }
        bytes
    }

    #[test]
    fn frozen_profile_records_have_exact_lengths_seals_and_ids() {
        let digest = DigestSpecV1::frozen();
        let chunker = ChunkerSpecV1::frozen();
        let profile = ProfileSpecV1::frozen();

        assert_eq!(digest.canonical_bytes().len(), DIGEST_SPEC_BYTES);
        assert_eq!(chunker.canonical_bytes().len(), CHUNKER_SPEC_BYTES);
        assert_eq!(profile.canonical_bytes().len(), PROFILE_SPEC_BYTES);
        assert_eq!(
            sha256(digest.canonical_bytes()),
            expected("2d9ac18268aeffbf22c8a75391d4fe3d6e384334309a241f89f0276b4022e515")
        );
        assert_eq!(
            sha256(chunker.canonical_bytes()),
            expected("6c4fe77a0d015024d2d4dc60f0186343c0214dbf5c681d3446785b68fd479f84")
        );
        assert_eq!(
            sha256(profile.canonical_bytes()),
            expected("726b30797b6eac121dd2ccd65e48a8632e38a04a8157750bcfec08e58797fc94")
        );
        assert_eq!(
            digest.id().as_bytes(),
            &expected("ea17622d03b3baaacf09dff8877df6d8834306c32d9ed5d950a6a15ef01ad5cb")
        );
        assert_eq!(
            chunker.id().as_bytes(),
            &expected("88b1ca8e3b1d9076916818a484b907e9bc6913fe54013ced176c6d9eb23408e7")
        );
        assert_eq!(
            profile.id().as_bytes(),
            &expected("3d372f239a0e55b7001f0cb89648de46650de4c43421d645c927a2f7d0d8702b")
        );
    }

    #[test]
    fn frozen_gear_and_gear_ls_table_seals_are_exact() {
        let chunker = ChunkerSpecV1::frozen();
        let raw_table = &chunker.canonical_bytes()[68..];
        assert_eq!(raw_table.len(), 256 * 8);
        assert_eq!(
            sha256(raw_table),
            expected("9df0a720752a7d211fdebaf39bed01610983756fc340a1cfef41052b7356ae73")
        );

        let mut shifted = Vec::with_capacity(raw_table.len());
        let mut interleaved = Vec::with_capacity(raw_table.len() * 2);
        for word in raw_table.chunks_exact(8) {
            let gear = u64::from_be_bytes(word.try_into().expect("eight-byte GEAR word"));
            let gear_ls = gear.wrapping_shl(1).to_be_bytes();
            shifted.extend_from_slice(&gear_ls);
            interleaved.extend_from_slice(word);
            interleaved.extend_from_slice(&gear_ls);
        }
        assert_eq!(
            sha256(&shifted),
            expected("93123c215ae531383c1b660bb185d4013ba3c87faa99796879f97c4076bdfce2")
        );
        assert_eq!(
            sha256(&interleaved),
            expected("0ff906fefd2f6ce85c431130c1146e62746dc6e984d96d8d13ce7a55359d113a")
        );
    }

    #[test]
    fn profile_decoders_fail_closed_on_mutations_and_non_exact_input() {
        let digest = *DigestSpecV1::frozen().canonical_bytes();
        assert_eq!(
            DigestSpecV1::decode_exact(&digest),
            Ok(DigestSpecV1::frozen())
        );
        assert_eq!(
            DigestSpecV1::decode_exact(&digest[..15]),
            Err(CoreError::Truncated)
        );
        let mut digest_trailing = digest.to_vec();
        digest_trailing.push(0);
        assert_eq!(
            DigestSpecV1::decode_exact(&digest_trailing),
            Err(CoreError::TrailingBytes)
        );
        let mut digest_reserved = digest;
        digest_reserved[15] = 1;
        assert_eq!(
            DigestSpecV1::decode_exact(&digest_reserved),
            Err(CoreError::Reserved)
        );

        let chunker = *ChunkerSpecV1::frozen().canonical_bytes();
        assert_eq!(
            ChunkerSpecV1::decode_exact(&chunker),
            Ok(ChunkerSpecV1::frozen())
        );
        assert_eq!(
            ChunkerSpecV1::decode_exact(&chunker[..2_115]),
            Err(CoreError::Truncated)
        );
        let mut chunker_mask = chunker;
        chunker_mask[39] ^= 1;
        assert_eq!(
            ChunkerSpecV1::decode_exact(&chunker_mask),
            Err(CoreError::TypeDomain)
        );
        let mut chunker_table = chunker;
        chunker_table[68] ^= 1;
        assert_eq!(
            ChunkerSpecV1::decode_exact(&chunker_table),
            Err(CoreError::TypeDomain)
        );

        let profile = *ProfileSpecV1::frozen().canonical_bytes();
        assert_eq!(
            ProfileSpecV1::decode_exact(&profile),
            Ok(ProfileSpecV1::frozen())
        );
        assert_eq!(
            ProfileSpecV1::decode_exact(&profile[..135]),
            Err(CoreError::Truncated)
        );
        let mut profile_flags = profile;
        profile_flags[11] = 1;
        assert_eq!(
            ProfileSpecV1::decode_exact(&profile_flags),
            Err(CoreError::Flags)
        );
        let mut profile_reserved = profile;
        profile_reserved[99] = 1;
        assert_eq!(
            ProfileSpecV1::decode_exact(&profile_reserved),
            Err(CoreError::Reserved)
        );
    }

    fn sha256(input: &[u8]) -> [u8; 32] {
        const INITIAL: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];
        const ROUND: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        let bit_len = u64::try_from(input.len())
            .expect("test input fits u64")
            .checked_mul(8)
            .expect("test bit length fits u64");
        let padded_len = input
            .len()
            .checked_add(9)
            .and_then(|len| len.checked_add((64 - (len % 64)) % 64))
            .expect("test padding length fits usize");
        let mut padded = vec![0_u8; padded_len];
        padded[..input.len()].copy_from_slice(input);
        padded[input.len()] = 0x80;
        padded[padded_len - 8..].copy_from_slice(&bit_len.to_be_bytes());

        let mut state = INITIAL;
        for block in padded.chunks_exact(64) {
            let mut schedule = [0_u32; 64];
            for (index, word) in block.chunks_exact(4).enumerate() {
                schedule[index] = u32::from_be_bytes(word.try_into().expect("four-byte SHA word"));
            }
            for index in 16..64 {
                let s0 = schedule[index - 15].rotate_right(7)
                    ^ schedule[index - 15].rotate_right(18)
                    ^ (schedule[index - 15] >> 3);
                let s1 = schedule[index - 2].rotate_right(17)
                    ^ schedule[index - 2].rotate_right(19)
                    ^ (schedule[index - 2] >> 10);
                schedule[index] = schedule[index - 16]
                    .wrapping_add(s0)
                    .wrapping_add(schedule[index - 7])
                    .wrapping_add(s1);
            }

            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
            for index in 0..64 {
                let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let choice = (e & f) ^ ((!e) & g);
                let temporary1 = h
                    .wrapping_add(sum1)
                    .wrapping_add(choice)
                    .wrapping_add(ROUND[index])
                    .wrapping_add(schedule[index]);
                let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let majority = (a & b) ^ (a & c) ^ (b & c);
                let temporary2 = sum0.wrapping_add(majority);
                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(temporary1);
                d = c;
                c = b;
                b = a;
                a = temporary1.wrapping_add(temporary2);
            }
            for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
                *slot = slot.wrapping_add(value);
            }
        }

        let mut output = [0_u8; 32];
        for (slot, word) in output.chunks_exact_mut(4).zip(state) {
            slot.copy_from_slice(&word.to_be_bytes());
        }
        output
    }
}

#[cfg(feature = "operation-polymorphism")]
mod l1_resources {
    use layerfs_storage::qualification::resources::{
        base_ledger_bytes_v1, observe_forbidden_work_v1, observe_memory_plan_v1,
        observe_memory_profile_v1, operation_slot_bytes_v1, MemoryBudgetV1, MemoryResourceKindV1,
    };
    use layerfs_storage::CoreError;

    #[test]
    fn qualified_memory_profiles_admit_the_exact_frozen_slot_counts() {
        for (budget, expected_slots) in [
            (MemoryBudgetV1::ThirtyTwoMiB, 6),
            (MemoryBudgetV1::FortyEightMiB, 10),
            (MemoryBudgetV1::SeventyTwoMiB, 16),
        ] {
            let observation = observe_memory_profile_v1(
                budget.bytes(),
                &[
                    (MemoryResourceKindV1::CdcRing, 32_768),
                    (MemoryResourceKindV1::SourceWindow, 65_536),
                    (MemoryResourceKindV1::HashState, 2_048),
                ],
            )
            .unwrap();
            assert_eq!(budget.expected_capacity_slots(), expected_slots);
            assert_eq!(observation.capacity_slots(), expected_slots);
            assert_eq!(observation.admitted_slots(), expected_slots);
            assert_eq!(
                observation.high_water_bytes(),
                base_ledger_bytes_v1() + expected_slots * operation_slot_bytes_v1()
            );
            assert_eq!(
                observation.planned_high_water_bytes(),
                base_ledger_bytes_v1() + expected_slots * (32_768 + 65_536 + 2_048)
            );
            assert_eq!(
                observation.over_capacity_error(),
                Some(CoreError::ResourceRefused)
            );
            assert!(observation.cleaned_up());
        }
    }

    #[test]
    fn memory_plan_rejects_double_charging_and_slot_overflow() {
        let observation = observe_memory_plan_v1().unwrap();
        assert!(observation.comparison_window_present());
        assert_eq!(observation.total_bytes(), 65_536);
        assert_eq!(
            observation.duplicate_resource_error(),
            Some(CoreError::ResourceRefused)
        );
        assert_eq!(
            observation.slot_overflow_error(),
            Some(CoreError::ResourceRefused)
        );
    }

    #[test]
    fn forbidden_work_counters_are_writable_and_checked() {
        let observation = observe_forbidden_work_v1().unwrap();
        assert!(observation.zero_before());
        assert!(!observation.zero_after());
        assert_eq!(observation.retry_attempts(), 1);
        assert_eq!(observation.redispatches(), 1);
        assert_eq!(observation.automatic_fallbacks(), 1);
        assert_eq!(observation.provider_switches(), 1);
        assert_eq!(observation.cdc_switches(), 1);
        assert_eq!(observation.publication_dispatches(), 1);
        assert_eq!(observation.update_to_replace_fallbacks(), 1);
        assert_eq!(observation.full_base_payload_fallbacks(), 1);
        assert_eq!(observation.file_sync_calls(), 1);
        assert_eq!(observation.directory_sync_calls(), 1);
        assert_eq!(observation.wal_or_recovery_operations(), 1);
        assert_eq!(observation.memory_backend_operations(), 1);
        assert_eq!(observation.whole_pack_copies(), 1);
        assert_eq!(observation.filesystem_clone_reflink_operations(), 1);
        assert_eq!(observation.layerfs_created_threads(), 1);
        assert_eq!(observation.rayon_work_units(), 1);
        assert_eq!(observation.source_sized_staging_allocations(), 1);
        assert_eq!(observation.workspace_sized_staging_allocations(), 1);
    }

    #[test]
    fn update_has_no_static_replace_retry_or_publication_path() {
        let source = include_str!("../src/content/update.rs");
        assert!(!source.contains("replace_file_v1"));
        assert!(!source.contains("record_fallback_attempt"));
        assert!(!source.contains("record_redispatch"));
        assert!(!source.contains("record_publication_dispatch"));
    }
}

#[cfg(feature = "operation-polymorphism")]
mod l1_content {
    use crate::support;

    use layerfs_storage::qualification::content::semantic::{
        base_budget_bytes, create_and_replace_v1, create_v1, expected_planned_high_water,
        observe_failure_v1, operation_slot_bytes, ContentRequestV1,
    };
    use layerfs_storage::qualification::resources::{
        observe_memory_profile_v1, MemoryBudgetV1, MemoryResourceKindV1,
    };
    use layerfs_storage::CoreError;

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            output.push(char::from(DIGITS[usize::from(byte >> 4)]));
            output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
        output
    }

    #[test]
    fn large_source_uses_fixed_io_windows_and_the_qualified_memory_ledger() {
        let resources = [
            (MemoryResourceKindV1::CdcRing, 32_768),
            (MemoryResourceKindV1::SourceWindow, 65_536),
            (MemoryResourceKindV1::HashState, 2_048),
        ];
        let refusal =
            observe_memory_profile_v1(MemoryBudgetV1::ThirtyTwoMiB.bytes(), &resources).unwrap();
        let profile_48 =
            observe_memory_profile_v1(MemoryBudgetV1::FortyEightMiB.bytes(), &resources).unwrap();
        let profile_72 =
            observe_memory_profile_v1(MemoryBudgetV1::SeventyTwoMiB.bytes(), &resources).unwrap();
        assert_eq!(refusal.capacity_slots(), 6);
        assert_eq!(profile_48.capacity_slots(), 10);
        assert_eq!(profile_72.capacity_slots(), 16);
        assert_eq!(refusal.admitted_slots(), 6);
        assert_eq!(refusal.high_water_bytes(), 32 * 1024 * 1024);
        assert_eq!(
            refusal.over_capacity_error(),
            Some(CoreError::ResourceRefused)
        );
        assert!(refusal.cleaned_up());

        let data = support::fastcdc_golden_input(64 * 1024 * 1024);
        let length = data.len() as u64;
        let observation = create_v1(&ContentRequestV1::new(b"large.bin", 0o644, &data)).unwrap();

        assert!(observation.completed());
        assert_eq!(observation.source_remaining(), 0);
        assert_eq!(observation.bytes_read(), length);
        assert_eq!(observation.bytes_copied(), length);
        assert!(observation.source_max_request() <= 32_768);
        assert!(observation.sink_max_segment() <= 32_768);
        assert_eq!(
            observation.object_count(),
            u64::from(observation.chunk_count()) + 1
        );
        assert_eq!(
            observation.memory_high_water(),
            base_budget_bytes() + operation_slot_bytes()
        );
        assert_eq!(
            observation.ledger_high_water(),
            base_budget_bytes() + operation_slot_bytes()
        );
        assert_eq!(
            observation.planned_high_water(),
            expected_planned_high_water(length)
        );
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn create_and_explicit_replace_produce_exact_prepared_closures() {
        let data = support::fastcdc_golden_input(100_000);
        let request = ContentRequestV1::new(b"dir/file.bin", 0o644, &data);
        let (created, replaced) = create_and_replace_v1(&request).unwrap();

        assert!(created.completed());
        assert_eq!(created.bytes_read(), data.len() as u64);
        assert_eq!(created.source_read_calls(), 5);
        assert_eq!(created.bytes_copied(), data.len() as u64);
        assert_eq!(created.ring_fills(), 6);
        assert_eq!(created.ring_wrap_spans(), 3);
        assert_eq!(created.cdc_scan_calls(), 11);
        assert_eq!(created.cdc_scan_bytes(), data.len() as u64);
        assert!(created.bytes_boundary_inspected() > 0);
        assert!(created.bytes_boundary_inspected() <= created.cdc_scan_bytes());
        assert_eq!(created.logical_hash_bytes(), data.len() as u64);
        assert!(created.logical_hash_update_calls() > 0);
        assert!(created.physical_hash_bytes() > data.len() as u64);
        assert!(created.physical_hash_update_calls() > 0);
        assert!(created.zero_forbidden_work());
        assert_eq!(created.admitted_slots(), 0);
        assert_eq!(created.chunk_count() as u64, created.spool_ref_count());
        assert!(created.file_object_observed());
        assert_eq!(created.file_chunk_ref_count(), created.chunk_count() as u64);
        assert_eq!(created.logical_len(), data.len() as u64);

        assert_eq!(
            hex(&created.logical_id()),
            "6a3e35920006d8c817c8c526cf523b2b4ce676f01197587d0f5847414028c103"
        );
        assert_eq!(
            hex(&created.physical_id()),
            "e033f4a110b2b8e3e797a7b9bfa5958b1128756298d0b9de43e160b7f73998fb"
        );

        assert!(replaced.completed());
        assert_eq!(replaced.logical_id(), created.logical_id());
        assert_eq!(replaced.physical_id(), created.physical_id());
        assert_eq!(replaced.logical_len(), created.logical_len());
        assert_eq!(replaced.chunk_count(), created.chunk_count());
        assert_eq!(
            replaced.file_chunk_ref_count(),
            created.file_chunk_ref_count()
        );
        assert!(replaced.logical_chunks_reused() > 0);
        assert!(replaced.physical_objects_reused() > 0);
        assert!(replaced.zero_forbidden_work());
        assert_eq!(replaced.admitted_slots(), 0);
    }

    fn assert_rejected(request: ContentRequestV1<'_>, expected: CoreError) {
        let observation = observe_failure_v1(&request);
        assert_eq!(observation.error(), expected);
        assert!(!observation.sink_active());
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(observation.bytes_read(), 0);
        assert_eq!(observation.bytes_copied(), 0);
    }

    #[test]
    fn validation_and_reservation_happen_before_source_consumption() {
        let data = b"content";
        assert_rejected(
            ContentRequestV1::new(b"file", 0o644, data).with_budget(base_budget_bytes()),
            CoreError::ResourceRefused,
        );
        assert_rejected(
            ContentRequestV1::new(b"file", 0o644, data)
                .with_spool_residency(operation_slot_bytes()),
            CoreError::ResourceRefused,
        );
        assert_rejected(
            ContentRequestV1::new(b"file", 0o644, data)
                .with_source_residency(operation_slot_bytes()),
            CoreError::ResourceRefused,
        );
        assert_rejected(
            ContentRequestV1::new(b"file", 0o644, data).with_sink_residency(operation_slot_bytes()),
            CoreError::ResourceRefused,
        );
        assert_rejected(
            ContentRequestV1::new(b"/absolute", 0o644, data),
            CoreError::Path,
        );
    }

    #[test]
    fn invalid_source_count_is_rejected_without_slicing_or_partial_visibility() {
        let observation = observe_failure_v1(
            &ContentRequestV1::new(b"file.bin", 0o644, b"x").with_invalid_source_count(true),
        );
        assert_eq!(observation.error(), CoreError::SourceFailure);
        assert_eq!(observation.bytes_read(), 0);
        assert_eq!(observation.bytes_copied(), 0);
        assert_eq!(observation.sink_aborts(), 1);
        assert!(observation.spool_aborted());
        assert!(!observation.sink_active());
        assert_eq!(observation.admitted_slots(), 0);
    }
}

#[cfg(feature = "operation-polymorphism")]
mod operation_create_owner {
    use crate::support::temp_fs_cas::TempFsCas;
    use layerfs_storage::cdc::CdcAlgorithmV1;
    use layerfs_storage::qualification::cas::semantic::PublicationErrorV1;
    use layerfs_storage::qualification::lifecycle::semantic::{
        complete_create_case_v1, equivalent_create_lifecycle_v1, CompleteCreateCaseV1,
        CompleteCreateCountersV1, CompleteCreateObservationV1,
    };
    use layerfs_storage::CoreError;

    fn run(label: &str, case: CompleteCreateCaseV1) -> CompleteCreateObservationV1 {
        let fixture = TempFsCas::new(label);
        let observation = complete_create_case_v1(fixture.path(), case);
        assert_storage_equations(observation.counters);
        assert_eq!(observation.operation_admitted_slots, 0);
        assert_eq!(observation.operation_admission_active, 0);
        assert_eq!(observation.operation_admission_queue, (0, 0, 0));
        assert_eq!(observation.storage_admission_active, (0, 0, 0));
        assert_eq!(observation.preparation_entries, 0);
        observation
    }

    fn assert_storage_equations(counters: CompleteCreateCountersV1) {
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            Some(counters.storage_bytes_reserved),
            counters
                .storage_bytes_released
                .checked_add(counters.storage_bytes_committed)
                .and_then(|value| value.checked_add(counters.storage_bytes_retained))
        );
        assert_eq!(
            Some(counters.storage_inodes_reserved),
            counters
                .storage_inodes_released
                .checked_add(counters.storage_inodes_committed)
                .and_then(|value| value.checked_add(counters.storage_inodes_retained))
        );
    }

    fn overflow() -> Option<PublicationErrorV1> {
        Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
    }

    fn saturated_created(counters: CompleteCreateCountersV1) -> usize {
        [
            counters.version_objects_created,
            counters.tree_objects_created,
            counters.file_objects_created,
            counters.symlink_objects_created,
            counters.chunk_objects_created,
        ]
        .into_iter()
        .filter(|value| *value == u64::MAX)
        .count()
    }

    fn saturated_reused(counters: CompleteCreateCountersV1) -> usize {
        [
            counters.version_objects_reused,
            counters.tree_objects_reused,
            counters.file_objects_reused,
            counters.symlink_objects_reused,
            counters.chunk_objects_reused,
        ]
        .into_iter()
        .filter(|value| *value == u64::MAX)
        .count()
    }

    #[test]
    fn one_file_and_multi_entry_create_share_outer_lifecycle_trace_and_fault_terminal() {
        let fixture = TempFsCas::new("equivalent-create-lifecycle");
        let observation = equivalent_create_lifecycle_v1(fixture.path());
        let one = observation.success_one_counters;
        let tree = observation.success_tree_counters;
        let failed_one = observation.failed_one_counters;
        let failed_tree = observation.failed_tree_counters;

        assert_eq!(observation.success_traces_equal, true);
        assert_eq!(observation.success_starts_at_slot_reservation, true);
        assert_eq!(observation.success_ends_at_validated_handoff, true);
        assert!(observation.failed_one_control_fired);
        assert!(observation.failed_tree_control_fired);
        assert_eq!(observation.failure_errors_equal, true);
        assert_eq!(observation.failure_traces_equal, true);
        assert!(observation.failure_trace_has_no_handoff);
        assert_eq!(observation.failed_one_clean, true);
        assert_eq!(observation.failed_tree_clean, true);
        assert_eq!(one.storage_bytes_requested, one.storage_bytes_reserved);
        assert_eq!(tree.storage_bytes_requested, tree.storage_bytes_reserved);
        assert!(one.storage_bytes_requested > 0);
        assert_eq!(
            one.storage_bytes_reserved,
            one.storage_bytes_released + one.storage_bytes_committed + one.storage_bytes_retained
        );
        assert_eq!(
            tree.storage_bytes_reserved,
            tree.storage_bytes_released
                + tree.storage_bytes_committed
                + tree.storage_bytes_retained
        );
        assert_eq!(
            failed_one.storage_bytes_requested,
            failed_one.storage_bytes_reserved
        );
        assert_eq!(
            failed_tree.storage_bytes_requested,
            failed_tree.storage_bytes_reserved
        );
        assert!(tree.storage_bytes_requested > 0);
        assert!(one.root_reserved_bytes_high_water >= one.storage_bytes_reserved);
        assert!(tree.root_reserved_bytes_high_water >= tree.storage_bytes_reserved);
        assert!(failed_one.root_reserved_bytes_high_water >= failed_one.storage_bytes_reserved);
        assert_eq!(one.storage_inodes_requested, one.storage_inodes_reserved);
        assert_eq!(tree.storage_inodes_requested, tree.storage_inodes_reserved);
        assert_eq!(
            failed_one.storage_inodes_requested,
            failed_one.storage_inodes_reserved
        );
        assert_eq!(
            failed_tree.storage_inodes_requested,
            failed_tree.storage_inodes_reserved
        );
        assert_eq!(one.storage_bytes_retained, 0);
        assert!(one.zero_forbidden_work);
        assert!(tree.zero_forbidden_work);
        assert_eq!(tree.storage_bytes_retained, 0);
        assert_eq!(one.storage_inodes_retained, 0);
        assert_eq!(tree.storage_inodes_retained, 0);
        assert_eq!(one.mutable_preparation_residue_bytes, 0);
        assert!(failed_one.zero_forbidden_work);
        assert!(failed_tree.zero_forbidden_work);
        assert_eq!(one.mutable_preparation_residue_inodes, 0);
        assert_eq!(tree.mutable_preparation_residue_bytes, 0);
        assert_eq!(tree.mutable_preparation_residue_inodes, 0);
        assert_eq!(failed_one.mutable_preparation_residue_bytes, 0);
        assert_eq!(failed_one.mutable_preparation_residue_inodes, 0);
        assert_eq!(failed_tree.mutable_preparation_residue_bytes, 0);
        assert_eq!(failed_tree.mutable_preparation_residue_inodes, 0);
        assert!(one.storage_equations_hold);
        assert_eq!(tree.storage_equations_hold, true);
        assert_eq!(failed_one.storage_equations_hold, true);
        assert_eq!(failed_tree.storage_equations_hold, true);
        assert_eq!(one.unreachable_installed_residue_bytes, 0);
        assert_eq!(tree.unreachable_installed_residue_bytes, 0);
        assert_eq!(failed_one.unreachable_installed_residue_bytes, 0);
        assert_eq!(failed_tree.unreachable_installed_residue_bytes, 0);
        assert_eq!(one.storage_bytes_committed > 0, true);
        assert_eq!(tree.storage_bytes_committed > 0, true);
        assert_eq!(one.storage_inodes_committed > 0, true);
        assert_eq!(tree.storage_inodes_committed > 0, true);
        assert_eq!(failed_one.storage_bytes_committed, 0);
        assert_eq!(failed_tree.storage_bytes_committed, 0);
        assert_eq!(
            (
                failed_one.storage_inodes_committed,
                failed_tree.storage_inodes_committed
            ),
            (0, 0)
        );
        assert!(one.closure_fences == 1 && tree.closure_fences == 1);
        assert_eq!(failed_one.closure_fences, 0);
        assert!(failed_tree.closure_fences == 0);
    }

    #[test]
    fn closure_marker_preparation_holds_neither_root_fence() {
        let observation = run(
            "closure-marker-lock-scope",
            CompleteCreateCaseV1::ClosureMarkerLockScope,
        );
        assert!(observation.closure_marker_observed);
        assert!(observation.visibility_lock_available);
        assert!(observation.publication_lock_available);
        assert_eq!(observation.closure_publication_acquisitions, 2);
        assert_eq!(observation.closure_publication_releases, 2);
        assert_eq!(
            observation.counters.visibility_lock_acquisitions,
            observation.observed_visibility_acquisitions
        );
        assert_eq!(
            observation.observed_visibility_acquisitions,
            observation.observed_visibility_releases
        );
        assert_eq!(
            observation.counters.publication_lock_acquisitions,
            observation.observed_publication_acquisitions
        );
        assert_eq!(
            observation.observed_publication_acquisitions,
            observation.observed_publication_releases
        );
        assert!(observation.counters.storage_equations_hold);
    }

    #[test]
    fn writer_transfers_direct_visibility_and_publication_observations() {
        let observation = run(
            "writer-direct-root-lock-observations",
            CompleteCreateCaseV1::WriterDirectLockObservations,
        );
        assert!(observation.counters.visibility_lock_acquisitions > 0);
        assert!(observation.counters.publication_lock_acquisitions > 0);
        assert!(observation.counters.storage_equations_hold);
        assert!(observation.counters.zero_forbidden_work);
    }

    #[test]
    fn lifecycle_storage_counter_merge_overflow_is_transactional_and_terminal() {
        let observation = run(
            "lifecycle-storage-counter-merge-overflow",
            CompleteCreateCaseV1::StorageCounterMergeOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), true, true)
        );
        assert_eq!(counters.physical_carrier_object_writes, 41);
        assert_eq!(counters.pack_entries, 43);
        assert_eq!(counters.pack_bytes, 47);
        assert_eq!(counters.carrier_bytes_total, u64::MAX);
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert!(counters.storage_bytes_retained > 0);
        assert!(counters.storage_inodes_retained > 0);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(
            counters.unreachable_installed_residue_bytes,
            observation.immutable_usage.0
        );
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_cdc_stream_overflow_is_transactional_and_terminal() {
        let observation = run(
            "complete-create-cdc-counter-overflow",
            CompleteCreateCaseV1::FastCdcCounterOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), true, true)
        );
        assert_eq!(counters.ring_fills, 41);
        assert_eq!(counters.ring_wrap_spans, 43);
        assert_eq!(counters.cdc_scan_calls, 47);
        assert_eq!(counters.cdc_scan_bytes, 53);
        assert_eq!(counters.bytes_boundary_inspected, u64::MAX);
        assert!(counters.source_read_calls > 0);
        assert!(counters.source_bytes_read > 0);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_seqcdc_overflow_is_transactional_and_terminal() {
        const LOGICAL_BYTES: u64 = 64 * 1024;
        let observation = run(
            "complete-create-seqcdc-counter-overflow",
            CompleteCreateCaseV1::SeqCdcCounterOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), true, true)
        );
        assert_eq!(counters.seqcdc_comparisons, 41);
        assert_eq!(counters.seqcdc_equal_absorptions, 43);
        assert_eq!(counters.seqcdc_opposing_slopes, 47);
        assert_eq!(counters.seqcdc_jumps, 53);
        assert_eq!(counters.seqcdc_jump_bytes, u64::MAX);
        assert!(counters.ring_fills > 0);
        assert!(counters.cdc_scan_calls > 0);
        assert!(counters.cdc_scan_bytes > 0);
        assert!(counters.bytes_boundary_inspected > 0);
        assert!(counters.source_read_calls > 0);
        assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_global_seen_overflow_is_transactional_and_terminal() {
        const LOGICAL_BYTES: u64 = 64 * 1024;
        let observation = run(
            "complete-create-global-seen-counter-overflow",
            CompleteCreateCaseV1::GlobalSeenCounterOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), true, true)
        );
        assert!(observation.control_fired);
        assert_eq!(counters.global_seen_lookups, 41);
        assert_eq!(counters.global_seen_probes, 43);
        assert_eq!(counters.global_seen_metadata_bytes_read, 47);
        assert_eq!(counters.global_seen_metadata_read_calls, 53);
        assert_eq!(counters.global_seen_metadata_bytes_written, u64::MAX);
        assert_eq!(counters.global_seen_maximum_probe, 59);
        assert_eq!(counters.global_seen_entries, 61);
        assert_eq!(counters.global_seen_table_bytes, 67);
        assert!(counters.source_read_calls > 0);
        assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
        assert!(counters.cdc_scan_calls > 0);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(
            counters.unreachable_installed_residue_bytes,
            observation.immutable_usage.0
        );
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_operation_spool_write_overflow_retains_typed_cause_and_cleans() {
        let observation = run(
            "complete-create-operation-spool-write-overflow",
            CompleteCreateCaseV1::OperationSpoolWriteOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.error_from_storage,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), true, true, true)
        );
        assert!(observation.control_fired);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(
            counters.unreachable_installed_residue_bytes,
            observation.immutable_usage.0
        );
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_operation_spool_read_overflow_is_transactional_and_terminal() {
        let observation = run(
            "complete-create-operation-spool-read-overflow",
            CompleteCreateCaseV1::OperationSpoolReadOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.error_from_storage,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), true, true, true)
        );
        assert_eq!(
            (
                counters.global_seen_metadata_bytes_read,
                counters.global_seen_metadata_read_calls
            ),
            (71, u64::MAX)
        );
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(
            counters.unreachable_installed_residue_bytes,
            observation.immutable_usage.0
        );
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_counted_pack_read_overflow_is_transactional_and_terminal() {
        let observation = run(
            "complete-create-counted-pack-read-overflow",
            CompleteCreateCaseV1::CountedPackReadOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.error_from_storage,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), false, true, true)
        );
        assert!(observation.control_fired);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(observation.immutable_usage, (0, 0));
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_same_carrier_comparison_overflow_is_transactional_and_terminal() {
        const LOGICAL_BYTES: u64 = 64 * 1024;
        let observation = run(
            "complete-create-same-carrier-comparison-overflow",
            CompleteCreateCaseV1::SameCarrierComparisonOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.error_from_storage,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), false, true, true)
        );
        assert!(observation.control_fired);
        assert!(counters.source_read_calls > 0);
        assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
        assert!(counters.cdc_scan_calls > 0);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(observation.immutable_usage, (0, 0));
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_post_admission_tally_overflow_retains_exact_visible_residue() {
        let observation = run(
            "complete-create-post-admission-tally-overflow",
            CompleteCreateCaseV1::PostAdmissionCarrierTallyOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.error_from_storage,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), false, true, true)
        );
        assert!(observation.control_fired);
        assert!(counters.source_read_calls > 0);
        assert!(counters.cdc_scan_calls > 0);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert!(observation.immutable_usage.0 > 0);
        assert!(observation.immutable_usage.1 > 0);
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(
            counters.unreachable_installed_residue_bytes,
            observation.immutable_usage.0
        );
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_create_created_disposition_overflow_is_transactional_and_terminal() {
        let observation = run(
            "complete-create-created-disposition-overflow",
            CompleteCreateCaseV1::CreatedDispositionOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.error_from_storage,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), false, true, true)
        );
        assert!(observation.control_fired);
        assert_eq!(saturated_created(counters), 1);
        assert_eq!(counters.pack_local_objects_created, 0);
        assert_eq!(counters.physical_carrier_object_writes, 0);
        assert_eq!(counters.pack_local_objects_reused, 0);
        assert_eq!(saturated_reused(counters), 0);
        assert!(counters.source_read_calls > 0);
        assert_eq!(counters.source_bytes_read, 1);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_tree_reused_disposition_overflow_is_transactional_and_terminal() {
        let observation = run(
            "complete-tree-reused-disposition-overflow",
            CompleteCreateCaseV1::TreeReusedDispositionOverflow,
        );
        let counters = observation.counters;
        assert_eq!(
            (
                observation.error,
                observation.error_from_storage,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (overflow(), false, true, true)
        );
        assert!(observation.control_fired);
        assert_eq!(saturated_reused(counters), 1);
        assert_eq!(counters.pack_local_objects_reused, 0);
        assert_eq!(
            counters.physical_carrier_object_writes,
            counters.pack_local_objects_created
        );
        assert!(counters.pack_local_objects_created > 0);
        assert!(counters.source_read_calls >= 2);
        assert_eq!(counters.source_bytes_read, 2);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            (
                counters.storage_bytes_retained,
                counters.storage_inodes_retained
            ),
            observation.immutable_usage
        );
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert!(counters.zero_forbidden_work);
        assert!(observation.root_usable);
        assert!(observation.stale_usable);
    }

    #[test]
    fn complete_algorithms_use_one_pre_supplier_slot_and_return_all_preparation_resources() {
        const LOGICAL_BYTES: u64 = 384 * 1024 + 73;
        for algorithm in [CdcAlgorithmV1::FastCdc, CdcAlgorithmV1::SeqCdc] {
            let observation = run(
                "complete-create-algorithm",
                CompleteCreateCaseV1::Algorithm(algorithm),
            );
            let counters = observation.counters;
            assert_eq!(
                observation.algorithm,
                Some(algorithm),
                "terminal: {:?}",
                observation.error
            );
            assert!(observation.pack_installed);
            assert_eq!(
                (
                    observation.operation_authority_clean,
                    counters.storage_equations_hold
                ),
                (true, true)
            );
            assert!(observation.object_count >= 4);
            assert_eq!(observation.reference_spool_observed, true);
            assert!(observation.reference_spool_bytes.unwrap_or(0) > 0);
            assert_eq!(observation.reference_spool_operation_scoped, true);
            assert_eq!(
                observation.reference_spool_method,
                "direct chunk-reference spool logical length"
            );
            assert_eq!(observation.index_spool_observed, true);
            assert!(observation.index_spool_bytes.unwrap_or(0) > 0);
            assert_eq!(observation.index_spool_operation_scoped, true);
            assert_eq!(
                observation.index_spool_method,
                "direct cumulative pack-index spool logical length"
            );
            assert_eq!(
                observation.terminal_optional_observations_match_counters,
                true
            );
            assert!(observation.terminal_optional_observations_empty);
            assert_eq!(observation.preparation_usage, (0, 0));
            assert!(counters.source_read_calls > 0);
            assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
            assert!(counters.fscas_read_calls > 0);
            assert!(counters.fscas_bytes_read > 0);
            assert!(counters.fscas_bytes_written > 0);
            assert_eq!(counters.closure_fences, 1);
            assert_eq!(counters.logical_reconstruction_cdc_passes, 1);
            assert_eq!(counters.logical_reconstruction_logical_bytes, LOGICAL_BYTES);
            assert!(counters.logical_reconstruction_payload_read_calls > 0);
            assert_eq!(counters.logical_reconstruction_payload_bytes, LOGICAL_BYTES);
            assert!(counters.logical_reconstruction_control_polls > 0);
            assert!(
                counters.logical_reconstruction_maximum_work_between_polls
                    <= layerfs_storage::cdc::MAXIMUM_CHUNK_BYTES as u64
            );
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert!(counters.storage_bytes_committed > 0);
            assert!(counters.storage_inodes_committed > 0);
            assert_eq!(counters.storage_bytes_retained, 0);
            assert_eq!(counters.storage_inodes_retained, 0);
            assert_eq!(counters.mutable_preparation_residue_bytes, 0);
            assert_eq!(counters.mutable_preparation_residue_inodes, 0);
            assert_eq!(observation.preparation_usage.0, 0);
            assert_eq!(observation.preparation_usage.1, 0);
            assert_eq!(
                counters.storage_bytes_committed,
                observation.immutable_usage.0
            );
            assert_eq!(
                counters.storage_inodes_committed,
                observation.immutable_usage.1
            );
            assert_eq!(counters.immutable_residue_inodes, 0);
            assert!(counters.zero_forbidden_work);
        }
    }

    #[test]
    fn complete_create_reconstruction_is_equivalent_across_frozen_fragmentation_schedules() {
        const LOGICAL_BYTES: u64 = 2 * (2 * 32 * 1024 + 17) + (32 * 1024 + 29);
        let observations = [
            ("fragmentation-one-byte", 1_u32),
            ("fragmentation-997-bytes", 997_u32),
            (
                "fragmentation-maximum-chunk",
                layerfs_storage::cdc::MAXIMUM_CHUNK_BYTES as u32,
            ),
        ]
        .map(|(label, maximum_read)| {
            run(
                label,
                CompleteCreateCaseV1::FragmentationSchedule(maximum_read),
            )
        });
        let expected = observations[0];

        for observation in observations {
            let counters = observation.counters;
            assert_eq!(observation.error, None);
            assert_eq!(observation.algorithm, Some(CdcAlgorithmV1::FastCdc));
            assert_eq!(observation.version_record, expected.version_record);
            assert_eq!(observation.root_tree, expected.root_tree);
            assert_eq!(observation.pack, expected.pack);
            assert_eq!(observation.object_count, expected.object_count);
            assert_eq!(
                observation.closure_object_count,
                Some(observation.object_count)
            );
            assert_eq!(observation.closure_transcript, expected.closure_transcript);
            assert_eq!(
                (
                    counters.pack_entries,
                    counters.pack_bytes,
                    counters.carrier_bytes_total,
                ),
                (
                    expected.counters.pack_entries,
                    expected.counters.pack_bytes,
                    expected.counters.carrier_bytes_total,
                )
            );
            assert!(counters.source_read_calls > 0);
            assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
            assert_eq!(counters.logical_reconstruction_cdc_passes, 1);
            assert_eq!(counters.logical_reconstruction_logical_bytes, LOGICAL_BYTES);
            assert!(counters.logical_reconstruction_payload_read_calls > 0);
            assert_eq!(counters.logical_reconstruction_payload_bytes, LOGICAL_BYTES);
            assert!(counters.logical_reconstruction_control_polls > 0);
            assert!(
                counters.logical_reconstruction_maximum_work_between_polls
                    <= layerfs_storage::cdc::MAXIMUM_CHUNK_BYTES as u64
            );
            assert!(counters.file_objects_reused > 0);
            assert!(counters.chunk_objects_reused > 0);
            assert_eq!(counters.closure_fences, 1);
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert!(observation.operation_authority_clean);
            assert!(counters.zero_forbidden_work);
        }
    }

    #[test]
    fn exact_10_mib_complete_operation_is_the_fast_iteration_path() {
        const LOGICAL_BYTES: u64 = 10 * 1024 * 1024;
        let observation = run("iteration-10m", CompleteCreateCaseV1::Exact10MiB);
        let counters = observation.counters;
        assert_eq!(
            (
                observation.carrier_count,
                observation.carrier_rollovers,
                observation.operation_authority_clean,
                counters.storage_equations_hold,
            ),
            (1, 0, true, true)
        );
        assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
        assert_eq!(counters.logical_reconstruction_cdc_passes, 1);
        assert_eq!(counters.logical_reconstruction_logical_bytes, LOGICAL_BYTES);
        assert_eq!(counters.logical_reconstruction_payload_bytes, LOGICAL_BYTES);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.zero_forbidden_work);
    }

    #[test]
    fn exact_100_mib_complete_operation_rolls_over_real_fscas_carriers() {
        const LOGICAL_BYTES: u64 = 100 * 1024 * 1024;
        let observation = run("multi-pack-100m", CompleteCreateCaseV1::Exact100MiB);
        let counters = observation.counters;
        assert_eq!(
            (
                observation.carrier_count,
                observation.operation_authority_clean,
                counters.storage_equations_hold
            ),
            (2, true, true)
        );
        assert_eq!(observation.carrier_rollovers, 1);
        assert_eq!(observation.carriers_installed, 2);
        assert_eq!(observation.carriers_reused, 0);
        assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
        assert_eq!(counters.closure_fences, 1);
        assert_eq!(counters.logical_reconstruction_cdc_passes, 1);
        assert_eq!(counters.logical_reconstruction_logical_bytes, LOGICAL_BYTES);
        assert!(counters.logical_reconstruction_payload_read_calls > 0);
        assert_eq!(counters.logical_reconstruction_payload_bytes, LOGICAL_BYTES);
        assert!(counters.logical_reconstruction_control_polls > 0);
        assert!(
            counters.logical_reconstruction_maximum_work_between_polls
                <= layerfs_storage::cdc::MAXIMUM_CHUNK_BYTES as u64
        );
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.file_sort_comparisons > 0);
        assert!(counters.file_sort_record_reads > 0);
        assert!(counters.file_sort_record_writes > 0);
        assert!(counters.file_sort_passes > 0);
        assert!(counters.file_sort_control_polls > 0);
        assert_eq!(
            counters.file_sort_work_units,
            counters.file_sort_comparisons
                + counters.file_sort_record_reads
                + counters.file_sort_record_writes
        );
        assert!(counters.file_sort_maximum_work_budget > 0);
        assert_eq!(counters.file_sort_temporary_bytes_high_water, 0);
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(
            counters.storage_bytes_committed,
            observation.immutable_usage.0
        );
        assert_eq!(
            counters.storage_inodes_committed,
            observation.immutable_usage.1
        );
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert!(counters.zero_forbidden_work);
    }
}
