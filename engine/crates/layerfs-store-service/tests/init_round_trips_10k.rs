//! #818 Init pins at 10k files (lane I owns this file with
//! `init_round_trips.rs`, which states the method): Namespace Init into a
//! new Store, exactly as the #818 probe's `init.nested_10k` and
//! `init.flat_10k` cases, on SQLite and on PostgreSQL. Two sizes above every
//! threshold: 10k files is past the shared locator cache
//! (`READ_OBJECT_LIMIT` = 4,096), the 512-entry wide-directory placement
//! and the 4,096-row write windows.
//!
//! - `nested_10k`: 100 directories of 100 files;
//! - `flat_10k`: one directory of 10,000 files.
//!
//! **Slack.** At 10k the baseline's statement and round-trip counts are not
//! exactly repeatable, while the 1k shapes are. Inferred cause (not yet
//! confirmed by a per-phase count): Init's own `logical_size` walk opens
//! every regular file's root, and once the roots no longer fit the locator
//! cache, which of them still hit depends on the order the parallel
//! construction workers finished in (the extra statements come with extra
//! returned rows, as locates do). Observed, one run each, at 056e920166 and
//! 3a904ac225: PostgreSQL nested 10,953 / 10,955 / 10,958 / 10,955 / 10,956
//! round trips, flat 31,039 / 31,041 / 31,040 / 31,044 / 31,042; SQLite
//! nested 12,164 / 12,168 / 12,163 / 12,166 / 12,172 statements, flat
//! 31,881 / 31,883 / 31,885 / 31,883 / 31,879. The pins (the 056e920166
//! probe rows) allow `SLACK` on round trips and statements; connects and
//! transactions are exact, and the logical bytes, canonical bytes and root
//! (fixed source times) repeat exactly on both backends.
//!
//! #818 I1 removed that walk (Init sums its logical bytes from the scan),
//! and with it the per-file locates: the pins are now exact (`SLACK` = 0).
//!
//! Each PostgreSQL case took 9–17 s in the test profile at baseline (6–13 s
//! after I1); the two run one after the other under the binary's PostgreSQL
//! turn.
#[path = "support/round_trips.rs"]
mod rt;

use rt::{Backend, InitPin, Store};

/// Allowed difference on round trips and statements (see the module doc).
const SLACK: u64 = 0;

// The baseline rows in the comments are the 056e920166 probe rows
// (`receipts/056e920166/*.log`); the pins are after #818 I1.

// SQLite: session `[0, 0, statements, transactions]`.
// probe init.nested_10k at 056e920166: 12,164 statements, 57 transactions
const SQLITE_NESTED: InitPin = pin(
    "nested_10k",
    [0, 0, 1837, 57],
    108000,
    1379418,
    "a68d83d36cfcc6aa",
);
// probe init.flat_10k at 056e920166: 31,881 statements, 60 transactions;
// after I1 21,560; after I2b (314 placement windows of up to 32 children,
// three statements each) 2,502
const SQLITE_FLAT: InitPin = pin(
    "flat_10k",
    [0, 0, 2502, 60],
    118890,
    1380691,
    "a0b8d9600c933c0a",
);

// PostgreSQL: session `[connects, round_trips, statements, transactions]`.
// probe init.nested_10k at 056e920166 (`pg.log`): [0, 10,955, 10,666, 120];
// after I1 [0, 624, 335, 120]; after I3a (87 read units, each one fenced
// statement instead of a transaction with an owner select and one or two
// reads) [0, 339, 226, 33]; after I3c (8 Save publications, each one
// fenced flight; 6 carry packs) [0, 303, 250, 33]
const POSTGRESQL_NESTED: InitPin = pin(
    "nested_10k",
    [0, 303, 250, 33],
    108000,
    1379418,
    "a68d83d36cfcc6aa",
);
// probe init.flat_10k at 056e920166 (`pg.log`): [0, 31,039, 30,700, 144];
// after I1 [0, 20,720, 20,381, 144]; after I2a (three placement units, each
// one dependency check and one placement statement) [0, 726, 387, 144];
// after I3a (108 read units, each one fenced statement) [0, 378, 257, 36];
// after I3c (8 Save publications in fenced flights) [0, 342, 281, 36]
const POSTGRESQL_FLAT: InitPin = pin(
    "flat_10k",
    [0, 342, 281, 36],
    118890,
    1380691,
    "a0b8d9600c933c0a",
);

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
        slack: SLACK,
        logical,
        canonical,
        root,
    }
}

fn one(backend: Backend, pin: InitPin, dirs: usize, per: usize) {
    let mut store = match backend {
        Backend::Sqlite => Store::sqlite(&format!("init-{}", pin.case)),
        Backend::Postgresql => match Store::postgresql(&format!("init_{}", pin.case)) {
            Some(store) => store,
            None => return,
        },
    };
    let measured = rt::init_shape(&mut store, pin.case, &|source| rt::files(source, dirs, per));
    templates(&measured);
    rt::check_init("init_round_trips_10k", backend, &[measured], &[pin]);
}

/// Prints every server statement template of a PostgreSQL Init with its
/// top-level calls (`RT_INIT_TEMPLATE`, printed, not pinned): the per-phase
/// attribution lane I's I3 predictions start from.
fn templates(measured: &rt::InitMeasured) {
    let Some(server) = &measured.outcome.server else {
        return;
    };
    for (top, _, text) in server.templates.iter().filter(|t| t.0 > 0) {
        let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text: String = text.chars().take(160).collect();
        println!("RT_INIT_TEMPLATE {} {top} {text}", measured.case);
    }
}

#[test]
fn sqlite_init_nested_10k() {
    one(Backend::Sqlite, SQLITE_NESTED, 100, 100);
}

#[test]
fn sqlite_init_flat_10k() {
    one(Backend::Sqlite, SQLITE_FLAT, 1, 10_000);
}

#[test]
fn postgresql_init_nested_10k() {
    let _turn = rt::postgresql_turn();
    one(Backend::Postgresql, POSTGRESQL_NESTED, 100, 100);
}

#[test]
fn postgresql_init_flat_10k() {
    let _turn = rt::postgresql_turn();
    one(Backend::Postgresql, POSTGRESQL_FLAT, 1, 10_000);
}
