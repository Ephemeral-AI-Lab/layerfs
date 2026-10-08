//! Counted validation work for one fixed small change at three base sizes.
//!
//! The base grows in a `bulk` subtree the change never names; the `work`
//! subtree is allocated first, so its serials and its directory pages are the
//! same at every size. Each change runs on the streamed-backed route with a
//! fresh record fixture. What must not grow is asserted: the demands, entries,
//! placements, steps, territory and window rows validation counted, and the
//! topology record traffic. What the canonical trees add by their own height
//! and fan-out - validation's inode page reads and the sorted merges' page
//! reads - is printed as observed and asserted only where the inode table has
//! the same height at every size. A second set of larger bases, whose inode
//! table has several branches under its root, shows where the sorted merge's
//! reads stop following the base. No product operation is timed; the one wall
//! figure printed is how long this file took to build its own larger bases.

mod support;

use std::sync::OnceLock;

use layerfs_content::filesystem::inode::codec::{decode_inode_page, InodePage};
use layerfs_content::filesystem::limits::MAXIMUM_INODE_BRANCH_CHILDREN;
use layerfs_content::filesystem::validate::{ValidationWork, CLASSIFICATION_WINDOW_ROWS};
use layerfs_content::filesystem::{FilesystemResources, FilesystemResult, FilesystemRoot};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use support::filesystem::synthetic;
use support::topology::{update_backed, update_resident, Rowset, Shape, Tree, ROOT};

/// Bulk directories at N, 2N and 4N; each holds sixteen files.
const SIZES: [usize; 3] = [64, 128, 256];
/// Bulk directories of the larger bases: 4, 8 and 16 times the largest above.
const LARGE_SIZES: [usize; 3] = [1_024, 2_048, 4_096];
/// The first serial a change allocates, above every base at every size.
const FRESH: u64 = 1_000_000;
const WINDOW: u64 = CLASSIFICATION_WINDOW_ROWS as u64;

/// One base: the fixed `work` and `top` subtrees and a grown `bulk` subtree.
struct Grown {
    tree: Tree,
    /// `/work/`: `file`, `link`, `sub/` and `holder/`.
    work: u64,
    file: u64,
    link: u64,
    /// `/work/sub/`: three files and `inner/` with one file.
    sub: u64,
    /// `/work/holder/`: empty.
    holder: u64,
    /// `/top/`: two files.
    top: u64,
    /// Inodes the base holds.
    inodes: u64,
    /// Levels of the base inode table, leaf included.
    height: u64,
    /// Children of each branch page on the table's leftmost path, root first.
    /// Every serial a change here names lives in the leftmost leaf.
    spine: Vec<u64>,
}

fn grown(bulk: usize) -> Grown {
    let mut shape = Shape::new();
    let work = shape.dir(ROOT, "work");
    let file = shape.file(work, "file");
    let link = shape.symlink(work, "link");
    let sub = shape.dir(work, "sub");
    for index in 0..3 {
        shape.file(sub, &format!("f{index}"));
    }
    let inner = shape.dir(sub, "inner");
    shape.file(inner, "leaf");
    let holder = shape.dir(work, "holder");
    let top = shape.dir(ROOT, "top");
    shape.file(top, "a");
    shape.file(top, "b");
    let bulk_root = shape.dir(ROOT, "bulk");
    for directory in 0..bulk {
        let parent = shape.dir(bulk_root, &format!("d{directory:04}"));
        for index in 0..16 {
            shape.file(parent, &format!("f{index:02}"));
        }
    }
    let tree = shape.build();
    assert!(tree.next < FRESH);
    let root = FilesystemRoot::decode(tree.store.canonical(tree.root.0).expect("root bytes"))
        .expect("root");
    let mut page = root.inode_table();
    let mut height = 1;
    let mut spine = Vec::new();
    while let InodePage::Branch { children, .. } =
        decode_inode_page(tree.store.canonical(page).expect("inode page bytes"))
            .expect("inode page")
    {
        spine.push(children.len() as u64);
        page = children[0].1;
        height += 1;
    }
    Grown {
        inodes: tree.next - 1,
        tree,
        work,
        file,
        link,
        sub,
        holder,
        top,
        height,
        spine,
    }
}

fn bases() -> &'static [Grown] {
    static BASES: OnceLock<Vec<Grown>> = OnceLock::new();
    BASES.get_or_init(|| SIZES.iter().map(|bulk| grown(*bulk)).collect())
}

fn large_bases() -> &'static [Grown] {
    static BASES: OnceLock<Vec<Grown>> = OnceLock::new();
    BASES.get_or_init(|| {
        let started = std::time::Instant::now();
        let bases: Vec<Grown> = LARGE_SIZES.iter().map(|bulk| grown(*bulk)).collect();
        eprintln!(
            "larger bases: inodes {:?} built by this test file in {:?} (fixture wall time, debug build)",
            bases.iter().map(|base| base.inodes).collect::<Vec<_>>(),
            started.elapsed()
        );
        bases
    })
}

