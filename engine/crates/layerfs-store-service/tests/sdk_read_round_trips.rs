//! #818 read pins (lane RH owns this file): the counted SQL work of the SDK
//! read calls `fs.stat`, `fs.read`, `fs.list`, `fs.glob` and `fs.grep`,
//! through the method table on the Store as the SDK opens it, on SQLite and
//! on PostgreSQL.
//!
//! Setup (not counted): the #818 probe's Store (`fs_call_round_trips.rs`):
//! Init of 10 × 100 files, a depth-7 file and a 2,000-line file, Branch
//! `work`, 100 `fs.write` Commits. Then each case runs on a **new Store
//! open** (`.cold`: every session connects and prepares at first use, no
//! object cached), and the same call once more on that open (`.warm`: what a
//! repeated call costs with today's 64 KiB object cache and the sessions'
//! prepared statements). Last, one `project.remove_file` puts a marker on
//! the project, and three `hidden.*` cases run cold with markers present,
//! so a skipped walk cannot hide a regression.
//!
//! Counts per role (support `round_trips.rs`): writer, control reader
//! (session 0, which serves every SDK read), Workspace readers; each
//! `[connects, round_trips, statements, transactions]`. The PostgreSQL half
//! needs `LAYERFS_TEST_POSTGRES_URL` and reports NOT_RUN without it; it also
//! prints the server's top-level `pg_stat_statements` calls per case
//! (`RT_COUNTS ... "server"`), a cross-check that is printed, not pinned.
//!
//! Relation to the probe's baseline (056e920166 receipts `sqlite.log`,
//! `pg.log`): the probe runs every read case one after another on the Store
//! open that wrote the history, so its numbers are neither cold nor warm
//! here. Measured at 3a904ac225:
//!
//! - `.warm` equals the probe's row for `stat.root`, `read.small_depth3`,
//!   `read.lines_100_200`, `list.root`, `glob.dir_100` and
//!   `grep.one_file_dir`, on both backends (PostgreSQL 18, 38, 30, 18, 30,
//!   23 trips; SQLite 39, 83, 66, 39, 66, 49 statements).
//! - The probe's `stat.depth3` (PostgreSQL 50, its 3 prepares; SQLite 95)
//!   was its first read after the writes and paid the upper levels' reads;
//!   `stat.depth7` (73 / 152), `read.2000_lines` (32 / 68), `list.dir_100`
//!   (33 / 69) and `glob.tree` (23 / 44) also ran with less cached than a
//!   repeat finds (`.warm`: 65, 30, 31, 20; 144, 66, 67, 41). `grep.tree_1k`
//!   (78 / 131) is half as much warm (38 / 77): the repeat finds the tree's
//!   directory objects in the object cache.
//! - `.cold` adds the control reader's first use on the new open (PostgreSQL:
//!   its connect and the prepare of each statement it runs, 17 to 29 trips
//!   above `.warm`; SQLite: 8 to 20 statements) and an empty object cache;
//!   `grep.tree_1k` is 73 trips (79 statements) above its warm repeat.
//!
//! A lane that changes these counts updates the case's numbers and says why
//! in the comment above the table, as `mount_round_trips.rs` does.
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_bridge::control::SdkValue;
use rt::{branch_arg as branch, path_arg as path, t, Backend, Counts, Measured, Pin, Store};

const NONE: Counts = [0, 0, 0, 0];

