//! #818 write pins (lane W owns this file): the counted SQL work of the SDK
//! write calls `fs.write`, `fs.edit`, `fs.mkdir`, `fs.cp`, `fs.mv`, `fs.rm`
//! and `fs.apply`, through the method table on the Store as the SDK opens
//! it, on SQLite and on PostgreSQL. Each call binds a private Workspace,
//! commits once (the fused one-message Commit) and closes it.
//!
//! Setup (not counted): the #818 probe's Store (`fs_call_round_trips.rs`):
//! Init of 10 × 100 files and the rest, Branch `work`, 100 `fs.write`
//! Commits. Then, in the probe's order, each case runs on a **new Store
//! open** (`.cold`: first use of every session and statement, and for a
//! create the Store's first serial reservation, `POOL_BLOCK` = 4,096), and
//! the same kind of change at another path once more on that open (`.warm`:
//! steady state; prepares done, serials from the pool). First use and
//! steady state are named apart, as verification.md asks.
//!
//! Counts per role (support `round_trips.rs`): writer (with its lane
//! connection), control reader (session 0: project and Branch lookup, the
//! hidden-path setup), Workspace readers (the private Workspace's reader);
//! each `[connects, round_trips, statements, transactions]`. The PostgreSQL
//! half needs `LAYERFS_TEST_POSTGRES_URL` and reports NOT_RUN without it; it
//! prints the server's top-level calls per case (printed, not pinned).
//! writes.md W0 attributes these trips by phase: the probe's `w0` group
//! (receipts `P0/*-w0.log`, RECORD.md Phase 0a).
//!
//! Relation to the probe's baseline (056e920166 `sqlite.log`, `pg.log`):
//! the probe runs the cases one after another on the open that wrote the
//! history, so its `write.new_file` is the first create of its process
//! (1 connect, 12 prepares) and its other cases are warm. Measured at
//! 3a904ac225 (PostgreSQL trips; SQLite statements):
//!
//! - `cp.file.warm` equals the probe (44); `edit` (23 vs 24), `mkdir` (45 vs
//!   46), `rm.file` (37 vs 39), `apply` (46 vs 44), `overwrite` (24 vs 22)
//!   and `rm.dir` (151 vs 148) are within 3 of `.warm`; `mv.file` is 40 in
//!   the probe and 45 warm here (the probe's `mv` moved the file its `cp`
//!   had just written through the same open). SQLite `.warm` is within 13
//!   statements of the probe except `mv.file` (85 vs 60): it repeats a kind
//!   of change, the probe runs a sequence of different kinds.
//! - The probe's `write.new_file` (66 / 77) is a first create on an open
//!   whose sessions were already warm from the history; `.cold` here (112 /
//!   152) also opens the control and Workspace readers and prepares on
//!   every session, `.warm` (44 / 75) is a second create.
//! - Every `.cold` case pays the control reader's and the Workspace
//!   reader's connects and first prepares (PostgreSQL control [1, 17, 7, 2]
//!   against [0, 10, 6, 2] warm at the baseline).
//!
//! Pins moved by lane W (lanes/W.md has prediction and measurement):
//! - W1: the hidden-path check answers from the bind's kept markers, so the
//!   control reader's marker, serial and base-tree reads go: control
//!   PostgreSQL [1, 17, 7, 2] -> [1, 7, 3, 1] cold, [0, 10, 6, 2] ->
//!   [0, 4, 2, 1] warm; SQLite [0, 0, 14, 4] -> [0, 0, 6, 2].
//! - W2: the private bind finds project and Branch by name with its
//!   snapshot, so the control reader serves no write call: control
//!   [0, 0, 0, 0] on both backends (its cold connect goes too). A cold
//!   Commit on the separate path pays one more writer trip: its `head` read
//!   is now the first use of the by-id snapshot statement (a prepare).
//! - W3: the Store keeps the logical size of the node its Workspaces last
//!   read or published, so every `.warm` case (bound to the Commit its
//!   `.cold` call published on the same open) skips `logical.known`:
//!   writer PostgreSQL -1 trip and statement, SQLite -3 statements and
//!   -1 transaction; `.cold` unchanged (a new Store open).
//! - W5: the recursive remove takes its listed page's kinds in one inode
//!   lookup instead of a stat per child: `rm.dir_100_recursive` Workspace
//!   reader PostgreSQL [1, 123, 121, 1] -> [1, 24, 22, 1] cold and
//!   [0, 109, 109, 0] -> [0, 10, 10, 0] warm; SQLite 169 -> 70 and
//!   116 -> 17 statements.
//! - W6: new case `apply.10_dirs` (a new file in each of ten directories
//!   on seven levels), pinned at the M1 tip as Workspace reader PostgreSQL
//!   [1, 58, 56, 1] cold, [0, 16, 16, 0] warm; SQLite 113 and 27. The
//!   call plans its paths' base reads level by level on PostgreSQL
//!   (-> [1, 34, 32, 1] and [0, 8, 8, 0]; `cp.file.cold` 29 -> 28) and
//!   walks the directories it passes by kind only there. SQLite plans
//!   nothing and keeps the full-stat walk: every SQLite pin is unchanged
//!   (SPEC §3 guardrail; a kind-only walk there cost `write.overwrite` and
//!   `cp.file` cold 2 statements each).
//! - W4: the SDK's PostgreSQL Store open prepares the create statements in
//!   one flight (`OpenedStore::prepare_sdk_writes`; the harness's open does
//!   too), and the serial reservation is one message. Every `.cold` writer
//!   pays fewer prepares: -16 trips for a create that reserves serials
//!   (`write.new_file`, `mkdir`, `cp.file`, `apply.10_*`; +2 statements,
//!   the message's simple texts), -11 for `mv.file`, -10 for `rm.*`, -1
//!   for the fused `write.overwrite` and `edit` (the Save's block
//!   allocation). `.warm` and SQLite unchanged.
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_bridge::control::SdkValue;
use rt::{branch_arg as branch, path_arg as path, t, Backend, Counts, Measured, Pin, Store};

