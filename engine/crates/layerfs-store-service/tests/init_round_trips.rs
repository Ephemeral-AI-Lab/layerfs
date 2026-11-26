//! #818 Init pins (lane I owns this file and `init_round_trips_10k.rs`): the
//! counted SQL work of Namespace Init (`layerfs_project::init`) into a new
//! Store, on SQLite and on PostgreSQL, with the byte outcome.
//!
//! Every case Inits its source into a **new Store** (cold by construction:
//! the Store's one session prepares every statement at first use), exactly
//! as the #818 probe's Init cases do (`fs_call_round_trips.rs`), so the
//! nested and flat 1k pins equal the probe's baseline rows. Shapes:
//!
//! - `nested_1k`: 10 directories of 100 files (probe `init.nested_1k`);
//! - `flat_1k`: one directory of 1,000 files (probe `init.flat_1k`): above
//!   the 512-entry wide-directory placement (`WINDOW_ROWS`), two read
//!   windows (`READ_WINDOW_ROWS` = 512);
//! - `hardlink_window`: one directory of 50 files and 50 hard links to them,
//!   interleaved, so a repeated (device, inode) falls inside one window;
//! - `dir_600`: one directory of 600 files, just above 512;
//! - `mixed` (added at #818 I1, which sums Init's logical bytes from its
//!   scan instead of walking the built root): hard links within one
//!   directory and across directories, empty files (one hard-linked), a
//!   tiny, a whole-file (100,000 bytes) and a chunked file (300,000 bytes,
//!   above the whole-file bound), symlinks (to a file, to a directory and
//!   dangling), and empty directories. Its times, symlinks included, are
//!   fixed without following links;
//! - `wide_hardlinks` (added before #818 I2): one directory of 300 files and
//!   300 hard links to them, interleaved, so the 600-entry wide placement
//!   meets a repeated (device, inode) every other child (I2's windows cut
//!   before each repeat).
//!
//! The 10k shapes are pinned in `init_round_trips_10k.rs` (kept apart so
//! each binary stays well under the 120 s ceiling on PostgreSQL).
//!
//! Each case also checks that Init's recorded `logical` equals
//! `logical_size(reader, root)` walked afterwards, and pins the logical
//! bytes, the Store's canonical bytes after Init and the genesis root's id
//! (the Save outcome of a new Store; the same on both backends). Pack bytes
//! are printed, not pinned: they vary by a few bytes from run to run at
//! baseline (support `round_trips.rs`, `InitPin`).
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it; it prints the server's top-level calls (printed, not
//! pinned).
#[path = "support/round_trips.rs"]
mod rt;

use rt::{Backend, InitMeasured, InitPin, Store};
use std::os::unix::fs::symlink;
use std::path::Path;

/// SQLite: session `[0, 0, statements, transactions]`. Baseline 3a904ac225
/// (production source identical to 056e920166) in the comments; the pins
/// are after #818 I1 (Init no longer walks the built root for its logical
/// bytes: −20 statements at 1k, −12 at 600 entries, −1 on the hard-link
/// directory).
const SQLITE: &[InitPin] = &[
    // probe init.nested_1k at baseline: 308 statements, 24 transactions
    pin(
        "nested_1k",
        [0, 0, 288, 24],
        9900,
        137268,
        "6ac312b34cca5d86",
    ),
    // probe init.flat_1k at baseline: 2,284 statements, 25 transactions
    pin(
        "flat_1k",
        [0, 0, 2264, 25],
        10890,
        137359,
        "8a79e63caabd7530",
    ),
    // baseline [0, 0, 487, 23]
    pin(
        "hardlink_window",
        [0, 0, 486, 23],
        780,
        7524,
        "d767c58fa69403d6",
    ),
    // added before I2 (at I1: [0, 0, 1,702, 24])
    pin(
        "wide_hardlinks",
        [0, 0, 1702, 24],
        5180,
        43894,
        "e6cba9e302a3c58f",
    ),
    // baseline [0, 0, 1,428, 25]
    pin("dir_600", [0, 0, 1416, 25], 6490, 82511, "f2238ebb0be91b92"),
    // added at I1 (no baseline row)
    pin(
        "mixed",
        [0, 0, 175, 24],
        1100005,
        402976,
        "186f611869dd16c6",
    ),
];

/// PostgreSQL: session `[connects, round_trips, statements, transactions]`.
/// Baseline 3a904ac225 in the comments; the pins are after #818 I1 (−20
/// round trips at 1k, −12 at 600 entries, −1 on the hard-link directory)
/// and I2a (one placement statement and one dependency check per wide
/// window instead of two statements per child: flat_1k and dir_600
/// 2,226 / 1,426 -> 228, wide_hardlinks 2,634 -> 2,038).
const POSTGRESQL: &[InitPin] = &[
    // probe init.nested_1k at baseline: [0, 229, 118, 33]
    pin(
        "nested_1k",
        [0, 209, 98, 33],
        9900,
        137268,
        "6ac312b34cca5d86",
    ),
    // probe init.flat_1k at baseline: [0, 2,246, 2,125, 37]
    pin(
        "flat_1k",
        [0, 228, 107, 37],
        10890,
        137359,
        "8a79e63caabd7530",
    ),
    // baseline [0, 705, 492, 82]
    pin(
        "hardlink_window",
        [0, 704, 491, 82],
        780,
        7524,
        "d767c58fa69403d6",
    ),
    // added before I2 (at I1: [0, 2,634, 1,907, 339])
    pin(
        "wide_hardlinks",
        [0, 2038, 1309, 339],
        5180,
        43894,
        "e6cba9e302a3c58f",
    ),
    // baseline [0, 1,438, 1,317, 37]
    pin(
        "dir_600",
        [0, 228, 107, 37],
        6490,
        82511,
        "f2238ebb0be91b92",
    ),
    // added at I1 (no baseline row)
    pin(
        "mixed",
        [0, 255, 129, 38],
        1100005,
        402976,
        "186f611869dd16c6",
    ),
];

