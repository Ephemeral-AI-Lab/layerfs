use layerfs_storage::cas::{
    admit_complete_immutable_v1, compare_closure_object_ids_v1, read_complete_immutable_v1,
    AdmissionBuffersV1, BoundedImmutableReadSinkV1, ClosureObjectV1,
    CompleteImmutableClosureReadPortV1, ImmutablePortErrorV1, OccupiedImmutableReadPortV1,
    PreparedImmutableClosurePortV1, ValidatedOccupiedObjectV1,
};
use layerfs_storage::format::{ValidatedComponent, ValidatedSymlinkTarget};
use layerfs_storage::identity::{
    derive_file_node_v1, derive_implicit_root_directory_v1, derive_logical_chunk_v1,
    derive_logical_file_v1, derive_physical_chunk_id_v1, derive_physical_file_id_v1,
    derive_physical_symlink_id_v1, derive_physical_tree_id_v1,
    derive_physical_version_record_id_v1, derive_symlink_node_v1, derive_version_v1,
    LogicalChildIdV1, LogicalChunkRefV1, LogicalDirectoryEntryV1,
};
use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1, OPERATION_SLOT_BYTES};
use layerfs_storage::object::TypedPhysicalObjectIdV1;
use layerfs_storage::profile::{ChunkerSpecV1, DigestSpecV1, ProfileSpecV1};
use layerfs_storage::CoreError;

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

fn empty_closure() -> (
    Vec<u8>,
    Vec<u8>,
    TypedPhysicalObjectIdV1,
    TypedPhysicalObjectIdV1,
) {
    let root = object(2, &[1, 0x10, 0, 0, 0, 0, 0, 0, 0]);
    let root_id = derive_physical_tree_id_v1(&root).unwrap();
    let logical_root = derive_implicit_root_directory_v1(&[]).unwrap();
    let version_id = derive_version_v1(logical_root);
    let mut payload = Vec::with_capacity(184);
    payload.extend_from_slice(version_id.as_bytes());
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
    let version = object(1, &payload);
    let version_id = TypedPhysicalObjectIdV1::VersionRecord(
        derive_physical_version_record_id_v1(&version).unwrap(),
    );
    (
        version,
        root,
        version_id,
        TypedPhysicalObjectIdV1::Tree(root_id),
    )
}

struct SingleSymlinkClosure {
    version: Vec<u8>,
    root: Vec<u8>,
    leaf: Vec<u8>,
    symlink: Vec<u8>,
    ids: [TypedPhysicalObjectIdV1; 4],
}

struct RechunkedFileClosure {
    objects: Vec<(TypedPhysicalObjectIdV1, Vec<u8>)>,
}

fn single_symlink_closure() -> SingleSymlinkClosure {
    let target = b"destination";
    let mut symlink_payload = Vec::with_capacity(4 + target.len());
    symlink_payload.extend_from_slice(&(target.len() as u32).to_be_bytes());
    symlink_payload.extend_from_slice(target);
    let symlink = object(4, &symlink_payload);
    let symlink_id = derive_physical_symlink_id_v1(&symlink).unwrap();

    let name = b"link";
    let mut leaf_payload = vec![2, 0];
    leaf_payload.extend_from_slice(&1_u16.to_be_bytes());
    leaf_payload.extend_from_slice(&(name.len() as u16).to_be_bytes());
    leaf_payload.extend_from_slice(name);
    leaf_payload.push(3);
    leaf_payload.extend_from_slice(symlink_id.as_bytes());
    let leaf = object(2, &leaf_payload);
    let leaf_id = derive_physical_tree_id_v1(&leaf).unwrap();

    let mut root_payload = vec![1];
    root_payload.extend_from_slice(&0x1000_u16.to_be_bytes());
    root_payload.extend_from_slice(&1_u32.to_be_bytes());
    root_payload.push(0);
    root_payload.push(1);
    root_payload.extend_from_slice(leaf_id.as_bytes());
    let root = object(2, &root_payload);
    let root_id = derive_physical_tree_id_v1(&root).unwrap();

    let logical_symlink =
        derive_symlink_node_v1(ValidatedSymlinkTarget::new(target).unwrap()).unwrap();
    let logical_root = derive_implicit_root_directory_v1(&[LogicalDirectoryEntryV1::new(
        ValidatedComponent::new(name).unwrap(),
        LogicalChildIdV1::Symlink(logical_symlink),
    )])
    .unwrap();
    let logical_version = derive_version_v1(logical_root);
    let mut version_payload = Vec::with_capacity(184);
    version_payload.extend_from_slice(logical_version.as_bytes());
    version_payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
    version_payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
    version_payload.extend_from_slice(root_id.as_bytes());
    version_payload.extend_from_slice(&0_u64.to_be_bytes());
    version_payload.extend_from_slice(&0_u64.to_be_bytes());
    for count in [1_u32, 2, 0, 1, 0, 0, 0, 4] {
        version_payload.extend_from_slice(&count.to_be_bytes());
    }
    version_payload.extend_from_slice(&0_u64.to_be_bytes());
    let version = object(1, &version_payload);
    let version_id = derive_physical_version_record_id_v1(&version).unwrap();
    let ids = [
        TypedPhysicalObjectIdV1::VersionRecord(version_id),
        TypedPhysicalObjectIdV1::Tree(root_id),
        TypedPhysicalObjectIdV1::Tree(leaf_id),
        TypedPhysicalObjectIdV1::Symlink(symlink_id),
    ];
    SingleSymlinkClosure {
        version,
        root,
        leaf,
        symlink,
        ids,
    }
}