/// SQLite: `[0, 0, statements, transactions]` per role. Baseline,
/// 3a904ac225 (production source identical to 056e920166), then lane W's
/// items (module comment).
const SQLITE: &[Pin] = &[
    // probe write.new_file: 77 statements
    pin(
        "write.new_file.cold",
        [0, 0, 74, 12],
        [0, 0, 0, 0],
        [0, 0, 64, 4],
    ),
    pin(
        "write.new_file.warm",
        [0, 0, 44, 7],
        [0, 0, 0, 0],
        [0, 0, 14, 2],
    ),
    // probe write.overwrite: 54
    pin(
        "write.overwrite.cold",
        [0, 0, 61, 7],
        [0, 0, 0, 0],
        [0, 0, 73, 4],
    ),
    pin(
        "write.overwrite.warm",
        [0, 0, 37, 3],
        [0, 0, 0, 0],
        [0, 0, 8, 2],
    ),
    // probe edit.2000_lines: 52
    pin(
        "edit.2000_lines.cold",
        [0, 0, 67, 6],
        [0, 0, 0, 0],
        [0, 0, 70, 4],
    ),
    pin(
        "edit.2000_lines.warm",
        [0, 0, 34, 3],
        [0, 0, 0, 0],
        [0, 0, 9, 2],
    ),
    // probe mkdir: 69
    pin("mkdir.cold", [0, 0, 79, 11], [0, 0, 0, 0], [0, 0, 65, 4]),
    pin("mkdir.warm", [0, 0, 48, 7], [0, 0, 0, 0], [0, 0, 17, 2]),
    // probe cp.file: 64
    pin("cp.file.cold", [0, 0, 75, 11], [0, 0, 0, 0], [0, 0, 86, 4]),
    pin("cp.file.warm", [0, 0, 47, 7], [0, 0, 0, 0], [0, 0, 9, 2]),
    // probe mv.file: 60
    pin("mv.file.cold", [0, 0, 79, 10], [0, 0, 0, 0], [0, 0, 81, 4]),
    pin("mv.file.warm", [0, 0, 51, 7], [0, 0, 0, 0], [0, 0, 17, 2]),
    // probe rm.file: 58
    pin("rm.file.cold", [0, 0, 85, 9], [0, 0, 0, 0], [0, 0, 87, 4]),
    pin("rm.file.warm", [0, 0, 35, 6], [0, 0, 0, 0], [0, 0, 8, 2]),
    // probe apply.10_writes: 74
    pin(
        "apply.10_writes.cold",
        [0, 0, 93, 12],
        [0, 0, 0, 0],
        [0, 0, 77, 4],
    ),
    pin(
        "apply.10_writes.warm",
        [0, 0, 58, 7],
        [0, 0, 0, 0],
        [0, 0, 9, 2],
    ),
    // probe rm.dir_100_recursive: 179
    pin(
        "rm.dir_100_recursive.cold",
        [0, 0, 68, 9],
        [0, 0, 0, 0],
        [0, 0, 70, 4],
    ),
    pin(
        "rm.dir_100_recursive.warm",
        [0, 0, 44, 6],
        [0, 0, 0, 0],
        [0, 0, 17, 2],
    ),
    pin(
        "apply.10_dirs.cold",
        [0, 0, 88, 12],
        [0, 0, 0, 0],
        [0, 0, 113, 4],
    ),
    pin(
        "apply.10_dirs.warm",
        [0, 0, 53, 7],
        [0, 0, 0, 0],
        [0, 0, 27, 2],
    ),
];