const fn pin(
    case: &'static str,
    session: rt::Counts,
    logical: u64,
    canonical: u64,
    root: &'static str,
) -> InitPin {
    InitPin {
        case,
        session,
        slack: 0,
        logical,
        canonical,
        root,
    }
}

type Shape = (&'static str, fn(&Path));

const SHAPES: &[Shape] = &[
    ("nested_1k", |source| rt::files(source, 10, 100)),
    ("flat_1k", |source| rt::files(source, 1, 1000)),
    ("hardlink_window", hardlinks),
    ("wide_hardlinks", wide_hardlinks),
    ("dir_600", |source| rt::files(source, 1, 600)),
    ("mixed", mixed),
];

/// Every representation `logical_size` counts, and every one it counts as 0.
fn mixed(source: &Path) {
    let (m, n, e) = (source.join("m"), source.join("n"), source.join("e"));
    for dir in [&m, &n, &e.join("inner")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let bytes = |len: usize, seed: u64| {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 56) as u8
            })
            .collect::<Vec<u8>>()
    };
    std::fs::write(m.join("empty"), b"").unwrap();
    std::fs::hard_link(m.join("empty"), m.join("empty_link")).unwrap();
    std::fs::write(m.join("tiny"), b"tiny\n").unwrap();
    std::fs::write(m.join("whole"), bytes(100_000, 1)).unwrap();
    std::fs::write(m.join("chunked"), bytes(300_000, 2)).unwrap();
    std::fs::hard_link(m.join("chunked"), m.join("chunked_link")).unwrap();
    std::fs::hard_link(m.join("chunked"), n.join("chunked_link")).unwrap();
    std::fs::hard_link(m.join("whole"), n.join("whole_link")).unwrap();
    symlink("../m/tiny", n.join("to_file")).unwrap();
    symlink("../m", n.join("to_dir")).unwrap();
    symlink("missing", n.join("dangling")).unwrap();
}

/// Sets every entry's times under `path` (a directory's after its
/// children), symlinks themselves included, to the instant
/// `rt::freeze_times` uses; it never follows a link.
fn freeze_nofollow(path: &Path) {
    use nix::sys::stat::{utimensat, UtimensatFlags};
    use nix::sys::time::TimeSpec;
    let meta = std::fs::symlink_metadata(path).unwrap();
    if meta.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            freeze_nofollow(&entry.unwrap().path());
        }
    }
    let at = TimeSpec::new(1_700_000_000, 0);
    utimensat(
        nix::fcntl::AT_FDCWD,
        path,
        &at,
        &at,
        UtimensatFlags::NoFollowSymlink,
    )
    .unwrap();
}

/// As `rt::init_shape`, with times fixed by [`freeze_nofollow`].
fn init_nofollow(store: &mut Store, case: &'static str, build: fn(&Path)) -> InitMeasured {
    let source = store.dir.join(format!("source-{case}"));
    build(&source);
    freeze_nofollow(&source);
    let outcome = store.init(&source, &mut |_, _| {});
    InitMeasured { case, outcome }
}

/// 50 files and 50 hard links to them, interleaved by name in one directory.
fn hardlinks(source: &Path) {
    let dir = source.join("h");
    std::fs::create_dir_all(&dir).unwrap();
    for n in 0..50 {
        let file = dir.join(format!("f{n:03}a"));
        std::fs::write(&file, format!("file {n}\n")).unwrap();
        std::fs::hard_link(&file, dir.join(format!("f{n:03}b"))).unwrap();
    }
}

/// 300 files and 300 hard links to them, interleaved by name in one wide
/// directory.
fn wide_hardlinks(source: &Path) {
    let dir = source.join("w");
    std::fs::create_dir_all(&dir).unwrap();
    for n in 0..300 {
        let file = dir.join(format!("f{n:03}a"));
        std::fs::write(&file, format!("file {n}\n")).unwrap();
        std::fs::hard_link(&file, dir.join(format!("f{n:03}b"))).unwrap();
    }
}

fn run(backend: Backend) -> Option<Vec<InitMeasured>> {
    let mut out = Vec::new();
    for (case, build) in SHAPES {
        let mut store = match backend {
            Backend::Sqlite => Store::sqlite(&format!("init-{case}")),
            Backend::Postgresql => Store::postgresql(&format!("init_{case}"))?,
        };
        out.push(if *case == "mixed" {
            init_nofollow(&mut store, case, *build)
        } else {
            rt::init_shape(&mut store, case, build)
        });
    }
    Some(out)
}

#[test]
fn sqlite_init_counts() {
    let measured = run(Backend::Sqlite).unwrap();
    rt::check_init("init_round_trips", Backend::Sqlite, &measured, SQLITE);
}

#[test]
fn postgresql_init_counts() {
    let _turn = rt::postgresql_turn();
    let Some(measured) = run(Backend::Postgresql) else {
        return;
    };
    rt::check_init(
        "init_round_trips",
        Backend::Postgresql,
        &measured,
        POSTGRESQL,
    );
}
