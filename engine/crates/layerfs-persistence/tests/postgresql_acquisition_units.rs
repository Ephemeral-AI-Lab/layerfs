//! #818 I3b part A: a PostgreSQL `put_entries` unit is one transaction over
//! one flight per wait (the dependency check, a dependent row's binding).
//!
//! - **Equivalence.** The same `put_entries` script on SQLite and on
//!   PostgreSQL gives the same answers, charges and rows: independent
//!   windows, hard links inside a window and across units (aliases), and
//!   the first refusal of a window whose tail is `Changed`, malformed
//!   (`Bounds`), a stored key or a repeated position, in every order, each
//!   rolling the whole unit back; and stale owners.
//! - **Flights.** A warm independent window is two round trips and one
//!   transaction.
//! - **Sweeps.** A drop or a kill at every server message of each unit kind
//!   (prepared by the setup's unit) leaves it atomic: the rows are exactly as
//!   before or exactly as the clean unit left them; a dropped reply is never
//!   a success; a success matches the clean answer; never replayed.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
mod support;

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
use support::fault_proxy::{Fault, FaultProxy};
use support::{pg, Temp};

const BINDING: &[u8] = b"acquisition-units-818";
const CURSOR: [u8; 32] = [85; 32];

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

fn meta(mode: u32) -> layerfs_content::filesystem::attributes::PortableMetadata {
    layerfs_content::filesystem::attributes::PortableMetadata {
        mode,
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
    }
}

fn directory(parent: Option<u64>, name: &str, position: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent,
            name: name.as_bytes().to_vec(),
        },
        position: Some(position),
        kind: InodeKind::Directory,
        metadata: meta(0o755),
        target_root: None,
        host_path: Some(format!("/source/{name}").into_bytes()),
        host: None,
    }
}

/// A regular file in the root at `position`, of host inode `inode`.
fn file(position: u64, inode: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("f{position}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::RegularFile,
        metadata: meta(0o644),
        target_root: None,
        host_path: Some(
            format!("/source/{}f{position}", "p".repeat(position as usize % 40)).into_bytes(),
        ),
        host: Some(HostIdentity {
            device: 1,
            inode,
            evidence: [9; EVIDENCE_BYTES],
        }),
    }
}

fn changed(position: u64, inode: u64) -> NewEntry {
    let mut entry = file(position, inode);
    entry.host.as_mut().unwrap().evidence[0] ^= 1;
    entry
}

fn invalid(position: u64) -> NewEntry {
    let mut entry = file(position, position);
    entry.kind = InodeKind::Directory;
    entry
}

/// A state line: the answer, the charges and every entry and binding.
fn line(port: &dyn Acquisition, owner: Owner, label: &str, answer: String) -> String {
    format!(
        "{label} {answer} work {:?} entries {:?} roots {:?}",
        port.work(owner),
        port.entries(owner, None, Limits::MAXIMUM),
        port.file_roots(owner, None, Limits::MAXIMUM),
    )
}

/// The same `put_entries` units on either engine.
fn script(port: &dyn Acquisition) -> Vec<String> {
    let mut out = Vec::new();
    let owner = begun(port);
    let mut first = vec![directory(None, "", 0), directory(Some(0), "d1", 1)];
    first.extend([file(2, 1), file(3, 2), file(4, 1), file(5, 3), file(6, 2)]);
    let answer = format!("{:?}", port.put_entries(owner, &first));
    out.push(line(port, owner, "first", answer));
    let prefix: Vec<_> = (10..=30).map(|n| file(n, 100 + n)).collect();
    for (label, tail) in [
        ("changed_invalid", vec![changed(41, 1), invalid(42)]),
        ("invalid_changed", vec![invalid(42), changed(41, 1)]),
        ("stored_changed", vec![file(2, 43), changed(41, 1)]),
        ("changed_stored", vec![changed(41, 1), file(2, 43)]),
        (
            "repeat_invalid",
            vec![file(50, 50), file(50, 51), invalid(42)],
        ),
        (
            "alias_changed",
            vec![file(51, 51), file(52, 51), changed(41, 1)],
        ),
        ("stored_key", vec![directory(Some(0), "d1", 60)]),
        ("unsigned", vec![file(u64::MAX, 61)]),
    ] {
        let mut entries = prefix.clone();
        entries.extend(tail);
        let answer = format!("{:?}", port.put_entries(owner, &entries));
        out.push(line(port, owner, label, answer));
    }
    // Fresh and stored aliases across windows keep the first path.
    let mut entries = prefix;
    entries.extend([file(41, 2), file(42, 1), file(43, 143)]);
    let answer = format!("{:?}", port.put_entries(owner, &entries));
    out.push(line(port, owner, "aliases", answer));
    let answer = format!("{:?}", port.put_entries(owner, &[]));
    out.push(line(port, owner, "empty", answer));
    let other = begun(port);
    let missing = Owner {
        operation: other.operation + 1_000,
        epoch: other.epoch,
    };
    let foreign = Owner {
        operation: other.operation,
        epoch: other.epoch + 1,
    };
    port.dispose(other, 1_000).unwrap();
    for (label, stale) in [
        ("released", other),
        ("missing", missing),
        ("foreign", foreign),
    ] {
        out.push(format!(
            "{label} {:?}",
            port.put_entries(stale, &[file(70, 70)])
        ));
    }
    out
}