/// PostgreSQL: `[connects, round_trips, statements, transactions]` per
/// role. Baseline, 3a904ac225, then lane W's items (module comment).
const POSTGRESQL: &[Pin] = &[
    // probe write.new_file: 66 trips (its process's first create)
    pin(
        "write.new_file.cold",
        [1, 60, 43, 9],
        [0, 0, 0, 0],
        [1, 20, 18, 1],
    ),
    pin(
        "write.new_file.warm",
        [0, 28, 24, 4],
        [0, 0, 0, 0],
        [0, 5, 5, 0],
    ),
    // probe write.overwrite: 24
    pin(
        "write.overwrite.cold",
        [0, 36, 34, 6],
        [0, 0, 0, 0],
        [1, 21, 19, 1],
    ),
    pin(
        "write.overwrite.warm",
        [0, 9, 23, 2],
        [0, 0, 0, 0],
        [0, 2, 2, 0],
    ),
    // probe edit.2000_lines: 23
    pin(
        "edit.2000_lines.cold",
        [0, 33, 34, 5],
        [0, 0, 0, 0],
        [1, 16, 14, 1],
    ),
    pin(
        "edit.2000_lines.warm",
        [0, 11, 25, 2],
        [0, 0, 0, 0],
        [0, 2, 2, 0],
    ),
    // probe mkdir: 45
    pin("mkdir.cold", [1, 62, 48, 8], [0, 0, 0, 0], [1, 19, 17, 1]),
    pin("mkdir.warm", [0, 31, 27, 4], [0, 0, 0, 0], [0, 4, 4, 0]),
    // probe cp.file: 44
    pin("cp.file.cold", [1, 59, 45, 8], [0, 0, 0, 0], [1, 28, 26, 1]),
    pin("cp.file.warm", [0, 30, 26, 4], [0, 0, 0, 0], [0, 3, 3, 0]),
    // probe mv.file: 40
    pin("mv.file.cold", [1, 58, 41, 7], [0, 0, 0, 0], [1, 20, 18, 1]),
    pin("mv.file.warm", [0, 30, 26, 4], [0, 0, 0, 0], [0, 4, 4, 0]),
    // probe rm.file: 37
    pin("rm.file.cold", [1, 56, 41, 6], [0, 0, 0, 0], [1, 20, 18, 1]),
    pin("rm.file.warm", [0, 26, 24, 3], [0, 0, 0, 0], [0, 2, 2, 0]),
    // probe apply.10_writes: 46
    pin(
        "apply.10_writes.cold",
        [1, 65, 48, 9],
        [0, 0, 0, 0],
        [1, 25, 23, 1],
    ),
    pin(
        "apply.10_writes.warm",
        [0, 31, 27, 4],
        [0, 0, 0, 0],
        [0, 2, 2, 0],
    ),
    // probe rm.dir_100_recursive: 151
    pin(
        "rm.dir_100_recursive.cold",
        [1, 58, 42, 6],
        [0, 0, 0, 0],
        [1, 24, 22, 1],
    ),
    pin(
        "rm.dir_100_recursive.warm",
        [0, 28, 26, 3],
        [0, 0, 0, 0],
        [0, 10, 10, 0],
    ),
    pin(
        "apply.10_dirs.cold",
        [1, 64, 46, 9],
        [0, 0, 0, 0],
        [1, 34, 32, 1],
    ),
    pin(
        "apply.10_dirs.warm",
        [0, 32, 28, 4],
        [0, 0, 0, 0],
        [0, 8, 8, 0],
    ),
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
fn sqlite_write_counts() {
    let mut store = Store::sqlite("writes-sqlite");
    let measured = scenarios(&mut store);
    rt::check("sdk_write_round_trips", Backend::Sqlite, &measured, SQLITE);
}

#[test]
fn postgresql_write_counts() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("writes") else {
        return;
    };
    let measured = scenarios(&mut store);
    rt::check(
        "sdk_write_round_trips",
        Backend::Postgresql,
        &measured,
        POSTGRESQL,
    );
}

