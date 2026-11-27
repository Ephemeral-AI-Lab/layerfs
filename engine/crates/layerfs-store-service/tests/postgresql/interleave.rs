//! Independent #768 tester (not the author's): seeded random schedules of
//! Workspace mounts, unmounts, Commits, path removals, Branch removals and
//! compaction attempts on one Store, from two or three Store services on
//! PostgreSQL (one service with several Workspaces on SQLite), checked
//! against a model of which mounts are live; then two concurrent races
//! (mount/unmount against path removals, and against compaction) checked
//! by the calls' time intervals.
//!
//! Asserted, sequential schedules:
//! * a mount of a live Branch is admitted (`StaleStore` only on a service
//!   opened before an admitted compaction; that service is then replaced);
//!   on PostgreSQL its row and its three keys are there right after;
//! * `project.remove_file` is admitted exactly when no mount of the project
//!   is live, `branch.remove` exactly when no mount of that Branch is live,
//!   and a compaction only when no mount is live;
//! * every path removed is hidden in every Workspace mounted after it;
//! * at the end: no live intent row, no key held, no open transaction, and
//!   the Store audit is clean.
//!
//! Asserted, concurrent races: an exclusive call answered `Ok` never lies
//! inside one mount's live interval (from its `Ok` to its unmount's start);
//! a mount whose call started after a removal answered hides the path, one
//! answered before the removal started shows it; every admitted mount's
//! row and keys are seen right after (PostgreSQL).
//!
//! The PostgreSQL tests need `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "mount_tester.rs"]
#[allow(dead_code)]
mod tester768;

use adversary::{commit, tree_of, verdict, write, Kind, Service, TestStore, Verdict};
use layerfs_history::{BranchId, HistoryError};
use layerfs_persistence::{CompactRequest, Handles, SqliteHandles, StoreSelection};
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::store::BoundWorkspace;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tester768::*;

const VICTIMS: usize = 12;

fn seed() -> Vec<(String, Vec<u8>)> {
    let mut seed = vec![("/seed".to_owned(), b"seed\n".to_vec())];
    for n in 0..VICTIMS {
        seed.push((format!("/v{n}"), format!("victim {n}\n").into_bytes()));
    }
    seed
}

fn sqlite(label: &str) -> TestStore {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    TestStore::sqlite(label, &seed)
}

fn postgres(label: &str) -> Option<TestStore> {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    TestStore::postgres(label, &seed)
}

/// xorshift64*: a failing schedule is reproduced from its seed.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn compact(store: &TestStore, op: u64) -> String {
    let mut id = [0xEEu8; 16];
    id[..8].copy_from_slice(&op.to_le_bytes());
    let request = CompactRequest {
        op_id: id,
        ..CompactRequest::default()
    };
    let result = match &store.selection {
        StoreSelection::Postgres(_) => {
            let Handles::Postgres(handles) = store.handles() else {
                unreachable!()
            };
            let result = handles.compact(&request);
            handles.close();
            result
        }
        StoreSelection::Sqlite(config) => {
            match SqliteHandles::open_writable(config.clone(), &store.binding, adversary::CURSOR) {
                Ok(handles) => handles.compact(&request),
                Err(error) => return format!("open: {error:?}"),
            }
        }
    };
    match result {
        Ok(_) => "Ok".into(),
        Err(HistoryError::MountsActive(_)) => "MountsActive".into(),
        Err(HistoryError::Busy | HistoryError::BusyOp(_)) => "Busy".into(),
        Err(other) => format!("{other:?}"),
    }
}

fn code(answer: &Result<SdkValue, layerfs_protocol::sdk::SdkRefusal>) -> String {
    match answer {
        Ok(_) => "Ok".into(),
        Err(refusal) => refusal.code.clone(),
    }
}

fn excluded(code: &str) -> bool {
    code == "MountsActive" || code == "Busy"
}

