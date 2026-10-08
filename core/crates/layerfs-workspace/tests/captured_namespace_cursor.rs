//! R4-5 and R4-7 on the producer side: every cursor pass and point the update
//! makes answers the same sealed rows, pages stay within the service window,
//! and counted work follows the change, not the base. No timing is measured.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::Bench;
use layerfs_content::filesystem::FilesystemUpdateCounters;
use layerfs_overlay::DirectoryEntry;
use layerfs_workspace::CapturedNamespaceWork;
use producer::{build_over, evidence, Call, Drive, Log, Recording, CALLS, FRESH, HEADER, VALUE};
use std::collections::{BTreeMap, BTreeSet};

const WINDOW: usize = 64;
/// One parent's captured names in binary order, whiteouts as None.
type Rows = Vec<(Vec<u8>, Option<u64>)>;

/// The whole-capture sequence: each page resumes after the last returned key,
/// rows are strictly ordered and the first short page ends it.
fn whole(log: &Log) -> Vec<DirectoryEntry> {
    let mut all = Vec::new();
    let mut last: Option<(u64, Vec<u8>)> = None;
    let mut ended = false;
    for page in &log.name_pages {
        assert!(!ended, "a page was requested after a short page");
        assert_eq!(page.after, last, "a page resumes after the last key");
        assert!(page.rows.len() <= WINDOW);
        ended = page.rows.len() < WINDOW;
        for row in &page.rows {
            let key = (row.parent, row.name.clone());
            assert!(
                last.as_ref().is_none_or(|last| *last < key),
                "names are strictly ordered by parent and binary name"
            );
            last = Some(key);
            all.push(row.clone());
        }
    }
    assert!(ended, "the sequence ends with one short page");
    all
}
fn by_parent(all: &[DirectoryEntry]) -> BTreeMap<u64, Rows> {
    let mut parents = BTreeMap::<u64, Rows>::new();
    for row in all {
        parents
            .entry(row.parent)
            .or_default()
            .push((row.name.clone(), row.serial));
    }
    parents
}
/// Every parent-local pass starts at the beginning, resumes after its own
/// last name and, when it reaches its short page, has returned exactly that
/// parent's sealed rows. Returns the number of complete passes per parent.
fn passes(log: &Log, sealed: &BTreeMap<u64, Rows>) -> BTreeMap<u64, usize> {
    let mut open = BTreeMap::<u64, Rows>::new();
    let mut complete = BTreeMap::<u64, usize>::new();
    for page in &log.parent_pages {
        let parent = page.parent;
        let expected = sealed
            .get(&parent)
            .unwrap_or_else(|| panic!("a pass over parent {parent}, which has no sealed name"));
        assert!(page.rows.len() <= WINDOW);
        if page.after.is_none() {
            // An abandoned earlier pass must still have been an exact prefix.
            if let Some(seen) = open.insert(parent, Rows::new()) {
                assert!(expected.starts_with(&seen), "parent {parent}");
            }
        }
        let seen = open
            .get_mut(&parent)
            .unwrap_or_else(|| panic!("parent {parent}: a continuation without a pass"));
        assert_eq!(
            page.after.as_deref(),
            seen.last().map(|row| row.0.as_slice()),
            "parent {parent}: a page resumes after its last name"
        );
        for row in &page.rows {
            assert_eq!(row.parent, parent);
            seen.push((row.name.clone(), row.serial));
        }
        if page.rows.len() < WINDOW {
            assert_eq!(*seen, *expected, "parent {parent}: one complete pass");
            *complete.entry(parent).or_default() += 1;
            open.remove(&parent);
        }
    }
    for (parent, seen) in open {
        assert!(sealed[&parent].starts_with(&seen), "parent {parent}");
    }
    complete
}
/// Every name point answers exactly the sealed row of that name, or nothing.
fn points(log: &Log, sealed: &BTreeMap<u64, Rows>) {
    for point in &log.points {
        let expected = sealed
            .get(&point.parent)
            .and_then(|rows| rows.iter().find(|row| row.0 == point.name))
            .map(|row| row.1);
        let answer = point.answer.as_ref().map(|row| {
            assert_eq!((row.parent, &row.name), (point.parent, &point.name));
            row.serial
        });
        assert_eq!(
            answer,
            expected,
            "point {} {:?}",
            point.parent,
            String::from_utf8_lossy(&point.name)
        );
    }
}
fn serial_key(serial: u64) -> [u8; 32] {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&serial.to_be_bytes());
    key
}

