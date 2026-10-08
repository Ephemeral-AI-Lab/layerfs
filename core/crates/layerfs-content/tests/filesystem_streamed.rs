//! Stable generated directory streams exercise the shared public constructor.
mod support;

use layerfs_content::filesystem::rows::RowSource;
use layerfs_content::filesystem::{
    build_filesystem, build_filesystem_streamed, check_streamed_input, scope_for_seed,
    update_filesystem_streamed, DirectoryChangeLookup, DirectoryChangeSource, DirectoryHeader,
    DirectoryHeaderSource, DirectoryRowSource, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, FilesystemRootId, InodeRowSource, InodeUpdate, PathName, SerialRowSource,
    StreamedFilesystemInput, StreamedRowSource,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ContentResult};
use std::cell::Cell;
use support::filesystem::{name_of, synthetic, value, Session, TreeStore};

const NAMES: usize = 1_031;
fn generated_name(index: usize) -> PathName {
    name_of(&format!("f{index:05}"))
}
fn generated_value(serial: u64) -> InodeValue {
    value(
        if serial == 1 {
            InodeKind::Directory
        } else {
            InodeKind::RegularFile
        },
        synthetic("generated-content"),
        synthetic("generated-metadata"),
    )
}
#[derive(Clone, Copy)]
enum Fault {
    None,
    ShortCount,
    LongCount,
    Duplicate,
    Source,
    Point,
    HeaderPoint,
    HeaderCount,
    HeaderOrder,
    Serial,
}
struct Generated {
    fault: Fault,
    changes_read: Cell<usize>,
    changes_opened: Cell<usize>,
}
impl Generated {
    fn new(fault: Fault) -> Self {
        Self {
            fault,
            changes_read: Cell::new(0),
            changes_opened: Cell::new(0),
        }
    }
    fn header(&self) -> DirectoryHeader {
        DirectoryHeader {
            parent: 1,
            change_rows: match self.fault {
                Fault::ShortCount => NAMES as u64 - 1,
                Fault::LongCount => NAMES as u64 + 1,
                _ => NAMES as u64,
            },
        }
    }
}
struct Headers {
    header: Option<DirectoryHeader>,
    repeat: bool,
}
impl DirectoryHeaderSource for Headers {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        let header = self.header.take();
        if self.repeat {
            self.header = header;
            self.repeat = false;
        }
        Ok(header)
    }
}
struct GeneratedChanges<'a> {
    source: &'a Generated,
    at: usize,
}
impl DirectoryChangeSource for GeneratedChanges<'_> {
    fn next_row(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        if self.at == NAMES {
            return Ok(None);
        }
        let index = self.at;
        self.at += 1;
        self.source
            .changes_read
            .set(self.source.changes_read.get() + 1);
        if matches!(self.source.fault, Fault::Source) && index == 1 {
            return Err(ContentError::ProviderFailure {
                what: "original directory cursor",
            });
        }
        let name = if matches!(self.source.fault, Fault::Duplicate) && index == 1 {
            generated_name(0)
        } else {
            generated_name(index)
        };
        let serial = if matches!(self.source.fault, Fault::Serial) && index == 0 {
            0
        } else {
            index as u64 + 2
        };
        Ok(Some((name, Some(serial))))
    }
}
struct Values {
    at: u64,
}
impl InodeRowSource for Values {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        if self.at > NAMES as u64 + 1 {
            return Ok(None);
        }
        let serial = self.at;
        self.at += 1;
        Ok(Some(InodeUpdate {
            serial,
            value: generated_value(serial),
        }))
    }
}
struct Serials {
    at: u64,
}
impl SerialRowSource for Serials {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        if self.at > NAMES as u64 + 1 {
            return Ok(None);
        }
        let serial = self.at;
        self.at += 1;
        Ok(Some(serial))
    }
}
impl StreamedRowSource for Generated {
    fn directory_rows(&self) -> usize {
        if matches!(self.fault, Fault::HeaderCount | Fault::HeaderOrder) {
            2
        } else {
            1
        }
    }
    fn inode_rows(&self) -> usize {
        NAMES + 1
    }
    fn new_rows(&self) -> usize {
        NAMES + 1
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(Headers {
            header: Some(self.header()),
            repeat: matches!(self.fault, Fault::HeaderOrder),
        }))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        Ok((parent == 1).then(|| {
            let mut header = self.header();
            if matches!(self.fault, Fault::HeaderPoint) {
                header.change_rows += 1;
            }
            header
        }))
    }
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>> {
        assert_eq!(parent, 1);
        self.changes_opened.set(self.changes_opened.get() + 1);
        Ok(Box::new(GeneratedChanges {
            source: self,
            at: 0,
        }))
    }
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup> {
        if parent != 1 {
            return Ok(DirectoryChangeLookup::Unchanged);
        }
        if matches!(self.fault, Fault::Point) && name == &generated_name(0) {
            return Ok(DirectoryChangeLookup::Removed);
        }
        let text = std::str::from_utf8(name.as_bytes()).unwrap();
        let index = text
            .strip_prefix('f')
            .and_then(|number| number.parse::<usize>().ok());
        Ok(match index {
            Some(index) if index < NAMES => DirectoryChangeLookup::Bound(index as u64 + 2),
            _ => DirectoryChangeLookup::Unchanged,
        })
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        Ok(Box::new(Values { at: 1 }))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        Ok(Box::new(Serials { at: 1 }))
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        Ok((1..=NAMES as u64 + 1)
            .contains(&serial)
            .then(|| generated_value(serial)))
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        Ok((1..=NAMES as u64 + 1)
            .contains(&serial)
            .then(|| serial as usize - 1))
    }
}
// Deliberately unavailable old APIs: the new public route must not materialize
// a DirectoryUpdate or call a legacy cursor/point as an alternate source.
impl RowSource for Generated {
    fn directory_rows(&self) -> usize {
        panic!("legacy directory count")
    }
    fn inode_rows(&self) -> usize {
        panic!("legacy inode count")
    }
    fn new_rows(&self) -> usize {
        panic!("legacy fresh count")
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        panic!("legacy directories")
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        panic!("legacy inodes")
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        panic!("legacy new rows")
    }
    fn directory_for(&self, _: u64) -> ContentResult<Option<DirectoryUpdate>> {
        panic!("legacy directory clone")
    }
    fn value_for(&self, _: u64) -> ContentResult<Option<InodeValue>> {
        panic!("legacy value")
    }
    fn new_position(&self, _: u64) -> ContentResult<Option<usize>> {
        panic!("legacy fresh lookup")
    }
}
fn input(rows: &Generated) -> StreamedFilesystemInput<'_> {
    StreamedFilesystemInput {
        base: None,
        scope: scope_for_seed([27; 32]),
        root_serial: 1,
        resources: FilesystemResources::default(),
        rows,
    }
}