fn bytes(text: &str) -> (&'static str, SdkValue) {
    ("bytes", SdkValue::Bytes(text.as_bytes().to_vec()))
}

fn apply(dir: &str, prefix: &str) -> Vec<(&'static str, SdkValue)> {
    let changes: Vec<SdkValue> = (0..10)
        .map(|n| {
            SdkValue::map([
                ("op", t("write")),
                ("path", t(&format!("{dir}/{prefix}{n}.txt"))),
                (
                    "bytes",
                    SdkValue::Bytes(format!("{prefix}{n}\n").into_bytes()),
                ),
            ])
        })
        .collect();
    vec![branch(), ("changes", SdkValue::List(changes))]
}

/// Ten new files `name`, one in each of ten directories the earlier cases
/// left in place (#818 W6: an `apply` across directories).
fn apply_dirs(name: &str) -> Vec<(&'static str, SdkValue)> {
    let dirs = [
        "/src/d00",
        "/src/d01",
        "/src/d02",
        "/src/d03",
        "/src/d04",
        "/src/d05",
        "/src/d06",
        "/src/d09",
        "/deep/a/b/c/d/e",
        "/hist",
    ];
    let changes: Vec<SdkValue> = dirs
        .iter()
        .map(|dir| {
            SdkValue::map([
                ("op", t("write")),
                ("path", t(&format!("{dir}/{name}"))),
                ("bytes", SdkValue::Bytes(format!("{dir}\n").into_bytes())),
            ])
        })
        .collect();
    vec![branch(), ("changes", SdkValue::List(changes))]
}

fn scenarios(store: &mut Store) -> Vec<Measured> {
    rt::probe_store(store);
    let mut out = Vec::new();
    let mut case = |name: &str, method: &str, cold, warm| {
        rt::cold_warm(store, &mut out, name, method, "Committed", cold, Some(warm));
    };
    case(
        "write.new_file",
        "fs.write",
        vec![branch(), path("/src/d05/new.txt"), bytes("new\n")],
        vec![branch(), path("/src/d05/new2.txt"), bytes("new2\n")],
    );
    case(
        "write.overwrite",
        "fs.write",
        vec![branch(), path("/src/d05/f050.txt"), bytes("changed\n")],
        vec![
            branch(),
            path("/src/d05/f050.txt"),
            bytes("changed again\n"),
        ],
    );
    case(
        "edit.2000_lines",
        "fs.edit",
        vec![
            branch(),
            path("/text/big.txt"),
            ("oldText", t("line 1000\n")),
            ("newText", t("LINE 1000\n")),
        ],
        vec![
            branch(),
            path("/text/big.txt"),
            ("oldText", t("line 1001\n")),
            ("newText", t("LINE 1001\n")),
        ],
    );
    case(
        "mkdir",
        "fs.mkdir",
        vec![branch(), path("/newdir")],
        vec![branch(), path("/newdir2")],
    );
    case(
        "cp.file",
        "fs.cp",
        vec![
            branch(),
            ("from", t("/src/d05/f051.txt")),
            ("to", t("/newdir/copy.txt")),
        ],
        vec![
            branch(),
            ("from", t("/src/d05/f052.txt")),
            ("to", t("/newdir/copy2.txt")),
        ],
    );
    case(
        "mv.file",
        "fs.mv",
        vec![
            branch(),
            ("from", t("/newdir/copy.txt")),
            ("to", t("/newdir/moved.txt")),
        ],
        vec![
            branch(),
            ("from", t("/newdir/copy2.txt")),
            ("to", t("/newdir/moved2.txt")),
        ],
    );
    case(
        "rm.file",
        "fs.rm",
        vec![branch(), path("/newdir/moved.txt")],
        vec![branch(), path("/newdir/moved2.txt")],
    );
    case(
        "apply.10_writes",
        "fs.apply",
        apply("/src/d06", "a"),
        apply("/src/d06", "b"),
    );
    case(
        "rm.dir_100_recursive",
        "fs.rm",
        vec![
            branch(),
            path("/src/d07"),
            ("recursive", SdkValue::Bool(true)),
        ],
        vec![
            branch(),
            path("/src/d08"),
            ("recursive", SdkValue::Bool(true)),
        ],
    );
    case(
        "apply.10_dirs",
        "fs.apply",
        apply_dirs("p.txt"),
        apply_dirs("q.txt"),
    );
    out
}