#[test]
fn every_cursor_pass_and_point_answers_the_same_sealed_rows() {
    let b = Bench::new("cc-cursor");
    let mut d = Drive::new(&b);
    let wide = d.mkdir("wide", 0o755);
    // More than two full windows of names in one directory, then the longest
    // names the format allows and names that differ only after 253 bytes.
    let mut created = BTreeMap::<Vec<u8>, u64>::new();
    let mut names: Vec<String> = (0..140).map(|index| format!("n{index:03}")).collect();
    let prefix = "p".repeat(253);
    names.push("L".repeat(255));
    names.push(prefix.clone());
    for suffix in ["a", "aa", "ab", "b", "\u{7f}", "\u{e9}"] {
        names.push(format!("{prefix}{suffix}"));
    }
    names.push("\u{e9}t\u{e9}".to_owned());
    for name in &names {
        assert!(name.len() <= 255);
        let serial = d.create(&format!("wide/{name}"), 0o644);
        assert!(created.insert(name.clone().into_bytes(), serial).is_none());
    }
    // Whiteouts and rebindings in base directories.
    d.remove("alias");
    d.rename("output/result", "output/renamed");
    // Names that came and went leave the directory's final names unchanged.
    d.create("wide/transient", 0o644);
    d.remove("wide/transient");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "cursor");
    let log = recording.log.borrow();
    let all = whole(&log);
    let sealed = by_parent(&all);

    // The sealed rows are what the test did, independently of any cursor.
    let expected: Rows = created
        .iter()
        .map(|(name, serial)| (name.clone(), Some(*serial)))
        .collect();
    let bound: Rows = sealed[&wide]
        .iter()
        .filter(|row| row.1.is_some())
        .cloned()
        .collect();
    assert_eq!(bound, expected);
    assert!(expected.len() > 2 * WINDOW);
    assert_eq!(
        sealed[&1],
        vec![(b"alias".to_vec(), None), (b"wide".to_vec(), Some(wide)),]
    );
    assert_eq!(
        sealed[&7],
        vec![(b"renamed".to_vec(), Some(8)), (b"result".to_vec(), None),]
    );

    // Replays: each complete pass is exactly the sealed sequence again.
    let complete = passes(&log, &sealed);
    for parent in [1, 7, wide] {
        assert!(
            complete.get(&parent).copied().unwrap_or(0) >= 1,
            "parent {parent} was never read to its end"
        );
    }
    let widest = log
        .parent_pages
        .iter()
        .filter(|page| page.parent == wide)
        .map(|page| page.rows.len())
        .max()
        .unwrap();
    assert_eq!(widest, WINDOW, "a full window, never more");
    points(&log, &sealed);
    assert!(!log.points.is_empty(), "the update asked no name point");
    assert!(log.largest_page <= WINDOW);
    assert!(built.work.largest_page <= WINDOW as u64);
    assert_eq!(built.work.entry_rows, all.len() as u64);

    // The sealed record passes: bounded ordered windows over the same keys.
    for window in &log.key_windows {
        assert!(window.keys.len() <= WINDOW);
        let mut last = window.after;
        for key in &window.keys {
            assert!(last.is_none_or(|last| last < *key));
            last = Some(*key);
        }
    }
    let live: BTreeSet<[u8; 32]> = log
        .inode_pages
        .iter()
        .flat_map(|(_, rows)| rows)
        .filter(|row| row.serial == 1 || row.nlink != 0)
        .map(|row| serial_key(row.serial))
        .collect();
    // Fresh: every created file and the directory that holds them.
    let mut fresh: BTreeSet<[u8; 32]> =
        created.values().map(|serial| serial_key(*serial)).collect();
    fresh.insert(serial_key(wide));
    let headers: BTreeSet<[u8; 32]> = [1, 7, wide].into_iter().map(serial_key).collect();
    for (kind, expected) in [(VALUE, &live), (FRESH, &fresh), (HEADER, &headers)] {
        let keys: BTreeSet<[u8; 32]> = log
            .key_windows
            .iter()
            .filter(|window| window.kind == kind)
            .flat_map(|window| window.keys.iter().copied())
            .collect();
        assert_eq!(&keys, expected, "kind {kind:#x}");
    }
    drop(log);
    built.release(&b);
}

