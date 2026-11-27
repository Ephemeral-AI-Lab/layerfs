//! #818 history pins (lane RH owns this file): the counted SQL work of
//! `fs.blame`, `fs.revision` and `fs.diff_files` through the method table on
//! the Store as the SDK opens it, on SQLite and on PostgreSQL.
//!
//! Setup (not counted): the #818 probe's Store (`fs_call_round_trips.rs`):
//! Init, Branch `work`, then 100 `fs.write` Commits alternating
//! `/hist/a.txt` (one line appended each time) and `/hist/other.txt`. `hN`
//! is the history up to the N-th of those Commits (V = N Commits on the
//! Branch above its base Layer). Each case runs on a **new Store open**
//! (`.cold`) and then once more on that open (`.warm`), except
//! `revision.h10`, which is there only for the formula check.
//!
//! Counts per role (support `round_trips.rs`): writer, control reader
//! (session 0), Workspace readers; each `[connects, round_trips, statements,
//! transactions]`. The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and
//! reports NOT_RUN without it; it prints the server's top-level calls per
//! case beside the counts (printed, not pinned).
//!
//! **Formula check** (verification.md): the cold PostgreSQL round trips of
//! `blame` and `revision` (all roles) at V = 10 and V = 100 must stay within
//! `F + ceil(V / W) * K`. At baseline every version costs its own reads
//! (W = 1); lane H's windows set W (16, 32, 64, then 128) and K.
//!
//! Relation to the probe's baseline (056e920166 `sqlite.log`, `pg.log`):
//! the probe runs these cases one after another on the open that wrote the
//! history. Measured at 3a904ac225: `revision.h100.warm` equals the probe's
//! row on both backends (PostgreSQL 3,890 trips, SQLite 12,658 statements);
//! the probe's `blame.h10` (267 / 714) was its group's first case (5
//! prepares) and sits between `.cold` (287 / 725) and `.warm` (246 / 684);
//! its `blame.h100` (2,325 / 7,575) ran after `blame.h10` with part of the
//! history cached (`.warm` 2,229 / 7,349); its `diff_files` (88 / 198) ran
//! last in the reads group (`.warm` 83 / 193). `.cold` adds the control
//! reader's first use (connect, prepares) and an empty object cache.
//! `revision.h10` is new at P0 (the probe now appends it to its history
//! group).
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_bridge::control::SdkValue;
use rt::{branch_arg as branch, path_arg as path, t, Backend, Counts, Measured, Pin, Store};

const NONE: Counts = [0, 0, 0, 0];

/// SQLite: `[0, 0, statements, transactions]` per role. Baseline,
/// 3a904ac225 (production source identical to 056e920166); the `probe`
/// comments keep the baseline row.
///
/// - #818 R2 (dedupe): `blame` reads its Commit row once and the serial
///   with the markers; each `diff_files` side reads its Commit row once:
///   2 reads fewer (−6 statements, −2 transactions); `revision` unchanged.
/// - #818 H1 (dedupe): one per-call `Rows`: each walked row is read once
///   (as the parent, then carried), Branch names once per call and on the
///   control reader (they were read on the writer's session: writer 597 →
///   0 on `blame.h100`), owners' and items' references from held rows, and
///   blame's text carried between comparisons. `blame.h100` warm control
///   6,746 → 5,829, `blame.h10` 621 → 548, `revision.h100` 12,382 → 10,483,
///   `revision.h10` cold 1,325 → 1,106; writer reads 0 on all four;
///   `diff_files` unchanged.
/// - #818 H2 (dedupe): `revision` walks each node's path once (a node's
///   parent side, entry and serial, is the next node's own): `revision.h100`
///   warm 10,483 → 5,797, cold 10,827 → 6,141; `revision.h10` cold 1,106 →
///   634; `blame` and `diff_files` unchanged.
/// - #818 H3 (dedupe): every version's objects through one call cache, and
///   names bound through a per-call directory memo; `diff_files` keeps its
///   strict lookup: warm `blame.h100` 5,829 → 779, `blame.h10` 548 → 91,
///   `revision.h100` 5,797 → 769, `diff_files` 187 → 71; cold alike
///   (`revision.h10` 634 → 147).
const SQLITE: &[Pin] = &[
    // probe blame.h10: 714 statements
    pin("blame.h10.cold", NONE, [0, 0, 132, 24], NONE),
    pin("blame.h10.warm", NONE, [0, 0, 91, 24], NONE),
    // probe blame.h100: 7,575
    pin("blame.h100.cold", NONE, [0, 0, 1135, 205], NONE),
    pin("blame.h100.warm", NONE, [0, 0, 779, 205], NONE),
    // probe revision.h10 (appended at P0): 1,314
    pin("revision.h10.cold", NONE, [0, 0, 147, 29], NONE),
    // probe revision.h100: 12,658
    pin("revision.h100.cold", NONE, [0, 0, 1113, 203], NONE),
    pin("revision.h100.warm", NONE, [0, 0, 769, 203], NONE),
    // probe diff_files.h10_vs_h99: 198
    pin("diff_files.h10_vs_h99.cold", NONE, [0, 0, 102, 15], NONE),
    pin("diff_files.h10_vs_h99.warm", NONE, [0, 0, 71, 15], NONE),
];