/// The validation counters that do not depend on the height of a base tree.
fn logical(work: ValidationWork) -> [u64; 12] {
    [
        work.objects_read,
        work.inode_demands,
        work.directory_pages_read,
        work.entries_examined,
        work.placements,
        work.ancestry_steps,
        work.territory_directories,
        work.territory_entries,
        work.peak_window_rows,
        work.in_place_scans,
        work.in_place_rows,
        work.in_place_directories,
    ]
}

/// Runs one change at N, 2N and 4N and returns the smallest base's result.
fn counted(label: &str, change: impl Fn(&Grown) -> Rowset) -> FilesystemResult {
    counted_over(bases(), label, change)[0]
}

/// Runs one change over every given base and returns each result, in order.
///
/// Asserts that the counted validation work, the classification window, the
/// topology record traffic and the release work are the same at every size,
/// that the record jobs stay inside one window, and that the root equals the
/// resident route's.
fn counted_over(
    bases: &[Grown],
    label: &str,
    change: impl Fn(&Grown) -> Rowset,
) -> Vec<FilesystemResult> {
    let mut first: Option<(FilesystemResult, [usize; 3])> = None;
    let mut results = Vec::with_capacity(bases.len());
    let same_height = bases.iter().all(|base| base.height == bases[0].height);
    for base in bases {
        let rows = change(base);
        let run = update_backed(&base.tree, &rows, FilesystemResources::default());
        let result = *run.result.as_ref().expect("the backed route accepts");
        let work = result.counters.validation;
        let traffic = [
            run.records.topology_reads,
            run.records.topology_batches,
            run.records.topology_enumerations,
        ];
        eprintln!(
            "{label}: inodes {} table height {} spine {:?} | validation: demands {} objects {} \
             dir pages {} entries {} placements {} steps {} territory ({}, {}) peak window {} \
             in place (scans, rows, directories) ({}, {}, {}) | inode pages {} \
             waves {} sites {:?} | topology records (reads, batches, enumerations) {traffic:?} \
             record calls {} max keys {} max job {} | sorted merge pages read: directories {} \
             inodes {} | release {:?}",
            base.inodes,
            base.height,
            base.spine,
            work.inode_demands,
            work.objects_read,
            work.directory_pages_read,
            work.entries_examined,
            work.placements,
            work.ancestry_steps,
            work.territory_directories,
            work.territory_entries,
            work.peak_window_rows,
            work.in_place_scans,
            work.in_place_rows,
            work.in_place_directories,
            work.inode_pages_read,
            work.read_waves,
            work.inode_pages_by_site,
            run.records.calls,
            run.records.maximum_keys,
            run.records.maximum_job,
            result.counters.directories.pages_read,
            result.counters.inodes.pages_read,
            result.counters.release,
        );
        assert!(work.peak_window_rows <= WINDOW, "{label}");
        assert!(run.records.maximum_keys <= 64, "{label}");
        assert!(run.records.maximum_job <= 65_536, "{label}");
        assert_eq!(run.records.after_failure, 0, "{label}");
        let resident = update_resident(&base.tree, &rows, FilesystemResources::default())
            .0
            .expect("the resident route accepts");
        assert_eq!(resident.root, result.root, "{label}: same canonical root");
        results.push(result);
        match &first {
            None => first = Some((result, traffic)),
            Some((smallest, smallest_traffic)) => {
                let expected = smallest.counters.validation;
                assert_eq!(
                    logical(work),
                    logical(expected),
                    "{label}: counted validation work grew with the base"
                );
                assert_eq!(
                    traffic, *smallest_traffic,
                    "{label}: topology record traffic grew with the base"
                );
                assert_eq!(
                    result.counters.release, smallest.counters.release,
                    "{label}: release work grew with the base"
                );
                if same_height {
                    // Equal table heights leave no height term: every counter
                    // validation keeps is then the same.
                    assert_eq!(work, expected, "{label}");
                }
            }
        }
    }
    results
}

#[test]
fn the_bases_grow_and_the_work_subtree_does_not() {
    let bases = bases();
    for pair in bases.windows(2) {
        assert!(pair[1].inodes > pair[0].inodes);
        assert_eq!(
            (pair[0].work, pair[0].file, pair[0].link, pair[0].sub),
            (pair[1].work, pair[1].file, pair[1].link, pair[1].sub)
        );
        assert_eq!((pair[0].holder, pair[0].top), (pair[1].holder, pair[1].top));
        assert_eq!(
            pair[0].tree.stored(pair[0].work).content_root,
            pair[1].tree.stored(pair[1].work).content_root,
            "the work directory page is the same object at every size"
        );
    }
    eprintln!(
        "bases: inodes {:?}, inode table heights {:?}",
        bases.iter().map(|base| base.inodes).collect::<Vec<_>>(),
        bases.iter().map(|base| base.height).collect::<Vec<_>>()
    );
}