/// Every counter of the attempt, by name.
fn counters(work: &CapturedNamespaceWork) -> Vec<(&'static str, u64)> {
    vec![
        ("inode_rows", work.inode_rows),
        ("entry_rows", work.entry_rows),
        ("inode_pages", work.inode_pages),
        ("entry_pages", work.entry_pages),
        ("change_pages", work.change_pages),
        ("parent_points", work.parent_points),
        ("base_lookups", work.base_lookups),
        ("headers_written", work.headers_written),
        ("headers_dropped", work.headers_dropped),
        ("values_written", work.values_written),
        ("fresh_serials", work.fresh_serials),
        ("tombstones_skipped", work.tombstones_skipped),
        ("files_constructed", work.files_constructed),
        ("files_unchanged", work.files_unchanged),
        ("symlinks", work.symlinks),
        ("metadata_built", work.metadata_built),
        ("metadata_patched", work.metadata_patched),
        ("record_jobs", work.record_jobs),
        ("header_opens", work.header_opens),
        ("header_rows", work.header_rows),
        ("header_points", work.header_points),
        ("change_opens", work.change_opens),
        ("change_rows", work.change_rows),
        ("change_points", work.change_points),
        ("value_opens", work.value_opens),
        ("value_rows", work.value_rows),
        ("value_points", work.value_points),
        ("fresh_opens", work.fresh_opens),
        ("fresh_rows", work.fresh_rows),
        ("fresh_points", work.fresh_points),
    ]
}
struct Counted {
    work: CapturedNamespaceWork,
    content: FilesystemUpdateCounters,
    calls: BTreeMap<Call, u64>,
    largest: usize,
}
impl Counted {
    fn report(&self, what: &str) {
        evidence(format_args!("R4-7 {what}: producer {:?}", self.work));
        evidence(format_args!("R4-7 {what}: provider calls {:?}", self.calls));
        evidence(format_args!("R4-7 {what}: content {:?}", self.content));
    }
}
/// `files` empty files created in one new directory, then one attempt.
fn bulk(tag: &str, files: u64) -> Counted {
    let b = Bench::new(tag);
    let mut d = Drive::new(&b);
    d.mkdir("bulk", 0o755);
    for index in 0..files {
        d.create(&format!("bulk/f{index:05}"), 0o644);
    }
    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, tag);
    let work = built.work;
    // Exact rows: the root, the directory and every file; one name each.
    assert_eq!(work.inode_rows, files + 2, "{tag}");
    assert_eq!(work.entry_rows, files + 1, "{tag}");
    assert_eq!(work.values_written, files + 2, "{tag}");
    assert_eq!(work.fresh_serials, files + 1, "{tag}");
    assert_eq!(work.files_constructed, files, "{tag}");
    assert_eq!(work.headers_written, 2, "{tag}");
    // Every full window, then the one short page that ends the pass.
    assert_eq!(work.inode_pages, (files + 2) / 64 + 1, "{tag}");
    assert_eq!(work.entry_pages, (files + 1) / 64 + 1, "{tag}");
    let (calls, largest) = {
        let log = recording.log.borrow();
        (log.counts.clone(), log.largest_page)
    };
    let content = built.content;
    // Content classifies in windows of the same bound, whatever the change.
    assert!(
        content.validation.peak_window_rows <= WINDOW as u64,
        "{tag}"
    );
    built.release(&b);
    let counted = Counted {
        work,
        content,
        calls,
        largest,
    };
    counted.report(tag);
    counted
}