/// PostgreSQL: `[connects, round_trips, statements, transactions]` per
/// role. Baseline, 3a904ac225; the `probe` comments keep the baseline row.
///
/// - #818 R1: each Branch-by-name and marker read is one fenced statement,
///   no read transaction (3 trips before, 1 after): `blame.h10` −2 (one
///   marker read), `blame.h100` and `revision.h100` −4 (Branch and markers),
///   `revision.h10` −6 (its Branch, its markers and one more Branch lookup),
///   `diff_files` −4 (one marker read per side); statements unchanged.
/// - #818 R2: `blame` and `diff_files` −2 round trips and −2 statements
///   (see SQLITE), cold and warm; `revision` unchanged.
/// - #818 R5: `blame` resolves through the one context statement:
///   `blame.h10` (a full Commit id: project, Commit, markers) −2 warm, −3
///   cold; `blame.h100` (a Branch: project, Branch, Commit, markers) −3
///   warm, −5 cold (prepares of the statements it no longer sends);
///   `revision` and `diff_files` unchanged.
/// - #818 H1 (dedupe, see SQLITE): writer 0 (Branch names now read once per
///   call on the control reader); control warm `blame.h100` 2,021 → 1,768,
///   `blame.h10` 221 → 193, `revision.h100` 3,794 → 3,161; cold alike
///   (`revision.h10` 493 → 420); `diff_files` unchanged.
/// - #818 H2 (dedupe, see SQLITE): one path walk and serial per node fewer:
///   `revision.h100` warm 3,161 → 1,705, cold 3,389 → 1,933;
///   `revision.h10` cold 420 → 260.
/// - #818 H3 (dedupe, see SQLITE): a version's objects mostly come from
///   packs the call already holds: warm `blame.h100` 1,768 → 254,
///   `blame.h10` 193 → 33, `revision.h100` 1,705 → 253, `diff_files` 77 →
///   23; cold `blame.h100` 2,005 → 491, `revision.h10` 260 → 82.
/// - #818 H4 (batch): the walk's Commit rows and serials in windows of 16,
///   32, 64 then 128 (`lineage_chain_window`): warm `blame.h100` 254 → 59,
///   `blame.h10` 33 → 16, `revision.h100` 253 → 72; cold `blame.h100`
///   491 → 296, `blame.h10` 71 → 54, `revision.h100` 481 → 301,
///   `revision.h10` 82 → 66 (one more prepare); `diff_files` unchanged.
///   SQLite reads no window: its pins are unchanged.
const POSTGRESQL: &[Pin] = &[
    // probe blame.h10: 267 trips
    pin("blame.h10.cold", NONE, [1, 54, 46, 0], NONE),
    pin("blame.h10.warm", NONE, [0, 16, 16, 0], NONE),
    // probe blame.h100: 2,325
    pin("blame.h100.cold", NONE, [1, 296, 288, 0], NONE),
    pin("blame.h100.warm", NONE, [0, 59, 59, 0], NONE),
    // probe revision.h10 (appended at P0)
    pin("revision.h10.cold", NONE, [1, 66, 54, 0], NONE),
    // probe revision.h100: 3,890
    pin("revision.h100.cold", NONE, [1, 301, 289, 0], NONE),
    pin("revision.h100.warm", NONE, [0, 72, 72, 0], NONE),
    // probe diff_files.h10_vs_h99: 88
    pin("diff_files.h10_vs_h99.cold", NONE, [1, 49, 40, 0], NONE),
    pin("diff_files.h10_vs_h99.warm", NONE, [0, 23, 23, 0], NONE),
];

