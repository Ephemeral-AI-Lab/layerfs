use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

use layerfs_storage::cas::{
    AdmissionBuffersV1, CompleteImmutableClosureReadPortV1, ImmutablePortErrorV1,
    OccupiedImmutableReadPortV1,
};
use layerfs_storage::fscas::{
    FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasV1,
    FsPackAdmissionOutcomeV1, FsPrivatePackV1,
};
use layerfs_storage::identity::{
    derive_implicit_root_directory_v1, derive_physical_tree_id_v1,
    derive_physical_version_record_id_v1, derive_version_v1,
};
use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1, BASE_LEDGER_BYTES};
use layerfs_storage::object::{
    decode_physical_object_v1, DiscardStrongEdgesV1, TypedPhysicalObjectIdV1,
};
use layerfs_storage::pack::{
    build_dense_pack_v1, PackIndexEntryV1, PackIndexSpoolV1, PackObjectSourceV1, PackPortErrorV1,
    PackReadPortV1, PrivatePackPortV1,
};
use layerfs_storage::profile::{ChunkerSpecV1, DigestSpecV1, ProfileSpecV1};
use layerfs_storage::{CoreError, CoreResult};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

fn make_owner_writable(path: &Path) -> fs::Permissions {
    let original = fs::metadata(path).unwrap().permissions();
    let mut permissions = original.clone();
    #[cfg(unix)]
    permissions.set_mode(permissions.mode() | 0o200);
    #[cfg(windows)]
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).unwrap();
    original
}

#[derive(Clone, Copy)]
enum StopKind {
    Cancellation,
    Deadline,
}

struct StopAtBoundary {
    kind: StopKind,
    target: FsCasBoundaryV1,
    current: Option<FsCasBoundaryV1>,
}

impl StopAtBoundary {
    const fn new(kind: StopKind, target: FsCasBoundaryV1) -> Self {
        Self {
            kind,
            target,
            current: None,
        }
    }
}

impl FsCasControlV1 for StopAtBoundary {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        self.current = Some(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        matches!(self.kind, StopKind::Cancellation) && self.current == Some(self.target)
    }

    fn deadline_exceeded(&mut self) -> bool {
        matches!(self.kind, StopKind::Deadline) && self.current == Some(self.target)
    }
}

struct StopWithCleanupFailure {
    stop: FsCasBoundaryV1,
    cleanup_target: FsCasCleanupTargetV1,
    current: Option<FsCasBoundaryV1>,
    injected: bool,
}

impl StopWithCleanupFailure {
    const fn new(stop: FsCasBoundaryV1, cleanup_target: FsCasCleanupTargetV1) -> Self {
        Self {
            stop,
            cleanup_target,
            current: None,
            injected: false,
        }
    }
}

impl FsCasControlV1 for StopWithCleanupFailure {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        self.current = Some(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        self.current == Some(self.stop)
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if !self.injected && target == self.cleanup_target {
            self.injected = true;
            true
        } else {
            false
        }
    }
}

struct BreakCatalogAtPublication {
    catalog: PathBuf,
    injected: bool,
}

impl FsCasControlV1 for BreakCatalogAtPublication {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.injected {
            fs::remove_dir(&self.catalog).unwrap();
            fs::write(&self.catalog, b"injected-not-a-directory").unwrap();
            self.injected = true;
        }
    }

    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "layerfs-c3-{label}-{}-{sequence:016x}",
            std::process::id()
        ));
        Self { path }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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

fn typed_id(bytes: &[u8]) -> TypedPhysicalObjectIdV1 {
    decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)
        .unwrap()
        .physical_id()
        .unwrap()
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

struct ClosureSource<'a> {
    objects: &'a [(TypedPhysicalObjectIdV1, Vec<u8>)],
}

impl CompleteImmutableClosureReadPortV1 for ClosureSource<'_> {
    fn object_count(&mut self) -> Result<u64, ImmutablePortErrorV1> {
        u64::try_from(self.objects.len()).map_err(|_| ImmutablePortErrorV1::Failure)
    }

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(0)
    }

    fn object_id_at(
        &mut self,
        ordinal: u64,
    ) -> Result<TypedPhysicalObjectIdV1, ImmutablePortErrorV1> {
        let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.objects
            .get(ordinal)
            .map(|(id, _)| *id)
            .ok_or(ImmutablePortErrorV1::Failure)
    }

    fn object_len_at(&mut self, ordinal: u64) -> Result<u64, ImmutablePortErrorV1> {
        let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.objects
            .get(ordinal)
            .ok_or(ImmutablePortErrorV1::Failure)
            .and_then(|(_, bytes)| {
                u64::try_from(bytes.len()).map_err(|_| ImmutablePortErrorV1::Failure)
            })
    }

    fn read_object_exact_at(
        &mut self,
        ordinal: u64,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let offset = usize::try_from(offset).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let end = offset
            .checked_add(destination.len())
            .ok_or(ImmutablePortErrorV1::Failure)?;
        let bytes = &self
            .objects
            .get(ordinal)
            .ok_or(ImmutablePortErrorV1::Failure)?
            .1;
        destination.copy_from_slice(
            bytes
                .get(offset..end)
                .ok_or(ImmutablePortErrorV1::Failure)?,
        );
        Ok(())
    }
}