fn rechunked_file_closure() -> RechunkedFileClosure {
    let source = b"logical bytes reconstructed across physical chunks";
    let split = 9;
    let mut objects = Vec::new();
    let mut chunk_ids = Vec::new();
    for payload in [&source[..split], &source[split..]] {
        let bytes = object(5, payload);
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
    let file = object(3, &file_payload);
    let file_id = derive_physical_file_id_v1(&file).unwrap();
    objects.push((TypedPhysicalObjectIdV1::File(file_id), file));

    let name = b"file";
    let mut leaf_payload = vec![2, 0];
    leaf_payload.extend_from_slice(&1_u16.to_be_bytes());
    leaf_payload.extend_from_slice(&(name.len() as u16).to_be_bytes());
    leaf_payload.extend_from_slice(name);
    leaf_payload.push(2);
    leaf_payload.extend_from_slice(file_id.as_bytes());
    let leaf = object(2, &leaf_payload);
    let leaf_id = derive_physical_tree_id_v1(&leaf).unwrap();
    objects.push((TypedPhysicalObjectIdV1::Tree(leaf_id), leaf));

    let mut root_payload = vec![1];
    root_payload.extend_from_slice(&0x1000_u16.to_be_bytes());
    root_payload.extend_from_slice(&1_u32.to_be_bytes());
    root_payload.push(0);
    root_payload.push(1);
    root_payload.extend_from_slice(leaf_id.as_bytes());
    let root = object(2, &root_payload);
    let root_id = derive_physical_tree_id_v1(&root).unwrap();
    objects.push((TypedPhysicalObjectIdV1::Tree(root_id), root));

    let logical_chunk = derive_logical_chunk_v1(source).unwrap();
    let logical_file = derive_logical_file_v1(
        source.len() as u64,
        &[LogicalChunkRefV1::from_identity(logical_chunk)],
    )
    .unwrap();
    let logical_file_node = derive_file_node_v1(0o644, logical_file).unwrap();
    let logical_root = derive_implicit_root_directory_v1(&[LogicalDirectoryEntryV1::new(
        ValidatedComponent::new(name).unwrap(),
        LogicalChildIdV1::File(logical_file_node),
    )])
    .unwrap();
    let logical_version = derive_version_v1(logical_root);
    let mut version_payload = Vec::with_capacity(184);
    version_payload.extend_from_slice(logical_version.as_bytes());
    version_payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
    version_payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
    version_payload.extend_from_slice(root_id.as_bytes());
    version_payload.extend_from_slice(&0_u64.to_be_bytes());
    version_payload.extend_from_slice(&(source.len() as u64).to_be_bytes());
    for count in [1_u32, 2, 1, 0, 2, 1, 2, 6] {
        version_payload.extend_from_slice(&count.to_be_bytes());
    }
    version_payload.extend_from_slice(&(source.len() as u64).to_be_bytes());
    let version = object(1, &version_payload);
    let version_id = derive_physical_version_record_id_v1(&version).unwrap();
    objects.insert(
        0,
        (TypedPhysicalObjectIdV1::VersionRecord(version_id), version),
    );
    RechunkedFileClosure { objects }
}

fn indexed_symlink_closure() -> RechunkedFileClosure {
    let target = b"shared-target";
    let mut symlink_payload = Vec::with_capacity(4 + target.len());
    symlink_payload.extend_from_slice(&(target.len() as u32).to_be_bytes());
    symlink_payload.extend_from_slice(target);
    let symlink = object(4, &symlink_payload);
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
        let bytes = object(2, &payload);
        let id = derive_physical_tree_id_v1(&bytes).unwrap();
        leaves.push((id, bytes, range));
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
    let index = object(2, &index_payload);
    let index_id = derive_physical_tree_id_v1(&index).unwrap();

    let mut root_payload = vec![1];
    root_payload.extend_from_slice(&0x1000_u16.to_be_bytes());
    root_payload.extend_from_slice(&193_u32.to_be_bytes());
    root_payload.push(1);
    root_payload.push(1);
    root_payload.extend_from_slice(index_id.as_bytes());
    let root = object(2, &root_payload);
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
    let logical_version = derive_version_v1(logical_root);

    let mut version_payload = Vec::with_capacity(184);
    version_payload.extend_from_slice(logical_version.as_bytes());
    version_payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
    version_payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
    version_payload.extend_from_slice(root_id.as_bytes());
    version_payload.extend_from_slice(&0_u64.to_be_bytes());
    version_payload.extend_from_slice(&0_u64.to_be_bytes());
    for count in [193_u32, 4, 0, 1, 0, 0, 0, 6] {
        version_payload.extend_from_slice(&count.to_be_bytes());
    }
    version_payload.extend_from_slice(&0_u64.to_be_bytes());
    let version = object(1, &version_payload);
    let version_id = derive_physical_version_record_id_v1(&version).unwrap();

    let mut objects = vec![
        (TypedPhysicalObjectIdV1::VersionRecord(version_id), version),
        (TypedPhysicalObjectIdV1::Tree(root_id), root),
        (TypedPhysicalObjectIdV1::Tree(index_id), index),
        (TypedPhysicalObjectIdV1::Symlink(symlink_id), symlink),
    ];
    objects.extend(
        leaves
            .into_iter()
            .map(|(id, bytes, _)| (TypedPhysicalObjectIdV1::Tree(id), bytes)),
    );
    objects.sort_by(|left, right| compare_closure_object_ids_v1(left.0, right.0));
    RechunkedFileClosure { objects }
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
    object(3, &payload)
}

struct ClosurePort<'a> {
    objects: &'a [ClosureObjectV1<'a>],
    resident_memory: u64,
    maximum_read: usize,
    count_calls: u64,
    reads: u64,
}