#[test]
fn postgresql_units_equal_sqlite_units() {
    let Some(url) = pg::url() else { return };
    let schema = pg::Schema::new(url, "i3b_units");
    let handles = PostgresHandles::create(
        &schema.selection(),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    let postgresql = script(&handles.acquisition);
    let temp = Temp::new("i3b-units");
    let sqlite = SqliteHandles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    let sqlite = script(&sqlite.acquisition);
    for (pg_line, sqlite_line) in postgresql.iter().zip(&sqlite) {
        assert_eq!(pg_line, sqlite_line);
    }
    assert_eq!(postgresql.len(), sqlite.len());
    eprintln!("i3b_units_compared {}", postgresql.len());
}

#[test]
fn a_warm_window_is_two_round_trips() {
    let Some(url) = pg::url() else { return };
    let schema = pg::Schema::new(url, "i3b_units_trips");
    let handles = schema.create(BINDING, CURSOR);
    let port = &handles.acquisition;
    let owner = begun(port);
    port.put_entries(owner, &[directory(None, "", 0), file(1, 1)])
        .unwrap();
    let before = handles.diagnostics().unwrap();
    let window: Vec<_> = (2..200).map(|n| file(n, n)).collect();
    port.put_entries(owner, &window).unwrap();
    let after = handles.diagnostics().unwrap();
    assert_eq!(after.round_trips - before.round_trips, 2, "round trips");
    assert_eq!(after.transactions - before.transactions, 1, "transactions");
    assert_eq!(after.write_commits - before.write_commits, 1, "commits");
}

/// A queued placement whose decode is `Changed`, then a binding the server
/// refuses in the same flight: the answer is the placement's `Changed`, as
/// the per-statement loop found it first, and the unit is counted once, as
/// rolled back.
#[test]
fn a_failed_check_then_a_refusal_counts_one_rollback() {
    let Some(url) = pg::url() else { return };
    let schema = pg::Schema::new(url, "i3b_units_count");
    let handles = schema.create(BINDING, CURSOR);
    let port = &handles.acquisition;
    let owner = begun(port);
    let unplaced = |name: &str, inode: u64| {
        let mut entry = file(0, inode);
        entry.key.name = name.as_bytes().to_vec();
        entry.position = None;
        entry
    };
    port.put_entries(
        owner,
        &[
            directory(None, "", 0),
            file(5, 1),
            unplaced("u1", 10),
            unplaced("u2", 12),
        ],
    )
    .unwrap();
    let placed = |name: &str, position: u64, inode: u64| Placed {
        name: name.as_bytes().to_vec(),
        position,
        host: Some((
            HostIdentity {
                device: 1,
                inode,
                evidence: [9; EVIDENCE_BYTES],
            },
            format!("/source/{name}").into_bytes(),
        )),
    };
    // `missing` has no unplaced row (its decode is `Changed`); `u2` takes
    // the stored file's canonical position, so it binds alone and the
    // server refuses its binding.
    let children = [
        placed("u1", 10, 10),
        placed("missing", 11, 11),
        placed("u2", 5, 12),
    ];
    let before = handles.diagnostics().unwrap();
    let answer = port.place_children(owner, 0, &children);
    let after = handles.diagnostics().unwrap();
    assert_eq!(answer, Err(AcquisitionError::Changed { position: 11 }));
    assert_eq!(after.transactions - before.transactions, 1, "one unit");
    assert_eq!(after.rollbacks - before.rollbacks, 1, "rolled back");
    assert_eq!(
        after.write_commits, before.write_commits,
        "nothing committed"
    );
    let unplaced = port
        .unplaced_children(owner, 0, None, Limits::MAXIMUM)
        .unwrap();
    let names: Vec<&[u8]> = unplaced.iter().map(|child| child.name.as_slice()).collect();
    assert_eq!(names, [&b"u1"[..], &b"u2"[..]], "the unit placed nothing");
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

fn state(admin: &mut postgres::Client) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT concat_ws('|',parent_position,encode(name,'hex'),coalesce(position,-9),kind,coalesce(canonical_position,-9)) FROM init_entry ORDER BY parent_position,name",
        "SELECT concat_ws('|',canonical_position,aliases,encode(host_path,'hex')) FROM init_host_file ORDER BY canonical_position",
        "SELECT concat_ws('|',owner_epoch,phase,held_rows,held_bytes,peak_rows,peak_bytes) FROM init_operation ORDER BY operation_id",
    ] {
        let found = admin.query(sql, &[]).unwrap();
        rows.extend(found.iter().map(|row| row.get::<_, String>(0)));
    }
    rows
}