#[test]
fn generated_wide_stream_matches_the_existing_resident_canonical_root() {
    let rows = Generated::new(Fault::None);
    let reader = TreeStore::new();
    let mut streamed = TreeStore::new();
    let result = build_filesystem_streamed(
        &mut FilesystemObjects::new(&reader, &mut streamed),
        &input(&rows),
        None,
    )
    .unwrap();
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: (0..NAMES)
            .map(|index| (generated_name(index), Some(index as u64 + 2)))
            .collect(),
    }];
    let inodes = (1..=NAMES as u64 + 1)
        .map(|serial| InodeUpdate {
            serial,
            value: generated_value(serial),
        })
        .collect::<Vec<_>>();
    let new = (1..=NAMES as u64 + 1).collect::<Vec<_>>();
    let legacy = FilesystemInput {
        base: None,
        scope: input(&rows).scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources: FilesystemResources::default(),
    };
    let mut resident = TreeStore::new();
    let expected = build_filesystem(
        &mut FilesystemObjects::new(&reader, &mut resident),
        &legacy,
        None,
    )
    .unwrap();
    assert_eq!(result.root, expected.root);
    assert_eq!(result.value, expected.value);
    assert_eq!(result.counters.bindings_added, NAMES as u64);
    assert!(
        rows.changes_opened.get() > 1,
        "stable input is replayed, never replaced"
    );
    let mut read =
        layerfs_content::filesystem::FilesystemRead::new(&streamed, result.root).unwrap();
    assert_eq!(
        read.resolve_child(1, &generated_name(NAMES - 1))
            .unwrap()
            .serial,
        NAMES as u64 + 1
    );
}