impl<'a> ClosurePort<'a> {
    const fn new(objects: &'a [ClosureObjectV1<'a>]) -> Self {
        Self {
            objects,
            resident_memory: 0,
            maximum_read: 0,
            count_calls: 0,
            reads: 0,
        }
    }

    const fn with_resident_memory(
        objects: &'a [ClosureObjectV1<'a>],
        resident_memory: u64,
    ) -> Self {
        Self {
            objects,
            resident_memory,
            maximum_read: 0,
            count_calls: 0,
            reads: 0,
        }
    }
}

impl CompleteImmutableClosureReadPortV1 for ClosurePort<'_> {
    fn object_count(&mut self) -> Result<u64, ImmutablePortErrorV1> {
        self.count_calls = self
            .count_calls
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        u64::try_from(self.objects.len()).map_err(|_| ImmutablePortErrorV1::Failure)
    }

    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
    }

    fn object_id_at(
        &mut self,
        ordinal: u64,
    ) -> Result<TypedPhysicalObjectIdV1, ImmutablePortErrorV1> {
        let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.objects
            .get(ordinal)
            .copied()
            .map(ClosureObjectV1::expected_id)
            .ok_or(ImmutablePortErrorV1::Failure)
    }

    fn object_len_at(&mut self, ordinal: u64) -> Result<u64, ImmutablePortErrorV1> {
        let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let len = self
            .objects
            .get(ordinal)
            .copied()
            .map(ClosureObjectV1::canonical_bytes)
            .ok_or(ImmutablePortErrorV1::Failure)?
            .len();
        u64::try_from(len).map_err(|_| ImmutablePortErrorV1::Failure)
    }

    fn read_object_exact_at(
        &mut self,
        ordinal: u64,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        self.maximum_read = self.maximum_read.max(destination.len());
        self.reads = self
            .reads
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let start = usize::try_from(offset).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let end = start
            .checked_add(destination.len())
            .ok_or(ImmutablePortErrorV1::Failure)?;
        let bytes = self
            .objects
            .get(ordinal)
            .copied()
            .map(ClosureObjectV1::canonical_bytes)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        destination.copy_from_slice(bytes.get(start..end).ok_or(ImmutablePortErrorV1::Failure)?);
        Ok(())
    }
}

#[derive(Default)]
struct Occupied {
    entry: Option<(TypedPhysicalObjectIdV1, Vec<u8>)>,
    resident_memory: u64,
    maximum_read: usize,
    lookups: u64,
    reads: u64,
}

