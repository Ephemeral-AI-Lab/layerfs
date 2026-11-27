//! #818 I3a: every PostgreSQL acquisition read unit is one fenced
//! autocommit statement with the owner joined in.
//!
//! - **Equivalence.** The same working rows on SQLite and on PostgreSQL
//!   give the same windows, paged to the end under row and byte limits that
//!   cut paths in the middle of a window (the folded sizing reads), and the
//!   same single reads and work counts.
//! - **Owners.** A live owner's empty read is empty; a released owner, a
//!   missing operation under the session's epoch and a foreign epoch are
//!   `Stale` for every read.
//! - **Sweeps.** A drop or a kill at every server message of each read
//!   kind (its prepare included) is answered `Uncertain` or a definite
//!   refusal, never a read; the rows are unchanged and the session never
//!   reconnects or replays.
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

const BINDING: &[u8] = b"acquisition-reads-818";
const CURSOR: [u8; 32] = [83; 32];

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

fn identity(inode: u64) -> HostIdentity {
    HostIdentity {
        device: 1,
        inode,
        evidence: [9; EVIDENCE_BYTES],
    }
}

/// A host path whose length varies with `n`, so byte limits cut windows at
/// different rows.
fn path(name: &str, n: u64) -> Vec<u8> {
    format!("/source/{}{name}", "p".repeat((n as usize * 37) % 300)).into_bytes()
}

fn entry(
    parent: Option<u64>,
    name: &str,
    kind: InodeKind,
    position: Option<u64>,
    host: Option<HostIdentity>,
    n: u64,
) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent,
            name: name.as_bytes().to_vec(),
        },
        position,
        kind,
        metadata: meta(match kind {
            InodeKind::Symlink => 0o777,
            InodeKind::Directory => 0o755,
            _ => 0o644,
        }),
        target_root: (kind == InodeKind::Symlink).then(|| ObjectId::for_bytes(name.as_bytes())),
        host_path: (kind != InodeKind::Symlink).then(|| path(name, n)),
        host,
    }
}

/// The root, 12 directories, 40 placed files (every fifth a hard link of
/// the one before, so those carry aliases), 6 symlinks and 9 unplaced
/// children of directory 1; roots recorded for half the files and for the
/// root and directory 1.
fn fill(port: &dyn Acquisition, owner: Owner) {
    let mut rows = vec![entry(None, "", InodeKind::Directory, Some(0), None, 0)];
    for d in 1..=12 {
        rows.push(entry(
            Some(0),
            &format!("d{d:02}"),
            InodeKind::Directory,
            Some(d),
            None,
            d,
        ));
    }
    let mut inode = 0;
    for f in 0..40_u64 {
        if f % 5 != 4 {
            inode += 1;
        }
        rows.push(entry(
            Some(1 + f % 12),
            &format!("f{f:02}"),
            InodeKind::RegularFile,
            Some(100 + f),
            Some(identity(inode)),
            f,
        ));
    }
    for s in 0..6 {
        rows.push(entry(
            Some(2),
            &format!("s{s}"),
            InodeKind::Symlink,
            Some(200 + s),
            None,
            s,
        ));
    }
    for u in 0..9 {
        rows.push(entry(
            Some(1),
            &format!("u{u}"),
            InodeKind::RegularFile,
            None,
            Some(identity(500 + u)),
            u,
        ));
    }
    let canonical = port.put_entries(owner, &rows).unwrap();
    let roots: Vec<(u64, ObjectId)> = canonical
        .iter()
        .step_by(2)
        .map(|position| (*position, ObjectId::for_bytes(&position.to_be_bytes())))
        .collect();
    port.complete_files(owner, &roots).unwrap();
    port.set_directory_roots(
        owner,
        &[
            (0, ObjectId::for_bytes(b"root")),
            (1, ObjectId::for_bytes(b"d1")),
        ],
    )
    .unwrap();
}

const LIMITS: &[Limits] = &[
    Limits::MAXIMUM,
    Limits { rows: 1, bytes: 0 },
    Limits { rows: 3, bytes: 1 },
    Limits {
        rows: 7,
        bytes: 200,
    },
    Limits {
        rows: 512,
        bytes: 700,
    },
    Limits {
        rows: 512,
        bytes: 2_000,
    },
    Limits {
        rows: 5,
        bytes: 1 << 20,
    },
];

/// The unplaced children of directory 1 paged under every limit, then the
/// placement of every one of them (entries and bindings read placed rows
/// only).
fn unplaced_then_place(port: &dyn Acquisition, owner: Owner) -> Vec<String> {
    let mut out = Vec::new();
    for limits in LIMITS {
        let mut after: Option<Vec<u8>> = None;
        loop {
            let window = port
                .unplaced_children(owner, 1, after.as_deref(), *limits)
                .unwrap();
            out.push(format!("unplaced {limits:?} {window:?}"));
            match window.last() {
                Some(child) => after = Some(child.name.clone()),
                None => break,
            }
        }
    }
    out.push(format!(
        "unplaced 2 {:?}",
        port.unplaced_children(owner, 2, None, Limits::MAXIMUM)
    ));
    place(port, owner);
    out
}

