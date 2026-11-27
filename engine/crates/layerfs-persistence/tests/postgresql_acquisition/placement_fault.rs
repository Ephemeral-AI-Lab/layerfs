//! #818 I2a: drop and kill every server message of one wide-directory
//! placement unit on PostgreSQL. The unit holds two set windows (each a
//! dependency check and one placement statement, the new messages) and a
//! one-child window (the per-row bind and place). Whatever is lost, the unit
//! is atomic: the working rows and charges are either exactly as before it
//! or exactly as the clean run left them; an answer that is not uncertain
//! is never a refusal of a committed unit nor a success of a rolled-back
//! one; the session never reconnects or replays.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::support;

use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::port::{acquisition::*, PersistenceError};
use std::time::{Duration, Instant};
use support::pg::fault_proxy::{Fault, FaultProxy, Window};
use support::pg::{url, Schema};

const BINDING: &[u8] = b"placement-fault";
const CURSOR: [u8; 32] = [121; 32];

fn identity(inode: u64) -> HostIdentity {
    HostIdentity {
        device: 1,
        inode,
        evidence: [7; EVIDENCE_BYTES],
    }
}

fn entry(
    name: &str,
    kind: InodeKind,
    position: Option<u64>,
    host: Option<HostIdentity>,
) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: (!name.is_empty()).then_some(0),
            name: name.as_bytes().to_vec(),
        },
        position,
        kind,
        metadata: layerfs_content::filesystem::attributes::PortableMetadata {
            mode: if kind == InodeKind::Symlink {
                0o777
            } else {
                0o644
            },
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        },
        target_root: (kind == InodeKind::Symlink).then(|| ObjectId::for_bytes(b"target")),
        host_path: (kind != InodeKind::Symlink).then(|| format!("/source/{name}").into_bytes()),
        host,
    }
}

/// The setup rows and the unit: window one `f00..f09, x, d, s` (`x` is an
/// alias of the stored identity 900), cut before `y` (the identity of
/// `f00`); window two `y, f10..f14`, cut before `w` (the identity of `f10`),
/// which runs alone.
fn shape() -> (Vec<NewEntry>, Vec<Placed>) {
    let mut rows = vec![
        entry("", InodeKind::Directory, Some(0), None),
        entry(
            "stored",
            InodeKind::RegularFile,
            Some(30),
            Some(identity(900)),
        ),
    ];
    let mut placed = Vec::new();
    let mut add = |name: &str, kind: InodeKind, inode: Option<u64>| {
        let host = inode.map(identity);
        rows.push(entry(name, kind, None, host));
        placed.push(Placed {
            name: name.as_bytes().to_vec(),
            position: 100 + placed.len() as u64,
            host: host.map(|host| (host, format!("/source/{name}").into_bytes())),
        });
    };
    for n in 0..10 {
        add(&format!("f{n:02}"), InodeKind::RegularFile, Some(n));
    }
    add("x", InodeKind::RegularFile, Some(900));
    add("d", InodeKind::Directory, None);
    add("s", InodeKind::Symlink, None);
    add("y", InodeKind::RegularFile, Some(0));
    for n in 10..15 {
        add(&format!("f{n:02}"), InodeKind::RegularFile, Some(n));
    }
    add("w", InodeKind::RegularFile, Some(10));
    (rows, placed)
}

fn state(admin: &mut postgres::Client) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT concat_ws('|',parent_position,encode(name,'hex'),coalesce(position,-9),kind,coalesce(canonical_position,-9),coalesce(encode(host_identity,'hex'),'-')) FROM init_entry ORDER BY parent_position,name",
        "SELECT concat_ws('|',canonical_position,encode(device,'hex'),encode(inode,'hex'),encode(host_path,'hex'),aliases) FROM init_host_file ORDER BY canonical_position",
        "SELECT concat_ws('|',held_rows,held_bytes,peak_rows,peak_bytes) FROM init_operation",
    ] {
        let found = admin.query(sql, &[]).unwrap();
        rows.extend(found.iter().map(|row| row.get::<_, String>(0)));
    }
    rows
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