struct ObjectSource<'a> {
    bytes: &'a [Vec<u8>],
    ids: Vec<TypedPhysicalObjectIdV1>,
    fail_payload: bool,
    payload_bytes_read: u64,
}

impl<'a> ObjectSource<'a> {
    fn new(bytes: &'a [Vec<u8>]) -> Self {
        Self {
            bytes,
            ids: bytes.iter().map(|bytes| typed_id(bytes)).collect(),
            fail_payload: false,
            payload_bytes_read: 0,
        }
    }
}

impl PackObjectSourceV1 for ObjectSource<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(self.ids.capacity())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_mul(core::mem::size_of::<TypedPhysicalObjectIdV1>() as u64)
            .ok_or(CoreError::IntegerOverflow)
    }

    fn declared_object_count(&self) -> CoreResult<u32> {
        u32::try_from(self.bytes.len()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn object_id(&mut self, ordinal: u32) -> Result<TypedPhysicalObjectIdV1, PackPortErrorV1> {
        self.ids
            .get(ordinal as usize)
            .copied()
            .ok_or(PackPortErrorV1::Failure)
    }

    fn object_len(&mut self, ordinal: u32) -> Result<u64, PackPortErrorV1> {
        self.bytes
            .get(ordinal as usize)
            .ok_or(PackPortErrorV1::Failure)
            .and_then(|bytes| u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure))
    }

    fn read_object_exact_at(
        &mut self,
        ordinal: u32,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1> {
        if self.fail_payload {
            return Err(PackPortErrorV1::Failure);
        }
        let bytes = self
            .bytes
            .get(ordinal as usize)
            .ok_or(PackPortErrorV1::Failure)?;
        let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
        let end = start
            .checked_add(destination.len())
            .ok_or(PackPortErrorV1::Failure)?;
        destination.copy_from_slice(bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?);
        self.payload_bytes_read = self
            .payload_bytes_read
            .checked_add(destination.len() as u64)
            .ok_or(PackPortErrorV1::Failure)?;
        Ok(())
    }
}

#[derive(Default)]
struct Spool {
    entries: Vec<PackIndexEntryV1>,
    cursor: usize,
    maximum: usize,
}

impl PackIndexSpoolV1 for Spool {
    fn resident_memory_bound_bytes(&self, maximum_entries: u32) -> CoreResult<u64> {
        u64::from(maximum_entries)
            .checked_mul(core::mem::size_of::<PackIndexEntryV1>() as u64)
            .ok_or(CoreError::IntegerOverflow)
    }

    fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1> {
        self.entries.clear();
        self.cursor = 0;
        self.maximum = maximum_entries as usize;
        Ok(())
    }

    fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1> {
        if self.entries.len() == self.maximum {
            return Err(PackPortErrorV1::Failure);
        }
        self.entries.push(entry);
        Ok(())
    }

    fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1> {
        self.entries.sort_by(PackIndexEntryV1::compare_key);
        Ok(())
    }

    fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1> {
        self.entries.sort_by(PackIndexEntryV1::compare_offset);
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), PackPortErrorV1> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
        let result = self.entries.get(self.cursor).copied();
        self.cursor = self
            .cursor
            .checked_add(usize::from(result.is_some()))
            .ok_or(PackPortErrorV1::Failure)?;
        Ok(result)
    }

    fn abort(&mut self) {
        self.entries.clear();
        self.cursor = 0;
    }
}

fn build_private_pack(
    cas: &FsCasV1,
    objects: &[Vec<u8>],
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; 65_536],
) -> FsPrivatePackV1 {
    let mut source = ObjectSource::new(objects);
    let mut pack = cas.begin_private_pack().unwrap();
    let mut spool = Spool::default();
    build_dense_pack_v1(
        &mut source,
        &mut pack,
        &mut spool,
        ledger,
        counters,
        scratch,
    )
    .unwrap();
    assert_eq!(
        source.payload_bytes_read,
        objects.iter().map(|bytes| bytes.len() as u64).sum::<u64>()
    );
    pack
}

fn only_entry(directory: &Path) -> PathBuf {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    entries.pop().unwrap()
}

fn locator_path(root: &Path, id: TypedPhysicalObjectIdV1) -> PathBuf {
    let prefix = match id {
        TypedPhysicalObjectIdV1::VersionRecord(_) => "01-",
        TypedPhysicalObjectIdV1::Tree(_) => "02-",
        TypedPhysicalObjectIdV1::File(_) => "03-",
        TypedPhysicalObjectIdV1::Symlink(_) => "04-",
        TypedPhysicalObjectIdV1::Chunk(_) => "05-",
    };
    let mut name = String::from(prefix);
    for byte in id.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02x}").unwrap();
    }
    root.join("objects").join(name)
}