impl OccupiedImmutableReadPortV1 for Occupied {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
    }

    fn occupied_len(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<Option<u64>, ImmutablePortErrorV1> {
        self.lookups = self
            .lookups
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        Ok(self
            .entry
            .as_ref()
            .filter(|(occupied_id, _)| *occupied_id == id)
            .map(|(_, bytes)| bytes.len() as u64))
    }

    fn read_occupied_exact_at(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        self.maximum_read = self.maximum_read.max(destination.len());
        self.reads = self
            .reads
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        let (_, bytes) = self
            .entry
            .as_ref()
            .filter(|(occupied_id, _)| *occupied_id == id)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        let start = usize::try_from(offset).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let end = start
            .checked_add(destination.len())
            .ok_or(ImmutablePortErrorV1::Failure)?;
        destination.copy_from_slice(bytes.get(start..end).ok_or(ImmutablePortErrorV1::Failure)?);
        Ok(())
    }
}

#[derive(Default)]
struct Sink {
    resident_memory: u64,
    begun: u64,
    staged: Vec<TypedPhysicalObjectIdV1>,
    active: Option<(TypedPhysicalObjectIdV1, u64, u64)>,
    maximum_write: usize,
    writes: u64,
    reused: Vec<TypedPhysicalObjectIdV1>,
    visible: Option<TypedPhysicalObjectIdV1>,
    aborts: u64,
}

impl PreparedImmutableClosurePortV1 for Sink {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
    }

    fn begin_private_closure(&mut self, object_count: u64) -> Result<(), ImmutablePortErrorV1> {
        self.begun = object_count;
        Ok(())
    }

    fn begin_private_object(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        exact_len: u64,
    ) -> Result<(), ImmutablePortErrorV1> {
        if self.active.replace((id, exact_len, 0)).is_some() {
            return Err(ImmutablePortErrorV1::Failure);
        }
        Ok(())
    }

    fn write_private_object(
        &mut self,
        canonical_fragment: &[u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        self.maximum_write = self.maximum_write.max(canonical_fragment.len());
        self.writes = self
            .writes
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        let (_, exact_len, written) = self.active.as_mut().ok_or(ImmutablePortErrorV1::Failure)?;
        *written = written
            .checked_add(canonical_fragment.len() as u64)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        if *written > *exact_len {
            return Err(ImmutablePortErrorV1::Failure);
        }
        Ok(())
    }

    fn finish_private_object(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1> {
        let (active_id, exact_len, written) =
            self.active.take().ok_or(ImmutablePortErrorV1::Failure)?;
        if active_id != id || exact_len != written {
            return Err(ImmutablePortErrorV1::Failure);
        }
        self.staged.push(id);
        Ok(())
    }

    fn note_reused_object(
        &mut self,
        validated: ValidatedOccupiedObjectV1,
    ) -> Result<(), ImmutablePortErrorV1> {
        self.reused.push(validated.id());
        Ok(())
    }

    fn make_closure_visible(
        &mut self,
        version_record: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1> {
        self.visible = Some(version_record);
        Ok(())
    }

    fn abort_private_closure(&mut self) {
        self.aborts += 1;
        self.staged.clear();
        self.active = None;
        self.reused.clear();
        self.visible = None;
    }
}

#[derive(Default)]
struct ReadSink {
    resident_memory: u64,
    begins: u64,
    id: Option<TypedPhysicalObjectIdV1>,
    expected_len: u64,
    bytes: Vec<u8>,
    maximum_write: usize,
    writes: u64,
    finished: bool,
    aborts: u64,
}

impl BoundedImmutableReadSinkV1 for ReadSink {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
    }

    fn begin_complete_immutable(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        exact_len: u64,
    ) -> Result<(), ImmutablePortErrorV1> {
        self.begins = self
            .begins
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        self.id = Some(id);
        self.expected_len = exact_len;
        Ok(())
    }

    fn write_complete_immutable(
        &mut self,
        canonical_fragment: &[u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        self.maximum_write = self.maximum_write.max(canonical_fragment.len());
        self.writes = self
            .writes
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        self.bytes.extend_from_slice(canonical_fragment);
        Ok(())
    }

    fn finish_complete_immutable(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1> {
        if self.id != Some(id) || self.bytes.len() as u64 != self.expected_len {
            return Err(ImmutablePortErrorV1::Failure);
        }
        self.finished = true;
        Ok(())
    }

    fn abort_complete_immutable(&mut self) {
        self.aborts += 1;
        self.bytes.clear();
        self.finished = false;
    }
}

#[test]
fn complete_empty_closure_is_visible_only_after_all_objects_validate() {
    let (version, root, version_id, root_id) = empty_closure();
    let closure = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
    ];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut sink = Sink::default();
    let admitted = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        version_id,
        &mut Occupied::default(),
        &mut sink,
        &ledger,
        &mut counters,
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    )
    .unwrap();
    assert_eq!(admitted.object_count(), 2);
    assert_eq!(admitted.created_count(), 2);
    assert_eq!(admitted.reused_count(), 0);
    assert_eq!(sink.begun, 2);
    assert_eq!(sink.staged, vec![version_id, root_id]);
    assert_eq!(sink.visible, Some(version_id));
    assert_eq!(sink.aborts, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(counters.physical_objects_created, 0);
    assert_eq!(counters.closure_objects_missing, 2);
    assert_eq!(counters.closure_objects_occupied_validated, 0);
    assert_eq!(counters.publication_authority_dispatches, 0);
}

#[test]
fn admission_requires_canonical_typed_id_order_before_sink_use() {
    let (version, root, version_id, root_id) = empty_closure();
    let closure = [
        ClosureObjectV1::new(root_id, &root),
        ClosureObjectV1::new(version_id, &version),
    ];
    let mut sink = Sink::default();
    let result = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        version_id,
        &mut Occupied::default(),
        &mut sink,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut OperationCountersV1::default(),
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 1],
        ),
    );
    assert_eq!(result, Err(CoreError::NonCanonicalOrder));
    assert_eq!(sink.begun, 0);
    assert_eq!(sink.visible, None);
}

