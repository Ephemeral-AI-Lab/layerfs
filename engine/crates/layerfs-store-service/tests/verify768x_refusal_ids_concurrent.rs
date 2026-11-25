//! External #768 check of 2900aa2002 (a definitely refused fused Commit
//! returns its Save's unused ids), not the author's test: the refused
//! Commit races a Commit from ANOTHER service, on PostgreSQL.
//!
//! A trigger on the Commit row (`"commit"`, matched by the refused Commit's
//! own message, so the other service's Commit is never the target) sleeps
//! inside the fused transaction and then raises a definite refusal: `P0001`
//! (Integrity) or `40001` (Busy). Row triggers on `inode_value_block` and
//! `pack` note, in transaction-local settings, the ordinals and pack ids
//! the fused transaction assigned; the Commit-row trigger copies them into
//! sequences before it raises (a sequence is not rolled back), so the test
//! learns exactly which ids the refused publication carried.
//!
//! While the refused transaction sleeps, the other service commits on its
//! own Branch (its reservation waits on the `store_policy` row the fused
//! transaction updated, its fused message on the lane), started either once
//! the refusal's sleep is seen or with no wait at all. Rounds vary the
//! SQLSTATE, the sleep, and whether each service's handle is new (reserves a
//! block) or kept from an earlier Commit.
//!
//! Checked each round: the refused Commit answers a definite refusal and
//! its follow-up on the same Workspace lands; the other service's Commit
//! lands; no stored value block overlaps the refused ordinals and no stored
//! pack id is a refused one (never reused, by either service); both counters
//! stand past them; no duplicated allocation. A round with no other service
//! also checks that the unused block came back: the counter stands right
//! after the refused ordinals (P0001). After a Busy refusal (40001) the
//! handle keeps the block instead (#768 review 6b): the counter stays past
//! it, and the follow-up Commit's values start right after the refused
//! ordinals with no reservation. At the end the Store audit and amcheck.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it. SQLite is not run
//! here: its triggers cannot hold a transaction open while another
//! connection commits (the author's `fused_refusal_ids.rs` covers its
//! serial case).
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;
#[path = "support/verify768x.rs"]
#[allow(dead_code)]
mod verify768x;

use adversary::{apply, commit, verdict, wait_until, write, Op, Service, TestStore, Verdict};
use layerfs_store_service::store::{BoundWorkspace, CommitOptions};
use std::time::Duration;
use verify768x::{alloc, allocation_failures};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
const REFUSE: &str = "v768x-refuse";

fn schema(store: &TestStore) -> String {
    store.postgres.as_ref().unwrap().1.clone()
}