fn place(port: &dyn Acquisition, owner: Owner) {
    let placed: Vec<Placed> = (0..9)
        .map(|u| Placed {
            name: format!("u{u}").into_bytes(),
            position: 300 + u,
            host: Some((identity(500 + u), path(&format!("u{u}"), u))),
        })
        .collect();
    port.place_children(owner, 1, &placed).unwrap();
}

/// Every read of `owner`, each window kind paged to its end under every
/// limit, as text.
fn every_read(port: &dyn Acquisition, owner: Owner) -> Vec<String> {
    let mut out = Vec::new();
    for limits in LIMITS {
        let mut after = None;
        loop {
            let window = port.jobs(owner, after, *limits).unwrap();
            out.push(format!("jobs {limits:?} {window:?}"));
            match window.last() {
                Some(job) => after = Some(job.position),
                None => break,
            }
        }
        let mut after = None;
        loop {
            let window = port.directories(owner, after, *limits).unwrap();
            out.push(format!("directories {limits:?} {window:?}"));
            match window.last() {
                Some(directory) => after = Some(directory.position),
                None => break,
            }
        }
        let mut after = None;
        loop {
            let window = port.file_roots(owner, after, *limits).unwrap();
            out.push(format!("file_roots {limits:?} {window:?}"));
            match window.last() {
                Some(root) => after = Some(root.position),
                None => break,
            }
        }
        let mut after: Option<EntryKey> = None;
        loop {
            let window = port.entries(owner, after.as_ref(), *limits).unwrap();
            out.push(format!("entries {limits:?} {window:?}"));
            match window.last() {
                Some(entry) => after = Some(entry.key.clone()),
                None => break,
            }
        }
        let mut after: Option<EntryKey> = None;
        loop {
            let window = port.bindings(owner, after.as_ref(), *limits).unwrap();
            out.push(format!("bindings {limits:?} {window:?}"));
            match window.last() {
                Some(binding) => after = Some(binding.key.clone()),
                None => break,
            }
        }
    }
    for position in [0, 1, 7, 12, 13, 100, 104, 139, 140, 200, 999] {
        out.push(format!("job {position} {:?}", port.job(owner, position)));
        out.push(format!(
            "directory_path {position} {:?}",
            port.directory_path(owner, position)
        ));
    }
    out.push(format!("work {:?}", port.work(owner)));
    out
}

fn sqlite_reads() -> Vec<String> {
    let temp = Temp::new("i3a-reads");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(temp.join("store.sqlite"))
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    let owner = begun(&handles.acquisition);
    fill(&handles.acquisition, owner);
    let mut out = unplaced_then_place(&handles.acquisition, owner);
    out.extend(every_read(&handles.acquisition, owner));
    out
}