struct Sample {
    messages: Window,
    state: Vec<String>,
    answer: AcquisitionResult<Vec<u64>>,
}

fn sample(
    schema: &Schema,
    proxy: &FaultProxy,
    proxied_url: &str,
    damage: Option<(u64, Fault)>,
    clean: Option<&Sample>,
) -> Sample {
    drop(schema.create(BINDING, CURSOR));
    let writer = PostgresHandles::open_writable(
        &PostgresSelection::new(proxied_url, &schema.name),
        BINDING,
        CURSOR,
    )
    .unwrap();
    let port = &writer.acquisition;
    let owner = port
        .begin(&Begin {
            source_device: 1,
            source_inode: 2,
            stack: [3; 16],
            scope: ObjectId::for_bytes(b"scope"),
        })
        .unwrap();
    let (rows, placed) = shape();
    port.put_entries(owner, &rows).unwrap();
    let mut admin = schema.admin();
    let before = state(&mut admin);
    let connections = proxy.pids().len();
    let before_work = writer.diagnostics().unwrap();
    proxy.arm(damage);
    let answer = port.place_children(owner, 0, &placed);
    let messages = proxy.disarm();
    let after_work = writer.diagnostics().unwrap();
    assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
    assert_eq!(before_work.connects, after_work.connects);
    match damage {
        None => {
            assert!(answer.is_ok(), "{answer:?}");
            assert!(messages.fired.is_none());
        }
        Some((at, fault)) => {
            let fired = messages.fired.as_ref().expect("the armed fault fired");
            let clean = clean.unwrap();
            assert_eq!(fired.at, at);
            assert_eq!(fired.kind, clean.messages.log[at as usize].1);
            if fault == Fault::HoldKill {
                assert_eq!(fired.killed, Some(true), "the target backend died");
            }
            if fault == Fault::Drop {
                assert_eq!(
                    answer,
                    Err(AcquisitionError::Persistence(PersistenceError::Uncertain)),
                    "message {at}"
                );
            }
            let pid = proxy.pids()[fired.connection].expect("the writer's backend pid");
            wait_backend_gone(&mut admin, pid);
            eprintln!(
                "placement_fault {fault:?} at {at} {} {answer:?}",
                fired.kind
            );
        }
    }
    let after = state(&mut admin);
    let uncertain = answer == Err(AcquisitionError::Persistence(PersistenceError::Uncertain));
    match clean {
        None => assert_ne!(after, before, "the clean unit placed its children"),
        Some(clean) if after == clean.state => {
            assert!(
                answer == clean.answer || uncertain,
                "a committed unit is answered as committed or uncertain: {answer:?}"
            );
        }
        Some(_) => {
            assert_eq!(after, before, "a unit that did not commit left no row");
            assert!(answer.is_err(), "a rolled-back unit never succeeds");
        }
    }
    drop(writer);
    Sample {
        messages,
        state: after,
        answer,
    }
}

fn sweep(tag: &str, fault: Fault) {
    let Some(url) = url() else { return };
    let (proxy, proxied_url) = FaultProxy::start(&url);
    let schema = Schema::new(url, tag);
    let clean = sample(&schema, &proxy, &proxied_url, None, None);
    assert!(clean.messages.seen > 0);
    eprintln!("placement_clean_messages {}", clean.messages.seen);
    for at in 0..clean.messages.seen {
        schema.drop_schema();
        sample(
            &schema,
            &proxy,
            &proxied_url,
            Some((at, fault)),
            Some(&clean),
        );
    }
}

#[test]
fn placement_drop_at_every_message() {
    sweep("i2_place_drop", Fault::Drop);
}

#[test]
fn placement_kill_at_every_message() {
    sweep("i2_place_kill", Fault::HoldKill);
}