#[test]
fn admission_counts_validation_staging_graph_and_version_reconstruction_reads() {
    let (version, root, version_id, root_id) = empty_closure();
    let closure = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
    ];
    let total_canonical_bytes = (version.len() + root.len()) as u64;
    let mut counters = OperationCountersV1::default();
    admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        version_id,
        &mut Occupied::default(),
        &mut Sink::default(),
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut counters,
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 1],
        ),
    )
    .unwrap();
    // One complete authentication pass, one complete private-staging pass,
    // and the nine-byte root payload used for logical-root reconstruction.
    assert_eq!(counters.bytes_read, 2 * total_canonical_bytes + 9);
    assert_eq!(counters.bytes_copied, total_canonical_bytes);
    assert_eq!(counters.bytes_written, total_canonical_bytes);
}

#[test]
fn nonempty_closure_reconstructs_its_logical_root_before_visibility() {
    let fixture = single_symlink_closure();
    let closure = [
        ClosureObjectV1::new(fixture.ids[0], &fixture.version),
        ClosureObjectV1::new(fixture.ids[1], &fixture.root),
        ClosureObjectV1::new(fixture.ids[2], &fixture.leaf),
        ClosureObjectV1::new(fixture.ids[3], &fixture.symlink),
    ];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut sink = Sink::default();
    let mut counters = OperationCountersV1::default();
    let admitted = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        fixture.ids[0],
        &mut Occupied::default(),
        &mut sink,
        &ledger,
        &mut counters,
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    )
    .unwrap();
    assert_eq!(admitted.object_count(), 4);
    assert_eq!(sink.staged, fixture.ids);
    assert_eq!(sink.visible, Some(fixture.ids[0]));
    assert!(counters.bytes_read > 3 * (fixture.version.len() + fixture.root.len()) as u64);
    assert_eq!(counters.memory_high_water, 12_582_912);
    // The authenticated logical-directory reconstruction owns one fixed
    // inline frame for every allowed path component plus the Vec header.  The
    // frame grew when page traversal state became inline and explicitly
    // charged; retain the exact qualified aarch64 layout instead of the old
    // pre-inline approximation.
    let admission_traversal = 1_788_744;
    let planned =
        2 * 65_536 + 2 * 32_768 + 2 + admission_traversal + core::mem::size_of::<blake3::Hasher>();
    assert_eq!(
        ledger.planned_high_water_bytes(),
        8_388_608 + planned as u64
    );
}

#[test]
fn admission_refuses_source_resident_memory_before_reading_object_bytes() {
    let (version, root, version_id, root_id) = empty_closure();
    let closure = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
    ];
    let mut source = ClosurePort::with_resident_memory(&closure, 4_000_000);
    let mut sink = Sink::default();
    let result = admit_complete_immutable_v1(
        &mut source,
        version_id,
        &mut Occupied::default(),
        &mut sink,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut OperationCountersV1::default(),
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    );
    assert_eq!(result, Err(CoreError::ResourceRefused));
    assert_eq!(source.count_calls, 0);
    assert_eq!(source.reads, 0);
    assert_eq!(sink.begun, 0);
}

