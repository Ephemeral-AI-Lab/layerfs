use layerfs_storage::format::{
    checked_encoded_len, checked_u32, compare_paths_unsigned, compare_unsigned, require_at_most,
    require_nonzero_at_most, require_strictly_increasing, require_strictly_increasing_paths,
    validate_chunk_object_count, validate_chunk_reference_len, validate_chunk_refs_per_file,
    validate_chunk_refs_per_version, validate_component, validate_directory_mode,
    validate_directory_page_depth, validate_domain, validate_entry_count,
    validate_extents_per_file, validate_extents_per_version, validate_file_mode,
    validate_flags_zero, validate_index_page_depth, validate_leaf_page_depth,
    validate_logical_chunk_payload_len, validate_logical_length, validate_path,
    validate_physical_chunk_payload_len, validate_physical_object_len, validate_reserved_zero,
    validate_schema_v1, validate_symlink_target, validate_total_object_count,
    validate_tree_index_fanout, validate_tree_leaf_fanout, validate_tree_object_count, ByteSink,
    DirectoryModeContext, ExtentTagV1, LogicalChildKindV1, PathValidator, PhysicalObjectKindV1,
    PhysicalTreeChildKindV1, PresenceV1, SliceCursor, SliceSink, TreeSubtypeV1, ValidatedComponent,
    ValidatedPath, ValidatedSymlinkTarget, MAX_CHUNK_BYTES, MAX_CHUNK_OBJECTS,
    MAX_CHUNK_REFS_PER_FILE, MAX_CHUNK_REFS_PER_VERSION, MAX_COMPONENT_BYTES, MAX_ENTRIES,
    MAX_EXTENTS_PER_FILE, MAX_EXTENTS_PER_VERSION, MAX_LOGICAL_BYTES, MAX_PATH_BYTES,
    MAX_PATH_DEPTH, MAX_PHYSICAL_OBJECT_BYTES, MAX_SYMLINK_TARGET_BYTES, MAX_TOTAL_OBJECTS,
    MAX_TREE_INDEX_FANOUT, MAX_TREE_LEAF_FANOUT, MAX_TREE_OBJECTS, MAX_TREE_PAGE_DEPTH,
    ROOT_DIRECTORY_MODE_SENTINEL_V1,
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
