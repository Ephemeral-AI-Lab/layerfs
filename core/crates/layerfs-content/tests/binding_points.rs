//! Issued point grammar and actual sparse16 source work, outside product source.
mod support;
use layerfs_content::filesystem::rows::*;
use layerfs_content::filesystem::{scope_for_seed, DirectoryUpdate, FilesystemInput, PathName};
use layerfs_content::{ContentError, ContentResult};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use support::filesystem::{resources, TempDir};

fn name(index: usize) -> PathName {
    PathName::new(&format!("{}n{index:06}", "a".repeat(248))).unwrap()
}
fn child(index: usize) -> Option<u64> {
    (index % 7 != 0).then_some(index as u64 + 2)
}
fn create(count: usize) -> (TempDir, RowSpool, BindingSourceId) {
    let dir = TempDir::new("issued-points");
    let declaration = SpoolDeclaration {
        directories: 2,
        inodes: 0,
        fresh: 0,
        bindings: count as u64,
        wire_name_bytes: count as u64 * 265,
    };
    let preparation =
        SpoolPreparation::new(declaration, declaration.required_bytes_upper().unwrap()).unwrap();
    let source = preparation.source_id();
    let path = dir.path().join("rows");
    assert!(!path.exists());
    let mut spool = RowSpool::create_prepared(path, preparation).unwrap();
    assert_eq!(spool.binding_source_id().unwrap(), source);
    spool.begin_directory(1, 0).unwrap();
    spool.end_directory(1, 0, 0).unwrap();
    spool.begin_directory(2, count as u32).unwrap();
    for index in 0..count {
        spool.push_binding(&name(index), child(index)).unwrap();
    }
    spool
        .end_directory(2, count as u32, declaration.wire_name_bytes)
        .unwrap();
    spool.seal().unwrap();
    (dir, spool, source)
}

#[test]
fn literal_point28_empty_eof_range_and_full_u64_descriptor() {
    let authority = BindingAuthority::new().unwrap();
    let source = authority.source_id();
    let header = authority.header(i64::MAX as u64, u64::MAX, 3, 795).unwrap();
    let point = BindingPoint::new(&header, 2).unwrap();
    let mut expected = [0; 28];
    expected[..8].copy_from_slice(&source.as_bytes());
    expected[8..16].copy_from_slice(&(i64::MAX as u64).to_be_bytes());
    expected[16..24].copy_from_slice(&u64::MAX.to_be_bytes());
    expected[24..28].copy_from_slice(&2u32.to_be_bytes());
    assert_eq!(point.encode(), expected);
    assert_eq!(BindingPoint::decode_for(source, &expected).unwrap(), point);
    assert_eq!(
        (
            point.parent(),
            point.header_descriptor(),
            point.binding_ordinal()
        ),
        (i64::MAX as u64, u64::MAX, 2)
    );
    assert!(BindingPoint::new(&header, 3).is_err());
    let empty = authority.header(1, 0, 0, 0).unwrap();
    assert!(BindingPoint::new(&empty, 0).is_err());
    for parent in [0u64, i64::MAX as u64 + 1, u64::MAX] {
        let mut bad = expected;
        bad[8..16].copy_from_slice(&parent.to_be_bytes());
        assert_eq!(
            BindingPoint::decode_for(source, &bad),
            Err(ContentError::InvalidRecord("binding point parent"))
        );
    }
    let other = BindingAuthority::new().unwrap();
    assert_eq!(
        BindingPoint::decode_for(other.source_id(), &expected),
        Err(ContentError::InvalidRecord("binding point issuer"))
    );
    assert!(BindingPoint::decode_for(source, &expected[..27]).is_err());
}

#[test]
fn real_spool_point_decodes_only_one_complete16_block_with_zero_name_probes() {
    let (_dir, spool, source) = create(4_097);
    assert_eq!(spool.binding_source_id().unwrap(), source);
    let header = spool.directory_header(2).unwrap().unwrap();
    for index in [4096, 0, 2047, 16, 4095, 1, 127, 128, 129] {
        let point = BindingPoint::new(&header, index as u32).unwrap();
        let before = spool.read_work();
        assert_eq!(
            spool.binding_at(&point).unwrap(),
            (name(index), child(index))
        );
        let after = spool.read_work();
        let decoded = after.bindings_decoded - before.bindings_decoded;
        assert_eq!(decoded, (4097 - index / 16 * 16).min(16) as u64);
        assert_eq!(after.checkpoint_probes, before.checkpoint_probes);
        assert_eq!(after.slot_reads - before.slot_reads, 2);
        assert!(after.checkpoint_reads - before.checkpoint_reads <= 6);
        assert_eq!(
            after.read_bytes - before.read_bytes,
            48 + 64 + 8 * (after.checkpoint_reads - before.checkpoint_reads) + 264 * decoded
        );
    }
    eprintln!("Point count proof:4097 names, exactly<=16 selected-block decodes,2 checked adjacent slots,0 name probes; cache/RSS/storage timing not measured");
}

