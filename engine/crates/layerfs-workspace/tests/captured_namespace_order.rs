//! Order of the producer's provider calls, observed and never self-reported:
//! every captured page is applied before the next page is requested, and real
//! mutations published between two producer passes never reach the root of
//! the capture. The recording provider numbers every call in one sequence.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::{create, rename, unlink, write, Bench};
use layerfs_workspace::{Operation, Time};
use producer::{
    build_over, build_taken, evidence, take, Call, Drive, Recording, FRESH, HEADER, VALUE,
};
use std::collections::BTreeSet;

const WINDOW: usize = 64;

#[test]
fn each_captured_page_is_applied_before_the_next_page_is_requested() {
    let b = Bench::new("co-pages");
    let mut d = Drive::new(&b);
    // 20 new directories of 10 files each: several full pages of names, with
    // many parents finishing inside each page, and several pages of inodes.
    for directory in 0..20 {
        d.mkdir(&format!("d{directory:02}"), 0o755);
        for file in 0..10 {
            d.create(&format!("d{directory:02}/f{file}"), 0o644);
        }
    }
    d.chmod("file", 0o600);
    d.remove("cache/state");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "page order");
    let log = recording.log.borrow();
    let applied = log.applied();
    let sealed = log.sealed_at().expect("the attempt sealed its rows");
    let inode_calls = log.sequence(Call::InodePage);
    let name_calls = log.sequence(Call::NamePage);
    assert_eq!(inode_calls.len(), log.inode_pages.len());
    assert_eq!(name_calls.len(), log.name_pages.len());
    assert!(inode_calls.len() >= 3 && name_calls.len() >= 3);
    // The name pass is complete before the inode pass begins.
    assert!(name_calls.last().unwrap() < inode_calls.first().unwrap());

    // Inode page k: every value and fresh rank of its live rows was applied
    // after page k was returned and before page k + 1 was requested; the
    // last page's before the seal.
    let mut values = 0;
    for (page, (_, rows)) in log.inode_pages.iter().enumerate() {
        let next = inode_calls.get(page + 1).copied().unwrap_or(sealed);
        if page + 1 < log.inode_pages.len() {
            assert_eq!(rows.len(), WINDOW, "only the last page is short");
        }
        for row in rows {
            let live = row.serial == 1 || row.nlink != 0;
            let fresh = live && row.serial != 1 && built.taken.reader.created_above(row.born);
            for (kind, expected) in [(VALUE, live), (FRESH, fresh)] {
                match applied.get(&(kind, row.serial)) {
                    Some(seq) => {
                        assert!(expected, "kind {kind:#x} for serial {}", row.serial);
                        assert!(
                            inode_calls[page] < *seq && *seq < next,
                            "inode page {page} at {}: kind {kind:#x} of serial {} applied at {seq}, next page requested at {next}",
                            inode_calls[page],
                            row.serial
                        );
                        values += 1;
                    }
                    None => assert!(!expected, "kind {kind:#x} for serial {}", row.serial),
                }
            }
        }
    }
    assert_eq!(
        values as u64,
        built.work.values_written + built.work.fresh_serials
    );

    // Name page k: the header of every parent the page finished (a later row
    // of the same page has another parent, or the page ends the sequence)
    // was applied after page k was returned and before page k + 1 was
    // requested.
    let mut finished = 0;
    for (page, rows) in log.name_pages.iter().map(|page| &page.rows).enumerate() {
        let last = page + 1 == log.name_pages.len();
        let next = name_calls.get(page + 1).copied().unwrap_or(inode_calls[0]);
        let open = rows.last().map(|row| row.parent);
        let parents: BTreeSet<u64> = rows
            .iter()
            .map(|row| row.parent)
            .filter(|parent| last || Some(*parent) != open)
            .collect();
        assert!(!parents.is_empty(), "name page {page} finished no parent");
        for parent in parents {
            let seq = applied
                .get(&(HEADER, parent))
                .unwrap_or_else(|| panic!("parent {parent} has no header"));
            assert!(
                name_calls[page] < *seq && *seq < next,
                "name page {page} at {}: header of parent {parent} applied at {seq}, next page requested at {next}",
                name_calls[page]
            );
            finished += 1;
        }
    }
    // Every header written by the name pass precedes the first inode page.
    let early = applied
        .iter()
        .filter(|((kind, _), seq)| *kind == HEADER && **seq < inode_calls[0])
        .count();
    assert_eq!(early as u64, built.work.headers_written);
    evidence(format_args!(
        "R4-5 order: inode pages at {inode_calls:?}, name pages at {name_calls:?}, sealed at {sealed}, {values} values and ranks and {finished} of {early} headers checked page by page, record_jobs={}",
        built.work.record_jobs
    ));
    drop(log);
    built.release(&b);
}

