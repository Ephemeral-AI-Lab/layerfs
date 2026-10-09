//! Reader-bound parent cursors, name points and symlink targets through the
//! public engine API: sealed rows only, with paired plan and runtime evidence.
//! No private source, fault hook, timing sample or cold-cache claim.
mod payload_support;
use layerfs_overlay::*;
use payload_support::Temp;
use std::{collections::BTreeMap, ops::Bound};

/// The one fact a consistent caller supplies for a name: whether the immutable
/// base binds it. Fixed per key, so the model needs no generation history.
fn in_base(parent: u64, name: &[u8]) -> bool {
    (name.iter().map(|byte| u64::from(*byte)).sum::<u64>() + parent) % 3 == 0
}
fn node(serial: u64, kind: InodeKind, size: u64, born: u64) -> Inode {
    Inode {
        serial,
        kind,
        mode: match kind {
            InodeKind::File => 0o644,
            InodeKind::Directory => 0o755,
            InodeKind::Symlink => 0o777,
        },
        mtime_seconds: 11,
        mtime_nanoseconds: 13,
        nlink: 1,
        size,
        inherited_cutoff: 0,
        born,
        entries: 0,
        subdirs: 0,
    }
}
/// The one payload cell Workspace writes for a fresh symlink or a small file.
fn bytes_cell(bytes: &[u8]) -> Cell {
    let mut data = Box::new([0_u8; CELL_BYTES]);
    data[..bytes.len()].copy_from_slice(bytes);
    let mut validity = Box::new([0_u8; MASK_BYTES]);
    for bit in 0..bytes.len() {
        validity[bit / 8] |= 1 << (bit % 8);
    }
    Cell {
        offset: 0,
        data,
        validity,
    }
}
/// Independent model beside real compound jobs: the final value of every
/// touched name and inode. A name row exists when it is bound or the base binds
/// it (a whiteout); a removal over nothing leaves no row.
struct World<'a> {
    db: &'a Overlay,
    source: BaseSource,
    names: BTreeMap<(u64, Vec<u8>), Option<u64>>,
    inodes: BTreeMap<u64, Inode>,
}
impl<'a> World<'a> {
    fn new(db: &'a Overlay, source: BaseSource) -> Self {
        Self {
            db,
            source,
            names: BTreeMap::new(),
            inodes: BTreeMap::new(),
        }
    }
    fn born(&self) -> u64 {
        self.db.source_rows(self.source).unwrap().active().number() as u64
    }
    fn apply(&self, changes: Changes) {
        let publication = self.db.apply(self.source, &changes).unwrap();
        self.db.reply_attempted(publication).unwrap();
    }
    fn name(&mut self, parent: u64, name: &[u8], serial: Option<u64>) {
        let inherited = in_base(parent, name);
        self.apply(Changes {
            directory_entries: vec![DirectoryEntryChange {
                parent,
                name: name.to_vec(),
                binding: match serial {
                    Some(serial) => Binding::Bound { serial, inherited },
                    None => Binding::Removed { inherited },
                },
            }],
            ..Changes::default()
        });
        self.names.insert((parent, name.to_vec()), serial);
    }
    fn inode(&mut self, inode: Inode, cell: Option<Cell>) {
        self.apply(Changes {
            detached: (inode.nlink == 0).then_some(inode.serial),
            inodes: vec![inode.clone()],
            cell: cell.map(|cell| (inode.serial, cell)),
            ..Changes::default()
        });
        self.inodes.insert(inode.serial, inode);
    }
    fn symlink(&mut self, serial: u64, target: &[u8]) {
        let inode = node(serial, InodeKind::Symlink, target.len() as u64, self.born());
        self.inode(inode, Some(bytes_cell(target)));
    }
    fn rows(&self) -> Vec<DirectoryEntry> {
        self.names
            .iter()
            .filter(|((parent, name), serial)| serial.is_some() || in_base(*parent, name))
            .map(|((parent, name), serial)| DirectoryEntry {
                inherited: in_base(*parent, name),
                parent: *parent,
                name: name.clone(),
                serial: *serial,
            })
            .collect()
    }
    fn parent(&self, parent: u64) -> Vec<DirectoryEntry> {
        let mut rows = self.rows();
        rows.retain(|row| row.parent == parent);
        rows
    }
    fn parents(&self) -> Vec<u64> {
        let mut parents: Vec<u64> = self.rows().iter().map(|row| row.parent).collect();
        parents.dedup();
        parents
    }
}
/// One parent's whole sealed sequence and its count of non-empty windows.
fn parent_rows(db: &Overlay, reader: CapturedReader, parent: u64) -> (Vec<DirectoryEntry>, usize) {
    let mut after: Option<Vec<u8>> = None;
    let (mut rows, mut windows) = (Vec::new(), 0);
    for _ in 0..1024 {
        let page = db
            .reader_parent_directory_entries(reader, parent, after.as_deref())
            .unwrap();
        assert!(page.len() <= PAGE_ROWS);
        if page.is_empty() {
            return (rows, windows);
        }
        for row in &page {
            assert_eq!(row.parent, parent, "a row of another parent");
            assert!(after.as_deref().is_none_or(|old| old < row.name.as_slice()));
            after = Some(row.name.clone());
        }
        windows += 1;
        rows.extend(page);
    }
    panic!("parent cursor did not end");
}
fn all_rows(db: &Overlay, reader: CapturedReader) -> Vec<DirectoryEntry> {
    let mut after: Option<(u64, Vec<u8>)> = None;
    let mut rows = Vec::new();
    for _ in 0..1024 {
        let page = db
            .reader_directory_entries(
                reader,
                after
                    .as_ref()
                    .map(|(parent, name)| (*parent, name.as_slice())),
            )
            .unwrap();
        let Some(last) = page.last() else {
            return rows;
        };
        after = Some((last.parent, last.name.clone()));
        rows.extend(page);
    }
    panic!("all-entries page did not end");
}
/// R4-5, reader side, against the model: each parent's cursor returns exactly
/// its rows, their concatenation is the existing page sequence, and every row
/// is answered identically by the name point.
fn check_names(db: &Overlay, reader: CapturedReader, world: &World<'_>) -> Vec<DirectoryEntry> {
    let expected = world.rows();
    let mut joined = Vec::new();
    for parent in world.parents() {
        let (rows, _) = parent_rows(db, reader, parent);
        assert_eq!(rows, world.parent(parent), "parent {parent}");
        joined.extend(rows);
    }
    assert_eq!(joined, expected);
    assert_eq!(all_rows(db, reader), expected);
    for row in &expected {
        assert_eq!(
            db.reader_directory_entry(reader, row.parent, &row.name)
                .unwrap()
                .as_ref(),
            Some(row)
        );
    }
    for ((parent, name), serial) in &world.names {
        if serial.is_none() && !in_base(*parent, name) {
            assert_eq!(
                db.reader_directory_entry(reader, *parent, name).unwrap(),
                None
            );
        }
    }
    expected
}
/// Every page row is answered identically by the inode point, and inside
/// `universe` the point answers for exactly the page's serials.
fn page_and_point(
    db: &Overlay,
    reader: CapturedReader,
    universe: std::ops::RangeInclusive<u64>,
) -> Vec<Inode> {
    let mut after = 0;
    let mut rows = Vec::new();
    for _ in 0..1024 {
        let page = db.reader_inodes(reader, after).unwrap();
        assert!(page.len() <= PAGE_ROWS);
        if page.is_empty() {
            break;
        }
        for row in &page {
            assert!(row.serial > after);
            after = row.serial;
            assert_eq!(
                db.reader_inode(reader, row.serial).unwrap().as_ref(),
                Some(row)
            );
        }
        rows.extend(page);
    }
    assert!(db.reader_inodes(reader, after).unwrap().is_empty());
    let mut paged = rows.iter().peekable();
    for serial in universe {
        let row = paged.next_if(|row| row.serial == serial);
        assert_eq!(
            db.reader_inode(reader, serial).unwrap().as_ref(),
            row,
            "serial {serial}"
        );
    }
    assert!(paged.next().is_none(), "page row outside the universe");
    rows
}
/// Evidence lines are written to the process output directly, so the receipt
/// of the one bounded run holds them without a second, uncaptured run.
fn evidence(line: std::fmt::Arguments<'_>) {
    use std::io::Write;
    writeln!(std::io::stdout().lock(), "{line}").unwrap();
}
fn drain(db: &Overlay) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..20_000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return;
        };
        cursor = step.cursor;
    }
    panic!("maintenance did not settle");
}
/// Every answer of the three reader jobs over fixed probes, for equality
/// across later activity, install, failure and close.
#[derive(Debug, PartialEq)]
struct Answers {
    parents: Vec<Vec<DirectoryEntry>>,
    points: Vec<Option<DirectoryEntry>>,
    links: Vec<Result<Vec<u8>, String>>,
}
fn answers(
    db: &Overlay,
    reader: CapturedReader,
    parents: &[u64],
    points: &[(u64, Vec<u8>)],
    links: &[u64],
) -> Answers {
    Answers {
        parents: parents
            .iter()
            .map(|parent| parent_rows(db, reader, *parent).0)
            .collect(),
        points: points
            .iter()
            .map(|(parent, name)| db.reader_directory_entry(reader, *parent, name).unwrap())
            .collect(),
        links: links
            .iter()
            .map(|serial| {
                db.reader_symlink(reader, *serial)
                    .map_err(|error| format!("{error:?}"))
            })
            .collect(),
    }
}

