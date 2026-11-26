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
//! - `dir_600`: one directory of 600 files, just above 512.
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
use std::path::Path;

/// SQLite: session `[0, 0, statements, transactions]`. Baseline,
/// 3a904ac225 (production source identical to 056e920166).
const SQLITE: &[InitPin] = &[
    // probe init.nested_1k: 308 statements, 24 transactions
    pin(
        "nested_1k",
        [0, 0, 308, 24],
        9900,
        137268,
        "6ac312b34cca5d86",
    ),
    // probe init.flat_1k: 2,284 statements, 25 transactions
    pin(
        "flat_1k",
        [0, 0, 2284, 25],
        10890,
        137359,
        "8a79e63caabd7530",
    ),
    pin(
        "hardlink_window",
        [0, 0, 487, 23],
        780,
        7524,
        "d767c58fa69403d6",
    ),
    pin("dir_600", [0, 0, 1428, 25], 6490, 82511, "f2238ebb0be91b92"),
];

/// PostgreSQL: session `[connects, round_trips, statements, transactions]`.
/// Baseline, 3a904ac225.
const POSTGRESQL: &[InitPin] = &[
    // probe init.nested_1k: [0, 229, 118, 33]
    pin(
        "nested_1k",
        [0, 229, 118, 33],
        9900,
        137268,
        "6ac312b34cca5d86",
    ),
    // probe init.flat_1k: [0, 2,246, 2,125, 37]
    pin(
        "flat_1k",
        [0, 2246, 2125, 37],
        10890,
        137359,
        "8a79e63caabd7530",
    ),
    pin(
        "hardlink_window",
        [0, 705, 492, 82],
        780,
        7524,
        "d767c58fa69403d6",
    ),
    pin(
        "dir_600",
        [0, 1438, 1317, 37],
        6490,
        82511,
        "f2238ebb0be91b92",
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
    ("dir_600", |source| rt::files(source, 1, 600)),
];

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

fn run(backend: Backend) -> Option<Vec<InitMeasured>> {
    let mut out = Vec::new();
    for (case, build) in SHAPES {
        let mut store = match backend {
            Backend::Sqlite => Store::sqlite(&format!("init-{case}")),
            Backend::Postgresql => Store::postgresql(&format!("init_{case}"))?,
        };
        out.push(rt::init_shape(&mut store, case, build));
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
