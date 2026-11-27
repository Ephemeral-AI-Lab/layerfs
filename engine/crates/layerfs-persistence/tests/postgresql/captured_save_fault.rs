//! Fault each server message of an ordinary captured Save's warm publication.
//! PostgreSQL may have committed when the reply goes missing, but the earlier
//! Save's pack and the Branch must survive, and the new batch is atomic.
#[path = "../support/mod.rs"]
mod support;

#[path = "../support/pg/chains.rs"]
mod chains;

use layerfs_history::HistoryCatalog;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::{
    port::{PackPersistence, PersistenceError, Published},
    save::PreparedTail,
    Storage,
};
use std::time::{Duration, Instant};
use support::fault_proxy::{Fault, FaultProxy, Window};
use support::pg::{project, url, Schema};

const BINDING: &[u8] = b"captured-save-fault";
const CURSOR: [u8; 32] = [119; 32];

fn prepare(storage: &Storage, seed: u64) -> PreparedTail<'_> {
    let save = storage.begin_save().unwrap();
    save.accept(chains::whole(&chains::noise(seed, 6000)))
        .unwrap();
    save.finish_prepared().unwrap()
}

/// Observe all affected Store tables, including the statistics that account
/// for the pack, and verify that an acknowledged earlier object is still
/// present independently below.
fn state(admin: &mut postgres::Client) -> [i64; 8] {
    let row = admin
        .query_one(
            "SELECT canonical, stored, metadata,
                    (SELECT count(*) FROM pack),
                    (SELECT count(*) FROM object_location),
                    (SELECT count(*) FROM inode_value_block),
                    (SELECT count(*) FROM content_signature),
                    (SELECT next_ordinal FROM store_policy WHERE id=1)
             FROM store_totals WHERE id=1",
            &[],
        )
        .unwrap();
    std::array::from_fn(|i| row.get(i))
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
            "faulted backend {pid} still owns a session"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct Sample {
    messages: Window,
    state: [i64; 8],
}

fn sample(
    schema: &Schema,
    proxy: &FaultProxy,
    proxied_url: &str,
    damage: Option<(u64, Fault)>,
    clean: Option<&Sample>,
) -> Sample {
    let control = schema.create(BINDING, CURSOR);
    let (_, branch, _) = project(&control.history, 1);
    let head = control.history.branch_snapshot(branch).unwrap();
    let selection = PostgresSelection::new(proxied_url, &schema.name);
    let writer = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let storage = Storage::new(writer.storage.clone()).unwrap();

    let warm = prepare(&storage, 101);
    let first = warm.publication().unwrap();
    let first_id = first.objects[0].object_id;
    let first_pack = first.packs[0].info().pack_id;
    assert_eq!(
        writer.storage.publish_captured(first),
        Ok(Published::default())
    );
    warm.acknowledged().unwrap();

    let tail = prepare(&storage, 102);
    let batch = tail.publication().unwrap();
    assert_eq!((batch.packs.len(), batch.objects.len()), (1, 1));
    let location = batch.objects[0];
    let first_connection_count = proxy.pids().len();
    let before_work = writer.diagnostics().unwrap();
    let mut admin = schema.admin();
    let before_state = state(&mut admin);
    proxy.arm(damage);
    let result = writer.storage.publish_captured(batch);
    let messages = proxy.disarm();
    let after_work = writer.diagnostics().unwrap();
    assert_eq!(
        proxy.pids().len(),
        first_connection_count,
        "the publication never reconnects or replays"
    );
    assert_eq!(before_work.connects, after_work.connects);
    match damage {
        None => {
            assert_eq!(result, Ok(Published::default()));
            assert!(messages.fired.is_none());
            assert_eq!(after_work.prepares, before_work.prepares);
            assert_eq!(after_work.round_trips - before_work.round_trips, 1);
        }
        Some((at, fault)) => {
            let fired = messages.fired.as_ref().expect("the armed fault fired");
            let clean = clean.unwrap();
            assert_eq!(fired.at, at);
            assert_eq!(fired.kind, clean.messages.log[at as usize].1);
            assert_eq!(fired.connection + 1, first_connection_count);
            if fault == Fault::HoldKill {
                assert_eq!(fired.killed, Some(true), "the target backend died");
            }
            if fault == Fault::Drop {
                // Dropping any reply prevents a complete acknowledgement,
                // even if the server committed before the connection closed.
                assert_eq!(result, Err(PersistenceError::Uncertain), "message {at}");
            } else {
                assert!(
                    matches!(result, Ok(_) | Err(PersistenceError::Uncertain)),
                    "message {at} {fired:?}: {result:?}"
                );
            }
            let pid = proxy.pids()[fired.connection].expect("the writer's backend pid");
            wait_backend_gone(&mut admin, pid);
            eprintln!(
                "captured_save_fault {fault:?} at {at} {} {result:?}",
                fired.kind
            );
        }
    }

    let after_state = state(&mut admin);
    let target_packs: i64 = admin
        .query_one(
            "SELECT count(*) FROM pack WHERE pack_id=$1",
            &[&location.pack_id],
        )
        .unwrap()
        .get(0);
    let mut located = Vec::new();
    control
        .storage
        .locate(&[first_id, location.object_id], &mut located)
        .unwrap();
    assert!(located
        .iter()
        .any(|found| found.location.object_id == first_id && found.pack.pack_id == first_pack));
    assert_eq!(
        located
            .iter()
            .filter(|found| found.location == location)
            .count() as i64,
        target_packs,
        "a pack and its sole new location are atomic"
    );
    match (target_packs, clean) {
        (0, _) => {
            assert!(result.is_err(), "a success must acknowledge stored data");
            assert_eq!(after_state, before_state);
        }
        (1, Some(clean)) => assert_eq!(after_state, clean.state),
        (1, None) => {
            assert_eq!(
                after_state[0] - before_state[0],
                location.canonical_length as i64
            );
            assert_eq!(
                after_state[1] - before_state[1],
                batch.packs[0].body().len() as i64
            );
            assert_eq!(after_state[3] - before_state[3], 1);
            assert_eq!(after_state[4] - before_state[4], 1);
        }
        _ => panic!("unexpected pack count {target_packs}"),
    }
    assert_eq!(control.history.branch_snapshot(branch).unwrap(), head);
    Sample {
        messages,
        state: after_state,
    }
}

fn sweep(tag: &str, fault: Fault) {
    let Some(url) = url() else { return };
    let (proxy, proxied_url) = FaultProxy::start(&url);
    let schema = Schema::new(url, tag);
    let clean = sample(&schema, &proxy, &proxied_url, None, None);
    assert!(clean.messages.seen > 0);
    eprintln!("captured_save_clean_messages {}", clean.messages.seen);
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
fn captured_save_drop_at_every_message() {
    sweep("capture_save_drop", Fault::Drop);
}

#[test]
fn captured_save_kill_at_every_message() {
    sweep("capture_save_kill", Fault::HoldKill);
}
