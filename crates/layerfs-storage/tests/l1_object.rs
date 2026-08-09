use layerfs_storage::object::{
    decode_physical_object_from_port_v1, decode_physical_object_v1, PhysicalObjectPayloadV1,
    PhysicalObjectReadPortV1, StrongEdgeV1, StrongEdgeVisitorV1, TypedPhysicalObjectIdV1,
};
use layerfs_storage::profile::ProfileSpecV1;
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

#[derive(Default)]
struct RecordingVisitor {
    pending: Vec<StrongEdgeV1>,
    visible: Vec<StrongEdgeV1>,
    begins: usize,
    commits: usize,
    aborts: usize,
    refuse_after: Option<usize>,
}

impl StrongEdgeVisitorV1 for RecordingVisitor {
    fn begin_object(&mut self) {
        self.begins += 1;
        assert!(self.pending.is_empty());
    }

    fn visit_edge(&mut self, edge: StrongEdgeV1) -> CoreResult<()> {
        if self.refuse_after == Some(self.pending.len()) {
            return Err(CoreError::SinkRefused);
        }
        self.pending.push(edge);
        Ok(())
    }

    fn commit_object(&mut self) {
        self.commits += 1;
        self.visible.append(&mut self.pending);
    }

    fn abort_object(&mut self) {
        self.aborts += 1;
        self.pending.clear();
    }
}

struct FragmentedReadPort<'a> {
    bytes: &'a [u8],
    maximum_request: usize,
    reads: u64,
}

impl PhysicalObjectReadPortV1 for FragmentedReadPort<'_> {
    fn len(&mut self) -> CoreResult<u64> {
        u64::try_from(self.bytes.len()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
        self.maximum_request = self.maximum_request.max(destination.len());
        self.reads = self
            .reads
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let start = usize::try_from(offset).map_err(|_| CoreError::IntegerOverflow)?;
        let end = start
            .checked_add(destination.len())
            .ok_or(CoreError::IntegerOverflow)?;
        destination.copy_from_slice(self.bytes.get(start..end).ok_or(CoreError::SourceFailure)?);
        Ok(())
    }
}

#[test]
fn all_five_exact_object_kinds_decode_and_hash_in_separate_domains() {
    let root = [0x20; 32];
    let mut version_payload = Vec::with_capacity(184);
    version_payload.extend_from_slice(&[0x01; 32]);
    version_payload.extend_from_slice(&[0x02; 32]);
    version_payload.extend_from_slice(&[0x03; 32]);
    version_payload.extend_from_slice(&root);
    version_payload.extend_from_slice(&[0; 56]);
    assert_eq!(version_payload.len(), 184);

    let fixtures = [
        object(1, &version_payload),
        object(2, &[1, 0, 0, 0, 0, 0, 0, 0, 0]),
        object(3, &[0; 14]),
        object(4, &[0, 0, 0, 1, b'x']),
        object(5, &[0]),
    ];
    for (index, fixture) in fixtures.iter().enumerate() {
        let mut visitor = RecordingVisitor::default();
        let decoded = decode_physical_object_v1(fixture, &mut visitor).expect("valid object");
        assert_eq!(decoded.canonical_bytes(), fixture);
        assert_eq!(decoded.header().complete_len() as usize, fixture.len());
        assert_eq!(visitor.commits, 1);
        assert_eq!(visitor.aborts, 0);
        match (index, decoded.payload(), decoded.physical_id().unwrap()) {
            (
                0,
                PhysicalObjectPayloadV1::VersionRecord(_),
                TypedPhysicalObjectIdV1::VersionRecord(_),
            )
            | (1, PhysicalObjectPayloadV1::Tree(_), TypedPhysicalObjectIdV1::Tree(_))
            | (2, PhysicalObjectPayloadV1::File(_), TypedPhysicalObjectIdV1::File(_))
            | (3, PhysicalObjectPayloadV1::Symlink(_), TypedPhysicalObjectIdV1::Symlink(_))
            | (4, PhysicalObjectPayloadV1::Chunk(_), TypedPhysicalObjectIdV1::Chunk(_)) => {}
            _ => panic!("decoded kind or typed identity mismatch"),
        }
    }

    let mut visitor = RecordingVisitor::default();
    let chunk = decode_physical_object_v1(&fixtures[4], &mut visitor).unwrap();
    let TypedPhysicalObjectIdV1::Chunk(chunk_id) = chunk.physical_id().unwrap() else {
        panic!("expected chunk identity");
    };
    assert_eq!(
        chunk_id.as_bytes(),
        &expected("808d9a7d53976a4381d95842de87e1f45f996b75bc7601ecd076da11f0f36524")
    );
}