#[test]
fn original_cursor_failure_stops_before_future_rows_objects_or_replay() {
    let rows = Generated::new(Fault::Source);
    let reader = TreeStore::new();
    let mut sink = TreeStore::new();
    let result = build_filesystem_streamed(
        &mut FilesystemObjects::new(&reader, &mut sink),
        &input(&rows),
        None,
    );
    assert_eq!(
        result,
        Err(ContentError::ProviderFailure {
            what: "original directory cursor"
        })
    );
    assert_eq!(rows.changes_read.get(), 2);
    assert_eq!(rows.changes_opened.get(), 1);
    assert_eq!(sink.len(), 0);
}

#[test]
fn counts_order_and_point_disagreement_refuse_before_final_output() {
    for (fault, expected) in [
        (
            Fault::ShortCount,
            ContentError::InvalidRecord("directory change row count"),
        ),
        (
            Fault::LongCount,
            ContentError::InvalidRecord("directory change row count"),
        ),
        (Fault::Duplicate, ContentError::NonCanonicalOrdering),
        (
            Fault::Point,
            ContentError::InvalidRecord("directory change lookup"),
        ),
        (
            Fault::HeaderPoint,
            ContentError::InvalidRecord("directory header lookup"),
        ),
        (
            Fault::HeaderCount,
            ContentError::InvalidRecord("directory row count"),
        ),
        (Fault::HeaderOrder, ContentError::NonCanonicalOrdering),
        (Fault::Serial, ContentError::InvalidRecord("inode serial")),
    ] {
        let rows = Generated::new(fault);
        assert_eq!(check_streamed_input(&input(&rows)), Err(expected.clone()));
        let reader = TreeStore::new();
        let mut sink = TreeStore::new();
        assert_eq!(
            build_filesystem_streamed(
                &mut FilesystemObjects::new(&reader, &mut sink),
                &input(&rows),
                None
            ),
            Err(expected)
        );
        assert_eq!(sink.len(), 0);
    }
}

struct BorrowedHeaders<'a> {
    rows: std::slice::Iter<'a, DirectoryUpdate>,
}
impl DirectoryHeaderSource for BorrowedHeaders<'_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        Ok(self.rows.next().map(|row| DirectoryHeader {
            parent: row.parent,
            change_rows: row.changes.len() as u64,
        }))
    }
}
struct BorrowedChanges<'a> {
    rows: std::slice::Iter<'a, (PathName, Option<u64>)>,
}
impl DirectoryChangeSource for BorrowedChanges<'_> {
    fn next_row(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        Ok(self.rows.next().cloned())
    }
}
struct BorrowedStreams<'a> {
    input: &'a FilesystemInput<'a>,
    points: Cell<usize>,
}
impl StreamedRowSource for BorrowedStreams<'_> {
    fn directory_rows(&self) -> usize {
        self.input.directories.len()
    }
    fn inode_rows(&self) -> usize {
        self.input.inodes.len()
    }
    fn new_rows(&self) -> usize {
        self.input.new_inodes.len()
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(BorrowedHeaders {
            rows: self.input.directories.iter(),
        }))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        Ok(self
            .input
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .map(|row| DirectoryHeader {
                parent,
                change_rows: row.changes.len() as u64,
            }))
    }
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>> {
        let row = self
            .input
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .expect("declared header");
        Ok(Box::new(BorrowedChanges {
            rows: row.changes.iter(),
        }))
    }
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup> {
        self.points.set(self.points.get() + 1);
        Ok(self
            .input
            .directories
            .iter()
            .find(|row| row.parent == parent)
            .and_then(|row| row.changes.iter().find(|(key, _)| key == name))
            .map_or(DirectoryChangeLookup::Unchanged, |(_, binding)| {
                binding.map_or(DirectoryChangeLookup::Removed, DirectoryChangeLookup::Bound)
            }))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
}
fn update_streamed(
    session: &Session,
    directories: &[DirectoryUpdate],
) -> ContentResult<layerfs_content::filesystem::FilesystemResult> {
    let legacy = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: session.root_serial,
        directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let rows = BorrowedStreams {
        input: &legacy,
        points: Cell::new(0),
    };
    let prepared = StreamedFilesystemInput {
        base: legacy.base,
        scope: legacy.scope,
        root_serial: legacy.root_serial,
        resources: legacy.resources,
        rows: &rows,
    };
    let mut sink = TreeStore::new();
    let result = update_filesystem_streamed(
        &mut FilesystemObjects::new(&session.store, &mut sink),
        &prepared,
        None,
    );
    if directories.iter().any(|row| !row.changes.is_empty()) {
        assert!(
            rows.points.get() > 0,
            "validation consumes exact changed-name points"
        );
    }
    result
}
fn directory_base() -> Session {
    let mut session = Session::new(1).unwrap();
    let directory = session.allocate();
    let changes = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name_of("directory"), Some(directory))],
        },
        DirectoryUpdate {
            parent: directory,
            changes: Vec::new(),
        },
    ];
    let values = [InodeUpdate {
        serial: directory,
        value: generated_value(1),
    }];
    session.apply(&changes, &values, &[directory]).unwrap();
    session
}