#[test]
fn pack_is_transferred_once_then_reopened_through_committed_catalog() {
    let fixture = TestRoot::new("install");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    assert!(cas.fixed_handle_ledger_charge_bytes().unwrap() <= BASE_LEDGER_BYTES);
    let objects = [object(5, b"first"), object(5, b"second")];
    let ids = objects
        .iter()
        .map(|bytes| typed_id(bytes))
        .collect::<Vec<_>>();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
        .unwrap();

    assert_eq!(admission.outcome(), FsPackAdmissionOutcomeV1::Installed);
    assert_eq!(admission.sealed().pack_len(), 432);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(counters.fscas_bytes_written, 0);
    assert!(counters.fscas_bytes_read > 0);
    assert!(counters.fscas_read_calls > 0);
    assert_eq!(counters.fscas_catalog_operations, 1);
    assert!(counters.pack_allocated_blocks >= admission.sealed().pack_len());
    assert!(counters.has_zero_forbidden_work());
    assert_eq!(ledger.admitted_slots(), 0);

    let reopened = FsCasV1::open_existing(&fixture.path).unwrap();
    let mut occupied = reopened.occupied().unwrap();
    for (id, expected) in ids.into_iter().zip(&objects) {
        assert_eq!(
            occupied.occupied_len(id).unwrap(),
            Some(expected.len() as u64)
        );
        let mut actual = vec![0_u8; expected.len()];
        for (offset, block) in actual.chunks_mut(7).enumerate() {
            occupied
                .read_occupied_exact_at(id, (offset * 7) as u64, block)
                .unwrap();
        }
        assert_eq!(actual, *expected);
    }
    assert!(occupied.read_calls() > 0);
    assert!(occupied.bytes_read() >= objects.iter().map(|bytes| bytes.len() as u64).sum());
}

