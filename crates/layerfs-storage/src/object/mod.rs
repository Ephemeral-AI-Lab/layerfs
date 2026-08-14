//! Canonical typed-object model, codec, and bounded traversal laws.
//!
//! Object identity and canonical bytes remain backend neutral. Filesystem,
//! pack, CAS, lifecycle, content, and read layers consume these semantic
//! definitions without acquiring ownership of the frozen object format.

mod decode;
mod encode;
mod model;
mod port_decode;
mod traversal;

pub mod semantic {
    use super::{
        decode_physical_object_from_port_v1, decode_physical_object_v1, PhysicalObjectHeaderV1,
        PhysicalObjectPayloadV1, PhysicalObjectReadPortV1, StrongEdgeV1, StrongEdgeVisitorV1,
        TypedPhysicalObjectIdV1,
    };
    use crate::format::PhysicalObjectKindV1;
    use crate::identity::COMPARISON_WINDOW_BYTES;
    use crate::{CoreError, CoreResult};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ObjectKindV1 {
        VersionRecord,
        Tree,
        File,
        Symlink,
        Chunk,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum EdgeKindV1 {
        Tree,
        File,
        Symlink,
        Chunk,
    }

    pub struct ObjectDecodeRequestV1<'a> {
        bytes: &'a [u8],
        bounded_random_read: bool,
        refuse_after: Option<usize>,
    }

    impl<'a> ObjectDecodeRequestV1<'a> {
        pub const fn new(bytes: &'a [u8]) -> Self {
            Self {
                bytes,
                bounded_random_read: false,
                refuse_after: None,
            }
        }

        pub const fn with_bounded_random_read(mut self) -> Self {
            self.bounded_random_read = true;
            self
        }

        pub const fn with_refuse_after(mut self, edge_count: usize) -> Self {
            self.refuse_after = Some(edge_count);
            self
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ObjectDecodeObservationV1 {
        error: Option<CoreError>,
        object_kind: Option<ObjectKindV1>,
        identity_kind: Option<ObjectKindV1>,
        physical_id: [u8; 32],
        canonical_len: u64,
        complete_len: u64,
        payload_fingerprint: [u8; 32],
        file_logical_len: Option<u64>,
        file_chunk_ref_count: Option<u64>,
        edge_kinds: Vec<EdgeKindV1>,
        begins: usize,
        commits: usize,
        aborts: usize,
        pending_edges: usize,
        reads: u64,
        maximum_request: usize,
    }

    impl ObjectDecodeObservationV1 {
        pub const fn error(&self) -> Option<CoreError> {
            self.error
        }

        pub const fn object_kind(&self) -> Option<ObjectKindV1> {
            self.object_kind
        }

        pub const fn identity_kind(&self) -> Option<ObjectKindV1> {
            self.identity_kind
        }

        pub const fn physical_id(&self) -> [u8; 32] {
            self.physical_id
        }

        pub const fn canonical_len(&self) -> u64 {
            self.canonical_len
        }

        pub const fn complete_len(&self) -> u64 {
            self.complete_len
        }

        pub const fn payload_fingerprint(&self) -> [u8; 32] {
            self.payload_fingerprint
        }

        pub const fn file_logical_len(&self) -> Option<u64> {
            self.file_logical_len
        }

        pub const fn file_chunk_ref_count(&self) -> Option<u64> {
            self.file_chunk_ref_count
        }

        pub fn edge_kinds(&self) -> &[EdgeKindV1] {
            &self.edge_kinds
        }

        pub const fn begins(&self) -> usize {
            self.begins
        }

        pub const fn commits(&self) -> usize {
            self.commits
        }

        pub const fn aborts(&self) -> usize {
            self.aborts
        }

        pub const fn pending_edges(&self) -> usize {
            self.pending_edges
        }

        pub const fn reads(&self) -> u64 {
            self.reads
        }

        pub const fn maximum_request(&self) -> usize {
            self.maximum_request
        }
    }

    #[derive(Default)]
    struct Visitor {
        pending: Vec<EdgeKindV1>,
        visible: Vec<EdgeKindV1>,
        begins: usize,
        commits: usize,
        aborts: usize,
        refuse_after: Option<usize>,
    }

    impl Visitor {
        fn new(refuse_after: Option<usize>) -> Self {
            Self {
                refuse_after,
                ..Self::default()
            }
        }
    }

    impl StrongEdgeVisitorV1 for Visitor {
        fn begin_object(&mut self) {
            self.begins += 1;
            assert!(self.pending.is_empty());
        }

        fn visit_edge(&mut self, edge: StrongEdgeV1) -> CoreResult<()> {
            if self.refuse_after == Some(self.pending.len()) {
                return Err(CoreError::SinkRefused);
            }
            self.pending.push(match edge {
                StrongEdgeV1::Tree(_) => EdgeKindV1::Tree,
                StrongEdgeV1::File(_) => EdgeKindV1::File,
                StrongEdgeV1::Symlink(_) => EdgeKindV1::Symlink,
                StrongEdgeV1::Chunk(_) => EdgeKindV1::Chunk,
            });
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

    struct FragmentedRead<'a> {
        bytes: &'a [u8],
        reads: u64,
        maximum_request: usize,
    }

    impl PhysicalObjectReadPortV1 for FragmentedRead<'_> {
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
            destination
                .copy_from_slice(self.bytes.get(start..end).ok_or(CoreError::SourceFailure)?);
            Ok(())
        }
    }

