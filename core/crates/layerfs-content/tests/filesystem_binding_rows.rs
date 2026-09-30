//! Scalar final names from a real owned spool and an independent iterator source.

use layerfs_content::filesystem::rows::{
    check_binding_input, BindingAuthority, BindingLookup, BindingRowSource, BindingRows,
    CompatibilityBindingRows, RowSource, RowSpool, SpoolDeclaration,
};
use layerfs_content::filesystem::PathName;
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ContentResult, ObjectId};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "support/legacy_binding_observer.rs"]
mod legacy_binding_observer;

struct OwnedPath(PathBuf);
impl OwnedPath {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(std::env::temp_dir().join(format!(
            "layerfs-binding-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for OwnedPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn name(index: usize, long: bool) -> PathName {
    let text = if long {
        format!("{}{:07}", "a".repeat(248), index)
    } else {
        format!("f{index:06}")
    };
    PathName::new(&text).unwrap()
}

fn child(index: usize) -> Option<u64> {
    if index % 11 == 0 {
        None
    } else {
        Some((index as u64 * 37) % 8_192 + 2)
    }
}

fn create(count: usize, long: bool) -> (OwnedPath, RowSpool) {
    let path = OwnedPath::new("rows");
    let length = if long { 255 } else { 7 };
    let declaration = SpoolDeclaration {
        directories: 2,
        inodes: 0,
        fresh: 0,
        bindings: count as u64,
        wire_name_bytes: count as u64 * (10 + length),
    };
    let mut spool = RowSpool::create_declared(
        path.path().to_path_buf(),
        declaration,
        declaration.required_bytes_upper().unwrap(),
    )
    .unwrap();
    spool.begin_directory(1, 0).unwrap();
    spool.end_directory(1, 0, 0).unwrap();
    spool.begin_directory(2, count as u32).unwrap();
    for index in 0..count {
        spool
            .push_binding(&name(index, long), child(index))
            .unwrap();
    }
    spool
        .end_directory(2, count as u32, declaration.wire_name_bytes)
        .unwrap();
    spool.seal().unwrap();
    (path, spool)
}

#[test]
fn exact_names_children_empty_rows_and_replay_cross_every_cursor_boundary() {
    for count in [0, 15, 16, 17, 127, 128, 129, 1_024] {
        let (path, mut spool) = create(count, false);
        let mut headers = spool.directory_headers().unwrap();
        let empty = headers.next_header().unwrap().unwrap();
        let selected = headers.next_header().unwrap().unwrap();
        assert!(headers.next_header().unwrap().is_none());
        assert_eq!(
            (empty.parent(), empty.ordinal(), empty.binding_count()),
            (1, 0, 0)
        );
        assert_eq!(
            (
                selected.parent(),
                selected.ordinal(),
                selected.binding_count()
            ),
            (2, 1, count as u32)
        );
        let mut empty_cursor = spool.bindings(&empty).unwrap();
        assert!(empty_cursor.next_binding().unwrap().is_none());
        assert!(empty_cursor.finish().unwrap().matches(&empty));
        drop(empty_cursor);
        drop(headers);
        for _ in 0..2 {
            let mut cursor = spool.bindings(&selected).unwrap();
            for index in 0..count {
                assert_eq!(
                    cursor.next_binding().unwrap(),
                    Some((name(index, false), child(index)))
                );
            }
            assert!(cursor.next_binding().unwrap().is_none());
            let done = cursor.finish().unwrap();
            assert!(done.matches(&selected));
            assert_eq!(done.wire_name_bytes(), 17 * count as u64);
        }
        for index in 0..count {
            assert_eq!(
                spool.binding_for(2, name(index, false).as_bytes()).unwrap(),
                child(index).map_or(BindingLookup::Absent, BindingLookup::Present),
            );
        }
        assert_eq!(
            spool.binding_for(2, b"a").unwrap(),
            BindingLookup::Unmentioned
        );
        assert_eq!(
            spool.binding_for(2, b"z").unwrap(),
            BindingLookup::Unmentioned
        );
        assert_eq!(
            spool.binding_for(3, b"f000000").unwrap(),
            BindingLookup::Unmentioned
        );
        spool.cleanup().unwrap();
        assert!(!path.path().exists());
        assert!(spool.directory_headers().is_err());
    }
}

#[test]
fn full_name255_lookup_has_logarithmic_probes_and_at_most_one16_record_block() {
    const COUNT: usize = 4_097;
    let (_path, spool) = create(COUNT, true);
    let groups = COUNT.div_ceil(16);
    let maximum_probes = u64::from(usize::BITS - groups.leading_zeros());
    for (query, index) in [COUNT - 1, 0, 2_047, 16, COUNT - 2, 1, 127, 128, 129]
        .into_iter()
        .enumerate()
    {
        let before = spool.read_work();
        let key = name(index, true);
        let found = if query % 2 == 0 {
            spool.binding_for(2, key.as_bytes())
        } else {
            spool.legacy_binding_lookup(2, key.as_bytes())
        }
        .unwrap();
        assert_eq!(
            found,
            child(index).map_or(BindingLookup::Absent, BindingLookup::Present),
        );
        let after = spool.read_work();
        let probes = after.checkpoint_probes - before.checkpoint_probes;
        let decoded = after.bindings_decoded - before.bindings_decoded;
        let slots = after.slot_reads - before.slot_reads;
        let checkpoints = after.checkpoint_reads - before.checkpoint_reads;
        let read_bytes = after.read_bytes - before.read_bytes;
        assert!(
            probes <= maximum_probes,
            "{index}: {probes}>{maximum_probes}"
        );
        assert!(
            decoded >= probes && decoded - probes <= 16,
            "{index}: {decoded} decoded/{probes} probes"
        );
        assert!(
            slots <= 6,
            "point search must not walk all directory slots: {slots}"
        );
        assert_eq!(
            read_bytes,
            48 + 32 * slots + 8 * checkpoints + 264 * decoded
        );
    }
    let header = spool.directory_header(2).unwrap().unwrap();
    let before = spool.read_work();
    let mut cursor = spool.bindings(&header).unwrap();
    let mut seen = 0;
    while let Some((key, binding)) = cursor.next_binding().unwrap() {
        assert_eq!((key, binding), (name(seen, true), child(seen)));
        seen += 1;
    }
    assert!(cursor.finish().unwrap().matches(&header));
    let after = spool.read_work();
    assert_eq!(seen, COUNT);
    assert_eq!(
        after.bindings_decoded - before.bindings_decoded,
        COUNT as u64
    );
    assert_eq!(
        after.checkpoint_reads - before.checkpoint_reads,
        ((COUNT - 1) / 16) as u64
    );
}

#[test]
fn foreign_headers_and_early_finish_cannot_complete_or_consume_another_source() {
    let (_first_path, first) = create(17, false);
    let (_second_path, second) = create(17, false);
    let selected = first.directory_header(2).unwrap().unwrap();
    let foreign = second.directory_header(2).unwrap().unwrap();
    assert!(matches!(
        first.bindings(&foreign),
        Err(ContentError::InvalidRecord("directory issuer"))
    ));
    let mut early = first.bindings(&selected).unwrap();
    assert_eq!(early.finish(), Err(ContentError::IncompleteOperation));
    assert_eq!(early.next_binding(), Err(ContentError::IncompleteOperation));
    let mut complete = first.bindings(&selected).unwrap();
    while complete.next_binding().unwrap().is_some() {}
    assert!(!complete.finish().unwrap().matches(&foreign));
}

#[test]
fn declaration_and_per_row_counts_refuse_before_native_effects() {
    let path = OwnedPath::new("admission");
    let declared = SpoolDeclaration {
        directories: 1,
        inodes: 0,
        fresh: 0,
        bindings: 17,
        wire_name_bytes: 17 * 17,
    };
    let required = declared.required_bytes_upper().unwrap();
    assert!(matches!(
        RowSpool::create_declared(path.path().to_path_buf(), declared, required - 1),
        Err(ContentError::ResourceUnavailable {
            what: "prepared row spool"
        }),
    ));
    assert!(!path.path().exists());
    let impossible = SpoolDeclaration {
        directories: 0,
        ..declared
    };
    assert!(RowSpool::create_declared(path.path().to_path_buf(), impossible, required).is_err());
    assert!(!path.path().exists());
    let mut spool =
        RowSpool::create_declared(path.path().to_path_buf(), declared, required).unwrap();
    let held = spool.held_bytes();
    assert!(spool.begin_directory(1, 18).is_err());
    assert_eq!(spool.held_bytes(), held);
    assert_eq!(std::fs::metadata(path.path()).unwrap().len(), held);
    assert!(!spool.completed_directory(1).unwrap());
    spool.begin_directory(1, 17).unwrap();
    spool.push_binding(&name(0, false), None).unwrap();
    assert!(!spool.completed_directory(1).unwrap());
    assert_eq!(spool.written_directories(), 0);
    assert!(spool.directory_headers().is_err());
    assert!(spool.seal().is_err());
}

#[test]
fn completed_rows_are_visible_before_global_seal_without_reading_names() {
    let path = OwnedPath::new("presence");
    let mut spool = RowSpool::create(path.path().to_path_buf(), 2, 0, 0, 4_096).unwrap();
    spool.begin_directory(1, 1).unwrap();
    spool.push_binding(&name(0, false), Some(2)).unwrap();
    spool.end_directory(1, 1, 17).unwrap();
    let before = spool.read_work();
    assert!(spool.completed_directory(1).unwrap());
    assert!(!spool.completed_directory(2).unwrap());
    assert_eq!(spool.read_work().bindings_decoded, before.bindings_decoded);
    assert!(spool.directory_header(1).is_err());
    spool.begin_directory(2, 0).unwrap();
    spool.end_directory(2, 0, 0).unwrap();
    spool.seal().unwrap();
    assert!(spool.directory_header(1).unwrap().is_some());
}

fn overwrite(path: &Path, at: u64, bytes: &[u8]) {
    let mut file = OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(at)).unwrap();
    file.write_all(bytes).unwrap();
}

#[test]
fn native_corrupt_slot_and_checkpoint_spans_fail_without_source_adoption() {
    let (path, spool) = create(17, false);
    // The second32B slot starts at80; its offset field starts at88.
    overwrite(path.path(), 88, &1_u64.to_be_bytes());
    assert!(matches!(
        spool.directory_header(2),
        Err(ContentError::InvalidRecord("row slot"))
    ));
    let (path, spool) = create(17, false);
    let selected = spool.directory_header(2).unwrap().unwrap();
    // Two slots follow the48B header. The single explicit checkpoint is first.
    overwrite(path.path(), 112, &u64::MAX.to_be_bytes());
    let mut cursor = spool.bindings(&selected).unwrap();
    for _ in 0..16 {
        cursor.next_binding().unwrap().unwrap();
    }
    let failure = cursor.next_binding().unwrap_err();
    assert_eq!(
        failure,
        ContentError::InvalidRecord("directory checkpoint span")
    );
    // Repairing native bytes cannot resurrect this failed cursor or completion.
    overwrite(path.path(), 112, &(16_u64 * 16).to_be_bytes());
    assert_eq!(cursor.next_binding(), Err(failure.clone()));
    assert_eq!(cursor.finish(), Err(failure));
}

#[test]
fn a_real_truncated_provider_preserves_the_cursor_failure_and_unfinished_file() {
    let (path, spool) = create(17, false);
    let selected = spool.directory_header(2).unwrap().unwrap();
    let mut cursor = spool.bindings(&selected).unwrap();
    let original = std::fs::metadata(path.path()).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(path.path())
        .unwrap()
        .set_len(original - 1)
        .unwrap();
    for _ in 0..16 {
        cursor.next_binding().unwrap().unwrap();
    }
    assert_eq!(cursor.next_binding(), Err(ContentError::UnexpectedEof));
    assert_eq!(cursor.finish(), Err(ContentError::UnexpectedEof));
    assert!(path.path().exists());
}

#[test]
fn independent_iterator_sources_get_exact_sticky_completion_without_forgeable_fields() {
    let authority = BindingAuthority::new().unwrap();
    let selected = authority.header(1, 0, 1, 17).unwrap();
    let input = vec![Ok((name(0, false), Some(2)))].into_iter();
    let mut cursor = authority.cursor(&selected, input).unwrap();
    assert_eq!(
        cursor.next_binding().unwrap(),
        Some((name(0, false), Some(2)))
    );
    assert!(cursor.next_binding().unwrap().is_none());
    assert!(cursor.finish().unwrap().matches(&selected));

    let input = vec![Ok((name(0, false), None)), Ok((name(1, false), Some(3)))].into_iter();
    let mut cursor = authority.cursor(&selected, input).unwrap();
    cursor.next_binding().unwrap();
    let extra = ContentError::InvalidRecord("directory binding count");
    assert_eq!(cursor.next_binding(), Err(extra.clone()));
    assert_eq!(cursor.next_binding(), Err(extra.clone()));
    assert_eq!(cursor.finish(), Err(extra));

    let absent = authority.header(1, 0, 2, 34).unwrap();
    let mut short = authority
        .cursor(&absent, std::iter::once(Ok((name(0, false), None))))
        .unwrap();
    short.next_binding().unwrap();
    assert_eq!(
        short.next_binding(),
        Err(ContentError::InvalidRecord("directory completion"))
    );
    let other = BindingAuthority::new().unwrap();
    assert!(other
        .cursor(
            &selected,
            std::iter::empty::<ContentResult<(PathName, Option<u64>)>>()
        )
        .is_err());
}

#[test]
fn a_real14000_fresh_shape_preserves_the_old_stream_plus1mib_admission() {
    const COUNT: usize = 14_000;
    let path = OwnedPath::new("old-admission");
    let declared = SpoolDeclaration {
        directories: 1,
        inodes: COUNT + 1,
        fresh: COUNT,
        bindings: COUNT as u64,
        wire_name_bytes: 17 * COUNT as u64,
    };
    // Independent wire arithmetic: one root patch25 and COUNT rooted rows73.
    let wire = 1 + 12 + declared.wire_name_bytes + 25 + 73 * COUNT as u64;
    let capacity = wire + 1_048_576;
    let old_overhead = 159 + 71 * COUNT as u64;
    assert!(old_overhead <= 1_048_576);
    assert!(old_overhead + 8 * COUNT as u64 > 1_048_576);
    let mut spool =
        RowSpool::create_declared(path.path().to_path_buf(), declared, capacity).unwrap();
    spool.begin_directory(1, COUNT as u32).unwrap();
    for index in 0..COUNT {
        spool
            .push_binding(&name(index, false), Some(index as u64 + 2))
            .unwrap();
    }
    spool
        .end_directory(1, COUNT as u32, declared.wire_name_bytes)
        .unwrap();
    let value = |kind| InodeValue {
        kind,
        namespace_ref_count: 0,
        content_root: ObjectId::for_bytes(b"owned-content"),
        metadata_root: ObjectId::for_bytes(b"owned-metadata"),
    };
    spool
        .push_inode(&layerfs_content::filesystem::InodeUpdate {
            serial: 1,
            value: value(InodeKind::Directory),
        })
        .unwrap();
    let before_late_directory = spool.held_bytes();
    assert_eq!(
        spool.begin_directory(3, 0),
        Err(ContentError::InvalidRecord("directory phase finished"))
    );
    assert_eq!(spool.held_bytes(), before_late_directory);
    for index in 0..COUNT {
        let serial = index as u64 + 2;
        spool
            .push_inode(&layerfs_content::filesystem::InodeUpdate {
                serial,
                value: value(InodeKind::RegularFile),
            })
            .unwrap();
        // Empty fresh payloads can interleave without changing inode offsets.
        spool.push_serial(serial).unwrap();
    }
    spool.seal().unwrap();
    assert!(spool.held_bytes() <= capacity);
    assert_eq!(
        spool.held_bytes(),
        std::fs::metadata(path.path()).unwrap().len()
    );
    let mut serials = spool.new_inodes().unwrap();
    for expected in 2..COUNT as u64 + 2 {
        assert_eq!(serials.next_row().unwrap(), Some(expected));
    }
    assert!(serials.next_row().unwrap().is_none());
    drop(serials);
    spool.cleanup().unwrap();
    assert!(!path.path().exists());
}

#[test]
fn compatibility_descriptors_use_point_selection_without_directory_prefix_rescans() {
    use layerfs_content::filesystem::{
        scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemResources, FilesystemRootId,
    };
    const DIRECTORIES: usize = 257;
    let directories: Vec<_> = (0..DIRECTORIES)
        .map(|index| DirectoryUpdate {
            parent: 10 + 17 * index as u64,
            changes: vec![(PathName::new("a").unwrap(), Some(10_000 + index as u64))],
        })
        .collect();
    let input = FilesystemInput {
        base: Some(FilesystemRootId(ObjectId::for_bytes(
            b"selected-legacy-base",
        ))),
        scope: scope_for_seed([0x45; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let observed = legacy_binding_observer::LegacyObserver::new(&input);
    let selected = CompatibilityBindingRows::new(&observed).unwrap();
    for directory in directories.iter().rev() {
        let header = selected
            .directory_header(directory.parent)
            .unwrap()
            .unwrap();
        assert_eq!(header.ordinal(), directory.parent);
    }
    assert_eq!(observed.point_calls.get(), DIRECTORIES);
    assert_eq!(
        observed.sequence_opens.get(),
        0,
        "point selection must not open a sequence"
    );

    let mut cursor = selected.directory_headers().unwrap();
    for directory in &directories {
        let sequential = cursor.next_header().unwrap().unwrap();
        let point = selected
            .directory_header(directory.parent)
            .unwrap()
            .unwrap();
        assert_eq!(
            sequential, point,
            "both selection paths must issue the same descriptor"
        );
        let mut names = selected.bindings(&sequential).unwrap();
        assert_eq!(
            names.next_binding().unwrap(),
            Some(directory.changes[0].clone())
        );
        assert!(names.next_binding().unwrap().is_none());
        assert!(names.finish().unwrap().matches(&point));
    }
    assert!(cursor.next_header().unwrap().is_none());
    assert_eq!(observed.sequence_opens.get(), 1);
    assert_eq!(observed.point_calls.get(), 3 * DIRECTORIES);
    // Opaque descriptors preserve logical sequence ordering/count checks.
    check_binding_input(&selected).unwrap();
    assert_eq!(observed.sequence_opens.get(), 2);
    assert_eq!(observed.point_calls.get(), 4 * DIRECTORIES);
}

#[test]
fn first_party_legacy_name_points_borrow_wide_rows_through_update_wrappers() {
    use layerfs_content::filesystem::rows::{
        PreparedBindingUpdate, PreparedUpdate, SliceBindingRows,
    };
    use layerfs_content::filesystem::{
        scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemResources, FilesystemRootId,
    };
    for count in [257, 1_024] {
        let directories: Vec<_> = [10, 20]
            .into_iter()
            .map(|parent| DirectoryUpdate {
                parent,
                changes: (0..count)
                    .map(|index| {
                        (
                            name(index, false),
                            child(index).map(|serial| serial + parent * 10_000),
                        )
                    })
                    .collect(),
            })
            .collect();
        let input = FilesystemInput {
            base: Some(FilesystemRootId(ObjectId::for_bytes(b"wide-legacy-base"))),
            scope: scope_for_seed([0x46; 32]),
            root_serial: 1,
            directories: &directories,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        };
        let observed = legacy_binding_observer::LegacyObserver::new(&input);
        let update = PreparedUpdate {
            base: input.base,
            scope: input.scope,
            root_serial: input.root_serial,
            resources: input.resources,
            rows: &observed,
        };
        let legacy = CompatibilityBindingRows::new(&update).unwrap();
        for _ in 0..3 {
            // Descending queries alternate parents; a last-row cache cannot
            // substitute for the producer's actual borrowed point capability.
            for index in (0..count).rev() {
                let parent = if index % 2 == 0 { 10 } else { 20 };
                let expected = child(index)
                    .map(|serial| serial + parent * 10_000)
                    .map_or(BindingLookup::Absent, BindingLookup::Present);
                assert_eq!(
                    legacy
                        .binding_for(parent, name(index, false).as_bytes())
                        .unwrap(),
                    expected
                );
            }
        }
        assert_eq!(observed.binding_queries.get(), 3 * count);
        assert_eq!(
            observed.point_calls.get(),
            0,
            "name points must not request full directory rows"
        );
        assert_eq!(
            observed.sequence_opens.get(),
            0,
            "name points must not open directory sequences"
        );
        assert_eq!(
            legacy.binding_for(10, b"z").unwrap(),
            BindingLookup::Unmentioned
        );

        // Direct delegation preserves the same cheap source through both
        // addressed update wrappers and the explicit borrowed slice adapter.
        let slice = SliceBindingRows::new(&input).unwrap();
        let bounded = PreparedBindingUpdate {
            base: input.base,
            scope: input.scope,
            root_serial: input.root_serial,
            resources: input.resources,
            rows: &slice,
        };
        let from_bound = CompatibilityBindingRows::new(&bounded).unwrap();
        assert_eq!(
            from_bound
                .binding_for(10, name(0, false).as_bytes())
                .unwrap(),
            BindingLookup::Absent
        );
        assert_eq!(
            from_bound.binding_for(20, b"z").unwrap(),
            BindingLookup::Unmentioned
        );
    }
}