#[test]
fn real_spool_receiver_checks_foreign_wrong_parent_descriptor_and_ordinal_before_names() {
    let (_dir, mut spool, source) = create(17);
    let header = spool.directory_header(2).unwrap().unwrap();
    let point = BindingPoint::new(&header, 16).unwrap();
    for (offset, replacement) in [(8, 3u64), (16, 0), (16, 2), (16, u64::MAX)] {
        let mut encoded = point.encode();
        encoded[offset..offset + 8].copy_from_slice(&replacement.to_be_bytes());
        let bad = BindingPoint::decode_for(source, &encoded).unwrap();
        let before = spool.read_work();
        assert!(spool.binding_at(&bad).is_err());
        assert_eq!(spool.read_work().bindings_decoded, before.bindings_decoded);
    }
    let mut encoded = point.encode();
    encoded[24..28].copy_from_slice(&17u32.to_be_bytes());
    let bad = BindingPoint::decode_for(source, &encoded).unwrap();
    assert!(spool.binding_at(&bad).is_err());
    let foreign = BindingAuthority::new().unwrap();
    let foreign_point = BindingPoint::new(&foreign.header(2, 1, 1, 11).unwrap(), 0).unwrap();
    let before = spool.read_work();
    assert_eq!(
        spool.binding_at(&foreign_point),
        Err(ContentError::InvalidRecord("directory issuer"))
    );
    assert_eq!(spool.read_work(), before);
    spool.cleanup().unwrap();
    assert_eq!(
        spool.binding_at(&point),
        Err(ContentError::IncompleteOperation)
    );
    assert_eq!(
        spool.binding_at(&foreign_point),
        Err(ContentError::InvalidRecord("directory issuer"))
    );
}

fn write(spool: &RowSpool, at: u64, bytes: &[u8]) {
    let mut file = OpenOptions::new().write(true).open(spool.path()).unwrap();
    file.seek(SeekFrom::Start(at)).unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
}
fn selected_offset(spool: &RowSpool) -> u64 {
    let mut file = OpenOptions::new().read(true).open(spool.path()).unwrap();
    file.seek(SeekFrom::Start(48 + 32 + 8)).unwrap();
    let mut encoded = [0; 8];
    file.read_exact(&mut encoded).unwrap();
    u64::from_be_bytes(encoded)
}

#[test]
fn later_corruption_in_selected_block_prevents_return_of_an_earlier_valid_point() {
    let (_dir, spool, _) = create(17);
    let header = spool.directory_header(2).unwrap().unwrap();
    let point = BindingPoint::new(&header, 0).unwrap();
    let offset = selected_offset(&spool);
    // Independent private format: one8-byte sparse checkpoint, each name record
    // len1+name255+child8=264. Selected0 must validate corrupt later record15.
    write(&spool, offset + 8 + 15 * 264, &[0]);
    let before = spool.read_work();
    assert_eq!(
        spool.binding_at(&point),
        Err(ContentError::InvalidRecord("directory row name"))
    );
    assert_eq!(
        spool.read_work().bindings_decoded - before.bindings_decoded,
        15
    );
}

#[test]
fn adjacent_checkpoint_span_corruption_refuses_before_selected_name_decode() {
    let (_dir, spool, _) = create(33);
    let point = BindingPoint::new(&spool.directory_header(2).unwrap().unwrap(), 16).unwrap();
    let offset = selected_offset(&spool);
    write(&spool, offset + 8, &(16u64 * 264).to_be_bytes());
    let before = spool.read_work();
    assert_eq!(
        spool.binding_at(&point),
        Err(ContentError::InvalidRecord("directory checkpoint order"))
    );
    assert_eq!(spool.read_work().bindings_decoded, before.bindings_decoded);
}