    fn object_kind(kind: PhysicalObjectKindV1) -> ObjectKindV1 {
        match kind {
            PhysicalObjectKindV1::VersionRecord => ObjectKindV1::VersionRecord,
            PhysicalObjectKindV1::Tree => ObjectKindV1::Tree,
            PhysicalObjectKindV1::File => ObjectKindV1::File,
            PhysicalObjectKindV1::Symlink => ObjectKindV1::Symlink,
            PhysicalObjectKindV1::Chunk => ObjectKindV1::Chunk,
        }
    }

    fn fingerprint(payload: PhysicalObjectPayloadV1) -> [u8; 32] {
        let mut bytes = Vec::new();
        macro_rules! put {
            ($value:expr) => {
                bytes.extend_from_slice(&$value.to_be_bytes())
            };
        }
        match payload {
            PhysicalObjectPayloadV1::VersionRecord(record) => {
                bytes.push(1);
                bytes.extend_from_slice(record.version_id.as_bytes());
                bytes.extend_from_slice(record.chunker_spec_id.as_bytes());
                bytes.extend_from_slice(record.digest_spec_id.as_bytes());
                bytes.extend_from_slice(record.root_tree_id.as_bytes());
                put!(record.canonical_len);
                put!(record.logical_file_bytes);
                put!(record.entry_count);
                put!(record.tree_count);
                put!(record.file_count);
                put!(record.symlink_count);
                put!(record.chunk_count);
                put!(record.extent_count);
                put!(record.chunk_ref_count);
                put!(record.total_object_count);
                put!(record.physical_chunk_bytes);
            }
            PhysicalObjectPayloadV1::Tree(record) => {
                bytes.push(2);
                match record {
                    super::TreeRecordV1::Directory(record) => {
                        bytes.push(1);
                        put!(record.mode);
                        put!(record.entry_count);
                        bytes.push(record.page_depth);
                        bytes.push(record.root_page_id.is_some() as u8);
                        if let Some(id) = record.root_page_id {
                            bytes.extend_from_slice(id.as_bytes());
                        }
                    }
                    super::TreeRecordV1::Leaf(record) => {
                        bytes.push(2);
                        bytes.push(record.depth);
                        put!(record.count);
                    }
                    super::TreeRecordV1::Index(record) => {
                        bytes.push(3);
                        bytes.push(record.depth);
                        put!(record.count);
                        put!(record.subtree_entry_count);
                    }
                }
            }
            PhysicalObjectPayloadV1::File(record) => {
                bytes.push(3);
                put!(record.mode);
                put!(record.logical_len);
                put!(record.extent_count);
                put!(record.chunk_ref_count);
            }
            PhysicalObjectPayloadV1::Symlink(record) => {
                bytes.push(4);
                put!(record.target_len);
            }
            PhysicalObjectPayloadV1::Chunk(record) => {
                bytes.push(5);
                put!(record.payload_len);
            }
        }
        *blake3::hash(&bytes).as_bytes()
    }