#[test]
fn admission_charges_occupied_and_sink_residency_before_any_port_operation() {
    let (version, root, version_id, root_id) = empty_closure();
    let closure = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
    ];

    for oversized_occupied in [true, false] {
        let mut source = ClosurePort::new(&closure);
        let mut occupied = Occupied {
            resident_memory: if oversized_occupied {
                OPERATION_SLOT_BYTES
            } else {
                0
            },
            ..Occupied::default()
        };
        let mut sink = Sink {
            resident_memory: if oversized_occupied {
                0
            } else {
                OPERATION_SLOT_BYTES
            },
            ..Sink::default()
        };
        let result = admit_complete_immutable_v1(
            &mut source,
            version_id,
            &mut occupied,
            &mut sink,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut OperationCountersV1::default(),
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut [0_u8; 2],
            ),
        );
        assert_eq!(result, Err(CoreError::ResourceRefused));
        assert_eq!(source.count_calls, 0);
        assert_eq!(source.reads, 0);
        assert_eq!(occupied.lookups, 0);
        assert_eq!(sink.begun, 0);
    }
}

#[test]
fn admission_replays_logical_cdc_across_different_physical_chunking() {
    let fixture = rechunked_file_closure();
    let mut closure: Vec<_> = fixture
        .objects
        .iter()
        .map(|(id, bytes)| ClosureObjectV1::new(*id, bytes))
        .collect();
    closure.sort_by(|left, right| {
        compare_closure_object_ids_v1(left.expected_id(), right.expected_id())
    });
    let expected = fixture.objects[0].0;
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut sink = Sink::default();
    let admitted = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        expected,
        &mut Occupied::default(),
        &mut sink,
        &ledger,
        &mut OperationCountersV1::default(),
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    )
    .unwrap();
    assert_eq!(admitted.object_count(), 6);
    assert_eq!(sink.visible, Some(expected));
    assert_eq!(
        fixture
            .objects
            .iter()
            .filter(|(id, _)| matches!(id, TypedPhysicalObjectIdV1::Chunk(_)))
            .count(),
        2
    );
}

#[test]
fn admission_reconstructs_an_indexed_directory_and_shared_child_once() {
    let fixture = indexed_symlink_closure();
    let closure: Vec<_> = fixture
        .objects
        .iter()
        .map(|(id, bytes)| ClosureObjectV1::new(*id, bytes))
        .collect();
    let expected = fixture
        .objects
        .iter()
        .find_map(|(id, _)| matches!(id, TypedPhysicalObjectIdV1::VersionRecord(_)).then_some(*id))
        .unwrap();
    let mut sink = Sink::default();
    let admitted = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        expected,
        &mut Occupied::default(),
        &mut sink,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut OperationCountersV1::default(),
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    )
    .unwrap();
    assert_eq!(admitted.object_count(), 6);
    assert_eq!(sink.visible, Some(expected));
}

#[test]
fn occupied_complete_equality_deduplicates_without_overwrite() {
    let (version, root, version_id, root_id) = empty_closure();
    let closure = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
    ];
    let mut occupied = Occupied {
        entry: Some((root_id, root.clone())),
        ..Occupied::default()
    };
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut sink = Sink::default();
    let admitted = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        version_id,
        &mut occupied,
        &mut sink,
        &ledger,
        &mut counters,
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    )
    .unwrap();
    assert_eq!(admitted.created_count(), 1);
    assert_eq!(admitted.reused_count(), 1);
    assert_eq!(sink.staged, vec![version_id]);
    assert_eq!(sink.reused, vec![root_id]);
    assert_eq!(counters.physical_objects_reused, 0);
    assert_eq!(counters.closure_objects_missing, 1);
    assert_eq!(counters.closure_objects_occupied_validated, 1);
}