#[test]
fn one_file_value_update_costs_no_validation_work() {
    let result = counted("file value update", |base| {
        Rowset::new().value(
            base.file,
            InodeValue {
                metadata_root: synthetic("counts/changed-metadata"),
                ..base.tree.stored(base.file)
            },
        )
    });
    assert_eq!(result.counters.validation, ValidationWork::default());
}

#[test]
fn one_hard_link_is_two_demands_and_no_placement() {
    let result = counted("hard link added", |base| {
        Rowset::new().bind(base.work, "file2", base.file)
    });
    let work = result.counters.validation;
    // The header's directory record and the bound child's record.
    assert_eq!((work.inode_demands, work.peak_window_rows), (2, 2));
    assert_eq!((work.placements, work.directory_pages_read), (0, 0));
}

#[test]
fn one_new_file_is_two_demands_and_no_placement() {
    let result = counted("new file", |base| {
        Rowset::new()
            .fresh(FRESH, InodeKind::RegularFile)
            .bind(base.work, "new", FRESH)
    });
    let work = result.counters.validation;
    // The header's record and the child's absence. The allocator precondition
    // reads its page and charges no demand: a serial above the table's last key
    // is answered at a branch page, and a demand is charged at a leaf.
    assert_eq!((work.inode_demands, work.peak_window_rows), (2, 2));
    assert_eq!(work.inode_pages_by_site.allocation, 1);
    assert_eq!((work.placements, work.directory_pages_read), (0, 0));
}

#[test]
fn one_file_rename_is_two_demands_and_no_placement() {
    let result = counted("file rename", |base| {
        Rowset::new()
            .unbind(base.work, "file")
            .bind(base.work, "renamed", base.file)
    });
    let work = result.counters.validation;
    assert_eq!((work.inode_demands, work.peak_window_rows), (2, 2));
    assert_eq!((work.placements, work.directory_pages_read), (0, 0));
}

#[test]
fn one_symlink_rename_is_two_demands_and_no_placement() {
    let result = counted("symlink rename", |base| {
        Rowset::new()
            .unbind(base.work, "link")
            .bind(base.work, "renamed", base.link)
    });
    let work = result.counters.validation;
    assert_eq!((work.inode_demands, work.peak_window_rows), (2, 2));
    assert_eq!((work.placements, work.directory_pages_read), (0, 0));
}

#[test]
fn one_file_removal_is_one_demand() {
    let result = counted("file removal", |base| {
        Rowset::new().unbind(base.work, "file")
    });
    let work = result.counters.validation;
    // One header and no bound name.
    assert_eq!((work.inode_demands, work.peak_window_rows), (1, 1));
    assert_eq!(work.placements, 0);
}

#[test]
fn one_directory_removal_releases_its_subtree_without_validation_walks() {
    let result = counted("directory removal", |base| {
        Rowset::new().unbind(base.work, "sub")
    });
    let work = result.counters.validation;
    assert_eq!((work.inode_demands, work.peak_window_rows), (1, 1));
    assert_eq!((work.placements, work.entries_examined), (0, 0));
    // Below the unbound `sub`: its three files, `inner` and `inner`'s file.
    assert_eq!(result.counters.release.released, 5);
}

#[test]
fn one_directory_rename_within_the_root_is_one_placement_and_no_walk() {
    let result = counted("directory rename in the root", |base| {
        Rowset::new()
            .unbind(ROOT, "top")
            .bind(ROOT, "renamed", base.top)
    });
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    // One point lookup of the new name in the root's base listing.
    assert_eq!(work.directory_pages_read, 1);
    assert_eq!(
        (
            work.territory_directories,
            work.territory_entries,
            work.entries_examined
        ),
        (0, 0, 0)
    );
}

#[test]
fn one_directory_moved_under_a_fresh_root_directory_is_two_placements_and_no_walk() {
    let result = counted("directory moved under a fresh directory", |base| {
        Rowset::new()
            .mkdir(ROOT, "fresh", FRESH)
            .unbind(base.work, "sub")
            .bind(FRESH, "sub", base.sub)
    });
    let work = result.counters.validation;
    // `sub` walks through the fresh directory to the root: two steps, twice.
    assert_eq!((work.placements, work.ancestry_steps), (2, 4));
    assert_eq!(
        (
            work.territory_directories,
            work.territory_entries,
            work.entries_examined
        ),
        (0, 0, 0)
    );
}