#[test]
fn catalog_counter_overflow_precedes_every_visibility_transition() {
    let fixture = TestRoot::new("catalog-counter-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"counter-overflow")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    counters.fscas_catalog_operations = u64::MAX;
    let mut spool = Spool::default();

    assert_eq!(
        cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch,),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(counters.fscas_catalog_operations, u64::MAX);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(counters.fscas_bytes_written, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn carrier_cleanup_failure_invalidates_owner_and_root() {
    let fixture = TestRoot::new("carrier-cleanup-failure");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"carrier-cleanup")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut stale_occupied = cas.occupied().unwrap();
    let mut stale_private = cas.begin_private_pack().unwrap();
    stale_private.begin_private(1).unwrap();
    let mut stale_operation = cas.begin_closure_operation().unwrap();
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let pack_len = pack.len().unwrap();
    let mut spool = Spool::default();
    let mut control = StopWithCleanupFailure::new(
        FsCasBoundaryV1::AfterCarrierInstall,
        FsCasCleanupTargetV1::Carrier,
    );

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Invalidated)
    );
    assert!(control.injected);
    assert_eq!(counters.unreachable_installed_residue_bytes, pack_len);
    assert_eq!(stale_private.append(b"x"), Err(PackPortErrorV1::Failure));
    drop(stale_private);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert!(fixture.path.join("invalidated").is_dir());
    assert_eq!(
        stale_occupied.occupied_len(typed_id(&objects[0])),
        Err(ImmutablePortErrorV1::Failure)
    );
    let no_objects = [];
    let mut stale_closure = ClosureSource {
        objects: &no_objects,
    };
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    let (_, _, version_id, _) = empty_closure();
    assert_eq!(
        cas.admit_complete_closure(
            &mut stale_operation,
            &mut stale_closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::SinkRefused)
    );
    assert!(matches!(
        cas.begin_private_pack(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        cas.begin_closure_operation(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn locator_cleanup_failure_is_counted_and_cannot_poison_a_later_admission() {
    let fixture = TestRoot::new("locator-cleanup-failure");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [
        object(5, b"locator-cleanup-a"),
        object(5, b"locator-cleanup-b"),
    ];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = StopWithCleanupFailure::new(
        FsCasBoundaryV1::AfterObjectLocatorPublication,
        FsCasCleanupTargetV1::ObjectLocator,
    );

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Invalidated)
    );
    assert!(control.injected);
    assert_eq!(counters.unreachable_installed_residue_bytes, 160);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert!(fixture.path.join("invalidated").is_dir());
    assert!(matches!(
        cas.begin_private_pack(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn overlapping_packs_reuse_one_object_without_poisoning_lookup() {
    let fixture = TestRoot::new("overlapping-packs");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let extra = object(5, b"pack-b-only");
    let extra_id = typed_id(&extra);
    let pack_a_objects = [version.clone(), root.clone()];
    let pack_b_objects = [version.clone(), extra.clone()];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();

    let mut canonical_shared_locator = None;
    for (index, objects) in [&pack_a_objects[..], &pack_b_objects[..]]
        .into_iter()
        .enumerate()
    {
        let mut pack = build_private_pack(&cas, objects, &ledger, &mut counters, &mut scratch);
        assert_eq!(
            cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch,)
                .unwrap()
                .outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );
        let observed = fs::read(locator_path(&fixture.path, version_id)).unwrap();
        if index == 0 {
            canonical_shared_locator = Some(observed);
        } else {
            assert_eq!(Some(observed), canonical_shared_locator);
        }
    }
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        3
    );

    let mut occupied = cas.occupied().unwrap();
    for (id, expected) in [(version_id, &version), (root_id, &root), (extra_id, &extra)] {
        assert_eq!(
            occupied.occupied_len(id).unwrap(),
            Some(expected.len() as u64)
        );
        let mut actual = vec![0_u8; expected.len()];
        occupied.read_occupied_exact_at(id, 0, &mut actual).unwrap();
        assert_eq!(actual, *expected);
    }

    let closure_objects = [(version_id, version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert!(cas
        .admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        )
        .is_ok());
}

#[test]
fn nonexistent_objects_cannot_mint_a_closure_capability() {
    let fixture = TestRoot::new("closure-spoof-regression");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [(version_id, version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::SinkRefused)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn spoofed_closure_bytes_cannot_mint_a_capability() {
    let fixture = TestRoot::new("closure-spoofed-bytes");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &[version.clone(), root.clone()],
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut spoofed_version = version;
    spoofed_version[52] ^= 1;
    let closure_objects = [(version_id, spoofed_version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::IdMismatch)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn duplicate_typed_ids_cannot_enter_the_closure_transcript() {
    let fixture = TestRoot::new("closure-duplicate-id");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [
        (version_id, version.clone()),
        (version_id, version),
        (root_id, root),
    ];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::NonCanonicalOrder)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn wrong_version_record_cannot_mint_a_closure_capability() {
    let fixture = TestRoot::new("closure-wrong-version");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let wrong_version = TypedPhysicalObjectIdV1::VersionRecord(
        derive_physical_version_record_id_v1(&object(1, b"wrong-version")).unwrap(),
    );
    let closure_objects = [(version_id, version.clone()), (root_id, root.clone())];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &[version, root],
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            wrong_version,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::MissingClosureEdge)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn forced_equal_typed_id_with_unequal_incumbent_bytes_fails_closed() {
    let fixture = TestRoot::new("forced-object-id-collision");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let shared = object(5, &[0x71; 4096]);
    let shared_id = typed_id(&shared);
    let winner_only = object(5, b"winner-only");
    let loser_only = object(5, b"loser-only");
    let loser_only_id = typed_id(&loser_only);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut winner_counters = OperationCountersV1::default();
    let mut winner = build_private_pack(
        &cas,
        &[shared.clone(), winner_only],
        &ledger,
        &mut winner_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut winner_counters,
        &mut scratch,
    )
    .unwrap();

    // Preserve the incumbent locator's claimed typed ID while changing one
    // byte in the canonical object it names. A new valid pack with the same
    // object ID must validate those incumbent bytes completely and fail
    // closed, rather than publishing a second locator.
    let marker = fs::read(locator_path(&fixture.path, shared_id)).unwrap();
    let object_offset = u64::from_be_bytes(marker[104..112].try_into().unwrap())
        .checked_add(4)
        .unwrap();
    let object_len = u32::from_be_bytes(marker[112..116].try_into().unwrap());
    let carrier = only_entry(&fixture.path.join("carriers"));
    let original_permissions = make_owner_writable(&carrier);
    let mut bytes = fs::read(&carrier).unwrap();
    let corrupt_at = usize::try_from(object_offset + u64::from(object_len) - 1).unwrap();
    bytes[corrupt_at] ^= 0xff;
    fs::write(&carrier, bytes).unwrap();
    fs::set_permissions(&carrier, original_permissions).unwrap();

    let mut loser_counters = OperationCountersV1::default();
    let mut loser = build_private_pack(
        &cas,
        &[shared, loser_only],
        &ledger,
        &mut loser_counters,
        &mut scratch,
    );
    assert_eq!(
        cas.admit_pack(
            &mut loser,
            &mut spool,
            &ledger,
            &mut loser_counters,
            &mut scratch,
        ),
        Err(FsCasErrorV1::Integrity)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        2
    );
    assert!(!locator_path(&fixture.path, loser_only_id).exists());
    assert_eq!(loser_counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(loser_counters.has_zero_forbidden_work());
}

#[test]
fn cancellation_during_shared_object_validation_removes_only_the_loser() {
    let fixture = TestRoot::new("overlap-cancel");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, _) = empty_closure();
    let extra = object(5, b"loser-only");
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut winner_counters = OperationCountersV1::default();
    let mut winner = build_private_pack(
        &cas,
        &[version.clone(), root],
        &ledger,
        &mut winner_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut winner_counters,
        &mut scratch,
    )
    .unwrap();

    let mut loser_counters = OperationCountersV1::default();
    let mut loser = build_private_pack(
        &cas,
        &[version, extra],
        &ledger,
        &mut loser_counters,
        &mut scratch,
    );
    let mut control = StopAtBoundary::new(
        StopKind::Cancellation,
        FsCasBoundaryV1::AfterObjectComparisonWindow,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut loser,
            &mut spool,
            &ledger,
            &mut loser_counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Cancelled))
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        2
    );
    assert!(locator_path(&fixture.path, version_id).is_file());
    assert_eq!(loser_counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(loser_counters.has_zero_forbidden_work());
}

#[test]
fn malformed_object_locator_fails_closed_without_publishing_the_loser() {
    let fixture = TestRoot::new("malformed-object-locator");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, _) = empty_closure();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut counters = OperationCountersV1::default();
    let mut winner = build_private_pack(
        &cas,
        &[version.clone(), root],
        &ledger,
        &mut counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut counters,
        &mut scratch,
    )
    .unwrap();
    let locator = locator_path(&fixture.path, version_id);
    let original_permissions = make_owner_writable(&locator);
    fs::write(&locator, b"truncated").unwrap();
    fs::set_permissions(&locator, original_permissions).unwrap();

    let mut loser_counters = OperationCountersV1::default();
    let mut loser = build_private_pack(
        &cas,
        &[version, object(5, b"loser")],
        &ledger,
        &mut loser_counters,
        &mut scratch,
    );
    assert_eq!(
        cas.admit_pack(
            &mut loser,
            &mut spool,
            &ledger,
            &mut loser_counters,
            &mut scratch,
        ),
        Err(FsCasErrorV1::Invalidated)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(loser_counters.unreachable_installed_residue_bytes, 0);
    assert!(fixture.path.join("invalidated").is_dir());
    assert!(matches!(
        cas.begin_private_pack(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(loser_counters.has_zero_forbidden_work());
}

#[test]
fn simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator() {
    let fixture = TestRoot::new("overlap-race");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let left_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let right_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let shared = object(5, &[0x6d; 4096]);
    let left = object(5, b"left-only");
    let right = object(5, b"right-only");
    let shared_id = typed_id(&shared);
    let left_id = typed_id(&left);
    let right_id = typed_id(&right);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut build_scratch = [0_u8; 65_536];
    let mut left_build = OperationCountersV1::default();
    let mut right_build = OperationCountersV1::default();
    let left_objects = [shared.clone(), left.clone()];
    let right_objects = [shared.clone(), right.clone()];
    let mut left_pack = build_private_pack(
        &left_cas,
        &left_objects,
        &ledger,
        &mut left_build,
        &mut build_scratch,
    );
    let mut right_pack = build_private_pack(
        &right_cas,
        &right_objects,
        &ledger,
        &mut right_build,
        &mut build_scratch,
    );
    let barrier = Arc::new(Barrier::new(3));

    let (left_result, right_result) = std::thread::scope(|scope| {
        let left_barrier = barrier.clone();
        let left_ledger = &ledger;
        let left_join = scope.spawn(move || {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            left_barrier.wait();
            let admission = left_cas.admit_pack(
                &mut left_pack,
                &mut spool,
                left_ledger,
                &mut left_build,
                &mut scratch,
            );
            (admission, left_build)
        });
        let right_barrier = barrier.clone();
        let right_ledger = &ledger;
        let right_join = scope.spawn(move || {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            right_barrier.wait();
            let admission = right_cas.admit_pack(
                &mut right_pack,
                &mut spool,
                right_ledger,
                &mut right_build,
                &mut scratch,
            );
            (admission, right_build)
        });
        barrier.wait();
        (left_join.join().unwrap(), right_join.join().unwrap())
    });

    for (result, counters) in [left_result, right_result] {
        assert_eq!(
            result.unwrap().outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );
        assert_eq!(counters.fscas_bytes_written, 0);
        assert!(counters.has_zero_forbidden_work());
    }
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        3
    );
    let mut occupied = cas.occupied().unwrap();
    for (id, expected) in [(shared_id, shared), (left_id, left), (right_id, right)] {
        assert_eq!(
            occupied.occupied_len(id).unwrap(),
            Some(expected.len() as u64)
        );
    }
    assert_eq!(ledger.admitted_slots(), 0);
}

#[test]
fn every_fresh_admission_boundary_cleans_or_counts_exact_residue() {
    let boundaries = [
        FsCasBoundaryV1::BeforeCandidateValidation,
        FsCasBoundaryV1::AfterCandidateValidation,
        FsCasBoundaryV1::BeforeCarrierInstall,
        FsCasBoundaryV1::AfterCarrierInstall,
        FsCasBoundaryV1::AfterCarrierValidation,
        FsCasBoundaryV1::AfterCarrierMadeImmutable,
        FsCasBoundaryV1::BeforeObjectLocatorPublication,
        FsCasBoundaryV1::AfterObjectLocatorPublication,
        FsCasBoundaryV1::BeforeCatalogPublication,
        FsCasBoundaryV1::AfterCatalogPublication,
    ];
    for boundary in boundaries {
        let fixture = TestRoot::new(&format!("fresh-boundary-{boundary:?}"));
        let cas = FsCasV1::create_new(&fixture.path).unwrap();
        let objects = [object(5, b"boundary-fault")];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; 65_536];
        let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
        let pack_len = pack.len().unwrap();
        assert_eq!(pack_len, 296);
        let mut spool = Spool::default();
        let mut control = StopAtBoundary::new(StopKind::Cancellation, boundary);
        assert_eq!(
            cas.admit_pack_controlled(
                &mut pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
                &mut control,
            ),
            Err(FsCasErrorV1::Core(CoreError::Cancelled)),
            "boundary {boundary:?}"
        );
        assert_eq!(ledger.admitted_slots(), 0, "boundary {boundary:?}");
        assert_eq!(
            fs::read_dir(fixture.path.join("preparation"))
                .unwrap()
                .count(),
            0,
            "boundary {boundary:?}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("closures")).unwrap().count(),
            0,
            "boundary {boundary:?}"
        );
        if boundary == FsCasBoundaryV1::AfterCatalogPublication {
            assert_eq!(
                fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
                1
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
                1
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("objects")).unwrap().count(),
                1
            );
            assert_eq!(counters.unreachable_installed_residue_bytes, pack_len);
            assert_eq!(counters.fscas_bytes_written, 0);
        } else {
            assert_eq!(
                fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
                0,
                "boundary {boundary:?}"
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
                0,
                "boundary {boundary:?}"
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("objects")).unwrap().count(),
                0,
                "boundary {boundary:?}"
            );
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        }
        assert_eq!(counters.closure_fences, 0);
        assert!(counters.open_files_high_water <= 2);
        assert!(counters.has_zero_forbidden_work());
    }
}

#[test]
fn partial_multi_object_locator_publication_is_fully_rolled_back() {
    let fixture = TestRoot::new("partial-locator-publication");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [
        object(5, b"locator-a"),
        object(5, b"locator-b"),
        object(5, b"locator-c"),
    ];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = StopAtBoundary::new(
        StopKind::Cancellation,
        FsCasBoundaryV1::AfterObjectLocatorPublication,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Cancelled))
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(counters.fscas_bytes_written, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn every_incumbent_boundary_cleans_loser_without_changing_winner() {
    let fixture = TestRoot::new("incumbent-boundaries");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0x3c; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut incumbent,
        &mut spool,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    )
    .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    let original = fs::read(&carrier).unwrap();

    for boundary in [
        FsCasBoundaryV1::BeforeIncumbentMarkerRead,
        FsCasBoundaryV1::AfterIncumbentMarkerRead,
        FsCasBoundaryV1::AfterIncumbentValidation,
        FsCasBoundaryV1::BeforeIncumbentComparisonWindow,
        FsCasBoundaryV1::AfterIncumbentComparisonWindow,
        FsCasBoundaryV1::BeforeObjectLocatorRead,
        FsCasBoundaryV1::AfterObjectLocatorRead,
        FsCasBoundaryV1::AfterObjectIncumbentValidation,
    ] {
        let mut counters = OperationCountersV1::default();
        let mut candidate =
            build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
        let mut control = StopAtBoundary::new(StopKind::Cancellation, boundary);
        assert_eq!(
            cas.admit_pack_controlled(
                &mut candidate,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
                &mut control,
            ),
            Err(FsCasErrorV1::Core(CoreError::Cancelled)),
            "boundary {boundary:?}"
        );
        assert_eq!(ledger.admitted_slots(), 0);
        assert_eq!(
            fs::read_dir(fixture.path.join("preparation"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(fs::read(&carrier).unwrap(), original);
        assert_eq!(
            fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
            1
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
            1
        );
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert_eq!(counters.closure_fences, 0);
        assert!(counters.open_files_high_water <= 2);
        assert!(counters.has_zero_forbidden_work());
    }
}

#[test]
fn catalog_publication_io_fault_removes_validated_unpublished_carrier() {
    let fixture = TestRoot::new("catalog-fault");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"catalog-fault")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = BreakCatalogAtPublication {
        catalog: fixture.path.join("catalog"),
        injected: false,
    };
    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Io)
    );
    assert!(control.injected);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert!(fixture.path.join("catalog").is_file());
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(counters.closure_fences, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn lost_hard_link_containment_capability_returns_unsupported_without_fallback() {
    let fixture = TestRoot::new("unsupported-link-capability");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"unsupported")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    fs::remove_dir(fixture.path.join("carriers")).unwrap();
    fs::write(fixture.path.join("carriers"), b"not-a-private-directory").unwrap();
    let mut spool = Spool::default();
    assert_eq!(
        cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch),
        Err(FsCasErrorV1::Unsupported)
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(counters.fscas_bytes_written, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_catalog_is_visible_only_after_complete_carrier_backed_validation() {
    let fixture = TestRoot::new("closure-fence");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
        )
        .unwrap();
    assert_eq!(admission.outcome(), FsPackAdmissionOutcomeV1::Installed);
    assert_eq!(admission.sealed().pack_len(), 616);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let preclosure_fscas_bytes_read = counters.fscas_bytes_read;
    let preclosure_fscas_read_calls = counters.fscas_read_calls;
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    let (admitted, mut capability) = cas
        .admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        )
        .unwrap();

    assert_eq!(admitted.version_record(), version_id);
    assert_eq!(admitted.object_count(), 2);
    assert_eq!(admitted.created_count(), 0);
    assert_eq!(admitted.reused_count(), 2);
    assert_eq!(capability.version_record().unwrap(), version_id);
    assert_eq!(capability.object_count().unwrap(), 2);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        1
    );
    assert_eq!(counters.closure_fences, 1);
    assert!(counters.bytes_read > 0);
    assert!(counters.fscas_bytes_read > preclosure_fscas_bytes_read);
    assert!(counters.fscas_read_calls > preclosure_fscas_read_calls);
    cas.consume_validated_closure_for_handoff(&mut operation, &mut capability)
        .unwrap();
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_counter_overflow_precedes_the_complete_fence() {
    let fixture = TestRoot::new("closure-counter-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();
    counters.closure_fences = u64::MAX;

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.closure_fences, u64::MAX);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn fscas_read_counter_overflow_precedes_the_complete_fence() {
    let fixture = TestRoot::new("closure-read-counter-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut install_counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut install_counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut install_counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut counters = OperationCountersV1 {
        fscas_bytes_read: u64::MAX,
        ..OperationCountersV1::default()
    };
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.fscas_bytes_read, u64::MAX);
    assert_eq!(counters.fscas_read_calls, 0);
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_capability_rejects_cross_fscas_cross_operation_and_replay() {
    let fixture = TestRoot::new("closure-capability-binding");
    let other_fixture = TestRoot::new("closure-capability-other-fscas");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let other_cas = FsCasV1::create_new(&other_fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [(version_id, version.clone()), (root_id, root.clone())];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &[version, root],
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut operation_a = cas.begin_closure_operation().unwrap();
    let mut closure_a = ClosureSource {
        objects: &closure_objects,
    };
    let mut incoming_a = [0_u8; 65_536];
    let mut occupied_a = [0_u8; 65_536];
    let mut source_a = [0_u8; 32_768];
    let mut ring_a = [0_u8; 32_768];
    let mut traversal_a = [0_u8; 1];
    let (_, mut capability_a) = cas
        .admit_complete_closure(
            &mut operation_a,
            &mut closure_a,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_a,
                &mut occupied_a,
                &mut source_a,
                &mut ring_a,
                &mut traversal_a,
            ),
        )
        .unwrap();

    let mut operation_b = cas.begin_closure_operation().unwrap();
    let mut closure_b = ClosureSource {
        objects: &closure_objects,
    };
    let mut incoming_b = [0_u8; 65_536];
    let mut occupied_b = [0_u8; 65_536];
    let mut source_b = [0_u8; 32_768];
    let mut ring_b = [0_u8; 32_768];
    let mut traversal_b = [0_u8; 1];
    let (_, mut capability_b) = cas
        .admit_complete_closure(
            &mut operation_b,
            &mut closure_b,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_b,
                &mut occupied_b,
                &mut source_b,
                &mut ring_b,
                &mut traversal_b,
            ),
        )
        .unwrap();

    assert_eq!(
        other_cas.consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a,),
        Err(FsCasErrorV1::Integrity)
    );
    assert_eq!(
        cas.consume_validated_closure_for_handoff(&mut operation_b, &mut capability_a),
        Err(FsCasErrorV1::Integrity)
    );
    cas.consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a)
        .unwrap();
    assert_eq!(
        cas.consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a),
        Err(FsCasErrorV1::Integrity)
    );
    cas.consume_validated_closure_for_handoff(&mut operation_b, &mut capability_b)
        .unwrap();
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(other_fixture.path.join("closures"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(counters.closure_fences, 2);

    let doomed_objects = [object(5, b"invalidate-issued-closure")];
    let mut doomed_pack = build_private_pack(
        &cas,
        &doomed_objects,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut cleanup_failure = StopWithCleanupFailure::new(
        FsCasBoundaryV1::AfterCarrierInstall,
        FsCasCleanupTargetV1::Carrier,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut doomed_pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
            &mut cleanup_failure,
        ),
        Err(FsCasErrorV1::Invalidated)
    );
    assert_eq!(
        capability_b.version_record(),
        Err(FsCasErrorV1::Invalidated)
    );
    assert_eq!(capability_b.object_count(), Err(FsCasErrorV1::Invalidated));
    assert_eq!(
        cas.consume_validated_closure_for_handoff(&mut operation_b, &mut capability_b),
        Err(FsCasErrorV1::Invalidated)
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_validation_failure_returns_no_closure_and_counts_installed_residue() {
    let fixture = TestRoot::new("closure-validation-fault");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
        )
        .unwrap();
    assert_eq!(admission.sealed().pack_len(), 616);
    let mut malformed_root = root;
    *malformed_root.last_mut().unwrap() ^= 1;
    let closure_objects = [(version_id, version), (root_id, malformed_root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::TypedEdge)
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    admission
        .record_later_unreachable_residue(&mut counters)
        .unwrap();
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        admission.sealed().pack_len()
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_fence_io_failure_returns_no_closure_or_publication() {
    let fixture = TestRoot::new("closure-fence-fault");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
        )
        .unwrap();
    assert_eq!(admission.sealed().pack_len(), 616);
    fs::remove_dir(fixture.path.join("closures")).unwrap();
    fs::write(fixture.path.join("closures"), b"injected-not-a-directory").unwrap();

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::SinkRefused)
    );
    assert!(counters.bytes_read > 0);
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(counters.publication_dispatches, 0);
    assert!(fixture.path.join("closures").is_file());
    admission
        .record_later_unreachable_residue(&mut counters)
        .unwrap();
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        admission.sealed().pack_len()
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn same_pack_race_is_no_replace_and_compares_every_incumbent_byte() {
    let fixture = TestRoot::new("race");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0x5a; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut first_counters = OperationCountersV1::default();
    let mut first = build_private_pack(&cas, &objects, &ledger, &mut first_counters, &mut scratch);
    let mut spool = Spool::default();
    let installed = cas
        .admit_pack(
            &mut first,
            &mut spool,
            &ledger,
            &mut first_counters,
            &mut scratch,
        )
        .unwrap();
    assert_eq!(installed.sealed().pack_len(), 33_048);
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    let original_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut second_counters = OperationCountersV1::default();
    let mut second =
        build_private_pack(&cas, &objects, &ledger, &mut second_counters, &mut scratch);
    let reused = cas
        .admit_pack(
            &mut second,
            &mut spool,
            &ledger,
            &mut second_counters,
            &mut scratch,
        )
        .unwrap();
    assert_eq!(reused.outcome(), FsPackAdmissionOutcomeV1::ExistingComplete);
    assert_eq!(
        second_counters.incumbent_comparison_bytes,
        installed.sealed().pack_len()
    );
    assert_eq!(second_counters.incumbent_comparison_windows, 2);
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), original_inode);
    }
    assert!(second_counters.has_zero_forbidden_work());
}

#[test]
fn malformed_incumbent_fails_closed_without_overwrite_or_fallback() {
    let fixture = TestRoot::new("malformed");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"immutable")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut counters = OperationCountersV1::default();
    let mut first = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    cas.admit_pack(&mut first, &mut spool, &ledger, &mut counters, &mut scratch)
        .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&carrier, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(&carrier).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&carrier, permissions).unwrap();
    }
    let mut bytes = fs::read(&carrier).unwrap();
    bytes[0] ^= 0xff;
    fs::write(&carrier, bytes).unwrap();
    #[cfg(unix)]
    let incumbent_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    let result = cas.admit_pack(
        &mut candidate,
        &mut spool,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    assert!(matches!(
        result,
        Err(FsCasErrorV1::Core(CoreError::PackInvalid)) | Err(FsCasErrorV1::Integrity)
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), incumbent_inode);
    }
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert!(candidate_counters.has_zero_forbidden_work());
    drop(candidate);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn source_failure_cleans_preinstall_state_and_releases_the_slot() {
    let fixture = TestRoot::new("cleanup");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"never-installed")];
    let mut source = ObjectSource::new(&objects);
    source.fail_payload = true;
    let mut pack = cas.begin_private_pack().unwrap();
    let mut spool = Spool::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    assert_eq!(
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::SourceFailure)
    );
    drop(pack);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn deadline_before_install_removes_private_pack_and_releases_resources() {
    let fixture = TestRoot::new("deadline");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"deadline-before-install")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control =
        StopAtBoundary::new(StopKind::Deadline, FsCasBoundaryV1::BeforeCarrierInstall);
    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Deadline))
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn cancellation_during_loser_readback_keeps_incumbent_and_cleans_candidate() {
    let fixture = TestRoot::new("cancel-loser");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0xa5; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut incumbent,
        &mut spool,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    )
    .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    let incumbent_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    let mut control = StopAtBoundary::new(
        StopKind::Cancellation,
        FsCasBoundaryV1::AfterIncumbentComparisonWindow,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut candidate,
            &mut spool,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Cancelled))
    );
    assert_eq!(candidate_counters.incumbent_comparison_windows, 1);
    assert_eq!(candidate_counters.incumbent_comparison_bytes, 32_768);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), incumbent_inode);
    }
    assert_eq!(candidate_counters.unreachable_installed_residue_bytes, 0);
    assert!(candidate_counters.has_zero_forbidden_work());
}

#[test]
fn later_closure_failure_is_counted_residue_not_a_private_version() {
    let fixture = TestRoot::new("residue");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"carrier-only")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
        .unwrap();
    assert_eq!(admission.sealed().pack_len(), 296);
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [(version_id, version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            version_id,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::SinkRefused)
    );
    admission
        .record_later_unreachable_residue(&mut counters)
        .unwrap();
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        admission.sealed().pack_len()
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[cfg(unix)]
#[test]
fn symlinked_parent_is_typed_unsupported_before_namespace_creation() {
    use std::os::unix::fs::symlink;

    let fixture = TestRoot::new("symlink-parent");
    fs::create_dir(&fixture.path).unwrap();
    let actual = fixture.path.join("actual");
    fs::create_dir(&actual).unwrap();
    let linked = fixture.path.join("linked");
    symlink(&actual, &linked).unwrap();
    assert!(matches!(
        FsCasV1::create_new(&linked.join("cas")),
        Err(FsCasErrorV1::Unsupported)
    ));
    assert!(!actual.join("cas").exists());
}