#[test]
fn parent_cursor_and_name_point_answer_exactly_the_sealed_rows_of_each_parent() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([201; 32], [202; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut world = World::new(&db, source);
    // Parent 10 spans more than two full windows. Parents 9 and 11 hold names
    // sorting below, between and above them, so a parent-crossing cursor shows.
    for n in 0..200_u64 {
        world.name(10, format!("n{n:04}").as_bytes(), Some(1000 + n));
    }
    for name in [&b"a"[..], b"n0100", b"n0100x", b"zz", b"\x00", b"\xff\xff"] {
        world.name(9, name, Some(20));
        world.name(11, name, Some(21));
    }
    // Removals: a whiteout where the base binds the name, no row otherwise.
    for n in (0..200_u64).step_by(7) {
        world.name(10, format!("n{n:04}").as_bytes(), None);
    }
    world.name(10, b"never-bound-a", None);
    world.name(10, b"never-bound-b", None);
    world.name(10, b"never-bound-c", None);
    // Parent 12: names of exactly 255 bytes sharing a 253-byte prefix, with
    // that prefix and its 254-byte extension as shorter neighbours.
    let prefix = vec![b'x'; 253];
    let long = |tail: [u8; 2]| [prefix.as_slice(), tail.as_slice()].concat();
    for tail in 0..70_u16 {
        let name = long([(tail * 37 % 256) as u8, (tail * 101 % 256) as u8]);
        world.name(12, &name, Some(2000 + u64::from(tail)));
    }
    world.name(12, &long([0xff, 0xff]), Some(2100));
    world.name(12, &prefix, Some(2101));
    world.name(12, &[prefix.as_slice(), b"\x00"].concat(), Some(2102));
    for tail in [3_u16, 4, 5, 6, 7, 8] {
        world.name(
            12,
            &long([(tail * 37 % 256) as u8, (tail * 101 % 256) as u8]),
            None,
        );
    }
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();

    let expected = check_names(&db, reader, &world);
    assert_eq!(world.parents(), [9, 10, 11, 12]);
    let (ten, windows) = parent_rows(&db, reader, 10);
    assert!(ten.len() > 2 * PAGE_ROWS && windows > 2, "{windows}");
    for parent in [10, 12] {
        let rows = world.parent(parent);
        let whiteouts = rows.iter().filter(|row| row.serial.is_none()).count();
        assert!(
            whiteouts > 0 && whiteouts < rows.len(),
            "{parent}: {whiteouts}"
        );
    }
    // Removed names the base does not bind left no row: the point says so.
    let dropped = world
        .names
        .iter()
        .filter(|((parent, name), serial)| serial.is_none() && !in_base(*parent, name))
        .count();
    assert!(dropped > 0 && expected.len() + dropped == world.names.len());
    assert!(world
        .parent(12)
        .iter()
        .all(|row| row.name.len() >= 253 && row.name.starts_with(&prefix)));
    assert!(
        world
            .parent(12)
            .iter()
            .filter(|row| row.name.len() == 255)
            .count()
            > PAGE_ROWS
    );

    // Parents with no captured row end at once, also between and beyond.
    for parent in [0, 1, 8, 13, i64::MAX as u64] {
        assert_eq!(parent_rows(&db, reader, parent), (Vec::new(), 0));
    }
    // A cursor need not be a row: the window starts strictly after it.
    let window = |parent: u64, after: &[u8]| {
        let rows = db
            .reader_parent_directory_entries(reader, parent, Some(after))
            .unwrap();
        let want: Vec<DirectoryEntry> = world
            .names
            .range((
                Bound::Excluded((parent, after.to_vec())),
                Bound::Excluded((parent + 1, Vec::new())),
            ))
            .filter(|((parent, name), serial)| serial.is_some() || in_base(*parent, name))
            .take(PAGE_ROWS)
            .map(|((parent, name), serial)| DirectoryEntry {
                inherited: in_base(*parent, name),
                parent: *parent,
                name: name.clone(),
                serial: *serial,
            })
            .collect();
        assert_eq!(rows, want, "{parent} after {after:?}");
        rows
    };
    assert_eq!(window(10, b"n0100")[0].name, b"n0101");
    assert_eq!(window(10, b"n0100\x00")[0].name, b"n0101");
    assert_eq!(window(10, b"")[0].name, ten[0].name);
    assert!(window(10, &ten[ten.len() - 1].name).is_empty());
    assert!(window(10, &[0xff; 255]).is_empty());
    assert!(window(9, b"zz").len() == 1 && window(11, b"\xff\xff").is_empty());
    assert_eq!(window(12, &prefix[..252])[0].name, prefix);
    assert_eq!(window(12, &prefix)[0].name.len(), 254);
    assert_eq!(
        window(12, &long([0xff, 0xfe])),
        [expected[expected.len() - 1].clone()]
    );
    assert!(window(12, &long([0xff, 0xff])).is_empty());

    // Names the capture does not hold, each one byte away from a held one.
    assert!(world.names.contains_key(&(12, long([0, 0]))));
    for (parent, name) in [
        (10, b"n0200".to_vec()),
        (10, b"n010".to_vec()),
        (10, b"n0100x".to_vec()),
        (10, b"a".to_vec()),
        (8, b"a".to_vec()),
        (13, b"n0100".to_vec()),
        (10, Vec::new()),
        (12, long([0, 1])),
        (12, prefix[..252].to_vec()),
        (12, [prefix.as_slice(), b"\x01"].concat()),
        (12, long([b'x', b'y'])),
    ] {
        assert!(!world.names.contains_key(&(parent, name.clone())));
        assert_eq!(
            db.reader_directory_entry(reader, parent, &name).unwrap(),
            None,
            "{parent} {name:?}"
        );
    }
    // Over-long input is refused before any name statement, like the page.
    let captured = db.diagnostics().statements[StatementKind::Capture as usize];
    assert!(matches!(
        db.reader_parent_directory_entries(reader, 10, Some(&[b'x'; 256])),
        Err(OverlayError::Invalid("captured name cursor"))
    ));
    assert!(matches!(
        db.reader_directory_entries(reader, Some((10, &[b'x'; 256][..]))),
        Err(OverlayError::Invalid("captured name cursor"))
    ));
    assert!(matches!(
        db.reader_directory_entry(reader, 12, &[b'x'; 256]),
        Err(OverlayError::Invalid("captured name"))
    ));
    assert!(matches!(
        db.reader_parent_directory_entries(reader, u64::MAX, None),
        Err(OverlayError::Invalid(_))
    ));
    assert_eq!(
        db.diagnostics().statements[StatementKind::Capture as usize].attempts,
        captured.attempts
    );
    assert_eq!(db.retained_captured_reader(route, 1).unwrap(), Some(reader));
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn later_active_rows_install_and_close_leave_reader_answers_until_exact_release() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([203; 32], [204; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut world = World::new(&db, source);
    for n in 0..150_u64 {
        world.name(10, format!("n{n:04}").as_bytes(), Some(1000 + n));
    }
    for n in (0..150_u64).step_by(5) {
        world.name(10, format!("n{n:04}").as_bytes(), None);
    }
    world.symlink(50, b"sealed/target");
    world.symlink(52, b"removed/later");
    world.name(11, b"link", Some(50));
    world.name(11, b"gone", Some(52));
    world.name(11, b"file", Some(60));
    world.inode(
        node(60, InodeKind::File, 3, world.born()),
        Some(bytes_cell(b"abc")),
    );
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let sealed_names = check_names(&db, reader, &world);

    let parents = [9, 10, 11, 12];
    let mut points: Vec<(u64, Vec<u8>)> = world.names.keys().cloned().collect();
    points.extend([
        (10, b"n0001a".to_vec()),
        (11, b"link2".to_vec()),
        (12, b"fresh".to_vec()),
        (9, b"n0000".to_vec()),
    ]);
    let links = [50, 51, 52, 60, 61];
    let sealed = answers(&db, reader, &parents, &points, &links);
    assert_eq!(sealed.links[0], Ok(b"sealed/target".to_vec()));
    assert_eq!(sealed.links[2], Ok(b"removed/later".to_vec()));
    assert!(sealed.links[1].is_err() && sealed.links[3].is_err() && sealed.links[4].is_err());
    assert!(sealed.parents[0].is_empty() && sealed.parents[3].is_empty());
    assert!(sealed.points[points.len() - 4..]
        .iter()
        .all(Option::is_none));

    // Later active generation: added, changed and removed names interleaved
    // with the sealed ones, a changed and a removed symlink, and a fresh one.
    let source = db.acquire_base_source(route, 2).unwrap();
    let mut active = World::new(&db, source);
    for n in 0..150_u64 {
        active.name(10, format!("n{n:04}a").as_bytes(), Some(3000 + n));
    }
    for n in (1..150_u64).step_by(3) {
        active.name(10, format!("n{n:04}").as_bytes(), None);
    }
    for n in (0..150_u64).step_by(10) {
        active.name(10, format!("n{n:04}").as_bytes(), Some(4000 + n));
    }
    active.name(9, b"n0000", Some(4500));
    active.name(12, b"fresh", Some(4501));
    let mut changed = node(
        50,
        InodeKind::Symlink,
        13,
        capture.generation.number() as u64,
    );
    changed.mtime_seconds = 77;
    changed.inherited_cutoff = 13;
    active.inode(changed, None);
    let mut removed = node(
        52,
        InodeKind::Symlink,
        13,
        capture.generation.number() as u64,
    );
    removed.nlink = 0;
    active.inode(removed, None);
    active.name(11, b"gone", None);
    active.symlink(51, b"active/only");
    active.name(11, b"link2", Some(51));
    active.symlink(61, b"active/too");
    // The active view did change under the very keys the reader is asked for.
    assert_eq!(
        db.directory_entry(route, 10, b"n0001")
            .unwrap()
            .unwrap()
            .serial,
        None
    );
    assert_eq!(
        db.directory_entry(route, 10, b"n0010")
            .unwrap()
            .unwrap()
            .serial,
        Some(4010)
    );
    assert!(db.directory_entry(route, 12, b"fresh").unwrap().is_some());
    assert_eq!(db.inode(route, 52).unwrap().unwrap().nlink, 0);
    assert_eq!(db.inode(route, 50).unwrap().unwrap().mtime_seconds, 77);
    assert_eq!(answers(&db, reader, &parents, &points, &links), sealed);
    assert_eq!(check_names(&db, reader, &world), sealed_names);

    // R4-8, reader side: known install, retirement turns, later activity and
    // terminal close leave every answer; exact release makes each job Stale.
    db.release_base_source(source).unwrap();
    db.install(capture, [205; 32]).unwrap();
    assert_eq!(answers(&db, reader, &parents, &points, &links), sealed);
    drain(&db);
    assert_eq!(answers(&db, reader, &parents, &points, &links), sealed);
    let source = db.acquire_base_source(route, 3).unwrap();
    let mut after_install = World::new(&db, source);
    after_install.name(10, b"n0002", Some(5000));
    after_install.name(11, b"link", None);
    after_install.symlink(62, b"after/install");
    db.release_base_source(source).unwrap();
    drain(&db);
    assert_eq!(answers(&db, reader, &parents, &points, &links), sealed);
    db.close(route).unwrap();
    drain(&db);
    assert_eq!(answers(&db, reader, &parents, &points, &links), sealed);
    assert_eq!(check_names(&db, reader, &world), sealed_names);
    assert_eq!(reader.root(), [204; 32]);
    assert!(db
        .explain_reader_directory_entries(reader, 10, b"n0001")
        .is_ok());
    db.release_captured_reader(reader).unwrap();
    assert!(matches!(
        db.reader_parent_directory_entries(reader, 10, None),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.reader_directory_entry(reader, 10, b"n0001"),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.reader_symlink(reader, 50),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.explain_reader_directory_entries(reader, 10, b"n0001"),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn inode_page_and_point_agree_and_a_folded_failed_capture_is_read_by_the_next_reader() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([206; 32], [207; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut world = World::new(&db, source);
    let first = world.born();
    // More than two inode windows: base files with a metadata row, fresh
    // files, removed ones, directories and symlinks.
    for serial in 1..=150_u64 {
        let mut inode = node(serial, InodeKind::File, 0, (serial % 2) * first);
        inode.nlink = u64::from(serial % 9 != 0);
        world.inode(inode, None);
    }
    world.inode(
        node(40, InodeKind::File, 4, first),
        Some(bytes_cell(b"open")),
    );
    for serial in 200..=203_u64 {
        let mut inode = node(serial, InodeKind::Directory, 0, 0);
        inode.entries = serial - 199;
        world.inode(inode, None);
    }
    for serial in 300..=303_u64 {
        world.symlink(serial, format!("target-{serial}").as_bytes());
    }
    for n in 0..140_u64 {
        world.name(200, format!("e{n:03}").as_bytes(), Some(1 + n));
    }
    for n in (0..140_u64).step_by(6) {
        world.name(200, format!("e{n:03}").as_bytes(), None);
    }
    for serial in 300..=303_u64 {
        world.name(201, format!("l{serial}").as_bytes(), Some(serial));
    }
    let open = db.open_file(source, 7, &world.inodes[&40], true).unwrap();
    db.release_base_source(source).unwrap();
    let failed = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(failed, 1).unwrap();
    let sealed = page_and_point(&db, reader, 1..=400);
    assert!(sealed.len() > 2 * PAGE_ROWS);
    assert_eq!(sealed, world.inodes.values().cloned().collect::<Vec<_>>());
    let sealed_names = check_names(&db, reader, &world);
    let sealed_world = World {
        db: &db,
        source,
        names: world.names.clone(),
        inodes: world.inodes.clone(),
    };

    // The next generation changes, removes and adds inodes and names, some
    // over sealed rows and some beside them; most sealed rows stay untouched.
    let source = db.acquire_base_source(route, 2).unwrap();
    world.source = source;
    let second = world.born();
    assert!(second > first);
    for serial in [1_u64, 2, 3, 4, 5] {
        let mut inode = world.inodes[&serial].clone();
        inode.mtime_seconds = 500 + serial as i64;
        world.inode(inode, None);
    }
    for serial in [11_u64, 12, 13, 40, 301] {
        let mut inode = world.inodes[&serial].clone();
        inode.nlink = 0;
        world.inode(inode, None);
    }
    for serial in 350..=360_u64 {
        world.inode(node(serial, InodeKind::File, 0, second), None);
    }
    let mut touched = world.inodes[&300].clone();
    touched.mtime_seconds = 600;
    world.inode(touched, None);
    world.symlink(304, b"second-generation");
    let mut directory = world.inodes[&200].clone();
    directory.entries += 9;
    world.inode(directory, None);
    for n in (0..140_u64).step_by(4) {
        world.name(200, format!("e{n:03}").as_bytes(), Some(350 + n % 11));
    }
    for n in (1..140_u64).step_by(4) {
        world.name(200, format!("e{n:03}").as_bytes(), None);
    }
    // Removed again over a sealed whiteout or over nothing.
    for n in [6_u64, 18, 30, 42] {
        world.name(200, format!("e{n:03}").as_bytes(), None);
    }
    for n in 0..70_u64 {
        world.name(202, format!("f{n:03}").as_bytes(), Some(350 + n % 11));
    }
    world.name(201, b"l301", None);
    world.name(201, b"l304", Some(304));
    db.release_base_source(source).unwrap();

    // Definite failure while the reader is held: composition parks and the
    // reader keeps the exact sealed rows of all three jobs and both pages.
    db.resolve_failed_capture(failed).unwrap();
    drain(&db);
    assert_eq!(
        db.state(route).unwrap().consolidating,
        Some(failed.generation)
    );
    assert_eq!(page_and_point(&db, reader, 1..=400), sealed);
    assert_eq!(check_names(&db, reader, &sealed_world), sealed_names);
    for serial in 300..=303_u64 {
        assert_eq!(
            db.reader_symlink(reader, serial).unwrap(),
            format!("target-{serial}").into_bytes()
        );
    }
    assert!(matches!(
        db.reader_symlink(reader, 304),
        Err(OverlayError::Missing)
    ));
    db.release_captured_reader(reader).unwrap();
    drain(&db);
    assert!(db.capture_ready(route).unwrap());

    // The next capture seals the folded generation: one row per touched key.
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    let folded = page_and_point(&db, reader, 1..=400);
    // The engine maintains the fall-through cutoff; every other field is the
    // caller's final value, from whichever generation last wrote the inode.
    let plain = |mut inode: Inode| {
        inode.inherited_cutoff = 0;
        inode
    };
    assert_eq!(
        folded.iter().cloned().map(plain).collect::<Vec<_>>(),
        world
            .inodes
            .values()
            .cloned()
            .map(plain)
            .collect::<Vec<_>>()
    );
    assert_eq!(folded.len(), sealed.len() + 12);
    check_names(&db, reader, &world);
    assert_eq!(world.parents(), [200, 201, 202]);
    for serial in [300_u64, 302, 303] {
        assert_eq!(
            db.reader_symlink(reader, serial).unwrap(),
            format!("target-{serial}").into_bytes()
        );
    }
    assert_eq!(
        db.reader_symlink(reader, 304).unwrap(),
        b"second-generation"
    );
    for serial in [301_u64, 40, 200, 1] {
        assert!(matches!(
            db.reader_symlink(reader, serial),
            Err(OverlayError::Missing)
        ));
    }
    db.release_captured_reader(reader).unwrap();
    db.close_file(open).unwrap();
}

#[test]
fn symlink_target_is_the_exact_local_bytes_of_a_live_symlink_or_missing() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([208; 32], [209; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut world = World::new(&db, source);
    let born = world.born();
    let targets: [(u64, Vec<u8>); 5] = [
        (50, b"../a/b".to_vec()),
        (51, b"/".to_vec()),
        (52, (0..4095).map(|i| b'a' + (i % 23) as u8).collect()),
        (53, (0..CELL_BYTES).map(|i| 1 + (i % 251) as u8).collect()),
        (54, b"\xff\x00opaque\x00".to_vec()),
    ];
    for (serial, target) in &targets {
        world.symlink(*serial, target);
    }
    // Rewritten metadata in the creating generation keeps the one target cell.
    let mut touched = world.inodes[&50].clone();
    touched.mtime_seconds = 99;
    world.inode(touched, None);
    // Created then removed, a regular file, a directory, and a base symlink
    // with only a metadata row: its target stays in the immutable base.
    world.symlink(60, b"gone");
    let mut removed = world.inodes[&60].clone();
    removed.nlink = 0;
    world.inode(removed, None);
    world.inode(node(61, InodeKind::File, 3, born), Some(bytes_cell(b"abc")));
    world.inode(node(62, InodeKind::Directory, 0, born), None);
    let mut base = node(63, InodeKind::Symlink, 9, 0);
    base.inherited_cutoff = 9;
    world.inode(base, None);
    // A symlink row longer than its one cell is not a representable target.
    world.inode(
        node(65, InodeKind::Symlink, CELL_BYTES as u64 + 1, born),
        Some(bytes_cell(&[b'z'; CELL_BYTES])),
    );
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    for (serial, target) in &targets {
        assert_eq!(db.reader_symlink(reader, *serial).unwrap(), *target);
        let read = db
            .read_captured(reader, *serial, 0, CELL_BYTES as u32)
            .unwrap()
            .unwrap();
        assert_eq!((read.kind, read.span), (InodeKind::Symlink, None));
        assert_eq!(read.data, *target);
    }
    for serial in [60_u64, 61, 62, 63, 64, 0] {
        assert!(
            matches!(
                db.reader_symlink(reader, serial),
                Err(OverlayError::Missing)
            ),
            "{serial}"
        );
    }
    // The refused base symlink is a live captured symlink whose whole target
    // the base supplies; the refused removal is a captured tombstone.
    let base = db.reader_inode(reader, 63).unwrap().unwrap();
    assert_eq!(
        (base.kind, base.nlink, base.born),
        (InodeKind::Symlink, 1, 0)
    );
    assert_eq!(
        db.read_captured(reader, 63, 0, CELL_BYTES as u32)
            .unwrap()
            .unwrap()
            .span,
        Some((0, 9))
    );
    assert_eq!(db.reader_inode(reader, 60).unwrap().unwrap().nlink, 0);
    assert!(matches!(
        db.reader_symlink(reader, 65),
        Err(OverlayError::Invalid("captured symlink window"))
    ));
    assert!(matches!(
        db.reader_symlink(reader, u64::MAX),
        Err(OverlayError::Invalid(_))
    ));
    // A refusal disposes nothing: the reader and its answers remain.
    assert_eq!(db.retained_captured_reader(route, 1).unwrap(), Some(reader));
    assert_eq!(db.reader_symlink(reader, 52).unwrap(), targets[2].1);
    db.release_captured_reader(reader).unwrap();
}

#[test]
fn reader_name_and_symlink_jobs_seek_their_sealed_domain_while_the_active_generation_grows() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([210; 32], [211; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut world = World::new(&db, source);
    for n in 0..130_u64 {
        world.name(10, format!("n{n:04}").as_bytes(), Some(1000 + n));
    }
    world.symlink(50, b"sealed/target");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let sealed = world.parent(10);
    let source = db.acquire_base_source(route, 2).unwrap();
    let mut active = World::new(&db, source);
    let born = active.born();
    // The probed keys themselves get later active rows before any counting.
    active.name(10, b"n0064", Some(9000));
    active.name(10, b"n0065", None);
    let mut changed = world.inodes[&50].clone();
    changed.mtime_seconds = 77;
    changed.inherited_cutoff = changed.size;
    active.inode(changed, None);

    let plans = db
        .explain_reader_directory_entries(reader, 10, b"n0063")
        .unwrap();
    evidence(format_args!("READER_NAMESPACE_PLAN {plans:?}"));
    assert_eq!(plans.len(), 2, "{plans:?}");
    assert!(
        plans[0].starts_with("parent-names: SEARCH")
            && plans[0].contains("directory_entry_capture")
            && plans[0].contains("ns=? AND gen=? AND parent=? AND name>?"),
        "{plans:?}"
    );
    assert!(
        plans[1].starts_with("name-point: SEARCH")
            && plans[1].contains("PRIMARY KEY")
            && plans[1].contains("ns=? AND parent=? AND name=? AND gen=?"),
        "{plans:?}"
    );
    assert!(
        !plans
            .iter()
            .any(|plan| plan.contains("SCAN") || plan.contains("TEMP B-TREE")),
        "{plans:?}"
    );
    // The symlink job reuses the layered read: its two statements, as planned.
    let payload = db.explain_payload(source).unwrap();
    let layered: Vec<&String> = payload
        .iter()
        .filter(|plan| plan.starts_with("layers:") || plan.starts_with("cell-window:"))
        .collect();
    evidence(format_args!("READER_SYMLINK_PLAN {layered:?}"));
    for label in ["layers: SEARCH", "cell-window: SEARCH"] {
        assert!(
            layered.iter().any(|plan| plan.starts_with(label)),
            "{payload:?}"
        );
    }
    assert!(
        !layered
            .iter()
            .any(|plan| plan.contains("SCAN") || plan.contains("TEMP B-TREE")),
        "{payload:?}"
    );

    // Distinct count-driven cases, no latency, cold-cache or resident claim.
    let mut previous = 0_u64;
    let mut steps = None;
    for population in [128_u64, 1024, 4096] {
        for n in previous..population {
            // One later active inode and one later active name under the same
            // parent, sorting directly after a sealed name of the window.
            let name = format!("n{:04}-{n}", 64 + n % 64).into_bytes();
            active.apply(Changes {
                inodes: vec![node(20_000 + n, InodeKind::File, 0, born)],
                directory_entries: vec![DirectoryEntryChange {
                    parent: 10,
                    binding: Binding::Bound {
                        serial: 20_000 + n,
                        inherited: in_base(10, &name),
                    },
                    name,
                }],
                ..Changes::default()
            });
        }
        previous = population;
        let before = db.diagnostics();
        let window = db
            .reader_parent_directory_entries(reader, 10, Some(b"n0063"))
            .unwrap();
        let paged = db.diagnostics();
        let point = db.reader_directory_entry(reader, 10, b"n0064").unwrap();
        let pointed = db.diagnostics();
        let absent = db.reader_directory_entry(reader, 10, b"n0064-0").unwrap();
        let missed = db.diagnostics();
        let target = db.reader_symlink(reader, 50).unwrap();
        let linked = db.diagnostics();
        assert_eq!(window, sealed[64..128]);
        assert_eq!(point.as_ref(), Some(&sealed[64]));
        assert_eq!(absent, None);
        assert_eq!(target, b"sealed/target");
        let work = [
            paged.since(&before),
            pointed.since(&paged),
            missed.since(&pointed),
            linked.since(&missed),
        ];
        for (job, delta) in work.iter().enumerate() {
            for (family, row) in delta.statements.iter().enumerate() {
                assert_eq!(
                    (
                        row.fullscan_steps,
                        row.sorts,
                        row.autoindex_rows,
                        row.reprepares
                    ),
                    (0, 0, 0, 0),
                    "job {job} family {family}"
                );
            }
        }
        let capture_family =
            |delta: &DatabaseWork| delta.statements[StatementKind::Capture as usize];
        let (cursor, hit, miss) = (
            capture_family(&work[0]),
            capture_family(&work[1]),
            capture_family(&work[2]),
        );
        assert_eq!((cursor.executions, cursor.rows_returned), (1, 64));
        assert_eq!((hit.executions, hit.rows_returned), (1, 1));
        assert_eq!((miss.executions, miss.rows_returned), (1, 0));
        let inode = work[3].statements[StatementKind::Inode as usize];
        let cells = work[3].statements[StatementKind::Payload as usize];
        assert_eq!((inode.executions, inode.rows_returned), (1, 1));
        assert_eq!((cells.executions, cells.rows_returned), (1, 1));
        assert_eq!(capture_family(&work[3]).executions, 0);
        let current = (
            cursor.vm_steps,
            hit.vm_steps,
            miss.vm_steps,
            inode.vm_steps,
            cells.vm_steps,
            work.map(|delta| delta.total().vm_steps),
            work.map(|delta| delta.total().executions),
        );
        assert!(cursor.vm_steps > 0 && hit.vm_steps > 0 && inode.vm_steps > 0);
        if let Some(old) = steps {
            assert_eq!(old, current, "work grew with the active population");
        }
        steps = Some(current);
        evidence(format_args!(
            "READER_NAMESPACE_WORK later_active_rows={population} cursor_vm={} cursor_rows=64 \
             point_vm={} point_rows=1 absent_point_vm={} absent_rows=0 symlink_layers_vm={} \
             symlink_cells_vm={} job_total_vm={:?} job_statements={:?} fullscan=0 sorts=0 \
             autoindex=0 reprepare=0",
            current.0, current.1, current.2, current.3, current.4, current.5, current.6
        ));
    }
    assert_eq!(db.state(route).unwrap().dirty_inodes, 4097);
    db.release_base_source(source).unwrap();
    db.release_captured_reader(reader).unwrap();
}