#[test]
fn collision_and_malformed_occupant_fail_without_visibility_or_overwrite() {
    let (version, root, version_id, root_id) = empty_closure();
    let closure = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
    ];
    let different_valid_tree = object(2, &[1, 0, 0, 0, 0, 0, 0, 0, 0]);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);

    let mut sink = Sink::default();
    let mut counters = OperationCountersV1::default();
    let result = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        version_id,
        &mut Occupied {
            entry: Some((root_id, different_valid_tree)),
            ..Occupied::default()
        },
        &mut sink,
        &ledger,
        &mut counters,
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    );
    assert_eq!(result, Err(CoreError::OccupiedSameIdDifferentBytes));
    assert_eq!(sink.visible, None);
    assert_eq!(sink.aborts, 1);

    let mut sink = Sink::default();
    let result = admit_complete_immutable_v1(
        &mut ClosurePort::new(&closure),
        version_id,
        &mut Occupied {
            entry: Some((root_id, root[..root.len() - 1].to_vec())),
            ..Occupied::default()
        },
        &mut sink,
        &ledger,
        &mut OperationCountersV1::default(),
        AdmissionBuffersV1::new(
            &mut [0_u8; 65_536],
            &mut [0_u8; 65_536],
            &mut [0_u8; 32_768],
            &mut [0_u8; 32_768],
            &mut [0_u8; 2],
        ),
    );
    assert_eq!(result, Err(CoreError::MalformedOccupant));
    assert_eq!(sink.visible, None);
    assert_eq!(sink.aborts, 1);

    let mut wrong_version = root.clone();
    wrong_version[8..10].copy_from_slice(&2_u16.to_be_bytes());
    let mut wrong_profile = root.clone();
    wrong_profile[12] ^= 1;
    let mut unknown_kind = root.clone();
    unknown_kind[10] = 0xff;
    let wrong_typed_kind = object(5, &[0x44]);
    for (label, occupied_bytes, expected) in [
        ("schema", wrong_version, CoreError::Schema),
        ("profile-domain", wrong_profile, CoreError::TypeDomain),
        ("unknown-kind", unknown_kind, CoreError::UnknownKind),
        ("typed-kind", wrong_typed_kind, CoreError::TypeDomain),
    ] {
        let mut sink = Sink::default();
        let result = admit_complete_immutable_v1(
            &mut ClosurePort::new(&closure),
            version_id,
            &mut Occupied {
                entry: Some((root_id, occupied_bytes)),
                ..Occupied::default()
            },
            &mut sink,
            &ledger,
            &mut OperationCountersV1::default(),
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut [0_u8; 2],
            ),
        );
        assert_eq!(result, Err(expected), "{label}");
        assert_eq!(sink.visible, None, "{label}");
        assert_eq!(sink.aborts, 1, "{label}");
    }
}

#[test]
fn occupied_collision_reads_the_complete_large_object_in_bounded_windows() {
    let fixture = rechunked_file_closure();
    let mut closure: Vec<_> = fixture
        .objects
        .iter()
        .map(|(id, bytes)| ClosureObjectV1::new(*id, bytes))
        .collect();
    closure.sort_by(|left, right| {
        compare_closure_object_ids_v1(left.expected_id(), right.expected_id())
    });
    let file_id = fixture
        .objects
        .iter()
        .find_map(|(id, _)| matches!(id, TypedPhysicalObjectIdV1::File(_)).then_some(*id))
        .unwrap();

    let large_valid_file = large_valid_file(2_000);
    assert!(large_valid_file.len() > 65_536);
    let mut occupied = Occupied {
        entry: Some((file_id, large_valid_file.clone())),
        ..Occupied::default()
    };
    let mut sink = Sink::default();
    let mut counters = OperationCountersV1::default();
    assert_eq!(
        admit_complete_immutable_v1(
            &mut ClosurePort::new(&closure),
            fixture.objects[0].0,
            &mut occupied,
            &mut sink,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut counters,
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut [0_u8; 2]
            ),
        ),
        Err(CoreError::OccupiedSameIdDifferentBytes)
    );
    assert_eq!(occupied.maximum_read, 65_536);
    assert!(occupied.reads > 2);
    assert!(counters.bytes_read > large_valid_file.len() as u64);
    assert_eq!(sink.visible, None);
    assert_eq!(sink.aborts, 1);
}

#[test]
fn complete_read_validates_before_bounded_sink_delivery() {
    let bytes = large_valid_file(2_000);
    assert!(bytes.len() > 65_536);
    let id = TypedPhysicalObjectIdV1::File(derive_physical_file_id_v1(&bytes).unwrap());
    let mut occupied = Occupied {
        entry: Some((id, bytes.clone())),
        ..Occupied::default()
    };
    let mut sink = ReadSink::default();
    let mut counters = OperationCountersV1::default();
    let read = read_complete_immutable_v1(
        id,
        &mut occupied,
        &mut sink,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut counters,
        &mut [0_u8; 65_536],
    )
    .unwrap();
    assert_eq!(read.id(), id);
    assert_eq!(read.canonical_len(), bytes.len() as u64);
    assert_eq!(sink.bytes, bytes);
    assert!(sink.finished);
    assert_eq!(sink.aborts, 0);
    assert_eq!(sink.maximum_write, 65_536);
    assert!(sink.writes > 1);
    assert_eq!(occupied.maximum_read, 65_536);
    assert!(counters.bytes_read >= 2 * bytes.len() as u64);
    assert_eq!(counters.bytes_written, bytes.len() as u64);

    let mut corrupt = bytes;
    *corrupt.last_mut().unwrap() ^= 1;
    let mut sink = ReadSink::default();
    let result = read_complete_immutable_v1(
        id,
        &mut Occupied {
            entry: Some((id, corrupt)),
            ..Occupied::default()
        },
        &mut sink,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut OperationCountersV1::default(),
        &mut [0_u8; 65_536],
    );
    assert_eq!(result, Err(CoreError::IdMismatch));
    assert_eq!(sink.id, None);
    assert!(sink.bytes.is_empty());
}

