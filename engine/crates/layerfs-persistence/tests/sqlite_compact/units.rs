//! §7 steps 3–5 and §11 unit checks for the fresh-copy SQLite rebuild, moved out of
//! `backend/sqlite/compact/`: product source holds no tests (the
//! product-boundary guard).

use crate::support;

use layerfs_history::HistoryError;
use layerfs_persistence::internals::{io_error, peak_temp_bytes, receipt};
use layerfs_persistence::{CompactKeptWaste, CompactPath, CompactResidue};

/// §7 step 3 on a bounded filesystem: `ENOSPC`, `EDQUOT` and `EFBIG` are the
/// report's `Capacity` refusal, never a corruption verdict.
#[test]
fn bounded_filesystem_errors_are_capacity() {
    for code in [28, 122, 27, 69] {
        let error = io_error(std::io::Error::from_raw_os_error(code));
        assert!(
            matches!(error, HistoryError::Capacity(_)),
            "errno {code}: {error:?}"
        );
    }
}

/// Every other I/O failure keeps the integrity classification.
#[test]
fn another_io_error_is_integrity() {
    let error = io_error(std::io::Error::from_raw_os_error(5));
    assert!(matches!(error, HistoryError::Integrity(_)), "{error:?}");
}

/// §11 `peakTempBytes` for the fresh-copy rebuild: the fresh Store plus the
/// `visited` scratch, and nothing else. The path stages no whole-Store copy,
/// writes no prune journal and opens no rewrite temp database, so the old
/// `staged + journal + target + visited` bound no longer describes it.
#[test]
fn peak_temp_is_the_fresh_store_plus_the_visited_scratch() {
    let target_bytes = 45_763_203_072;
    let visited_bytes = 20_365_312;
    assert_eq!(
        peak_temp_bytes(target_bytes, visited_bytes),
        target_bytes + visited_bytes
    );
    assert_eq!(peak_temp_bytes(0, visited_bytes), visited_bytes);
    assert_eq!(peak_temp_bytes(target_bytes, 0), target_bytes);
    assert_eq!(peak_temp_bytes(0, 0), 0);
    // The bound saturates instead of wrapping.
    assert_eq!(peak_temp_bytes(u64::MAX, 1), u64::MAX);
}

/// §11/S7: `deltaBases.references` is capped, and the cap is reported; the
/// bytes and the retained set are unaffected.
#[test]
fn delta_base_references_are_capped_and_the_cap_is_reported() {
    use layerfs_content::ObjectId;
    use layerfs_persistence::internals::{cap_delta_references, DELTA_BASE_REFERENCES_CAP};

    let short: Vec<ObjectId> = (0..3u8)
        .map(|index| ObjectId::from_bytes(&[index; 32]).unwrap())
        .collect();
    let (references, capped) = cap_delta_references(&short);
    assert_eq!(references.len(), 3);
    assert!(!capped);

    let long: Vec<ObjectId> = (0..=DELTA_BASE_REFERENCES_CAP)
        .map(|index| ObjectId::from_bytes(&[(index % 251) as u8; 32]).unwrap())
        .collect();
    let (references, capped) = cap_delta_references(&long);
    assert_eq!(references.len(), DELTA_BASE_REFERENCES_CAP);
    assert!(capped, "a list at the cap is reported as capped");
}

/// A rebuild whose source Store had no free pages names that absence; it
/// does not claim there was no waste (there were 100 dead bytes).
#[test]
fn a_rebuild_without_free_pages_explains_no_free_pages() {
    let receipt = receipt(
        CompactPath::Rebuild,
        1,
        2,
        100,
        10,
        0,
        64,
        CompactKeptWaste::default(),
        64,
        CompactResidue::default(),
        1_000,
        2,
    );
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "backendReusable" && zero.reason == "no-free-pages"));
    assert!(
        receipt
            .zero_reasons
            .iter()
            .all(|zero| zero.field != "physicalShrink"),
        "a nonzero shrink carries no zero reason"
    );
}