/// Installs the noting triggers and the one-shot refusal on the Commit row.
fn arm(store: &TestStore, code: &str, sleep: f64) {
    let s = schema(store);
    store
        .admin()
        .batch_execute(&format!(
            "CREATE SEQUENCE IF NOT EXISTS {s}.v768x_fire; \
             ALTER SEQUENCE {s}.v768x_fire RESTART; \
             CREATE SEQUENCE IF NOT EXISTS {s}.v768x_ord_lo MINVALUE 0 START 0; \
             CREATE SEQUENCE IF NOT EXISTS {s}.v768x_ord_hi MINVALUE 0 START 0; \
             CREATE SEQUENCE IF NOT EXISTS {s}.v768x_pack_lo MINVALUE 0 START 0; \
             CREATE SEQUENCE IF NOT EXISTS {s}.v768x_pack_hi MINVALUE 0 START 0; \
             SELECT setval('{s}.v768x_ord_lo', 0, false), setval('{s}.v768x_ord_hi', 0, false), \
                    setval('{s}.v768x_pack_lo', 0, false), setval('{s}.v768x_pack_hi', 0, false); \
             CREATE OR REPLACE FUNCTION {s}.v768x_note() RETURNS trigger LANGUAGE plpgsql AS $f$ \
             DECLARE lo bigint; hi bigint; \
             BEGIN \
               IF coalesce(current_setting('layerfs.attempt', true), '') <> '' THEN \
                 IF TG_TABLE_NAME = 'inode_value_block' THEN \
                   lo := least(coalesce(nullif(current_setting('v768x.ord_lo', true), '')::bigint, NEW.first_ordinal), NEW.first_ordinal); \
                   hi := greatest(coalesce(nullif(current_setting('v768x.ord_hi', true), '')::bigint, NEW.last_ordinal), NEW.last_ordinal); \
                   PERFORM set_config('v768x.ord_lo', lo::text, true), set_config('v768x.ord_hi', hi::text, true); \
                 ELSE \
                   lo := least(coalesce(nullif(current_setting('v768x.pack_lo', true), '')::bigint, NEW.pack_id), NEW.pack_id); \
                   hi := greatest(coalesce(nullif(current_setting('v768x.pack_hi', true), '')::bigint, NEW.pack_id), NEW.pack_id); \
                   PERFORM set_config('v768x.pack_lo', lo::text, true), set_config('v768x.pack_hi', hi::text, true); \
                 END IF; \
               END IF; \
               RETURN NEW; \
             END $f$; \
             CREATE OR REPLACE FUNCTION {s}.v768x_refuse() RETURNS trigger LANGUAGE plpgsql AS $f$ \
             BEGIN \
               IF coalesce(current_setting('layerfs.attempt', true), '') <> '' \
                  AND NEW.message = '{REFUSE}' AND nextval('{s}.v768x_fire') = 1 THEN \
                 PERFORM setval('{s}.v768x_ord_lo', coalesce(nullif(current_setting('v768x.ord_lo', true), '')::bigint, 0), true), \
                         setval('{s}.v768x_ord_hi', coalesce(nullif(current_setting('v768x.ord_hi', true), '')::bigint, 0), true), \
                         setval('{s}.v768x_pack_lo', coalesce(nullif(current_setting('v768x.pack_lo', true), '')::bigint, 0), true), \
                         setval('{s}.v768x_pack_hi', coalesce(nullif(current_setting('v768x.pack_hi', true), '')::bigint, 0), true); \
                 PERFORM pg_sleep(TG_ARGV[1]::float8); \
                 RAISE EXCEPTION 'v768x injected refusal' USING ERRCODE = TG_ARGV[0]; \
               END IF; \
               RETURN NEW; \
             END $f$; \
             DROP TRIGGER IF EXISTS v768x_note_v ON {s}.inode_value_block; \
             DROP TRIGGER IF EXISTS v768x_note_p ON {s}.pack; \
             DROP TRIGGER IF EXISTS v768x_refuse ON {s}.\"commit\"; \
             CREATE TRIGGER v768x_note_v BEFORE INSERT ON {s}.inode_value_block FOR EACH ROW \
               EXECUTE FUNCTION {s}.v768x_note(); \
             CREATE TRIGGER v768x_note_p BEFORE INSERT ON {s}.pack FOR EACH ROW \
               EXECUTE FUNCTION {s}.v768x_note(); \
             CREATE TRIGGER v768x_refuse BEFORE INSERT ON {s}.\"commit\" FOR EACH ROW \
               EXECUTE FUNCTION {s}.v768x_refuse('{code}', '{sleep}');"
        ))
        .unwrap();
}

fn disarm(store: &TestStore) {
    let s = schema(store);
    store
        .admin()
        .batch_execute(&format!(
            "DROP TRIGGER IF EXISTS v768x_note_v ON {s}.inode_value_block; \
             DROP TRIGGER IF EXISTS v768x_note_p ON {s}.pack; \
             DROP TRIGGER IF EXISTS v768x_refuse ON {s}.\"commit\";"
        ))
        .unwrap();
}

/// What the refused transaction carried: (fired, ordinals, pack ids).
/// An inclusive id range.
type Range = Option<(i64, i64)>;