#[test]
fn typed_edges_stream_in_wire_order_without_decoder_edge_storage() {
    let tree_id = [0x10; 32];
    let file_id = [0x11; 32];
    let symlink_id = [0x12; 32];
    let mut leaf = vec![2, 0, 0, 3];
    for (name, kind, id) in [
        (b"a".as_slice(), 1_u8, tree_id),
        (b"b".as_slice(), 2_u8, file_id),
        (b"c".as_slice(), 3_u8, symlink_id),
    ] {
        leaf.extend_from_slice(&(name.len() as u16).to_be_bytes());
        leaf.extend_from_slice(name);
        leaf.push(kind);
        leaf.extend_from_slice(&id);
    }
    let mut visitor = RecordingVisitor::default();
    let leaf_object = object(2, &leaf);
    let decoded = decode_physical_object_v1(&leaf_object, &mut visitor).unwrap();
    assert!(matches!(
        decoded.payload(),
        PhysicalObjectPayloadV1::Tree(_)
    ));
    assert_eq!(visitor.visible.len(), 3);
    assert!(matches!(visitor.visible[0], StrongEdgeV1::Tree(_)));
    assert!(matches!(visitor.visible[1], StrongEdgeV1::File(_)));
    assert!(matches!(visitor.visible[2], StrongEdgeV1::Symlink(_)));

    let chunk_id = [0x33; 32];
    let mut file = Vec::new();
    file.extend_from_slice(&0o644_u16.to_be_bytes());
    file.extend_from_slice(&3_u64.to_be_bytes());
    file.extend_from_slice(&1_u32.to_be_bytes());
    file.push(2);
    file.extend_from_slice(&3_u64.to_be_bytes());
    file.extend_from_slice(&1_u32.to_be_bytes());
    file.extend_from_slice(&3_u32.to_be_bytes());
    file.extend_from_slice(&chunk_id);
    let mut visitor = RecordingVisitor::default();
    let file_object = object(3, &file);
    let decoded = decode_physical_object_v1(&file_object, &mut visitor).unwrap();
    let PhysicalObjectPayloadV1::File(summary) = decoded.payload() else {
        panic!("expected File");
    };
    assert_eq!(summary.logical_len, 3);
    assert_eq!(summary.chunk_ref_count, 1);
    assert!(matches!(
        visitor.visible.as_slice(),
        [StrongEdgeV1::Chunk(_)]
    ));
}

#[test]
fn hostile_envelopes_fail_before_visiting_edges() {
    let valid = object(5, &[0]);
    let cases = [
        (9, 1, CoreError::Schema),
        (10, 5, CoreError::UnknownKind),
        (11, 1, CoreError::Flags),
        (12, 1, CoreError::TypeDomain),
    ];
    for (offset, xor, expected_error) in cases {
        let mut mutated = valid.clone();
        mutated[offset] ^= xor;
        let mut visitor = RecordingVisitor::default();
        assert_eq!(
            decode_physical_object_v1(&mutated, &mut visitor),
            Err(expected_error)
        );
        assert_eq!(visitor.begins, 0);
    }

    let mut unknown = valid.clone();
    unknown[10] = 0xff;
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&unknown, &mut visitor),
        Err(CoreError::UnknownKind)
    );
    assert_eq!(visitor.begins, 0);

    let mut truncated = valid.clone();
    truncated.pop();
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&truncated, &mut visitor),
        Err(CoreError::Truncated)
    );
    assert_eq!(visitor.begins, 0);

    let mut trailing = valid.clone();
    trailing.push(0);
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&trailing, &mut visitor),
        Err(CoreError::TrailingBytes)
    );
    assert_eq!(visitor.begins, 0);

    let mut huge = valid.clone();
    huge[44..52].copy_from_slice(&u64::MAX.to_be_bytes());
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&huge, &mut visitor),
        Err(CoreError::IntegerOverflow)
    );
    assert_eq!(visitor.begins, 0);
}