#[test]
fn borrowed_compatibility_and_both_prepared_delegates_preserve_exact_elements() {
    let directories = [DirectoryUpdate {
        parent: 7,
        changes: (0..33).map(|i| (name(i), child(i))).collect(),
    }];
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x13; 32]),
        root_serial: 7,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let slice = SliceBindingRows::new(&input).unwrap();
    let selected = slice.directory_header(7).unwrap().unwrap();
    let prepared = PreparedUpdate {
        base: input.base,
        scope: input.scope,
        root_serial: 7,
        resources: input.resources,
        rows: &slice,
    };
    let compatibility = CompatibilityBindingRows::new(&prepared).unwrap();
    let legacy = compatibility.directory_header(7).unwrap().unwrap();
    let bounded = PreparedBindingUpdate {
        base: input.base,
        scope: input.scope,
        root_serial: 7,
        resources: input.resources,
        rows: &slice,
    };
    assert_eq!(
        bounded.binding_source_id().unwrap(),
        slice.binding_source_id().unwrap()
    );
    for i in [0, 7, 16, 32] {
        let point = BindingPoint::new(&selected, i as u32).unwrap();
        let expected = (name(i), child(i));
        assert_eq!(input.legacy_binding_at(7, i as u32).unwrap(), expected);
        assert_eq!(slice.binding_at(&point).unwrap(), expected);
        assert_eq!(prepared.legacy_binding_at(7, i as u32).unwrap(), expected);
        assert_eq!(bounded.binding_at(&point).unwrap(), expected);
        assert_eq!(bounded.legacy_binding_at(7, i as u32).unwrap(), expected);
        assert_eq!(
            compatibility
                .binding_at(&BindingPoint::new(&legacy, i as u32).unwrap())
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        slice.binding_at(&BindingPoint::new(&legacy, 0).unwrap()),
        Err(ContentError::InvalidRecord("directory issuer"))
    );
    assert!(input.legacy_binding_at(7, 33).is_err());
    assert!(input.legacy_binding_at(8, 0).is_err());
}

#[test]
fn preparation_admits_before_issuance_and_creation_preserves_known_failures() {
    let dir = TempDir::new("point-preparation");
    let declaration = SpoolDeclaration {
        directories: 1,
        inodes: 0,
        fresh: 0,
        bindings: 1,
        wire_name_bytes: 11,
    };
    let bytes = declaration.required_bytes_upper().unwrap();
    assert!(matches!(
        SpoolPreparation::new(declaration, bytes - 1),
        Err(ContentError::ResourceUnavailable {
            what: "prepared row spool"
        })
    ));
    let preparation = SpoolPreparation::new(declaration, bytes).unwrap();
    assert!(!dir.path().join("rows").exists());
    let source = preparation.source_id();
    let mut spool = RowSpool::create_prepared(dir.path().join("rows"), preparation).unwrap();
    assert_eq!(spool.binding_source_id().unwrap(), source);
    spool.begin_directory(1, 1).unwrap();
    spool
        .push_binding(&PathName::new("a").unwrap(), Some(2))
        .unwrap();
    let error = spool.end_directory(1, 1, 12).unwrap_err();
    assert_eq!(spool.seal(), Err(error.clone()));
    assert_eq!(spool.binding_source_id(), Err(error));
}

// Default capability is explicit, without a cloned-row rescue path.
struct UnsupportedOrdinal<'a>(&'a FilesystemInput<'a>, std::cell::Cell<u64>);
impl RowSource for UnsupportedOrdinal<'_> {
    fn directory_rows(&self) -> usize {
        self.0.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.0.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.0.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        self.1.set(self.1.get() + 1);
        self.0.directories()
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.0.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.0.new_inodes()
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.1.set(self.1.get() + 1);
        self.0.directory_for(parent)
    }
    fn value_for(
        &self,
        serial: u64,
    ) -> ContentResult<Option<layerfs_content::inode_leaf::InodeValue>> {
        self.0.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.0.new_position(serial)
    }
}
impl PreparedRows for UnsupportedOrdinal<'_> {
    fn base(&self) -> Option<layerfs_content::filesystem::FilesystemRootId> {
        self.0.base()
    }
    fn scope(&self) -> layerfs_content::filesystem::InodeScope {
        self.0.scope()
    }
    fn root_serial(&self) -> u64 {
        self.0.root_serial()
    }
    fn resources(&self) -> layerfs_content::filesystem::FilesystemResources {
        self.0.resources()
    }
}
#[test]
fn old_external_ordinal_capability_refuses_without_error_driven_row_fallback() {
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(PathName::new("a").unwrap(), Some(2))],
    }];
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x19; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let original = UnsupportedOrdinal(&input, std::cell::Cell::new(0));
    let compatibility = CompatibilityBindingRows::new(&original).unwrap();
    let point = BindingPoint::new(&compatibility.directory_header(1).unwrap().unwrap(), 0).unwrap();
    let before = original.1.get();
    assert_eq!(
        compatibility.binding_at(&point),
        Err(ContentError::UnsupportedProfile {
            what: "binding ordinal access"
        })
    );
    assert_eq!(original.1.get(), before);
}