struct Live {
    service: usize,
    branch: String,
    workspace: BoundWorkspace,
}

struct Slot {
    service: Service,
    /// Compactions admitted when this service opened.
    opened_at: usize,
}

/// Replaces service `s`, answered `StaleStore`: only after a compaction
/// admitted since it opened, and only with none of its mounts live.
fn reopen(
    store: &TestStore,
    slots: &mut [Slot],
    s: usize,
    compactions: usize,
    live: &[Live],
    label: &str,
) {
    assert!(
        compactions > slots[s].opened_at,
        "{label}: StaleStore with no compaction since the open"
    );
    assert!(
        live.iter().all(|l| l.service != s),
        "{label}: a stale service with live mounts"
    );
    let old = std::mem::replace(
        &mut slots[s],
        Slot {
            service: store.open(),
            opened_at: compactions,
        },
    );
    old.service.stop();
}

/// The Branch key of `name` is held by exactly the services mounting it,
/// the Store and project keys by exactly the services mounting anything.
fn check_keys(
    store: &TestStore,
    admin: &mut postgres::Client,
    live: &[Live],
    name: &str,
    label: &str,
) {
    let me = pid_of(admin);
    let held = key_holders(admin, &mount_keys(store, store.branch(name)), &[me]);
    let on_branch: BTreeSet<usize> = live
        .iter()
        .filter(|l| l.branch == name)
        .map(|l| l.service)
        .collect();
    let any: BTreeSet<usize> = live.iter().map(|l| l.service).collect();
    let count = |k: usize| held.iter().filter(|(n, _, _)| *n == k).count();
    assert_eq!(
        count(2),
        on_branch.len(),
        "{label}: Branch key of {name}: {held:?}"
    );
    assert_eq!(count(1), any.len(), "{label}: project key: {held:?}");
    assert_eq!(count(0), any.len(), "{label}: Store key: {held:?}");
    assert!(
        held.iter().all(|(_, _, mode)| mode == "ShareLock"),
        "{label}: {held:?}"
    );
}