fn noted(store: &TestStore) -> (bool, Range, Range) {
    let s = schema(store);
    let row = store
        .admin()
        .query_one(
            &format!(
                "SELECT (SELECT is_called FROM {s}.v768x_fire), \
                 (SELECT last_value FROM {s}.v768x_ord_lo), (SELECT last_value FROM {s}.v768x_ord_hi), \
                 (SELECT last_value FROM {s}.v768x_pack_lo), (SELECT last_value FROM {s}.v768x_pack_hi)"
            ),
            &[],
        )
        .unwrap();
    let range = |lo: i64, hi: i64| (hi > 0).then_some((lo, hi));
    (
        row.get(0),
        range(row.get(1), row.get(2)),
        range(row.get(3), row.get(4)),
    )
}

/// Backends of the test database sleeping in `pg_sleep` (only this test's
/// trigger sleeps).
fn sleeping(store: &TestStore) -> i64 {
    store
        .admin()
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() \
             AND wait_event = 'PgSleep'",
            &[],
        )
        .unwrap()
        .get(0)
}

fn change(workspace: &BoundWorkspace, tag: &str, files: usize) {
    apply(workspace, &Op::Mkdir(format!("/{tag}"))).unwrap();
    for n in 0..files {
        write(
            workspace,
            &format!("/{tag}/f{n}"),
            format!("{tag} {n}\n").as_bytes(),
        );
    }
}

fn refused_commit(workspace: &BoundWorkspace) -> Verdict {
    verdict(&workspace.commit_captured(&CommitOptions {
        force: false,
        message: Some(REFUSE.into()),
        expected: None,
    }))
}

fn close(service: Service) {
    service.service.close().expect("an orderly close");
    service.owner.stop().expect("the overlay owner stops");
}

/// How the other service joins a round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Other {
    /// No other service: the serial case.
    None,
    /// Starts once the refused transaction is seen sleeping.
    WhileSleeping,
    /// Starts together with the refused Commit, no wait.
    AtOnce,
}