#[test]
fn work_grows_linearly_with_the_change_and_pages_stay_bounded() {
    const RATIO: u64 = 4;
    // An allowance for per-attempt constants and partially filled windows.
    const SLACK: u64 = 64;
    // Ceilings per captured row (inode rows plus name rows) that hold at
    // every size: for each producer counter and for each kind of provider
    // call. They are counted bounds of this workload, not timing.
    const PER_ROW: u64 = 10;
    const CALLS_PER_ROW: u64 = 18;
    let sizes = [40, 160, 640];
    let runs: Vec<Counted> = sizes
        .iter()
        .map(|files| bulk(&format!("cc-bulk-{files}"), *files))
        .collect();
    for run in &runs {
        assert!(run.largest <= WINDOW);
        assert!(run.work.largest_page <= WINDOW as u64);
    }
    assert_eq!(runs[2].work.largest_page, WINDOW as u64);
    for run in &runs {
        let rows = run.work.inode_rows + run.work.entry_rows;
        for (name, count) in counters(&run.work) {
            assert!(
                count <= PER_ROW * rows + SLACK,
                "{name}: {count} for {rows} captured rows"
            );
        }
        for (call, count) in &run.calls {
            assert!(
                *count <= CALLS_PER_ROW * rows + SLACK,
                "{call:?}: {count} for {rows} captured rows"
            );
        }
    }
    // Growth between the two sizes that both exceed one window. A change
    // within one window is cheaper per row, because the cursor answers its
    // repeated points from the retained window, so it is not a baseline.
    for pair in runs[1..].windows(2) {
        let (small, big) = (&pair[0], &pair[1]);
        assert!(small.work.inode_rows > WINDOW as u64);
        for ((name, less), (_, more)) in counters(&small.work).into_iter().zip(counters(&big.work))
        {
            assert!(
                more <= RATIO * less + SLACK,
                "{name}: {less} -> {more} for {RATIO} times the change"
            );
        }
        for call in CALLS {
            let count = |run: &Counted| run.calls.get(&call).copied().unwrap_or(0);
            assert!(
                count(big) <= RATIO * count(small) + SLACK,
                "{call:?}: {} -> {} for {RATIO} times the change",
                count(small),
                count(big)
            );
        }
    }
}

/// One small change, with every provider call it caused.
fn probe(d: &mut Drive<'_>, what: &str) -> Counted {
    d.create("probe", 0o644);
    d.write("probe", 0, b"one small file");
    d.mkdir("probe-dir", 0o755);
    d.rename("symlink", "probe-dir/link");
    d.write("file", 0, b"O");
    let recording = Recording::new(&d.b.overlay);
    let built = build_over(d.b, &recording, &d.model, what);
    let counted = {
        let log = recording.log.borrow();
        Counted {
            work: built.work,
            content: built.content,
            calls: log.counts.clone(),
            largest: log.largest_page,
        }
    };
    built.release(d.b);
    counted.report(what);
    counted
}

#[test]
fn work_follows_the_change_and_not_the_installed_base() {
    let small = Bench::new("cc-base-small");
    let over_small = probe(&mut Drive::new(&small), "small base");

    // The same change over a base that first gained 300 files and a tree.
    let large = Bench::new("cc-base-large");
    let mut d = Drive::new(&large);
    d.mkdir("bulk", 0o755);
    for index in 0..300 {
        d.create(&format!("bulk/f{index:05}"), 0o644);
    }
    d.mkdir("bulk/deep", 0o755);
    d.mkdir("bulk/deep/deeper", 0o755);
    d.build("large base").install(&large);
    let over_large = probe(&mut d, "large base");

    assert_eq!(
        over_large.work, over_small.work,
        "counted producer work differs with the base"
    );
    assert_eq!(
        over_large.calls, over_small.calls,
        "provider calls differ with the base"
    );
    assert_eq!(over_large.largest, over_small.largest);
    // Content's own change-shaped work is the same too: the same bindings,
    // merges, placements and windows, and no base directory is ever listed.
    let (small, large) = (over_small.content, over_large.content);
    assert_eq!(
        (
            large.bindings_added,
            large.bindings_removed,
            large.directory_updates,
            large.validation.placements,
            large.validation.peak_window_rows,
        ),
        (
            small.bindings_added,
            small.bindings_removed,
            small.directory_updates,
            small.validation.placements,
            small.validation.peak_window_rows,
        )
    );
    for content in [small, large] {
        assert_eq!(
            (
                content.validation.territory_directories,
                content.validation.territory_entries,
                content.validation.entries_examined,
            ),
            (0, 0, 0)
        );
    }
}