    fn empty_observation(error: Option<CoreError>, visitor: Visitor) -> ObjectDecodeObservationV1 {
        ObjectDecodeObservationV1 {
            error,
            object_kind: None,
            identity_kind: None,
            physical_id: [0; 32],
            canonical_len: 0,
            complete_len: 0,
            payload_fingerprint: [0; 32],
            file_logical_len: None,
            file_chunk_ref_count: None,
            edge_kinds: visitor.visible,
            begins: visitor.begins,
            commits: visitor.commits,
            aborts: visitor.aborts,
            pending_edges: visitor.pending.len(),
            reads: 0,
            maximum_request: 0,
        }
    }

    fn complete_observation(
        header: PhysicalObjectHeaderV1,
        payload: PhysicalObjectPayloadV1,
        identity: TypedPhysicalObjectIdV1,
        visitor: Visitor,
        reads: u64,
        maximum_request: usize,
    ) -> ObjectDecodeObservationV1 {
        let (file_logical_len, file_chunk_ref_count) = match payload {
            PhysicalObjectPayloadV1::File(record) => {
                (Some(record.logical_len), Some(record.chunk_ref_count))
            }
            _ => (None, None),
        };
        ObjectDecodeObservationV1 {
            error: None,
            object_kind: Some(object_kind(header.kind())),
            identity_kind: Some(object_kind(identity.kind())),
            physical_id: *identity.as_bytes(),
            canonical_len: header.complete_len(),
            complete_len: header.complete_len(),
            payload_fingerprint: fingerprint(payload),
            file_logical_len,
            file_chunk_ref_count,
            edge_kinds: visitor.visible,
            begins: visitor.begins,
            commits: visitor.commits,
            aborts: visitor.aborts,
            pending_edges: visitor.pending.len(),
            reads,
            maximum_request,
        }
    }

    pub fn decode_v1(request: ObjectDecodeRequestV1<'_>) -> ObjectDecodeObservationV1 {
        let mut visitor = Visitor::new(request.refuse_after);
        if request.bounded_random_read {
            let mut reader = FragmentedRead {
                bytes: request.bytes,
                reads: 0,
                maximum_request: 0,
            };
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let result =
                decode_physical_object_from_port_v1(&mut reader, &mut visitor, &mut scratch);
            return match result {
                Ok(decoded) => complete_observation(
                    decoded.header(),
                    decoded.payload(),
                    decoded.physical_id(),
                    visitor,
                    reader.reads,
                    reader.maximum_request,
                ),
                Err(error) => {
                    let mut observation = empty_observation(Some(error), visitor);
                    observation.reads = reader.reads;
                    observation.maximum_request = reader.maximum_request;
                    observation
                }
            };
        }

        match decode_physical_object_v1(request.bytes, &mut visitor) {
            Ok(decoded) => match decoded.physical_id() {
                Ok(identity) => complete_observation(
                    decoded.header(),
                    decoded.payload(),
                    identity,
                    visitor,
                    0,
                    0,
                ),
                Err(error) => empty_observation(Some(error), visitor),
            },
            Err(error) => empty_observation(Some(error), visitor),
        }
    }
}

pub use decode::*;
#[cfg(test)]
pub(crate) use encode::encode_physical_object_header_v1;
pub(crate) use encode::{
    encode_version_record_v1, seal_physical_object_in_place_v1, CanonicalChunkObjectEncoderV1,
    CanonicalFileObjectEncoderV1,
};
pub use encode::{OBJECT_HEADER_BYTES, VERSION_RECORD_PAYLOAD_BYTES};
pub use model::*;
pub use port_decode::*;
#[cfg(feature = "operation-polymorphism")]
pub(crate) use traversal::MAX_CANONICAL_TRAVERSAL_FRAMES_V1;
pub(crate) use traversal::{
    require_canonical_traversal_depth_v1, traverse_strong_edges_v1, CanonicalTraversalBudgetV1,
    StrongEdgeTraversalQueueV1,
};