/// One seeded sequential schedule of `steps` calls.
fn schedule(store: &TestStore, seed: u64, steps: usize) {
    let mut rng = Rng(seed | 1);
    let services_n = if store.kind == Kind::Postgres { 3 } else { 1 };
    let mut compactions = 0usize;
    let mut slots: Vec<Slot> = (0..services_n)
        .map(|_| Slot {
            service: store.open(),
            opened_at: 0,
        })
        .collect();
    let mut branches: Vec<String> = Vec::new();
    let mut forks = 0usize;
    let mut fork = |branches: &mut Vec<String>| {
        let name = format!("b{seed:x}x{forks}");
        forks += 1;
        store.fork(&name);
        branches.push(name);
    };
    for _ in 0..3 {
        fork(&mut branches);
    }
    let mut removed_branches: Vec<String> = Vec::new();
    let mut live: Vec<Live> = Vec::new();
    let mut removed: BTreeSet<String> = BTreeSet::new();
    let mut tally: BTreeMap<String, u64> = BTreeMap::new();
    let mut files = 0usize;
    let mut admin = store.postgres.as_ref().map(|_| store.admin());
    for step in 0..steps {
        let pick = rng.below(100);
        let label = format!("seed {seed:#x} step {step}");
        let what: String = if pick < 30 {
            // Mount, on a live Branch or (rarely) a removed one.
            let s = rng.below(slots.len());
            let (name, gone) = if !removed_branches.is_empty() && rng.below(8) == 0 {
                (
                    removed_branches[rng.below(removed_branches.len())].clone(),
                    true,
                )
            } else {
                (branches[rng.below(branches.len())].clone(), false)
            };
            let branch = store.branch(&name);
            match slots[s].service.try_bind(branch) {
                Ok(workspace) => {
                    assert!(!gone, "{label}: a removed Branch {name} was mounted");
                    let tree = tree_of(&workspace);
                    for path in &removed {
                        assert!(
                            !tree.contains_key(path),
                            "{label}: {path} removed earlier is visible in a new mount"
                        );
                    }
                    live.push(Live {
                        service: s,
                        branch: name.clone(),
                        workspace,
                    });
                    if let Some(admin) = admin.as_mut() {
                        let rows = live_mounts(admin, branch);
                        let model = live.iter().filter(|l| l.branch == name).count() as i64;
                        assert_eq!(rows, model, "{label}: live rows of {name}");
                        check_keys(store, admin, &live, &name, &label);
                    }
                    "mount.ok".into()
                }
                Err(error) if gone && error.contains("Missing") => "mount.removed_branch".into(),
                Err(error) if error.contains("StaleStore") => {
                    reopen(store, &mut slots, s, compactions, &live, &label);
                    "mount.stale_reopened".into()
                }
                Err(error) => panic!("{label}: mount of {name} refused: {error}"),
            }
        } else if pick < 50 {
            if live.is_empty() {
                continue;
            }
            let at = rng.below(live.len());
            let gone = live.swap_remove(at);
            let branch = store.branch(&gone.branch);
            slots[gone.service]
                .service
                .try_unmount(gone.workspace)
                .unwrap_or_else(|error| panic!("{label}: unmount: {error}"));
            if let Some(admin) = admin.as_mut() {
                let model = live.iter().filter(|l| l.branch == gone.branch).count() as i64;
                assert_eq!(
                    live_mounts(admin, branch),
                    model,
                    "{label}: rows after unmount"
                );
                check_keys(store, admin, &live, &gone.branch, &label);
            }
            "unmount.ok".into()
        } else if pick < 68 {
            if live.is_empty() {
                continue;
            }
            let at = rng.below(live.len());
            files += 1;
            write(
                &live[at].workspace,
                &format!("/w{files}"),
                format!("file {files}\n").as_bytes(),
            );
            match verdict(&commit(&live[at].workspace)) {
                Verdict::Committed(_) => "commit.ok".into(),
                Verdict::Conflicted => "commit.conflicted".into(),
                other => panic!("{label}: Commit {other:?}"),
            }
        } else if pick < 80 {
            let s = rng.below(slots.len());
            let victim = format!("/v{}", rng.below(VICTIMS));
            let answer = code(&slots[s].service.call(
                "project.remove_file",
                vec![("path", SdkValue::text(victim.clone()))],
            ));
            if answer == "StaleStore" {
                reopen(store, &mut slots, s, compactions, &live, &label);
            } else if live.is_empty() {
                assert_eq!(answer, "Ok", "{label}: remove_file {victim} with no mount");
                removed.insert(victim);
            } else {
                assert!(
                    excluded(&answer),
                    "{label}: remove_file {victim} beside {} live mounts answered {answer}",
                    live.len()
                );
            }
            format!("remove_file.{answer}")
        } else if pick < 92 {
            let s = rng.below(slots.len());
            let at = rng.below(branches.len());
            let name = branches[at].clone();
            let answer = code(&slots[s].service.call(
                "branch.remove",
                vec![("branch", SdkValue::text(name.clone()))],
            ));
            let mounted = live.iter().any(|l| l.branch == name);
            if answer == "StaleStore" {
                reopen(store, &mut slots, s, compactions, &live, &label);
            } else if mounted {
                assert!(
                    excluded(&answer),
                    "{label}: branch.remove {name} mounted: {answer}"
                );
            } else {
                assert_eq!(answer, "Ok", "{label}: branch.remove {name} unmounted");
                branches.remove(at);
                removed_branches.push(name);
                fork(&mut branches);
            }
            format!("branch.remove.{answer}")
        } else {
            let answer = compact(store, seed.wrapping_add(step as u64));
            if !live.is_empty() {
                assert!(
                    excluded(&answer),
                    "{label}: compaction beside live mounts: {answer}"
                );
            }
            if answer == "Ok" {
                compactions += 1;
            }
            format!("compact.{answer}")
        };
        *tally.entry(what).or_default() += 1;
    }
    for gone in live.drain(..) {
        slots[gone.service]
            .service
            .try_unmount(gone.workspace)
            .unwrap_or_else(|error| panic!("seed {seed:#x}: final unmount: {error}"));
    }
    for slot in slots {
        slot.service.stop();
    }
    println!(
        "T768_SCHEDULE {{\"backend\":\"{:?}\",\"seed\":\"{seed:#x}\",\"tally\":{tally:?},\"removed\":{removed:?}}}",
        store.kind
    );
    if let Some(admin) = admin.as_mut() {
        let rows = live_rows(admin);
        assert!(
            rows.is_empty(),
            "seed {seed:#x}: live rows at the end: {rows:?}"
        );
        let me = pid_of(admin);
        let mut keys = vec![store_key(store), mount_keys(store, store.branch("x"))[1]];
        for name in branches.iter().chain(&removed_branches) {
            keys.push(mount_keys(store, store.branch(name))[2]);
        }
        let held = key_holders(admin, &keys, &[me]);
        assert!(held.is_empty(), "seed {seed:#x}: keys at the end: {held:?}");
        assert!(store.no_open_transaction());
    }
}