#[test]
fn mutations_published_between_producer_passes_do_not_reach_the_captured_root() {
    let b = Bench::new("co-later");
    let mut d = Drive::new(&b);
    let directory = d.mkdir("dir", 0o755);
    // More than one page of captured inodes: the files created last, and
    // `top` after them, are constructed after the second inode page.
    for file in 0..70 {
        d.create(&format!("dir/f{file:02}"), 0o644);
    }
    d.write("dir/f69", 0, &[0x69; 9000]);
    let top = d.create("top", 0o644);
    d.write("top", 0, b"the captured bytes of top");
    d.write(".git/index", 1000, &[9; 5000]);
    d.remove("alias");
    d.rename("cache/state", "output/moved");
    let last = d.serial("dir/f69");
    let first = d.serial("dir/f00");

    let taken = take(&b);
    // The model at the capture: nothing published later belongs to it.
    let captured = d.model.clone();
    let later = Time {
        seconds: 1_900_000_000,
        nanoseconds: 7,
    };
    let recording = Recording::new(&b.overlay);
    // Before the second inode page: the same parent, names and files.
    recording.before(Call::InodePage, 1, || {
        b.applied(unlink(directory, "f00"), later);
        b.applied(create(directory, "f00"), later);
        b.applied(
            write(top, 0, b"LATER BYTES OF TOP, AND LONGER THAN BEFORE"),
            later,
        );
        b.applied(write(last, 100, &[0x96; 20_000]), later);
        b.applied(write(8, 1000, &[1; 5000]), later);
        let chmod = Operation::SetAttributes {
            serial: directory,
            mode: Some(0o700),
            mtime: None,
            size: None,
        };
        b.applied(chmod, later);
    });
    // Before the update's second parent page: the same names again.
    recording.before(Call::ParentPage, 1, || {
        b.applied(unlink(directory, "f01"), later);
        b.applied(rename((1, "top"), (directory, "f01"), true, None), later);
        b.applied(create(1, "alias"), later);
        b.applied(unlink(7, "moved"), later);
        b.applied(write(8, 2000, &[2; 100]), later);
    });
    let built = build_taken(&b, &recording, taken, &captured, "later mutations");

    let log = recording.log.borrow();
    assert_eq!(recording.pending_hooks(), 0, "a hook never ran");
    assert!(log.count(Call::InodePage) >= 2 && log.count(Call::ParentPage) >= 2);
    // The later mutations are real: the live view has moved on.
    assert_eq!(b.lookup(1, "top"), None);
    assert_eq!(b.lookup(directory, "f01").unwrap().serial, top);
    assert_ne!(b.lookup(directory, "f00").unwrap().serial, first);
    assert_ne!(b.content(8), captured.bytes(".git/index"));
    assert_ne!(b.content(top), captured.bytes("top"));
    // The root is the capture: names, serials and bytes as they were.
    assert_eq!(built.walked.serial("top"), top);
    assert_eq!(built.walked.serial("dir/f69"), last);
    assert_eq!(built.walked.serial("dir/f00"), first);
    assert!(!built.walked.has("alias"));
    evidence(format_args!(
        "R4-1 later mutations: inode pages={} parent pages={} name points={}",
        log.count(Call::InodePage),
        log.count(Call::ParentPage),
        log.count(Call::NamePoint)
    ));
    drop(log);
    built.release(&b);
}
