//! #818 I3c: a Save that publishes in fenced flights (Namespace Init's
//! Saves, `Save::publishing_in_flights`) on PostgreSQL.
//!
//! A clean finish publishes in one flight (fewer round trips than the same
//! Save published ordinarily, the same Store rows). A drop or a kill at
//! every server message of the finish (reservation, prepares and the flight)
//! leaves the publication atomic: the Store is exactly as before or exactly
//! as the clean finish left it; a dropped reply is never a success; a
//! success always acknowledges stored rows; the session never reconnects or
//! replays.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/mod.rs"]
mod support;

#[path = "../support/pg/chains.rs"]
mod chains;

use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::Storage;
use std::time::{Duration, Instant};
use support::fault_proxy::{Fault, FaultProxy, Window};
use support::pg::{url, Schema};

const BINDING: &[u8] = b"save-flight-fault";
const CURSOR: [u8; 32] = [117; 32];

/// Store totals and the rows a publication writes. (The ordinal counter is
/// left out: a reservation commits apart from the publication.)
fn state(admin: &mut postgres::Client) -> [i64; 7] {
    let row = admin
        .query_one(
            "SELECT canonical, stored, metadata,
                    (SELECT count(*) FROM pack),
                    (SELECT count(*) FROM object_location),
                    (SELECT count(*) FROM inode_value_block),
                    (SELECT count(*) FROM content_signature)
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
    state: [i64; 7],
    round_trips: u64,
}

/// One Save of three whole files on a new Store, finished with `damage`
/// armed; `flights` sets `publishing_in_flights`.
fn sample(
    schema: &Schema,
    proxy: &FaultProxy,
    proxied_url: &str,
    flights: bool,
    damage: Option<(u64, Fault)>,
    clean: Option<&Sample>,
) -> Sample {
    schema.drop_schema();
    drop(schema.create(BINDING, CURSOR));
    let selection = PostgresSelection::new(proxied_url, &schema.name);
    let writer = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let storage = Storage::new(writer.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let save = match flights {
        true => save.publishing_in_flights(),
        false => save,
    };
    for seed in 0..3 {
        save.accept(chains::whole(&chains::noise(
            seed + 7,
            5000 + seed as usize,
        )))
        .unwrap();
    }
    let mut admin = schema.admin();
    let before = state(&mut admin);
    let connections = proxy.pids().len();
    let before_work = writer.diagnostics().unwrap();
    proxy.arm(damage);
    let result = save.finish();
    let messages = proxy.disarm();
    let after_work = writer.diagnostics().unwrap();
    assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
    assert_eq!(before_work.connects, after_work.connects);
    let after = state(&mut admin);
    match damage {
        None => {
            let outcome = result.expect("the clean finish publishes");
            assert_eq!(outcome.inserted, 3);
            assert!(messages.fired.is_none());
            assert_ne!(after, before, "the clean finish stored its rows");
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
                assert!(
                    result.is_err(),
                    "a dropped reply is never a success: message {at}"
                );
            }
            let pid = proxy.pids()[fired.connection].expect("the writer's backend pid");
            wait_backend_gone(&mut admin, pid);
            let after = state(&mut admin);
            if after == before {
                assert!(result.is_err(), "a success acknowledges stored rows: {at}");
            } else {
                assert_eq!(after, clean.state, "the publication is atomic: {at}");
            }
            eprintln!(
                "save_flight_fault {fault:?} at {at} {} {:?}",
                fired.kind,
                result.as_ref().map(|outcome| outcome.inserted)
            );
        }
    }
    Sample {
        messages,
        state: after,
        round_trips: after_work.round_trips - before_work.round_trips,
    }
}

fn sweep(tag: &str, fault: Fault) {
    let Some(url) = url() else { return };
    let (proxy, proxied_url) = FaultProxy::start(&url);
    let schema = Schema::new(url, tag);
    let ordinary = sample(&schema, &proxy, &proxied_url, false, None, None);
    let clean = sample(&schema, &proxy, &proxied_url, true, None, None);
    assert_eq!(clean.state, ordinary.state, "the same rows either way");
    assert!(
        clean.round_trips + 4 <= ordinary.round_trips,
        "one flight: {} round trips, ordinary {}",
        clean.round_trips,
        ordinary.round_trips
    );
    eprintln!(
        "save_flight_clean messages {} round_trips {} (ordinary {})",
        clean.messages.seen, clean.round_trips, ordinary.round_trips
    );
    for at in 0..clean.messages.seen {
        sample(
            &schema,
            &proxy,
            &proxied_url,
            true,
            Some((at, fault)),
            Some(&clean),
        );
    }
}

#[test]
fn save_flight_drop_at_every_message() {
    sweep("i3c_flight_drop", Fault::Drop);
}

#[test]
fn save_flight_kill_at_every_message() {
    sweep("i3c_flight_kill", Fault::HoldKill);
}