const SEEDS: [u64; 3] = [0x7681, 0xBEEF_0768, 0x0DD5_EED5];

#[test]
fn sqlite_sequential_schedules() {
    for seed in SEEDS {
        let store = sqlite(&format!("t768-seq-{seed:x}"));
        schedule(&store, seed, 70);
        let failures = store.integrity_failures();
        assert!(failures.is_empty(), "audit: {failures:#?}");
    }
}

#[test]
fn postgresql_sequential_schedules() {
    for seed in SEEDS {
        let Some(store) = postgres(&format!("t768-seq-{seed:x}")) else {
            return;
        };
        schedule(&store, seed, 70);
        let failures = store.integrity_failures();
        assert!(failures.is_empty(), "audit: {failures:#?}");
    }
}

/// One timed call: [start, end) in ms since the race began, and its answer.
#[derive(Clone, Debug)]
struct Timed {
    start: f64,
    end: f64,
    what: String,
    answer: String,
}

/// One mount cycle of the racing mounter: call start, Ok time, unmount
/// start, and which victims its tree showed.
#[derive(Clone, Debug)]
struct Cycle {
    start: f64,
    ok: f64,
    unmount: f64,
    present: BTreeSet<String>,
}

fn ms(origin: Instant) -> f64 {
    origin.elapsed().as_secs_f64() * 1e3
}