#[test]
fn removed_and_unchanged_points_distinguish_a_move_from_a_second_parent() {
    let mut session = directory_base();
    let moved = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("directory"), None), (name_of("moved"), Some(2))],
    }];
    let streamed = update_streamed(&session, &moved).unwrap();
    let resident = session.apply(&moved, &[], &[]).unwrap();
    assert_eq!(streamed.root, resident.root);
    let second = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name_of("second"), Some(2))],
    }];
    assert_eq!(
        update_streamed(&session, &second),
        Err(ContentError::InvalidRecord("multiple parents"))
    );
}

#[test]
fn paged_effective_names_match_the_resident_move_with_removals_and_insertions() {
    let mut session = directory_base();
    let serials = (0..131).map(|_| session.allocate()).collect::<Vec<_>>();
    let holder = session.allocate();
    let children = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name_of("holder"), Some(holder))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: serials
                .iter()
                .enumerate()
                .map(|(index, serial)| (generated_name(index), Some(*serial)))
                .collect(),
        },
        DirectoryUpdate {
            parent: holder,
            changes: Vec::new(),
        },
    ];
    let mut values = serials
        .iter()
        .map(|serial| InodeUpdate {
            serial: *serial,
            value: generated_value(*serial),
        })
        .collect::<Vec<_>>();
    values.push(InodeUpdate {
        serial: holder,
        value: generated_value(1),
    });
    let mut new = serials.clone();
    new.push(holder);
    session.apply(&children, &values, &new).unwrap();
    // The directory moves under a stored directory, so its effective names are
    // paged once by the territory walk; a move within the root lists nothing.
    let changes = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name_of("directory"), None)],
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![
                (generated_name(0), None),
                (generated_name(65), None),
                (name_of("g00000"), Some(serials[0])),
                (name_of("g00065"), Some(serials[65])),
            ],
        },
        DirectoryUpdate {
            parent: holder,
            changes: vec![(name_of("moved"), Some(2))],
        },
    ];
    let streamed = update_streamed(&session, &changes).unwrap();
    let resident = session.apply(&changes, &[], &[]).unwrap();
    assert_eq!(streamed.root, resident.root);
    assert_eq!(streamed.value, resident.value);
    let validation = streamed.counters.validation;
    // Three listing pages of the moved directory's 131 base names.
    assert!(validation.directory_pages_read >= 3);
    // Two names removed and two inserted leave 131 effective names, all files.
    assert_eq!(
        (
            validation.territory_directories,
            validation.territory_entries,
            validation.entries_examined,
        ),
        (1, 131, 131),
        "the walk covers the moved directory and nothing else: {validation:?}"
    );
    assert_eq!(validation, resident.counters.validation);
}

#[test]
fn the_same_effective_cycle_validator_rejects_a_streamed_self_move() {
    let session = directory_base();
    let cycle = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name_of("directory"), None)],
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![(name_of("inside"), Some(2))],
        },
    ];
    assert_eq!(
        update_streamed(&session, &cycle),
        Err(ContentError::InvalidRecord("effective tree cycle"))
    );
}

#[test]
fn a_present_empty_update_retains_the_original_directory_and_root() {
    let session = directory_base();
    let empty = [DirectoryUpdate {
        parent: 2,
        changes: Vec::new(),
    }];
    assert_eq!(
        update_streamed(&session, &empty).unwrap().root.0,
        session.root
    );
}