#[test]
fn hostile_payloads_abort_provisional_edges_and_reject_bad_order() {
    let mut duplicate_leaf = vec![2, 0, 0, 2];
    for id in [[0x10; 32], [0x11; 32]] {
        duplicate_leaf.extend_from_slice(&1_u16.to_be_bytes());
        duplicate_leaf.push(b'a');
        duplicate_leaf.push(1);
        duplicate_leaf.extend_from_slice(&id);
    }
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&object(2, &duplicate_leaf), &mut visitor),
        Err(CoreError::NonCanonicalOrder)
    );
    assert!(visitor.visible.is_empty());
    assert_eq!(visitor.aborts, 1);

    let mut late_trailing_leaf = vec![2, 0, 0, 1];
    late_trailing_leaf.extend_from_slice(&1_u16.to_be_bytes());
    late_trailing_leaf.push(b'a');
    late_trailing_leaf.push(1);
    late_trailing_leaf.extend_from_slice(&[0x20; 32]);
    late_trailing_leaf.push(0);
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&object(2, &late_trailing_leaf), &mut visitor),
        Err(CoreError::TrailingBytes)
    );
    assert!(visitor.visible.is_empty());
    assert!(visitor.pending.is_empty());
    assert_eq!(visitor.aborts, 1);

    let mut refused = vec![2, 0, 0, 1];
    refused.extend_from_slice(&1_u16.to_be_bytes());
    refused.push(b'a');
    refused.push(2);
    refused.extend_from_slice(&[0x30; 32]);
    let mut visitor = RecordingVisitor {
        refuse_after: Some(0),
        ..RecordingVisitor::default()
    };
    assert_eq!(
        decode_physical_object_v1(&object(2, &refused), &mut visitor),
        Err(CoreError::SinkRefused)
    );
    assert!(visitor.visible.is_empty());
    assert_eq!(visitor.aborts, 1);
}

#[test]
fn loop_counts_are_preflighted_against_declared_payload_bytes() {
    let leaf = object(2, &[2, 0, 0, 192]);
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&leaf, &mut visitor),
        Err(CoreError::LogicalLength)
    );
    assert_eq!(
        visitor.begins, 0,
        "minimum payload check precedes visitation"
    );

    let mut file = Vec::new();
    file.extend_from_slice(&0_u16.to_be_bytes());
    file.extend_from_slice(&1_u64.to_be_bytes());
    file.extend_from_slice(&262_144_u32.to_be_bytes());
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_v1(&object(3, &file), &mut visitor),
        Err(CoreError::LogicalLength)
    );
    assert_eq!(visitor.aborts, 1);
}

#[test]
fn bounded_random_read_decoder_matches_borrowed_decoder_and_never_requests_a_large_buffer() {
    let mut leaf = vec![2, 0, 0, 2];
    for (name, kind, id) in [(b"a", 1_u8, [0x41; 32]), (b"b", 2_u8, [0x42; 32])] {
        leaf.extend_from_slice(&(name.len() as u16).to_be_bytes());
        leaf.extend_from_slice(name);
        leaf.push(kind);
        leaf.extend_from_slice(&id);
    }
    let bytes = object(2, &leaf);
    let borrowed = decode_physical_object_v1(&bytes, &mut RecordingVisitor::default()).unwrap();
    let mut port = FragmentedReadPort {
        bytes: &bytes,
        maximum_request: 0,
        reads: 0,
    };
    let mut visitor = RecordingVisitor::default();
    let mut scratch = [0_u8; 65_536];
    let streamed =
        decode_physical_object_from_port_v1(&mut port, &mut visitor, &mut scratch).unwrap();
    assert_eq!(streamed.header(), borrowed.header());
    assert_eq!(streamed.payload(), borrowed.payload());
    assert_eq!(streamed.physical_id(), borrowed.physical_id().unwrap());
    assert_eq!(visitor.visible.len(), 2);
    assert!(port.reads > 1, "validation must use bounded random reads");
    assert!(port.maximum_request <= 65_536);

    let mut duplicate = leaf;
    duplicate[42] = b'a';
    let duplicate = object(2, &duplicate);
    let mut port = FragmentedReadPort {
        bytes: &duplicate,
        maximum_request: 0,
        reads: 0,
    };
    let mut visitor = RecordingVisitor::default();
    assert_eq!(
        decode_physical_object_from_port_v1(&mut port, &mut visitor, &mut scratch),
        Err(CoreError::NonCanonicalOrder)
    );
    assert!(visitor.visible.is_empty());
    assert_eq!(visitor.aborts, 1);
}