/// A rebuild can collect dead bytes and still measure no file delta; that
/// is `no-shrink`, never the misleading `no-waste`.
#[test]
fn a_rebuild_with_waste_but_no_shrink_explains_no_shrink() {
    let receipt = receipt(
        CompactPath::Rebuild,
        1,
        2,
        100,
        10,
        4_096,
        0,
        CompactKeptWaste::default(),
        64,
        CompactResidue::default(),
        1_000,
        2,
    );
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "physicalShrink" && zero.reason == "no-shrink"));
    assert!(
        receipt
            .zero_reasons
            .iter()
            .all(|zero| !(zero.field == "physicalShrink" && zero.reason == "no-waste")),
        "a zero delta with dead bytes is not no-waste"
    );
}

/// §7 step 2: the rebuild gives the attached sealed source a large `mmap_size`;
/// the session default is none, so the pragma must visibly apply.
#[test]
fn the_attached_source_gets_a_large_mmap_window() {
    use layerfs_persistence::internals::{source_mmap, SRC_MMAP_BYTES};
    use rusqlite::{params, Connection, OpenFlags};

    // `black_box` keeps this a runtime check: the constant alone would be
    // folded to `assert!(true)` and is rejected by clippy.
    let window = std::hint::black_box(SRC_MMAP_BYTES);
    assert!(
        window >= 256 * 1024 * 1024,
        "§7 step 2 wants a large source window, got {window}"
    );
    let temp = support::Temp::new("compact-source-mmap");
    let source = temp.join("source.sqlite");
    Connection::open(&source)
        .unwrap()
        .execute_batch(
            "PRAGMA journal_mode=OFF; \
             CREATE TABLE pack(pack_id INTEGER PRIMARY KEY, body BLOB NOT NULL); \
             INSERT INTO pack VALUES (1, x'00');",
        )
        .unwrap();
    let staged = temp.join("staged.sqlite");
    let connection = Connection::open_with_flags(
        &staged,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    connection
        .execute(
            "ATTACH DATABASE ?1 AS src",
            params![format!("file:{}?immutable=1", source.display())],
        )
        .unwrap();
    let applied = source_mmap(&connection).unwrap();
    assert_eq!(
        applied, SRC_MMAP_BYTES,
        "§7 step 2 must apply the large source mmap window"
    );
    assert_eq!(
        connection
            .query_row("PRAGMA src.mmap_size", params![], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        SRC_MMAP_BYTES,
        "the applied window reads back"
    );
}

/// The rebuild sizes the fresh file for the retained payload before the bulk
/// load (macOS; other platforms leave it to grow) and cuts the closed file
/// back to the pages its header counts, so a load that writes less than it
/// was sized for installs no page past the database's own end.
#[test]
fn a_sized_fresh_file_loads_and_is_cut_back_to_its_page_count() {
    use layerfs_persistence::internals::{reserve, trim};
    use rusqlite::{params, Connection};

    const PAGE: u64 = 4096;
    let temp = support::Temp::new("compact-space");
    let staged = temp.join("staged.sqlite");
    let connection = Connection::open(&staged).unwrap();
    connection
        .execute_batch(
            "PRAGMA page_size=4096; PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; \
             CREATE TABLE pack(pack_id INTEGER PRIMARY KEY, body BLOB NOT NULL);",
        )
        .unwrap();
    let schema_only = std::fs::metadata(&staged).unwrap().len();

    // Sized for 1 MiB and a part of a page; the load below writes far less.
    reserve(&staged, 1024 * 1024 + 100, PAGE).unwrap();
    let sized = std::fs::metadata(&staged).unwrap().len();
    if cfg!(target_os = "macos") {
        assert_eq!(sized, 1024 * 1024, "whole pages only");
    } else {
        assert_eq!(sized, schema_only, "only macOS sizes the file");
    }
    // A size the file already has is never cut.
    reserve(&staged, PAGE, PAGE).unwrap();
    assert_eq!(std::fs::metadata(&staged).unwrap().len(), sized);

    connection
        .execute_batch(
            "BEGIN IMMEDIATE; \
             INSERT INTO pack VALUES (1, zeroblob(100000)); \
             INSERT INTO pack VALUES (2, randomblob(200000)); \
             COMMIT;",
        )
        .unwrap();
    let pages: i64 = connection
        .query_row("PRAGMA page_count", params![], |row| row.get(0))
        .unwrap();
    let pages = u64::try_from(pages).unwrap();
    connection.close().unwrap();
    assert!(
        pages * PAGE < 1024 * 1024,
        "the load is smaller than its size"
    );

    assert_eq!(trim(&staged, pages, PAGE).unwrap(), pages * PAGE);
    assert_eq!(std::fs::metadata(&staged).unwrap().len(), pages * PAGE);
    // Nothing to cut the second time, and a count past the file cuts nothing.
    assert_eq!(trim(&staged, pages, PAGE).unwrap(), pages * PAGE);
    assert_eq!(trim(&staged, pages + 8, PAGE).unwrap(), pages * PAGE);

    let reopened = Connection::open(&staged).unwrap();
    let check: String = reopened
        .query_row("PRAGMA integrity_check", params![], |row| row.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    let lengths: (i64, i64) = reopened
        .query_row(
            "SELECT (SELECT length(body) FROM pack WHERE pack_id=1),\
                    (SELECT length(body) FROM pack WHERE pack_id=2)",
            params![],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(lengths, (100_000, 200_000));
    let reread: i64 = reopened
        .query_row("PRAGMA page_count", params![], |row| row.get(0))
        .unwrap();
    assert_eq!(u64::try_from(reread).unwrap(), pages);
}

/// The list of `packs`, as a mark hands its retained packs over.
fn ids(packs: &[i64]) -> layerfs_persistence::Sequence {
    let space = layerfs_persistence::internals::Space::new(std::env::temp_dir());
    let mut list = layerfs_persistence::Sequence::new(&space);
    for pack in packs {
        list.push(*pack as u64).unwrap();
    }
    list
}

/// The windows the product cuts for a file of `file` bytes whose packs are
/// `lengths`: of the sealed file, or (`staged`) of the file a rebuild writes.
fn cut_windows(
    staged: bool,
    lengths: &std::collections::BTreeMap<i64, u64>,
    retained: &[i64],
    file: u64,
) -> layerfs_history::error::HistoryResult<Vec<layerfs_persistence::internals::Window>> {
    use layerfs_persistence::internals::{staged_windows, windows};
    let packs = lengths.iter().map(|(pack, length)| Ok((*pack, *length)));
    let mut cut = Vec::new();
    let mut each = |window| {
        cut.push(window);
        Ok(())
    };
    if staged {
        let kept: u64 = retained.iter().filter_map(|pack| lengths.get(pack)).sum();
        staged_windows(packs, &ids(retained), file.saturating_sub(kept), &mut each)?;
    } else {
        let total: u64 = lengths.values().sum();
        windows(packs, &ids(retained), file.saturating_sub(total), &mut each)?;
    }
    Ok(cut)
}

/// The pack copy runs one window of retained packs at a time. A window ends
/// once it holds [`WINDOW_BYTES`] of retained payload or when a run of dead
/// packs longer than [`GAP_BYTES`] follows it, and its file range is the
/// payload offsets of its packs plus every byte of the file that is not pack
/// payload.
#[test]
fn the_pack_copy_is_cut_into_windows_in_pack_order() {
    use layerfs_persistence::internals::{Window, GAP_BYTES, WINDOW_BYTES};
    use std::collections::BTreeMap;

    const MIB: u64 = 1024 * 1024;
    let window = std::hint::black_box(WINDOW_BYTES);
    let gap = std::hint::black_box(GAP_BYTES);
    assert_eq!((window, gap), (64 * MIB, 8 * MIB));

    // Ten packs of 16 MiB, all retained, in a file with 1 MiB that is not pack
    // payload: four packs fill a window.
    let lengths: BTreeMap<i64, u64> = (1..=10).map(|pack| (pack, 16 * MIB)).collect();
    let retained: Vec<i64> = (1..=10).collect();
    let file = 160 * MIB + MIB;
    assert_eq!(
        cut_windows(false, &lengths, &retained, file).unwrap(),
        vec![
            Window {
                first: 1,
                last: 4,
                packs: 4,
                offset: 0,
                bytes: 65 * MIB
            },
            Window {
                first: 5,
                last: 8,
                packs: 4,
                offset: 64 * MIB,
                bytes: 65 * MIB
            },
            Window {
                first: 9,
                last: 10,
                packs: 2,
                offset: 128 * MIB,
                bytes: 33 * MIB
            },
        ]
    );

    // A short dead run stays inside its window (the filter skips its rows); a
    // long one ends the window, so the dead bytes are not read ahead.
    let lengths: BTreeMap<i64, u64> = [
        (1, MIB),
        (2, MIB), // dead, short
        (3, MIB),
        (4, 9 * MIB), // dead, longer than the gap
        (5, MIB),
        (7, 2 * MIB), // dead, last
    ]
    .into_iter()
    .collect();
    assert_eq!(
        cut_windows(false, &lengths, &[1, 3, 5], 15 * MIB).unwrap(),
        vec![
            Window {
                first: 1,
                last: 3,
                packs: 2,
                offset: 0,
                bytes: 3 * MIB
            },
            Window {
                first: 5,
                last: 5,
                packs: 1,
                offset: 12 * MIB,
                bytes: MIB
            },
        ]
    );

    // No retained pack is no window; a file shorter than its packs has no
    // slack to add.
    assert_eq!(
        cut_windows(false, &lengths, &[], 15 * MIB).unwrap(),
        Vec::new()
    );
    assert_eq!(
        cut_windows(false, &lengths, &[7], MIB).unwrap(),
        vec![Window {
            first: 7,
            last: 7,
            packs: 1,
            offset: 13 * MIB,
            bytes: 2 * MIB
        }]
    );

    // A retained pack the Store does not hold is an integrity failure, never
    // a silently shorter copy. The mark's packs are a list that refuses a
    // pack out of order when it is added, so no cut meets one.
    for broken in [&[6][..], &[5, 8][..]] {
        let error = cut_windows(false, &lengths, broken, 15 * MIB).unwrap_err();
        assert!(
            matches!(error, HistoryError::Integrity("mark pack")),
            "{broken:?}: {error:?}"
        );
    }

    // The file a rebuild writes holds the retained packs alone, so its plan
    // counts no dead pack: the long dead run neither ends a window nor moves
    // an offset, and the slack is that of the fresh file.
    assert_eq!(
        cut_windows(true, &lengths, &[1, 3, 5], 4 * MIB).unwrap(),
        vec![Window {
            first: 1,
            last: 5,
            packs: 3,
            offset: 0,
            bytes: 4 * MIB
        }]
    );
    assert_eq!(cut_windows(true, &lengths, &[], MIB).unwrap(), Vec::new());
    for broken in [&[6][..], &[5, 8][..]] {
        let error = cut_windows(true, &lengths, broken, 4 * MIB).unwrap_err();
        assert!(
            matches!(error, HistoryError::Integrity("mark pack")),
            "{broken:?}: {error:?}"
        );
    }
}

/// The run keeps every pack's stored length in bounded memory: past 65,536
/// packs the list is in an unnamed file beside the Store, and what it gives
/// back (the total, the retained bytes, the packs in order and the windows
/// cut from them) is what a list held whole gives.
#[test]
fn the_pack_lengths_of_a_large_store_are_read_back_from_a_file() {
    use layerfs_persistence::internals::{windows, PackLengths, Space};
    use std::collections::BTreeMap;

    const PACKS: i64 = 200_000;
    let directory =
        std::env::temp_dir().join(format!("layerfs-pack-lengths-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let length = |pack: i64| 1_000 + (pack as u64 * 7_919) % 90_000;
    let space = Space::new(directory.clone());
    let mut lengths = PackLengths::new(&space);
    let mut whole = BTreeMap::new();
    for pack in 1..=PACKS {
        // Identities need not be dense.
        let pack = pack * 3;
        lengths.push(pack, length(pack)).unwrap();
        whole.insert(pack, length(pack));
    }
    // 16 bytes a pack, less the 65,536 packs that may stay in memory.
    let file = space.peak_bytes();
    assert!(
        (16 * (PACKS as u64 - 65_536)..=16 * PACKS as u64).contains(&file),
        "{file} bytes in the file"
    );
    assert_eq!(
        std::fs::read_dir(&directory).unwrap().count(),
        0,
        "the file has no name"
    );
    assert_eq!(lengths.total(), whole.values().sum::<u64>());
    // Read twice: the list is not consumed.
    for _ in 0..2 {
        let read: Vec<(i64, u64)> = lengths.packs().unwrap().map(Result::unwrap).collect();
        assert!(read
            .iter()
            .copied()
            .eq(whole.iter().map(|(pack, length)| (*pack, *length))));
    }
    let retained: Vec<i64> = whole.keys().copied().filter(|pack| pack % 5 != 0).collect();
    let kept: u64 = retained.iter().map(|pack| whole[pack]).sum();
    assert_eq!(lengths.stored(&ids(&retained)).unwrap(), kept);
    assert!(matches!(
        lengths.stored(&ids(&[3, 4])).unwrap_err(),
        HistoryError::Integrity("mark pack")
    ));
    let slack = 4096;
    let mut streamed = Vec::new();
    windows(
        lengths.packs().unwrap(),
        &ids(&retained),
        slack,
        &mut |window| {
            streamed.push(window);
            Ok(())
        },
    )
    .unwrap();
    let held = cut_windows(false, &whole, &retained, lengths.total() + slack).unwrap();
    assert!(held.len() > 100, "{} windows", held.len());
    assert_eq!(streamed, held);
    assert_eq!(
        streamed.iter().map(|window| window.packs).sum::<u64>(),
        retained.len() as u64
    );
    std::fs::remove_dir(&directory).unwrap();
}

/// The verification's locators are sorted in bounded memory: past 524,288 of
/// them the sorter writes 8 MiB runs to an unnamed file, exactly the bytes
/// the report counts beforehand, and the pass by pack reads them back merged
/// with the rest. Every pack takes its locators, and a pack that is gone
/// leaves its locators as the foreign-key failure.
#[test]
fn the_staged_locators_of_a_large_store_are_sorted_through_a_file() {
    use layerfs_persistence::internals::StagedLocators;

    const PACKS: i64 = 3_000;
    const RECORDS: i64 = 200;
    let directory =
        std::env::temp_dir().join(format!("layerfs-staged-locators-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let connection = rusqlite::Connection::open(directory.join("staged.sqlite")).unwrap();
    // The two tables the pass reads, with the columns it reads; locators are
    // stored in an order that is not pack order, as object identities are.
    connection
        .execute_batch(&format!(
            "PRAGMA journal_mode=OFF;PRAGMA synchronous=OFF;\
CREATE TABLE pack(pack_id INTEGER PRIMARY KEY,length INTEGER,domain INTEGER);\
CREATE TABLE object_location(n INTEGER PRIMARY KEY,pack_id INTEGER,\
group_number INTEGER,record_number INTEGER,canonical_length INTEGER);\
WITH RECURSIVE p(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM p WHERE i<{PACKS}) \
INSERT INTO pack SELECT i*2,100+i,i%2 FROM p;\
WITH RECURSIVE o(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM o WHERE i<{}) \
INSERT INTO object_location SELECT i,((i*7919)%{PACKS}+1)*2,i%3,i,5 FROM o;",
            PACKS * RECORDS - 1
        ))
        .unwrap();
    let objects = (PACKS * RECORDS) as u64;
    let estimate = |objects| {
        let space = layerfs_persistence::internals::Space::new(directory.clone());
        StagedLocators::sort_file_bytes(&space, objects)
    };
    assert_eq!(estimate(objects), 8 * 1024 * 1024);
    assert_eq!(estimate(524_287), 0);
    let run = |connection: &rusqlite::Connection| {
        let space = layerfs_persistence::internals::Space::new(directory.clone());
        let mut locators = StagedLocators::read(connection, &space)?;
        assert_eq!(
            (locators.objects, locators.canonical),
            (objects, 5 * objects as i64)
        );
        assert_eq!(
            space.peak_bytes(),
            StagedLocators::sort_file_bytes(&space, objects)
        );
        let mut none = std::iter::empty();
        let mut pass = locators.pass(connection, &mut none)?;
        pass.through(PACKS)?;
        pass.finish()
    };
    let scan = run(&connection).unwrap();
    let lengths: i64 = (1..=PACKS).map(|pack| 100 + pack).sum();
    let metadata: i64 = (1..=PACKS)
        .filter(|pack| pack % 2 == 0)
        .map(|pack| 100 + pack)
        .sum();
    assert_eq!(
        (scan.packs, scan.stored, scan.metadata),
        (PACKS, lengths, metadata)
    );
    connection
        .execute("DELETE FROM pack WHERE pack_id=?1", [PACKS])
        .unwrap();
    assert!(matches!(
        run(&connection).unwrap_err(),
        HistoryError::Integrity("staged Store foreign key")
    ));
    drop(connection);
    std::fs::remove_file(directory.join("staged.sqlite")).unwrap();
    std::fs::remove_dir(&directory).unwrap();
}