#[test]
fn complete_read_charges_occupied_and_sink_residency_before_lookup_or_delivery() {
    let bytes = object(5, b"bounded");
    let id = TypedPhysicalObjectIdV1::Chunk(derive_physical_chunk_id_v1(&bytes).unwrap());

    for oversized_occupied in [true, false] {
        let mut occupied = Occupied {
            entry: Some((id, bytes.clone())),
            resident_memory: if oversized_occupied {
                OPERATION_SLOT_BYTES
            } else {
                0
            },
            ..Occupied::default()
        };
        let mut sink = ReadSink {
            resident_memory: if oversized_occupied {
                0
            } else {
                OPERATION_SLOT_BYTES
            },
            ..ReadSink::default()
        };
        let result = read_complete_immutable_v1(
            id,
            &mut occupied,
            &mut sink,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut OperationCountersV1::default(),
            &mut [0_u8; 65_536],
        );
        assert_eq!(result, Err(CoreError::ResourceRefused));
        assert_eq!(occupied.lookups, 0);
        assert_eq!(occupied.reads, 0);
        assert_eq!(sink.begins, 0);
        assert!(sink.bytes.is_empty());
    }
}

#[test]
fn missing_and_wrong_domain_edges_abort_the_private_closure() {
    let chunk = object(5, b"x");
    let chunk_id = derive_physical_chunk_id_v1(&chunk).unwrap();
    let mut leaf_payload = vec![2, 0, 0, 1, 0, 1, b'x', 2];
    leaf_payload.extend_from_slice(chunk_id.as_bytes());
    let leaf = object(2, &leaf_payload);
    let leaf_id = TypedPhysicalObjectIdV1::Tree(derive_physical_tree_id_v1(&leaf).unwrap());

    let (version, root, version_id, root_id) = empty_closure();
    let missing = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
        ClosureObjectV1::new(leaf_id, &leaf),
    ];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut sink = Sink::default();
    assert_eq!(
        admit_complete_immutable_v1(
            &mut ClosurePort::new(&missing),
            version_id,
            &mut Occupied::default(),
            &mut sink,
            &ledger,
            &mut OperationCountersV1::default(),
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut [0_u8; 2]
            ),
        ),
        Err(CoreError::MissingClosureEdge)
    );
    assert_eq!(sink.visible, None);

    let wrong_domain = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
        ClosureObjectV1::new(leaf_id, &leaf),
        ClosureObjectV1::new(TypedPhysicalObjectIdV1::Chunk(chunk_id), &chunk),
    ];
    let mut sink = Sink::default();
    assert_eq!(
        admit_complete_immutable_v1(
            &mut ClosurePort::new(&wrong_domain),
            version_id,
            &mut Occupied::default(),
            &mut sink,
            &ledger,
            &mut OperationCountersV1::default(),
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut [0_u8; 2]
            ),
        ),
        Err(CoreError::TypedEdge)
    );
    assert_eq!(sink.visible, None);
}

#[test]
fn identity_and_resource_failures_precede_visibility() {
    let (version, root, version_id, root_id) = empty_closure();
    let mut mutated = root.clone();
    *mutated.last_mut().unwrap() ^= 1;
    let closure = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &mutated),
    ];
    let mut sink = Sink::default();
    assert_eq!(
        admit_complete_immutable_v1(
            &mut ClosurePort::new(&closure),
            version_id,
            &mut Occupied::default(),
            &mut sink,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut OperationCountersV1::default(),
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut [0_u8; 2]
            ),
        ),
        Err(CoreError::TypedEdge)
    );
    assert_eq!(sink.visible, None);

    let valid = [
        ClosureObjectV1::new(version_id, &version),
        ClosureObjectV1::new(root_id, &root),
    ];
    let mut sink = Sink::default();
    assert_eq!(
        admit_complete_immutable_v1(
            &mut ClosurePort::new(&valid),
            version_id,
            &mut Occupied::default(),
            &mut sink,
            &ResourceLedgerV1::new(8_388_608),
            &mut OperationCountersV1::default(),
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut [0_u8; 2]
            ),
        ),
        Err(CoreError::ResourceRefused)
    );
    assert_eq!(sink.begun, 0);
}