/// Thread A mounts and unmounts Branch `a` `cycles` times; thread B runs
/// `exclusive` calls with small gaps. Returns both logs.
fn race(
    store: &TestStore,
    origin: Instant,
    cycles: usize,
    exclusive: &(dyn Fn(usize, &Service) -> Timed + Sync),
    calls: usize,
) -> (Vec<Cycle>, Vec<Timed>, usize) {
    let branch: BranchId = store.fork("a");
    let mounter = Mutex::new(store.open());
    let other = store.open();
    let reopened = Mutex::new(0usize);
    let refused = Mutex::new(0usize);
    let (cycles, calls) = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            let mut admin = store.postgres.as_ref().map(|_| store.admin());
            let mut log = Vec::new();
            for _ in 0..cycles {
                let start = ms(origin);
                let bound = mounter.lock().unwrap().try_bind(branch);
                let workspace = match bound {
                    Ok(workspace) => workspace,
                    Err(error) if error.contains("StaleStore") => {
                        let mut guard = mounter.lock().unwrap();
                        let old = std::mem::replace(&mut *guard, store.open());
                        old.stop();
                        *reopened.lock().unwrap() += 1;
                        continue;
                    }
                    Err(error)
                        if error.contains("BusyOp")
                            || error.contains("Busy")
                            || error.contains("MountsActive") =>
                    {
                        *refused.lock().unwrap() += 1;
                        continue;
                    }
                    Err(error) => panic!("race mount refused: {error}"),
                };
                let ok = ms(origin);
                if let Some(admin) = admin.as_mut() {
                    assert!(
                        live_mounts(admin, branch) >= 1,
                        "an admitted mount has its row"
                    );
                    let me = pid_of(admin);
                    let held = key_holders(admin, &mount_keys(store, branch), &[me]);
                    // Another backend may hold the Store key shared for a
                    // moment (a removal's attempt): one backend holds all three.
                    let mut per: BTreeMap<i32, usize> = BTreeMap::new();
                    for (_, pid, _) in &held {
                        *per.entry(*pid).or_default() += 1;
                    }
                    assert!(
                        per.values().any(|n| *n == 3),
                        "an admitted mount holds its keys: {held:?}"
                    );
                }
                let tree = tree_of(&workspace);
                let present = tree
                    .keys()
                    .filter(|p| p.starts_with("/v"))
                    .cloned()
                    .collect();
                std::thread::sleep(Duration::from_millis(3));
                let unmount = ms(origin);
                mounter
                    .lock()
                    .unwrap()
                    .try_unmount(workspace)
                    .unwrap_or_else(|error| panic!("race unmount: {error}"));
                log.push(Cycle {
                    start,
                    ok,
                    unmount,
                    present,
                });
                // Gaps of 0 to 9 ms between cycles: windows for the
                // exclusive calls to land in.
                std::thread::sleep(Duration::from_millis((log.len() as u64 * 3) % 10));
            }
            log
        });
        let b = scope.spawn(|| {
            let service = &other;
            (0..calls)
                .map(|n| {
                    std::thread::sleep(Duration::from_millis(2 + (n as u64 * 7) % 11));
                    exclusive(n, service)
                })
                .collect::<Vec<_>>()
        });
        (a.join().unwrap(), b.join().unwrap())
    });
    let reopened = *reopened.lock().unwrap();
    println!("T768_RACE_MOUNTS_REFUSED {}", refused.lock().unwrap());
    mounter.into_inner().unwrap().stop();
    other.stop();
    (cycles, calls, reopened)
}

