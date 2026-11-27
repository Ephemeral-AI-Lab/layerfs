//! #818 I3b: a PostgreSQL acquisition write unit with no wait (root
//! windows, `advance`, `discard`, `dispose`) is one fenced flight from
//! `BEGIN` to `COMMIT`.
//!
//! - **Equivalence.** The same script of units on SQLite and on PostgreSQL
//!   gives the same answers and charges: recorded, repeated, missing and
//!   filled root positions (`Changed` at the sequential check's position),
//!   an unconvertible position, every phase, removal jobs of every budget,
//!   disposal to the release with another operation live and then of the
//!   last one, and released, missing and foreign owners (`Stale`).
//! - **One flight.** A warm unit is one round trip and one transaction.
//! - **Sweeps.** A drop or a kill at every server message of each unit kind
//!   (its prepares included) leaves the unit atomic: the working rows are
//!   exactly as before or exactly as the clean unit left them; a dropped
//!   reply is never a success; a success always matches the clean answer;
//!   the session never reconnects or replays.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::support;

use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqliteAcquisitionSchema, SqliteHandles,
};
use layerfs_storage::{
    port::{acquisition::*, PersistenceError},
    StoragePolicy,
};
use std::time::{Duration, Instant};
use support::pg::fault_proxy::{Fault, FaultProxy};
use support::{pg, Temp};

const BINDING: &[u8] = b"acquisition-flights-818";
const CURSOR: [u8; 32] = [84; 32];

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn begun(port: &dyn Acquisition) -> Owner {
    port.begin(&Begin {
        source_device: 1,
        source_inode: 2,
        stack: [3; 16],
        scope: ObjectId::for_bytes(b"scope"),
    })
    .unwrap()
}

fn entry(
    parent: Option<u64>,
    name: &str,
    kind: InodeKind,
    position: u64,
    inode: Option<u64>,
) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent,
            name: name.as_bytes().to_vec(),
        },
        position: Some(position),
        kind,
        metadata: layerfs_content::filesystem::tree::attribute::PortableMetadata {
            mode: if kind == InodeKind::Directory {
                0o755
            } else {
                0o644
            },
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        },
        target_root: None,
        host_path: Some(
            format!("/source/{}{name}", "p".repeat(position as usize % 50)).into_bytes(),
        ),
        host: inode.map(|inode| HostIdentity {
            device: 1,
            inode,
            evidence: [9; EVIDENCE_BYTES],
        }),
    }
}

/// The root, `dirs` directories and `files` placed files spread over them
/// (every fourth a hard link of the one before); returns the distinct
/// canonical file positions in order.
fn fill(port: &dyn Acquisition, owner: Owner, dirs: u64, files: u64) -> Vec<u64> {
    let mut rows = vec![entry(None, "", InodeKind::Directory, 0, None)];
    for d in 1..=dirs {
        rows.push(entry(
            Some(0),
            &format!("d{d:02}"),
            InodeKind::Directory,
            d,
            None,
        ));
    }
    let mut inode = 0;
    for f in 0..files {
        if f % 4 != 3 {
            inode += 1;
        }
        rows.push(entry(
            Some(1 + f % dirs),
            &format!("f{f:02}"),
            InodeKind::RegularFile,
            100 + f,
            Some(inode),
        ));
    }
    let mut canonical = port.put_entries(owner, &rows).unwrap();
    canonical.dedup();
    canonical
}

fn root(position: u64, salt: &str) -> (u64, ObjectId) {
    (
        position,
        ObjectId::for_bytes(format!("{salt}{position}").as_bytes()),
    )
}