#[test]
fn one_directory_moved_under_a_stored_directory_lists_only_its_own_subtree() {
    let result = counted("directory moved under a stored directory", |base| {
        Rowset::new()
            .unbind(base.work, "sub")
            .bind(base.holder, "sub", base.sub)
    });
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    // `sub` and `inner` are listed: three files and `inner`, then one file.
    assert_eq!(
        (
            work.territory_directories,
            work.territory_entries,
            work.entries_examined
        ),
        (2, 5, 5)
    );
}

#[test]
fn a_directory_moved_under_a_fresh_directory_of_a_stored_one_lists_only_its_own_subtree() {
    // `mkdir work/holder/new; mv work/sub work/holder/new/sub`.
    let result = counted(
        "directory moved under a fresh child of a stored directory",
        |base| {
            Rowset::new()
                .mkdir(base.holder, "new", FRESH)
                .unbind(base.work, "sub")
                .bind(FRESH, "sub", base.sub)
        },
    );
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (2, 4));
    assert_eq!(
        (
            work.territory_directories,
            work.territory_entries,
            work.entries_examined
        ),
        (2, 5, 5)
    );
}

#[test]
fn a_change_wider_than_one_window_keeps_one_window_resident_at_every_size() {
    let result = counted("150 new files in one directory", |base| {
        let mut rows = Rowset::new();
        for index in 0..150 {
            rows = rows.fresh(FRESH + index, InodeKind::RegularFile).bind(
                base.work,
                &format!("new{index:03}"),
                FRESH + index,
            );
        }
        rows
    });
    let work = result.counters.validation;
    // One header and 150 bound names in windows of 64 rows.
    assert_eq!(work.peak_window_rows, WINDOW);
    // The header's record and 150 absences; the three allocator waves each
    // read the table's root page and charge no leaf demand.
    assert_eq!(work.inode_demands, 151);
    assert_eq!(work.inode_pages_by_site.allocation, 3);
    assert_eq!(work.placements, 0);
}

#[test]
fn one_directory_renamed_in_place_under_a_stored_directory_walks_nothing() {
    let result = counted(
        "directory renamed in place under a stored directory",
        |base| {
            Rowset::new()
                .unbind(base.work, "sub")
                .bind(base.work, "renamed", base.sub)
        },
    );
    let work = result.counters.validation;
    // `sub` keeps its base parent: one scan of `work`, two change rows, one
    // rename, and neither a walk nor an upward step.
    assert_eq!(
        (
            work.in_place_scans,
            work.in_place_rows,
            work.in_place_directories
        ),
        (1, 2, 1)
    );
    assert_eq!((work.placements, work.ancestry_steps), (1, 0));
    assert_eq!(
        (
            work.territory_directories,
            work.territory_entries,
            work.entries_examined
        ),
        (0, 0, 0)
    );
}

#[test]
fn larger_bases_bound_the_sorted_merge_inode_reads_by_the_branch_fan_out() {
    let bases = large_bases();
    for base in bases {
        assert!(
            base.height >= 3 && base.spine[0] > 1,
            "more than one branch under the table root: height {} spine {:?}",
            base.height,
            base.spine
        );
    }
    let update = counted_over(bases, "larger: file value update", |base| {
        Rowset::new().value(
            base.file,
            InodeValue {
                metadata_root: synthetic("counts/changed-metadata"),
                ..base.tree.stored(base.file)
            },
        )
    });
    let rename = counted_over(bases, "larger: directory rename in the root", |base| {
        Rowset::new()
            .unbind(ROOT, "top")
            .bind(ROOT, "renamed", base.top)
    });
    for (label, results) in [
        ("file value update", &update),
        ("directory rename", &rename),
    ] {
        for (base, result) in bases.iter().zip(results) {
            let merge = result.counters.inodes;
            eprintln!(
                "larger {label}: inodes {} table height {} root children {} leftmost spine {:?} | \
                 sorted merge inode pages read {} in {} waves, created {}, reused {}",
                base.inodes,
                base.height,
                base.spine[0],
                base.spine,
                merge.pages_read,
                merge.read_waves,
                merge.pages_created,
                merge.pages_reused,
            );
            // No level can supply more pages than one branch has children, so
            // the merge's reads are bounded by the fan-out times the height
            // whatever the number of inodes below.
            assert!(
                merge.pages_read <= MAXIMUM_INODE_BRANCH_CHILDREN * base.height,
                "{label}: {} pages over {} levels",
                merge.pages_read,
                base.height
            );
        }
    }
    assert_eq!(
        update[0].counters.validation,
        ValidationWork::default(),
        "a value update costs no validation work at any size"
    );
    let work = rename[0].counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    assert_eq!((work.territory_directories, work.entries_examined), (0, 0));
}