/// SQLite: `[0, 0, statements, transactions]` per role. Baseline,
/// 3a904ac225 (production source identical to 056e920166).
const SQLITE: &[Pin] = &[
    // probe stat.root: 39 statements
    pin("stat.root.cold", NONE, [0, 0, 47, 8], NONE),
    pin("stat.root.warm", NONE, [0, 0, 39, 8], NONE),
    // probe stat.depth3: 95
    pin("stat.depth3.cold", NONE, [0, 0, 95, 8], NONE),
    pin("stat.depth3.warm", NONE, [0, 0, 82, 8], NONE),
    // probe stat.depth7: 152
    pin("stat.depth7.cold", NONE, [0, 0, 161, 8], NONE),
    pin("stat.depth7.warm", NONE, [0, 0, 144, 8], NONE),
    // probe read.small_depth3: 83
    pin("read.small_depth3.cold", NONE, [0, 0, 96, 8], NONE),
    pin("read.small_depth3.warm", NONE, [0, 0, 83, 8], NONE),
    // probe read.2000_lines: 68
    pin("read.2000_lines.cold", NONE, [0, 0, 77, 8], NONE),
    pin("read.2000_lines.warm", NONE, [0, 0, 66, 8], NONE),
    // probe read.lines_100_200: 66
    pin("read.lines_100_200.cold", NONE, [0, 0, 77, 8], NONE),
    pin("read.lines_100_200.warm", NONE, [0, 0, 66, 8], NONE),
    // probe list.root: 39
    pin("list.root.cold", NONE, [0, 0, 48, 8], NONE),
    pin("list.root.warm", NONE, [0, 0, 39, 8], NONE),
    // probe list.dir_100: 69
    pin("list.dir_100.cold", NONE, [0, 0, 80, 8], NONE),
    pin("list.dir_100.warm", NONE, [0, 0, 67, 8], NONE),
    // probe glob.dir_100: 66
    pin("glob.dir_100.cold", NONE, [0, 0, 78, 8], NONE),
    pin("glob.dir_100.warm", NONE, [0, 0, 66, 8], NONE),
    // probe glob.tree: 44
    pin("glob.tree.cold", NONE, [0, 0, 61, 8], NONE),
    pin("glob.tree.warm", NONE, [0, 0, 41, 8], NONE),
    // probe grep.one_file_dir: 49
    pin("grep.one_file_dir.cold", NONE, [0, 0, 60, 8], NONE),
    pin("grep.one_file_dir.warm", NONE, [0, 0, 49, 8], NONE),
    // probe grep.tree_1k: 131
    pin("grep.tree_1k.cold", NONE, [0, 0, 156, 8], NONE),
    pin("grep.tree_1k.warm", NONE, [0, 0, 77, 8], NONE),
    // Markers present (one `project.remove_file` of /src/d09/f099.txt).
    pin("hidden.stat.depth3.cold", NONE, [0, 0, 95, 8], NONE),
    pin("hidden.list.dir_100.cold", NONE, [0, 0, 113, 8], NONE),
    pin("hidden.stat.removed.cold", NONE, [0, 0, 81, 6], NONE),
];

/// PostgreSQL: `[connects, round_trips, statements, transactions]` per
/// role. Baseline, 3a904ac225.
const POSTGRESQL: &[Pin] = &[
    // probe stat.root: 18 trips
    pin("stat.root.cold", NONE, [1, 35, 21, 2], NONE),
    pin("stat.root.warm", NONE, [0, 18, 14, 2], NONE),
    // probe stat.depth3: 50
    pin("stat.depth3.cold", NONE, [1, 59, 45, 2], NONE),
    pin("stat.depth3.warm", NONE, [0, 37, 33, 2], NONE),
    // probe stat.depth7: 73
    pin("stat.depth7.cold", NONE, [1, 91, 77, 2], NONE),
    pin("stat.depth7.warm", NONE, [0, 65, 61, 2], NONE),
    // probe read.small_depth3: 38
    pin("read.small_depth3.cold", NONE, [1, 60, 46, 2], NONE),
    pin("read.small_depth3.warm", NONE, [0, 38, 34, 2], NONE),
    // probe read.2000_lines: 32
    pin("read.2000_lines.cold", NONE, [1, 50, 36, 2], NONE),
    pin("read.2000_lines.warm", NONE, [0, 30, 26, 2], NONE),
    // probe read.lines_100_200: 30
    pin("read.lines_100_200.cold", NONE, [1, 50, 36, 2], NONE),
    pin("read.lines_100_200.warm", NONE, [0, 30, 26, 2], NONE),
    // probe list.root: 18
    pin("list.root.cold", NONE, [1, 36, 22, 2], NONE),
    pin("list.root.warm", NONE, [0, 18, 14, 2], NONE),
    // probe list.dir_100: 33
    pin("list.dir_100.cold", NONE, [1, 53, 39, 2], NONE),
    pin("list.dir_100.warm", NONE, [0, 31, 27, 2], NONE),
    // probe glob.dir_100: 30
    pin("glob.dir_100.cold", NONE, [1, 51, 37, 2], NONE),
    pin("glob.dir_100.warm", NONE, [0, 30, 26, 2], NONE),
    // probe glob.tree: 23
    pin("glob.tree.cold", NONE, [1, 49, 35, 2], NONE),
    pin("glob.tree.warm", NONE, [0, 20, 16, 2], NONE),
    // probe grep.one_file_dir: 23
    pin("grep.one_file_dir.cold", NONE, [1, 43, 29, 2], NONE),
    pin("grep.one_file_dir.warm", NONE, [0, 23, 19, 2], NONE),
    // probe grep.tree_1k: 78
    pin("grep.tree_1k.cold", NONE, [1, 111, 97, 2], NONE),
    pin("grep.tree_1k.warm", NONE, [0, 38, 34, 2], NONE),
    // Markers present.
    pin("hidden.stat.depth3.cold", NONE, [1, 59, 45, 2], NONE),
    pin("hidden.list.dir_100.cold", NONE, [1, 67, 53, 2], NONE),
    pin("hidden.stat.removed.cold", NONE, [1, 52, 39, 2], NONE),
];