/// `(method, F, W, K)`: cold PostgreSQL round trips at V versions are at
/// most `F + ceil(V / W) * K`. Baseline: W = 1, K the per-version slope
/// between V = 10 and V = 100 rounded up, F what is left at V = 10.
///
/// Baseline (3a904ac225, cold): blame 287 trips at V = 10 and 2,470 at
/// V = 100, so K = ceil(2,183 / 90) = 25 and F = 287 - 10 x 25 = 37;
/// revision 510 and 4,119, so K = ceil(3,609 / 90) = 41 and F = 510 - 410 =
/// 100.
const FORMULA: &[(&str, u64, u64, u64)] = &[("blame", 37, 1, 25), ("revision", 100, 1, 41)];

const fn pin(case: &'static str, writer: Counts, control: Counts, workspace: Counts) -> Pin {
    Pin {
        case,
        writer,
        control,
        workspace,
    }
}

#[test]
fn sqlite_history_counts() {
    let mut store = Store::sqlite("history-sqlite");
    let measured = scenarios(&mut store);
    rt::check(
        "sdk_history_round_trips",
        Backend::Sqlite,
        &measured,
        SQLITE,
    );
}

#[test]
fn postgresql_history_counts() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("history") else {
        return;
    };
    let measured = scenarios(&mut store);
    rt::check(
        "sdk_history_round_trips",
        Backend::Postgresql,
        &measured,
        POSTGRESQL,
    );
    formula(&measured);
}

/// The formula check on the cold cases at V = 10 and V = 100.
fn formula(measured: &[Measured]) {
    let trips = |case: &str| -> u64 {
        let m = measured
            .iter()
            .find(|m| m.case == case)
            .unwrap_or_else(|| panic!("no case {case}"));
        m.roles.iter().map(|r| r[1]).sum()
    };
    for (method, f, w, k) in FORMULA {
        for v in [10u64, 100] {
            let measured = trips(&format!("{method}.h{v}.cold"));
            let bound = f + v.div_ceil(*w) * k;
            println!(
                "RT_FORMULA {{\"method\":\"{method}\",\"versions\":{v},\"round_trips\":{measured},\"bound\":{bound},\"f\":{f},\"w\":{w},\"k\":{k}}}"
            );
            assert!(
                measured <= bound,
                "{method} at V = {v}: {measured} round trips > F + ceil(V/W)*K = {bound}"
            );
        }
    }
}

fn scenarios(store: &mut Store) -> Vec<Measured> {
    let ids = rt::probe_store(store);
    let at = |n: usize| ("commit", t(&ids[n - 1]));
    let a = || path("/hist/a.txt");
    let mut out = Vec::new();
    let mut case = |name: &str, method: &str, fields: Vec<(&'static str, SdkValue)>, warm: bool| {
        let again = warm.then(|| fields.clone());
        rt::cold_warm(store, &mut out, name, method, "", fields, again);
    };
    case("blame.h10", "fs.blame", vec![branch(), a(), at(10)], true);
    case("blame.h100", "fs.blame", vec![branch(), a()], true);
    case(
        "revision.h10",
        "fs.revision",
        vec![branch(), a(), at(10)],
        false,
    );
    case("revision.h100", "fs.revision", vec![branch(), a()], true);
    let selector = |id: &str| SdkValue::map([("commit", t(id)), ("path", t("/hist/a.txt"))]);
    case(
        "diff_files.h10_vs_h99",
        "fs.diff_files",
        vec![("left", selector(&ids[9])), ("right", selector(&ids[98]))],
        true,
    );
    for m in &out {
        assert!(
            !m.outcome.starts_with("refused"),
            "{}: {}",
            m.case,
            m.outcome
        );
    }
    out
}
