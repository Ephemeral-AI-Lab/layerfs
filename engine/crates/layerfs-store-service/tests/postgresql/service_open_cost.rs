//! Independent #768 tester (not the author's): what a Store service open
//! alone costs, as an SDK-style caller that never mounts pays it, then one
//! SDK Store call and the idle reader's prefill, on SQLite and PostgreSQL.
//!
//! Two views per step: the sessions' own counters (`SqlWork`: connects,
//! round trips, statements, transactions; writer, control reader, Workspace
//! readers) and, on PostgreSQL, the wire seen by a pass-through proxy armed
//! before the connect: server messages per connection and the number of
//! `ReadyForQuery` among them (one per simple query or `Sync`). The test
//! prints `T768_OPEN_COST` lines and asserts one writer connect, no reader
//! connect at open, and the PostgreSQL writer's open at [1, 1, 1, 1] (no
//! mount statements prepared outside the daemon; the open check alone since
//! #776 deleted the four in-place migration probes, 9f84e467b6, which made
//! it [1, 9, 5, 1]).
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;
use crate::fault_proxy;

use adversary::{Service, TestStore};
use fault_proxy::{FaultProxy, Window};
use layerfs_persistence::SqlWork;
use std::time::Instant;

fn work(service: &Service) -> [SqlWork; 3] {
    let opened = &service.service.opened;
    let writer = opened.session_diagnostics(None).unwrap().unwrap();
    let control = opened
        .session_diagnostics(Some(0))
        .unwrap()
        .unwrap_or_default();
    let mut readers = SqlWork::default();
    for reader in &opened.diagnostics().unwrap().readers {
        readers.accumulate(reader);
    }
    [writer, control, readers.since(&control)]
}

fn counts(work: &SqlWork) -> [u64; 4] {
    [
        work.connects,
        work.round_trips,
        work.statements,
        work.transactions,
    ]
}

fn delta(after: &[SqlWork; 3], before: &[SqlWork; 3]) -> [[u64; 4]; 3] {
    let mut out = [[0; 4]; 3];
    for (n, slot) in out.iter_mut().enumerate() {
        *slot = counts(&after[n].since(&before[n]));
    }
    out
}

fn wire(window: &Window) -> String {
    let mut per: std::collections::BTreeMap<usize, String> = Default::default();
    for (connection, kind) in &window.log {
        per.entry(*connection).or_default().push(*kind);
    }
    per.iter()
        .map(|(c, kinds)| {
            format!(
                "c{c}:{{msgs:{},ready:{}}}",
                kinds.len(),
                kinds.chars().filter(|k| *k == 'Z').count()
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn sqlite_open_alone() {
    let store = TestStore::sqlite("t768-open", &[("/seed", b"seed\n")]);
    let started = Instant::now();
    let service = store.open();
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let open = work(&service);
    let open = [counts(&open[0]), counts(&open[1]), counts(&open[2])];
    let before = work(&service);
    let listed = service.call("project.list_branches", vec![]);
    assert!(listed.is_ok(), "{listed:?}");
    let call = delta(&work(&service), &before);
    println!(
        "T768_OPEN_COST {{\"backend\":\"sqlite\",\"open\":{open:?},\"open_ms\":{ms:.1},\"list_branches\":{call:?}}}"
    );
    service.stop();
}

#[test]
fn postgresql_open_alone() {
    let Some(store) = TestStore::postgres("t768-open", &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let selection = store.selection_via(&proxied);
    proxy.arm(None);
    let started = Instant::now();
    let service = Service::open(&store, selection);
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let open_wire = proxy.disarm();
    let open = work(&service);
    let open = [counts(&open[0]), counts(&open[1]), counts(&open[2])];
    assert_eq!(open[0][0], 1, "one writer connect");
    assert_eq!(open[2][0], 0, "no Workspace reader at open");
    // #768 review: only the daemon prepares the writer's mount statements
    // (`OpenedStore::prepare_mounts`); an SDK-style open is back at its
    // cost before step A, less the four migration probes #776 deleted
    // (each [0, 2, 1, 0]: [1, 9, 5, 1] -> [1, 1, 1, 1]).
    assert_eq!(open[0], [1, 1, 1, 1], "the SDK-style writer open");

    let before = work(&service);
    proxy.arm(None);
    let started = Instant::now();
    let listed = service.call("project.list_branches", vec![]);
    let call_ms = started.elapsed().as_secs_f64() * 1e3;
    let call_wire = proxy.disarm();
    assert!(listed.is_ok(), "{listed:?}");
    let call = delta(&work(&service), &before);

    let before = work(&service);
    proxy.arm(None);
    if let Some(prefilled) = service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    let prefill_wire = proxy.disarm();
    let prefill = delta(&work(&service), &before);
    println!(
        "T768_OPEN_COST {{\"backend\":\"postgresql\",\"open\":{open:?},\"open_ms\":{ms:.1},\"open_wire\":\"{}\",\"list_branches\":{call:?},\"list_ms\":{call_ms:.1},\"list_wire\":\"{}\",\"prefill\":{prefill:?},\"prefill_wire\":\"{}\"}}",
        wire(&open_wire),
        wire(&call_wire),
        wire(&prefill_wire)
    );
    service.stop();
}