const fn pin(case: &'static str, writer: Counts, control: Counts, workspace: Counts) -> Pin {
    Pin {
        case,
        writer,
        control,
        workspace,
    }
}

#[test]
fn sqlite_read_counts() {
    let mut store = Store::sqlite("reads-sqlite");
    let measured = scenarios(&mut store);
    rt::check("sdk_read_round_trips", Backend::Sqlite, &measured, SQLITE);
}

#[test]
fn postgresql_read_counts() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("reads") else {
        return;
    };
    let measured = scenarios(&mut store);
    rt::check(
        "sdk_read_round_trips",
        Backend::Postgresql,
        &measured,
        POSTGRESQL,
    );
}

fn scenarios(store: &mut Store) -> Vec<Measured> {
    rt::probe_store(store);
    let mut out = Vec::new();
    let mut case = |name: &str, method: &str, fields: Vec<(&'static str, SdkValue)>| {
        rt::cold_warm(
            store,
            &mut out,
            name,
            method,
            "ok",
            fields.clone(),
            Some(fields),
        );
    };
    case("stat.root", "fs.stat", vec![branch(), path("/")]);
    case(
        "stat.depth3",
        "fs.stat",
        vec![branch(), path("/src/d05/f050.txt")],
    );
    case(
        "stat.depth7",
        "fs.stat",
        vec![branch(), path("/deep/a/b/c/d/e/leaf.txt")],
    );
    case(
        "read.small_depth3",
        "fs.read",
        vec![branch(), path("/src/d05/f050.txt")],
    );
    case(
        "read.2000_lines",
        "fs.read",
        vec![branch(), path("/text/big.txt")],
    );
    case(
        "read.lines_100_200",
        "fs.read",
        vec![
            branch(),
            path("/text/big.txt"),
            ("startLine", SdkValue::Int(100)),
            ("endLine", SdkValue::Int(200)),
        ],
    );
    case("list.root", "fs.list", vec![branch(), path("/")]);
    case(
        "list.dir_100",
        "fs.list",
        vec![branch(), path("/src/d05"), ("pageSize", SdkValue::Int(100))],
    );
    case(
        "glob.dir_100",
        "fs.glob",
        vec![branch(), path("/src/d05"), ("pattern", t("f0*"))],
    );
    case(
        "glob.tree",
        "fs.glob",
        vec![branch(), path("/"), ("pattern", t("*"))],
    );
    case(
        "grep.one_file_dir",
        "fs.grep",
        vec![branch(), path("/text"), ("pattern", t("needle"))],
    );
    case(
        "grep.tree_1k",
        "fs.grep",
        vec![branch(), path("/"), ("pattern", t("needle"))],
    );

    // A marker on the project (not counted), then reads with it present.
    let opened = store.open("remove");
    let removed = opened
        .call("project.remove_file", vec![path("/src/d09/f099.txt")])
        .unwrap();
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    opened.stop();
    rt::cold_warm(
        store,
        &mut out,
        "hidden.stat.depth3",
        "fs.stat",
        "ok",
        vec![branch(), path("/src/d05/f050.txt")],
        None,
    );
    rt::cold_warm(
        store,
        &mut out,
        "hidden.list.dir_100",
        "fs.list",
        "ok",
        vec![branch(), path("/src/d09"), ("pageSize", SdkValue::Int(100))],
        None,
    );
    rt::cold_warm(
        store,
        &mut out,
        "hidden.stat.removed",
        "fs.stat",
        "refused Removed",
        vec![branch(), path("/src/d09/f099.txt")],
        None,
    );
    out
}