const KINDS: usize = 3;

/// Unit kind `kind`: an independent window, a window with hard links (a
/// stored alias and a fresh one, each bound alone), and a window whose
/// stored key refuses after a queued prefix, before a malformed row.
fn unit(port: &dyn Acquisition, owner: Owner, kind: usize) -> (&'static str, String) {
    let entries: Vec<NewEntry> = match kind {
        0 => (2..8).map(|n| file(n, n)).collect(),
        1 => vec![file(2, 2), file(3, 1), file(4, 2), file(5, 5)],
        _ => vec![file(2, 2), directory(Some(0), "d1", 9), invalid(10)],
    };
    let name = ["window", "aliases", "refusal"][kind];
    (name, format!("{:?}", port.put_entries(owner, &entries)))
}

struct Sample {
    seen: u64,
    answer: String,
    before: Vec<String>,
    after: Vec<String>,
}

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
    // The base: the root, directory d1 and one file of inode 1.
    port.put_entries(
        owner,
        &[
            directory(None, "", 0),
            directory(Some(0), "d1", 1),
            file(20, 1),
        ],
    )
    .unwrap();
    let mut admin = schema.admin();
    let before = state(&mut admin);
    let connections = proxy.pids().len();
    let before_work = writer.diagnostics().unwrap();
    proxy.arm(damage);
    let (name, answer) = unit(port, owner, kind);
    let messages = proxy.disarm();
    let after_work = writer.diagnostics().unwrap();
    assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
    assert_eq!(before_work.connects, after_work.connects);
    let uncertain = format!(
        "{:?}",
        Err::<(), _>(AcquisitionError::Persistence(PersistenceError::Uncertain))
    );
    match damage {
        None => {
            assert!(messages.fired.is_none());
            assert_eq!(answer.starts_with("Ok("), kind != 2, "{name}: {answer}");
        }
        Some((at, fault)) => {
            let fired = messages.fired.as_ref().expect("the armed fault fired");
            let clean = clean.unwrap();
            assert_eq!(fired.at, at);
            if fault == Fault::HoldKill {
                assert_eq!(fired.killed, Some(true), "the target backend died");
            }
            // A dropped reply is never a success; a refusal answered
            // before it is definite (its transaction cannot commit).
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
            eprintln!("unit_fault {name} {fault:?} at {at} {answer}");
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
        eprintln!("unit_clean {kind} messages {} {}", clean.seen, clean.answer);
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
fn units_drop_at_every_message() {
    sweep("i3b_unit_drop", Fault::Drop);
}

#[test]
fn units_kill_at_every_message() {
    sweep("i3b_unit_kill", Fault::HoldKill);
}