/// The same units on either engine, each answer and the charges after it.
fn script(port: &dyn Acquisition) -> Vec<String> {
    let mut out = Vec::new();
    fn line(
        out: &mut Vec<String>,
        label: &str,
        answer: String,
        owner: Owner,
        port: &dyn Acquisition,
    ) {
        out.push(format!("{label} {answer} work {:?}", port.work(owner)));
    }
    macro_rules! note {
        ($label:expr, $answer:expr, $owner:expr, $port:expr) => {
            line(&mut out, $label, $answer, $owner, $port)
        };
    }
    let a = begun(port);
    let files = fill(port, a, 6, 24);
    note!("fill", format!("{files:?}"), a, port);
    let (c0, c1, c2) = (files[0], files[1], files[2]);
    let first: Vec<_> = files[..5].iter().map(|p| root(*p, "a")).collect();
    note!(
        "files",
        format!("{:?}", port.complete_files(a, &first)),
        a,
        port
    );
    let unit = [
        root(files[5], "b"),
        root(files[6], "b"),
        root(files[5], "c"),
    ];
    note!(
        "repeat",
        format!("{:?}", port.complete_files(a, &unit)),
        a,
        port
    );
    let unit = [root(files[7], "b"), root(99_999, "b"), root(files[8], "b")];
    note!(
        "missing",
        format!("{:?}", port.complete_files(a, &unit)),
        a,
        port
    );
    let unit = [root(files[9], "b"), root(c1, "b")];
    note!(
        "filled",
        format!("{:?}", port.complete_files(a, &unit)),
        a,
        port
    );
    let unit = [root(files[10], "b"), root(u64::MAX, "b")];
    note!(
        "unsigned",
        format!("{:?}", port.complete_files(a, &unit)),
        a,
        port
    );
    note!(
        "empty",
        format!("{:?}", port.complete_files(a, &[])),
        a,
        port
    );
    let rest: Vec<_> = files[5..].iter().map(|p| root(*p, "d")).collect();
    note!(
        "rest",
        format!("{:?}", port.complete_files(a, &rest)),
        a,
        port
    );
    let unit = [root(0, "r"), root(1, "r"), root(2, "r")];
    note!(
        "dirs",
        format!("{:?}", port.set_directory_roots(a, &unit)),
        a,
        port
    );
    let unit = [root(3, "r"), root(c0, "r")];
    note!(
        "dir_file",
        format!("{:?}", port.set_directory_roots(a, &unit)),
        a,
        port
    );
    let unit = [root(4, "r"), root(5, "r"), root(4, "s")];
    note!(
        "dir_repeat",
        format!("{:?}", port.set_directory_roots(a, &unit)),
        a,
        port
    );
    let unit = [root(c2, "x")];
    note!(
        "refill",
        format!("{:?}", port.complete_files(a, &unit)),
        a,
        port
    );
    for phase in [
        Phase::Files,
        Phase::Tree,
        Phase::Failed,
        Phase::Uncertain,
        Phase::Discarding,
    ] {
        note!("advance", format!("{:?}", port.advance(a, phase)), a, port);
    }
    let b = begun(port);
    let b_files = fill(port, b, 2, 5);
    note!("fill_b", format!("{b_files:?}"), b, port);
    for budget in [0, 1, 3, 7, 2] {
        note!("discard", format!("{:?}", port.discard(a, budget)), a, port);
    }
    loop {
        let disposal = port.dispose(a, 4);
        note!("dispose", format!("{disposal:?}"), a, port);
        if !matches!(
            disposal,
            Ok(Disposal {
                released: false,
                ..
            })
        ) {
            break;
        }
    }
    let missing = Owner {
        operation: b.operation + 1_000,
        epoch: b.epoch,
    };
    let foreign = Owner {
        operation: b.operation,
        epoch: b.epoch + 1,
    };
    for (label, owner) in [("released", a), ("missing", missing), ("foreign", foreign)] {
        out.push(format!(
            "{label} {:?} {:?} {:?} {:?} {:?}",
            port.complete_files(owner, &[root(c0, "z")]),
            port.set_directory_roots(owner, &[root(1, "z")]),
            port.advance(owner, Phase::Tree),
            port.discard(owner, 1),
            port.dispose(owner, 1),
        ));
    }
    // The last operation goes; a new one begins on the emptied tables.
    note!(
        "dispose_b",
        format!("{:?}", port.dispose(b, 1_000)),
        b,
        port
    );
    let c = begun(port);
    let c_files = fill(port, c, 1, 2);
    note!("fill_c", format!("{c_files:?}"), c, port);
    note!("dispose_c", format!("{:?}", port.dispose(c, 0)), c, port);
    note!(
        "dispose_c",
        format!("{:?}", port.dispose(c, 1_000)),
        c,
        port
    );
    out
}

fn sqlite_script() -> Vec<String> {
    let temp = Temp::new("i3b-flights");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    script(&handles.acquisition)
}