fn path_race(store: TestStore, label: &str) {
    let origin = Instant::now();
    let remove = |n: usize, service: &Service| {
        let victim = format!("/v{}", n % VICTIMS);
        let start = ms(origin);
        let answer = code(&service.call(
            "project.remove_file",
            vec![("path", SdkValue::text(victim.clone()))],
        ));
        Timed {
            start,
            end: ms(origin),
            what: victim,
            answer,
        }
    };
    let (cycles, calls, _) = race(&store, origin, 60, &remove, 3 * VICTIMS);
    let admitted: Vec<&Timed> = calls.iter().filter(|c| c.answer == "Ok").collect();
    let codes: BTreeMap<String, usize> = calls.iter().fold(BTreeMap::new(), |mut m, c| {
        *m.entry(c.answer.clone()).or_default() += 1;
        m
    });
    println!(
        "T768_PATH_RACE {{\"backend\":\"{:?}\",\"cycles\":{},\"answers\":{codes:?}}}",
        store.kind,
        cycles.len()
    );
    for call in &calls {
        assert!(
            call.answer == "Ok" || excluded(&call.answer),
            "{label}: remove_file answered {}",
            call.answer
        );
    }
    check_race_margin(&cycles, &calls, label, 1.0);
    for call in &admitted {
        for cycle in &cycles {
            if cycle.start > call.end + 1.0 {
                assert!(
                    !cycle.present.contains(&call.what),
                    "{label}: {} removed at {:.1} visible in a mount started at {:.1}",
                    call.what,
                    call.end,
                    cycle.start
                );
            }
        }
    }
    check_hidden(&cycles, &admitted, label);
    if store.kind == Kind::Postgres {
        let mut admin = store.admin();
        assert!(live_rows(&mut admin).is_empty());
        assert!(store.no_open_transaction());
    }
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

/// Every victim a mount's tree hid is explained by an admitted removal of
/// it that could have landed before the mount read its markers: one that
/// completed before the mount answered (its markers are read before that),
/// or one that overlapped the mount's admission (a removal may correctly
/// land while the admission is in flight). Both started before the mount
/// answered (`cycle.ok`); a removal that started after it cannot explain a
/// hidden path, and a hidden path no admitted removal names fails.
fn check_hidden(cycles: &[Cycle], admitted: &[&Timed], label: &str) {
    for cycle in cycles {
        for n in 0..VICTIMS {
            let victim = format!("/v{n}");
            if cycle.present.contains(&victim) {
                continue;
            }
            let explained = admitted.iter().any(|c| {
                let completed = c.end < cycle.ok;
                let overlapped = c.start < cycle.ok && c.end > cycle.start;
                c.what == victim && (completed || overlapped)
            });
            assert!(
                explained,
                "{label}: {victim} hidden in the mount at [{:.1}, {:.1}] with no removal \
                 that could explain it: {:?}",
                cycle.start,
                cycle.ok,
                admitted
                    .iter()
                    .filter(|c| c.what == victim)
                    .map(|c| (c.start, c.end))
                    .collect::<Vec<_>>()
            );
        }
    }
}

fn check_race_margin(cycles: &[Cycle], calls: &[Timed], label: &str, margin: f64) {
    for call in calls.iter().filter(|c| c.answer == "Ok") {
        for cycle in cycles {
            assert!(
                !(cycle.ok + margin < call.start && cycle.unmount > call.end + margin),
                "{label}: {call:?} admitted inside a live mount {cycle:?}"
            );
        }
    }
}

#[test]
fn postgresql_mounts_race_path_removals() {
    let Some(store) = postgres("t768-race-path") else {
        return;
    };
    path_race(store, "postgresql path race");
}

#[test]
fn postgresql_mounts_race_compaction() {
    let Some(store) = postgres("t768-race-compact") else {
        return;
    };
    // Dead bytes for a rebuild: a removed Branch's file.
    {
        let service = store.open();
        let trash = store.fork("trash");
        let workspace = service.bind(trash);
        write(&workspace, "/dead", &adversary::random_bytes(7, 1 << 20));
        assert!(matches!(
            verdict(&commit(&workspace)),
            Verdict::Committed(_)
        ));
        service.unmount(workspace);
        assert!(service
            .call("branch.remove", vec![("branch", SdkValue::text("trash"))])
            .is_ok());
        service.stop();
    }
    let origin = Instant::now();
    let compacting = |n: usize, _: &Service| {
        let start = ms(origin);
        let answer = compact(&store, 0x1000 + n as u64);
        Timed {
            start,
            end: ms(origin),
            what: "compact".into(),
            answer,
        }
    };
    let (cycles, calls, reopened) = race(&store, origin, 50, &compacting, 12);
    let codes: BTreeMap<String, usize> = calls.iter().fold(BTreeMap::new(), |mut m, c| {
        *m.entry(c.answer.clone()).or_default() += 1;
        m
    });
    println!(
        "T768_COMPACT_RACE {{\"cycles\":{},\"answers\":{codes:?},\"stale_reopens\":{reopened}}}",
        cycles.len()
    );
    for call in &calls {
        assert!(
            call.answer == "Ok" || excluded(&call.answer),
            "compaction answered {}",
            call.answer
        );
    }
    check_race_margin(&cycles, &calls, "postgresql compaction race", 1.0);
    let mut admin = store.admin();
    assert!(
        live_rows(&mut admin).is_empty(),
        "{:?}",
        live_rows(&mut admin)
    );
    assert!(store.no_open_transaction());
}
