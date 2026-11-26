//! Diagnostic, second independent writer-reopen round: what the final
//! audit of `verify_unknown_allocators` (`drop_warm_half_0`) has to read,
//! and how long it takes. The same sweep as that test (the shared body,
//! Drop at every even message of a warm Commit's flight, a direct Commit
//! every tenth run), then the Store's size (Commit rows, packs, located
//! objects, schema bytes) and the audit's wall time with a 60 s deadline
//! instead of 30 s, so its whole cost is seen. The sweep's per-run states
//! (committed / unknown, and how the follow-up Commit ended) are printed:
//! they show how many Commits a run lands, which a writer that reopens
//! changes. No assertion on the audit's time: it is the measurement.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#![allow(dead_code)]
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;
include!("support/verify_unknown_body.rs");

use layerfs_persistence::diagnostic::{Options, Outcome};
use layerfs_persistence::Handles;
use std::time::{Duration, Instant};

#[test]
fn audit_cost_drop_warm_half_0() {
    let _slot = verify768::sweep_slot();
    let Some(store) = TestStore::postgres("wr2_audit", SEED) else {
        return;
    };
    let started = Instant::now();
    let (url, schema) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    run(&store, &proxy, &proxied, "prime", Flight::Warm, None);
    let (k, _) = run(&store, &proxy, &proxied, "clean", Flight::Warm, None);
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    let mut direct = 0;
    for n in (0..k).filter(|n| n % 2 == 0) {
        let (_, state) = run(
            &store,
            &proxy,
            &proxied,
            &format!("n{n}"),
            Flight::Warm,
            Some((n, Fault::Drop)),
        );
        *states.entry(state).or_default() += 1;
        if n % 10 == 0 {
            direct += 1;
            let service = store.open();
            let branch = store.fork(&format!("direct{direct}"));
            let workspace = service.bind(branch);
            for op in change(&format!("direct{direct}"), 150) {
                apply(&workspace, &op).unwrap();
            }
            let answer = verdict(&commit(&workspace));
            assert!(matches!(answer, Verdict::Committed(_)), "{answer:?}");
            service.unmount(workspace);
            service.stop();
        }
    }
    let sweep = started.elapsed();
    let mut admin = store.admin();
    let count = |admin: &mut postgres::Client, table: &str| -> i64 {
        admin
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .unwrap()
            .get(0)
    };
    let commits = count(&mut admin, "\"commit\"");
    let packs = count(&mut admin, "pack");
    let located = count(&mut admin, "object_location");
    let bytes: i64 = admin
        .query_one(
            "SELECT coalesce(sum(pg_total_relation_size(c.oid)), 0)::bigint FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = $1 AND c.relkind = 'r'",
            &[&schema],
        )
        .unwrap()
        .get(0);
    let handles =
        Handles::open_read_only(&store.selection, &store.binding, adversary::CURSOR).unwrap();
    let Handles::Postgres(h) = &handles else {
        unreachable!()
    };
    let mut failures = Vec::new();
    let mut observations = 0u64;
    let audit_started = Instant::now();
    let done = h.diagnose(
        Options {
            quick: false,
            deadline: Some(Instant::now() + Duration::from_secs(60)),
            ..Options::default()
        },
        &mut |o: layerfs_persistence::diagnostic::Observation| {
            observations += 1;
            if o.outcome == Outcome::Fail {
                failures.push(format!("{}: {}", o.id, o.message));
            }
        },
    );
    let audit = audit_started.elapsed();
    handles.close();
    println!(
        "TESTERWR2_AUDIT {{\"K\":{k},\"runs\":{},\"states\":{states:?},\"direct\":{direct},\
         \"commits\":{commits},\"packs\":{packs},\"located\":{located},\"bytes\":{bytes},\
         \"sweep_ms\":{},\"audit_ms\":{},\"observations\":{observations},\"done\":{:?},\
         \"failures\":{failures:?}}}",
        states.values().sum::<u64>(),
        sweep.as_millis(),
        audit.as_millis(),
        done.as_ref().map(drop).map_err(|e| format!("{e:?}"))
    );
}