#[test]
fn postgresql_flights_equal_sqlite_units() {
    let Some(url) = pg::url() else { return };
    let schema = pg::Schema::new(url, "i3b_flights");
    let handles = PostgresHandles::create(
        &schema.selection(),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    let postgresql = script(&handles.acquisition);
    let sqlite = sqlite_script();
    for (pg_line, sqlite_line) in postgresql.iter().zip(&sqlite) {
        assert_eq!(pg_line, sqlite_line);
    }
    assert_eq!(postgresql.len(), sqlite.len());
    let mut admin = schema.admin();
    assert_eq!(
        state(&mut admin),
        Vec::<String>::new(),
        "the last disposal emptied the tables"
    );
    eprintln!("i3b_flights_compared {}", postgresql.len());
}

#[test]
fn a_warm_unit_is_one_round_trip() {
    let Some(url) = pg::url() else { return };
    let schema = pg::Schema::new(url, "i3b_one_trip");
    let handles = schema.create(BINDING, CURSOR);
    let port = &handles.acquisition;
    let owner = begun(port);
    let files = fill(port, owner, 2, 8);
    // Each root call records a position not yet recorded.
    let calls = std::cell::Cell::new(0_usize);
    type Unit<'a> = (&'a str, Box<dyn Fn() -> String + 'a>);
    let mut units: Vec<Unit<'_>> = vec![
        (
            "advance",
            Box::new(|| format!("{:?}", port.advance(owner, Phase::Files))),
        ),
        (
            "complete_files",
            Box::new(|| {
                let next = files[calls.get()];
                calls.set(calls.get() + 1);
                format!("{:?}", port.complete_files(owner, &[root(next, "w")]))
            }),
        ),
        (
            "set_directory_roots",
            Box::new(|| {
                let next = calls.get() as u64 % 3;
                calls.set(calls.get() + 1);
                format!("{:?}", port.set_directory_roots(owner, &[root(next, "w")]))
            }),
        ),
        (
            "discard",
            Box::new(|| format!("{:?}", port.discard(owner, 1))),
        ),
        (
            "dispose",
            Box::new(|| format!("{:?}", port.dispose(owner, 1))),
        ),
    ];
    for (name, unit) in units.drain(..) {
        // The first call prepares; the second is warm.
        let _ = unit();
        let before = handles.diagnostics().unwrap();
        let answer = unit();
        let after = handles.diagnostics().unwrap();
        assert!(!answer.contains("Err("), "{name}: {answer}");
        assert_eq!(
            after.round_trips - before.round_trips,
            1,
            "{name} round trips"
        );
        assert_eq!(
            after.transactions - before.transactions,
            1,
            "{name} transactions"
        );
        assert_eq!(
            after.write_commits - before.write_commits,
            1,
            "{name} commits"
        );
    }
}

fn wait_backend_gone(admin: &mut postgres::Client, pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let gone: bool = admin
            .query_one(
                "SELECT NOT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid=$1)",
                &[&pid],
            )
            .unwrap()
            .get(0);
        if gone {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "faulted backend {pid} still runs"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Every working row and record, with the charges and removal counts.
fn state(admin: &mut postgres::Client) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT concat_ws('|',parent_position,encode(name,'hex'),coalesce(position,-9),kind,coalesce(encode(content_root,'hex'),'-')) FROM init_entry ORDER BY parent_position,name",
        "SELECT concat_ws('|',canonical_position,aliases,coalesce(encode(file_root,'hex'),'-')) FROM init_host_file ORDER BY canonical_position",
        "SELECT concat_ws('|',owner_epoch,phase,held_rows,held_bytes,peak_rows,peak_bytes,removed_rows,removed_bytes) FROM init_operation ORDER BY operation_id",
    ] {
        let found = admin.query(sql, &[]).unwrap();
        rows.extend(found.iter().map(|row| row.get::<_, String>(0)));
    }
    rows
}

const KINDS: usize = 7;

/// Unit kind `kind` of `owner` once, by name.
fn unit(
    port: &dyn Acquisition,
    owner: Owner,
    files: &[u64],
    kind: usize,
) -> (&'static str, Result<String, AcquisitionError>) {
    let roots: Vec<_> = files[..3].iter().map(|p| root(*p, "s")).collect();
    match kind {
        0 => (
            "complete_files",
            port.complete_files(owner, &roots).map(|v| format!("{v:?}")),
        ),
        1 => (
            "changed",
            port.complete_files(owner, &[roots[0], root(99_999, "s")])
                .map(|v| format!("{v:?}")),
        ),
        2 => (
            "set_directory_roots",
            port.set_directory_roots(owner, &[root(0, "s"), root(1, "s")])
                .map(|v| format!("{v:?}")),
        ),
        3 => (
            "advance",
            port.advance(owner, Phase::Tree).map(|v| format!("{v:?}")),
        ),
        4 => ("discard", port.discard(owner, 3).map(|v| format!("{v:?}"))),
        5 => ("dispose", port.dispose(owner, 3).map(|v| format!("{v:?}"))),
        _ => (
            "dispose_last",
            port.dispose(owner, 1_000).map(|v| format!("{v:?}")),
        ),
    }
}