#[test]
fn a_refused_fused_commit_never_lends_its_ids_to_another_service() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("v768x-refusal-race", SEED) else {
        return;
    };
    let mut kept_a: Option<Service> = None;
    let mut kept_b: Option<Service> = None;
    let mut summary = Vec::new();
    for round in 0..12usize {
        let code = if round % 2 == 0 { "P0001" } else { "40001" };
        let other = match round % 3 {
            0 => Other::WhileSleeping,
            1 => Other::AtOnce,
            _ => Other::None,
        };
        let sleep = [0.2, 0.5, 0.05][round % 3];
        // A new handle (a reservation) on even rounds of four, else kept.
        let fresh_a = round % 4 < 2 || kept_a.is_none();
        let fresh_b = round % 4 != 1 || kept_b.is_none();
        if fresh_a {
            if let Some(service) = kept_a.take() {
                close(service);
            }
            kept_a = Some(store.open());
        }
        if fresh_b {
            if let Some(service) = kept_b.take() {
                close(service);
            }
            kept_b = Some(store.open());
        }
        let (a, b) = (kept_a.as_ref().unwrap(), kept_b.as_ref().unwrap());
        let refused_ws = a.bind(store.fork(&format!("a{round}")));
        let other_ws = b.bind(store.fork(&format!("b{round}")));
        change(&refused_ws, &format!("a{round}"), 20 + round);
        change(&other_ws, &format!("b{round}"), 20 + round);
        let before = alloc(&store);
        arm(&store, code, sleep);
        let (first, second) = std::thread::scope(|scope| {
            let refusing = scope.spawn(|| refused_commit(&refused_ws));
            let joining = (other != Other::None).then(|| {
                if other == Other::WhileSleeping {
                    assert!(
                        wait_until(Duration::from_secs(20), || sleeping(&store) > 0),
                        "round {round}: the refused transaction never slept"
                    );
                }
                scope.spawn(|| verdict(&commit(&other_ws)))
            });
            (
                refusing.join().unwrap(),
                joining.map(|thread| thread.join().unwrap()),
            )
        });
        disarm(&store);
        let (fired, ords, packs) = noted(&store);
        let after_refusal = alloc(&store);
        let label = format!(
            "round {round} {code} {other:?} sleep {sleep} fresh ({fresh_a}, {fresh_b}): \
             first {first:?}, other {second:?}, refused ordinals {ords:?} packs {packs:?}, \
             {before:?} -> {after_refusal:?}"
        );
        assert!(fired, "{label}: the refusal fired");
        assert!(
            matches!(&first, Verdict::Refused(_)),
            "{label}: a definite refusal"
        );
        let (lo, hi) = ords.unwrap_or_else(|| panic!("{label}: the refused ordinals"));
        // A Busy refusal (40001) sends no release: its handle keeps the
        // unused block for the Workspace's next Commit (#768 review 6b).
        let busy = code == "40001";
        if other == Other::None && !busy {
            // The unused block came back: the counter stands right after
            // the ordinals the refused publication assigned.
            assert_eq!(
                after_refusal.next_ordinal,
                hi + 1,
                "{label}: block returned"
            );
        }
        if other == Other::None && busy {
            // Kept, not returned: the counter stands past the block, so no
            // other handle can be lent any of it.
            assert!(
                after_refusal.next_ordinal > hi + 1
                    && after_refusal.next_ordinal >= before.next_ordinal,
                "{label}: block kept by the handle"
            );
        }
        if other != Other::None {
            assert!(
                matches!(second, Some(Verdict::Committed(_))),
                "{label}: the other service's Commit lands"
            );
        }
        // The follow-up on the refused Workspace lands.
        let again = verdict(&commit(&refused_ws));
        assert!(
            matches!(again, Verdict::Committed(_)),
            "{label}: follow-up {again:?}"
        );
        let blocks = verify768x::value_blocks(&store);
        if other == Other::None && busy {
            // The kept block served the follow-up: its values start right
            // after the refused ordinals, and it reserved nothing (its
            // finish may rewind the counter past its kept 128, never raise
            // it).
            assert!(
                blocks.iter().any(|(first, _)| *first == hi + 1),
                "{label}: follow-up from the kept block: {blocks:?}"
            );
            let follow = alloc(&store).next_ordinal;
            assert!(
                follow <= after_refusal.next_ordinal,
                "{label}: follow-up reserved nothing: {follow}"
            );
        }
        let reused: Vec<_> = blocks
            .iter()
            .filter(|(first, last)| *first <= hi && lo <= *last)
            .collect();
        assert!(
            reused.is_empty(),
            "{label}: refused ordinals reused by {reused:?}"
        );
        if let Some((plo, phi)) = packs {
            let ids = verify768x::pack_ids(&store);
            let reused: Vec<_> = ids.iter().filter(|id| plo <= **id && **id <= phi).collect();
            assert!(
                reused.is_empty(),
                "{label}: refused pack ids reused: {reused:?}"
            );
        }
        let now = alloc(&store);
        assert!(
            now.next_ordinal > hi,
            "{label}: counter behind the refused ordinals"
        );
        if let Some((_, phi)) = packs {
            assert!(
                now.next_pack > phi,
                "{label}: pack counter behind the refused ids"
            );
        }
        let failures = allocation_failures(&store);
        assert!(failures.is_empty(), "{label}: {failures:#?}");
        println!(
            "REFUSAL_ROUND {{\"round\":{round},\"code\":\"{code}\",\"other\":\"{other:?}\",\"fresh\":\"{fresh_a}/{fresh_b}\",\"refused_ordinals\":\"{ords:?}\",\"refused_packs\":\"{packs:?}\",\"counter\":\"{} -> {} -> {}\"}}",
            before.next_ordinal, after_refusal.next_ordinal, now.next_ordinal
        );
        summary.push((round, ords, after_refusal.next_ordinal));
        a.unmount(refused_ws);
        b.unmount(other_ws);
    }
    for service in [kept_a.take(), kept_b.take()].into_iter().flatten() {
        close(service);
    }
    let audit = store.integrity_failures();
    assert!(audit.is_empty(), "audit: {audit:#?}");
    let failures = allocation_failures(&store);
    assert!(failures.is_empty(), "{failures:#?}");
    println!("REFUSAL_SUMMARY {summary:?}");
}