#[test]
fn postgresql_reads_equal_sqlite_reads() {
    let Some(url) = pg::url() else { return };
    let schema = pg::Schema::new(url, "i3a_reads");
    let handles = PostgresHandles::create(
        &schema.selection(),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    let owner = begun(&handles.acquisition);
    fill(&handles.acquisition, owner);
    let mut postgresql = unplaced_then_place(&handles.acquisition, owner);
    postgresql.extend(every_read(&handles.acquisition, owner));
    let sqlite = sqlite_reads();
    assert_eq!(postgresql.len(), sqlite.len());
    for (pg_line, sqlite_line) in postgresql.iter().zip(&sqlite) {
        assert_eq!(pg_line, sqlite_line);
    }
    eprintln!("i3a_reads_compared {}", postgresql.len());
}

const KINDS: usize = 9;

/// Read kind `kind` of `owner` once, by name.
fn read_one(
    port: &dyn Acquisition,
    owner: Owner,
    kind: usize,
) -> (&'static str, Result<String, AcquisitionError>) {
    // Two rows a window keep each sweep's message count small.
    let m = Limits {
        rows: 2,
        bytes: Limits::MAXIMUM.bytes,
    };
    match kind {
        0 => ("jobs", port.jobs(owner, None, m).map(|v| format!("{v:?}"))),
        1 => ("job", port.job(owner, 100).map(|v| format!("{v:?}"))),
        2 => (
            "directories",
            port.directories(owner, None, m).map(|v| format!("{v:?}")),
        ),
        3 => (
            "directory_path",
            port.directory_path(owner, 1).map(|v| format!("{v:?}")),
        ),
        4 => (
            "file_roots",
            port.file_roots(owner, None, m).map(|v| format!("{v:?}")),
        ),
        5 => (
            "entries",
            port.entries(owner, None, m).map(|v| format!("{v:?}")),
        ),
        6 => (
            "bindings",
            port.bindings(owner, None, m).map(|v| format!("{v:?}")),
        ),
        7 => (
            "unplaced",
            port.unplaced_children(owner, 1, None, m)
                .map(|v| format!("{v:?}")),
        ),
        _ => ("work", port.work(owner).map(|v| format!("{v:?}"))),
    }
}

/// Every read kind of `owner` once.
fn reads(
    port: &dyn Acquisition,
    owner: Owner,
) -> Vec<(&'static str, Result<String, AcquisitionError>)> {
    (0..KINDS).map(|kind| read_one(port, owner, kind)).collect()
}

#[test]
fn empty_released_missing_and_foreign_owners_stay_distinct() {
    let Some(url) = pg::url() else { return };
    let schema = pg::Schema::new(url, "i3a_owners");
    let handles = schema.create(BINDING, CURSOR);
    let port = &handles.acquisition;
    let released = begun(port);
    port.release(released).unwrap();
    let live = begun(port);
    for (name, answer) in reads(port, live) {
        let answer = answer.unwrap();
        assert!(
            ["[]", "None"].contains(&answer.as_str()) || name == "work",
            "{name}: a live owner's empty read is empty: {answer}"
        );
    }
    let missing = Owner {
        operation: live.operation + 1_000,
        epoch: live.epoch,
    };
    let foreign = Owner {
        operation: live.operation,
        epoch: live.epoch + 1,
    };
    for owner in [released, missing, foreign] {
        for (name, answer) in reads(port, owner) {
            assert_eq!(answer, Err(AcquisitionError::Stale), "{name} {owner:?}");
        }
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

fn state(admin: &mut postgres::Client) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT concat_ws('|',parent_position,encode(name,'hex'),coalesce(position,-9),kind,coalesce(encode(content_root,'hex'),'-')) FROM init_entry ORDER BY parent_position,name",
        "SELECT concat_ws('|',canonical_position,aliases,coalesce(encode(file_root,'hex'),'-')) FROM init_host_file ORDER BY canonical_position",
        "SELECT concat_ws('|',owner_epoch,phase,held_rows,held_bytes,peak_rows,peak_bytes) FROM init_operation",
    ] {
        let found = admin.query(sql, &[]).unwrap();
        rows.extend(found.iter().map(|row| row.get::<_, String>(0)));
    }
    rows
}

/// One read kind on a fresh session (so its prepare is among the messages),
/// with `damage` armed; returns the server messages the read caused.
fn sample(
    schema: &pg::Schema,
    proxy: &FaultProxy,
    proxied_url: &str,
    kind: usize,
    damage: Option<(u64, Fault)>,
    clean: Option<&str>,
) -> (u64, String) {
    let writer = PostgresHandles::open_writable(
        &PostgresSelection::new(proxied_url, &schema.name),
        BINDING,
        CURSOR,
    )
    .unwrap();
    let port = &writer.acquisition;
    let owner = begun(port);
    fill(port, owner);
    // The unplaced read reads directory 1's children before they are placed.
    if kind != 7 {
        place(port, owner);
    }
    let mut admin = schema.admin();
    let before = state(&mut admin);
    let connections = proxy.pids().len();
    let before_work = writer.diagnostics().unwrap();
    proxy.arm(damage);
    let (name, answer) = read_one(port, owner, kind);
    let messages = proxy.disarm();
    let after_work = writer.diagnostics().unwrap();
    assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
    assert_eq!(before_work.connects, after_work.connects);
    assert_eq!(state(&mut admin), before, "{name}: a read changes no row");
    let answer = format!("{answer:?}");
    match damage {
        None => assert!(answer.starts_with("Ok("), "{name}: {answer}"),
        Some((at, fault)) => {
            let fired = messages.fired.as_ref().expect("the armed fault fired");
            assert_eq!(fired.at, at);
            if fault == Fault::HoldKill {
                assert_eq!(fired.killed, Some(true), "the target backend died");
            }
            let uncertain = format!(
                "{:?}",
                Err::<(), _>(AcquisitionError::Persistence(PersistenceError::Uncertain))
            );
            if fault == Fault::Drop {
                assert_eq!(answer, uncertain, "{name} message {at}");
            }
            // A kill may come after the whole reply was held and passed on:
            // then the read is answered, exactly as the clean read was.
            assert!(
                answer.starts_with("Err(") || Some(answer.as_str()) == clean,
                "{name} {fault:?} at {at}: {answer}"
            );
            let pid = proxy.pids()[fired.connection].expect("the writer's backend pid");
            wait_backend_gone(&mut admin, pid);
            eprintln!("read_fault {name} {fault:?} at {at} {answer}");
        }
    }
    drop(writer);
    (messages.seen, answer)
}

fn sweep(tag: &str, fault: Fault) {
    let Some(url) = pg::url() else { return };
    let (proxy, proxied_url) = FaultProxy::start(&url);
    let schema = pg::Schema::new(url, tag);
    for kind in 0..KINDS {
        schema.drop_schema();
        drop(schema.create(BINDING, CURSOR));
        let (seen, clean) = sample(&schema, &proxy, &proxied_url, kind, None, None);
        assert!(seen > 0);
        eprintln!("read_clean {kind} messages {seen}");
        for at in 0..seen {
            schema.drop_schema();
            drop(schema.create(BINDING, CURSOR));
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
fn reads_drop_at_every_message() {
    sweep("i3a_read_drop", Fault::Drop);
}

#[test]
fn reads_kill_at_every_message() {
    sweep("i3a_read_kill", Fault::HoldKill);
}
