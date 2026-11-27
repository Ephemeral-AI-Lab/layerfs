//! #818 R6: `fs.grep` and `fs.glob` on a large-file tree, pinned on both
//! backends: the probe's tree (10 folders of 100 files, a 2,000-line text)
//! plus `/big/` with a small file and four text files of 1, 4, 16 and
//! 64 MiB on Branch `work`, each chunked and ending in one `needle` line.
//! R6 widens grep's and glob's windows on PostgreSQL (byte bounds
//! unchanged); this shape shows the big files' reads are not moved by it
//! and, measured with `/usr/bin/time -l`, that the peak resident size does
//! not grow beyond the per-demand byte bound.
//!
//! `LAYERFS_RH_LARGE_SETUP_ONLY=1` builds and opens the Store without the
//! measured calls, for the peak of the setup alone (a memory receipt).
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_protocol::provision::install::SdkValue;
use rt::{branch_arg as branch, path_arg as path, t, Backend, Counts, Measured, Pin, Store};

const NONE: Counts = [0, 0, 0, 0];

/// `[connects, round_trips, statements, transactions]` by role.
///
/// `grep.large` reads the big files' chunks through the reader's bounded
/// caches, and its round trips and statements vary a little from run to
/// run with the same code and the same Store (observed: SQLite warm 266 and
/// 267; with the big files written by Init instead, SQLite cold 289–293 and
/// warm 257–265, PostgreSQL warm 248–249); the cause is not diagnosed. They
/// are held within [`SPREAD`] of the pin; connects, transactions and every
/// other case are exact.
///
/// - #818 R6 (batch), measured once with R6 off and once on: PostgreSQL
///   `grep.large` cold 321 → 291, warm 280 → 255 (the 1k-file part's wider
///   folder pages and root windows; the big files' chunk demands are
///   unchanged); `glob.large` 21 / 5 unchanged (one five-entry folder).
///   SQLite unchanged (307 / 266–267, 40 / 22). Peak resident size of the
///   PostgreSQL test process 242.24 → 242.22 MB (setup alone 238.93 MB).
const SQLITE: &[Pin] = &[
    pin("grep.large.cold", NONE, [0, 0, 307, 5], NONE),
    pin("grep.large.warm", NONE, [0, 0, 266, 5], NONE),
    pin("glob.large.cold", NONE, [0, 0, 40, 5], NONE),
    pin("glob.large.warm", NONE, [0, 0, 22, 5], NONE),
];

const POSTGRESQL: &[Pin] = &[
    pin("grep.large.cold", NONE, [1, 291, 286, 0], NONE),
    pin("grep.large.warm", NONE, [0, 255, 255, 0], NONE),
    pin("glob.large.cold", NONE, [1, 21, 16, 0], NONE),
    pin("glob.large.warm", NONE, [0, 5, 5, 0], NONE),
];

/// The run-to-run spread allowed on `grep.large`'s round trips and
/// statements: the widest observed range (8, SQLite warm 257–265).
const SPREAD: u64 = 8;

const fn pin(case: &'static str, writer: Counts, control: Counts, workspace: Counts) -> Pin {
    Pin {
        case,
        writer,
        control,
        workspace,
    }
}

#[test]
fn sqlite_large_counts() {
    let mut store = Store::sqlite("large-sqlite");
    if let Some(measured) = scenarios(&mut store) {
        check(Backend::Sqlite, &measured, SQLITE);
    }
}

#[test]
fn postgresql_large_counts() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("large") else {
        return;
    };
    if let Some(measured) = scenarios(&mut store) {
        check(Backend::Postgresql, &measured, POSTGRESQL);
    }
}

/// Compares each case with its pin: exact, except `grep.large`'s round
/// trips and statements, within [`SPREAD`].
fn check(backend: Backend, measured: &[Measured], pins: &[Pin]) {
    let cases: Vec<&str> = measured.iter().map(|m| m.case.as_str()).collect();
    let pinned: Vec<&str> = pins.iter().map(|p| p.case).collect();
    assert_eq!(cases, pinned, "{} cases", backend.name());
    let mut wrong = Vec::new();
    for (m, pin) in measured.iter().zip(pins) {
        let spread = if m.case.starts_with("grep.large") {
            SPREAD
        } else {
            0
        };
        let expected = [pin.writer, pin.control, pin.workspace];
        let fits = m.roles.iter().zip(expected).all(|(got, want)| {
            got[0] == want[0]
                && got[3] == want[3]
                && got[1].abs_diff(want[1]) <= spread
                && got[2].abs_diff(want[2]) <= spread
        });
        println!(
            "{} {}: measured {:?}, pinned {:?}",
            backend.name(),
            m.case,
            m.roles,
            expected
        );
        if !fits {
            wrong.push(m.case.clone());
        }
    }
    assert!(
        wrong.is_empty(),
        "{}: off their pins: {wrong:?}",
        backend.name()
    );
}

/// A text of about `mib` MiB of numbered lines, the last one `needle`.
fn big(mib: usize) -> Vec<u8> {
    let (mut text, mut n) = (Vec::with_capacity((mib << 20) + 64), 0u64);
    while text.len() < mib << 20 {
        text.extend_from_slice(format!("line {n} of a large text file\n").as_bytes());
        n += 1;
    }
    text.extend_from_slice(b"needle at the end\n");
    text
}

fn scenarios(store: &mut Store) -> Option<Vec<Measured>> {
    let source = store.dir.join("source");
    rt::build_tree(&source);
    std::fs::create_dir_all(source.join("big")).unwrap();
    std::fs::write(source.join("big/small.txt"), "small\n").unwrap();
    store.init(&source, &mut |_, _| {});
    // The big files are written after Init, each one Commit with its one
    // construction producer, rather than packed by Init's parallel workers
    // in the order they finish.
    let opened = store.open("setup");
    rt::fork_work(&opened);
    for mib in [1, 4, 16, 64] {
        opened
            .call(
                "fs.write",
                vec![
                    branch(),
                    path(&format!("/big/b{mib:02}.txt")),
                    ("bytes", SdkValue::Bytes(big(mib))),
                ],
            )
            .unwrap();
    }
    opened.stop();
    if std::env::var_os("LAYERFS_RH_LARGE_SETUP_ONLY").is_some() {
        return None;
    }
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
    case(
        "grep.large",
        "fs.grep",
        vec![branch(), path("/"), ("pattern", t("needle"))],
    );
    case(
        "glob.large",
        "fs.glob",
        vec![branch(), path("/big"), ("pattern", t("*"))],
    );
    Some(out)
}