struct Sample {
    seen: u64,
    answer: String,
    before: Vec<String>,
    after: Vec<String>,
}

/// One unit kind on a fresh Store and session (so its prepares are among
/// the messages), with `damage` armed.
fn sample(
    schema: &pg::Schema,
    proxy: &FaultProxy,
    proxied_url: &str,
    kind: usize,
    damage: Option<(u64, Fault)>,
    clean: Option<&Sample>,
) -> Sample {
    schema.drop_schema();
    drop(schema.create(BINDING, CURSOR));
    let writer = PostgresHandles::open_writable(
        &PostgresSelection::new(proxied_url, &schema.name),
        BINDING,
        CURSOR,
    )
    .unwrap();
    let port = &writer.acquisition;
    let owner = begun(port);
    let files = fill(port, owner, 2, 6);
    let mut admin = schema.admin();
    let before = state(&mut admin);
    let connections = proxy.pids().len();
    let before_work = writer.diagnostics().unwrap();
    proxy.arm(damage);
    let (name, answer) = unit(port, owner, &files, kind);
    let messages = proxy.disarm();
    let after_work = writer.diagnostics().unwrap();
    assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
    assert_eq!(before_work.connects, after_work.connects);
    let answer = format!("{answer:?}");
    match damage {
        None => {
            assert!(messages.fired.is_none());
            assert_eq!(
                answer.starts_with("Ok("),
                name != "changed",
                "{name}: {answer}"
            );
        }
        Some((at, fault)) => {
            let fired = messages.fired.as_ref().expect("the armed fault fired");
            let clean = clean.unwrap();
            assert_eq!(fired.at, at);
            if fault == Fault::HoldKill {
                assert_eq!(fired.killed, Some(true), "the target backend died");
            }
            let uncertain = format!(
                "{:?}",
                Err::<(), _>(AcquisitionError::Persistence(PersistenceError::Uncertain))
            );
            // A dropped reply is never a success. A refusal answered before
            // the lost reply is definite: its transaction cannot commit.
            if fault == Fault::Drop {
                assert!(
                    answer.starts_with("Err("),
                    "{name}: a dropped reply at {at}"
                );
            }
            let pid = proxy.pids()[fired.connection].expect("the writer's backend pid");
            wait_backend_gone(&mut admin, pid);
            let after = state(&mut admin);
            assert_eq!(before, clean.before, "{name}: the same setup");
            if answer.starts_with("Ok(") {
                assert_eq!(
                    answer, clean.answer,
                    "{name}: a success is the clean one: {at}"
                );
                assert_eq!(after, clean.after, "{name}: a success applied: {at}");
            } else if answer == uncertain {
                assert!(
                    after == before || after == clean.after,
                    "{name}: the unit is atomic: {fault:?} at {at}"
                );
            } else {
                assert_eq!(after, before, "{name}: a refusal changed nothing: {at}");
            }
            eprintln!("flight_fault {name} {fault:?} at {at} {answer}");
        }
    }
    drop(writer);
    let after = state(&mut admin);
    Sample {
        seen: messages.seen,
        answer,
        before,
        after,
    }
}

fn sweep(tag: &str, fault: Fault) {
    let Some(url) = pg::url() else { return };
    let (proxy, proxied_url) = FaultProxy::start(&url);
    let schema = pg::Schema::new(url, tag);
    for kind in 0..KINDS {
        let clean = sample(&schema, &proxy, &proxied_url, kind, None, None);
        assert!(clean.seen > 0);
        eprintln!(
            "flight_clean {kind} messages {} {}",
            clean.seen, clean.answer
        );
        for at in 0..clean.seen {
            sample(
                &schema,
                &proxy,
                &proxied_url,
                kind,
                Some((at, fault)),
                Some(&clean),
            );
        }
    }
}

#[test]
fn flights_drop_at_every_message() {
    sweep("i3b_flight_drop", Fault::Drop);
}

#[test]
fn flights_kill_at_every_message() {
    sweep("i3b_flight_kill", Fault::HoldKill);
}
