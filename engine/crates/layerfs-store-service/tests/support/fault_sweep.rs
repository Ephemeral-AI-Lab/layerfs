//! The fault sweep the `commit_fault_*` binaries run: one Workspace Commit
//! on PostgreSQL through [`FaultProxy`], first clean (to count the `K`
//! server messages of its flight), then once per message `N` in `0..K`
//! with one fault injected at `N`. Each run has its own Branch, forked from
//! the project's initial Layer, and its own Store service dialing through
//! the proxy, so every run sends the same flight. The [`Flight`] selects
//! the Commit: a warm or first one (fused since #768 stage 2), a forced one
//! (the separate path with its held-lane connection), or one whose head
//! another Workspace moved first (the fused attempt's race refusal and the
//! fallback on the separate path, in one call).
//!
//! After each run, from fresh sessions that do not go through the proxy:
//!
//! * the Branch is exactly at its old head, or exactly one Commit past it
//!   whose tree is the old tree plus the change, never anything else;
//! * a Commit answered `Committed` (or known published) is that head; a
//!   definite refusal left the old head (unless the follow-up Commit on the
//!   same Workspace published the change); an unknown outcome allows
//!   either state, and the follow-up Commit on that Workspace is refused;
//! * the first-parent chain has every Commit once;
//! * by `Store::fused_counts`: one call makes at most one fused Commit or
//!   one fallback; only a raced Commit falls back, at most once, and never
//!   when the fault came at or before the fused attempt's race refusal
//!   (the client then never saw a definite race: an unknown outcome).
//!
//! At the end the Store's full audit (`diagnose`) reports no failure, no
//! backend of the Store is left idle in a transaction, and a new Commit
//! through a direct service still publishes.
use super::adversary::{
    self, commit, verdict, wait_until, write, Node, Service, TestStore, Tree, Verdict,
};
use super::fault_proxy::{Fault, FaultProxy, Window};
use layerfs_history::CommitId;
use std::time::{Duration, Instant};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/dir/inner", b"inner\n")];
/// The lane wait of the faulted sessions: a lane row left held shows up as
/// a bounded `Busy`, never a hang.
const LOCK_TIMEOUT_MS: u32 = 5_000;
/// The fused attempt's race refusal as the proxy records it: an
/// `ErrorResponse` with SQLSTATE 22012 (the moved head's division by zero).
const RACE_REFUSAL: &str = "E22012";

/// The Commit a sweep faults.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flight {
    /// The second Commit of a mounted Workspace (fused).
    Warm,
    /// The first Commit on a newly opened service (fused, cold).
    First,
    /// A forced second Commit: the separate path and its lane connection.
    Forced,
    /// A second Commit after another Workspace moved the head: the fused
    /// attempt is refused as a race and the separate path merges.
    Raced,
}

impl Flight {
    fn warm(self) -> bool {
        self != Self::First
    }
}

/// One run's outcome.
struct Run {
    window: Window,
    first: Verdict,
    follow: Verdict,
    /// Whether the same service could still bind another Workspace.
    rebound: bool,
    /// The old head (the warm-up Commit, or the mover's), if any.
    old: Option<CommitId>,
    /// `fused_counts` growth over the armed Commit: (fused, fallbacks).
    counts: (u64, u64),
}

fn expected(flight: Flight, fault: &[u8]) -> Tree {
    let mut tree = Tree::new();
    tree.insert("/seed".into(), Node::File(b"seed\n".to_vec()));
    tree.insert("/dir".into(), Node::Dir);
    tree.insert("/dir/inner".into(), Node::File(b"inner\n".to_vec()));
    if flight.warm() {
        tree.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    }
    if flight == Flight::Raced {
        tree.insert("/other".into(), Node::File(b"other\n".to_vec()));
    }
    tree.insert("/fault".into(), Node::File(fault.to_vec()));
    tree
}

fn content(name: &str) -> Vec<u8> {
    format!("fault {name}\n").into_bytes()
}

/// One run on a new Branch `name`: (except `First`) a warm-up Commit,
/// (`Raced`) another Workspace's Commit of `/other` on a direct service,
/// then the armed Commit of `/fault`, then one follow-up Commit with no new
/// change.
fn run(
    store: &TestStore,
    proxy: &FaultProxy,
    name: &str,
    flight: Flight,
    target: Option<(u64, Fault)>,
) -> Run {
    let commit = |workspace: &_| {
        if flight == Flight::Forced {
            adversary::commit_forced(workspace)
        } else {
            commit(workspace)
        }
    };
    let branch = store.fork(name);
    let selection = store.with_lock_timeout(
        &store.selection_via(&proxied(store, proxy)),
        LOCK_TIMEOUT_MS,
    );
    let service = Service::open(store, selection);
    let workspace = service.bind(branch);
    let mut old = if flight.warm() {
        write(&workspace, "/warm", b"warm\n");
        match verdict(&commit(&workspace)) {
            Verdict::Committed(id) => Some(id),
            other => panic!("{name}: warm-up Commit: {other:?}"),
        }
    } else {
        None
    };
    if flight == Flight::Raced {
        let mover = store.open();
        let other = mover.bind(branch);
        write(&other, "/other", b"other\n");
        old = match verdict(&adversary::commit(&other)) {
            Verdict::Committed(id) => Some(id),
            other => panic!("{name}: the mover's Commit: {other:?}"),
        };
        mover.unmount(other);
        mover.stop();
    }
    write(&workspace, "/fault", &content(name));
    let before = service.service.store.fused_counts();
    proxy.arm(target);
    let first = verdict(&commit(&workspace));
    let window = proxy.disarm();
    let after = service.service.store.fused_counts();
    let counts = (after.0 - before.0, after.1 - before.1);
    let follow = verdict(&commit(&workspace));
    let rebound = match service.try_bind(store.branch(name)) {
        Ok(other) => {
            let _ = service.try_unmount(other);
            true
        }
        Err(_) => false,
    };
    let _ = service.try_unmount(workspace);
    service.abandon();
    Run {
        window,
        first,
        follow,
        rebound,
        old,
        counts,
    }
}

fn proxied(store: &TestStore, proxy: &FaultProxy) -> String {
    let (url, _) = store.postgres.as_ref().unwrap();
    super::fault_proxy::proxied_url(url, proxy.port).0
}

/// Checks the invariants of one run on Branch `name`; returns the state
/// label for the summary.
fn check(
    store: &TestStore,
    name: &str,
    flight: Flight,
    race_refusal: Option<u64>,
    run: &Run,
) -> &'static str {
    let label = format!(
        "{name}: fired {:?}, fused counts {:?}",
        run.window.fired, run.counts
    );
    let (fused, fallbacks) = run.counts;
    assert!(
        fused + fallbacks <= 1,
        "{label}: one fused Commit or one fallback"
    );
    match flight {
        Flight::Warm | Flight::First => assert_eq!(fallbacks, 0, "{label}: nothing raced"),
        Flight::Forced => assert_eq!(run.counts, (0, 0), "{label}: a forced Commit is not fused"),
        Flight::Raced => {
            assert_eq!(fused, 0, "{label}: a moved head never commits fused");
            // The fallback runs only with the race refusal in hand: it
            // reached the client before the fault, or among the messages
            // delivered from the fault on (a held reply the backend sent
            // before it was killed). Never after an unknown outcome.
            if let (Some(fired), Some(refusal)) = (&run.window.fired, race_refusal) {
                let in_hand =
                    fired.at > refusal || fired.delivered.iter().any(|kind| kind == RACE_REFUSAL);
                if !in_hand {
                    assert_eq!(
                        fallbacks, 0,
                        "{label}: no fallback without the race refusal in hand"
                    );
                }
            }
        }
    }
    if let Verdict::Unknown(_) = run.first {
        assert!(
            !run.rebound,
            "{label}: the session that lost a reply must refuse later calls ({:?})",
            run.first
        );
    }
    adversary::check_attempt(
        store,
        store.branch(name),
        run.old,
        &expected(flight, &content(name)),
        &run.first,
        &run.follow,
        &label,
    )
}

/// Which server messages of the flight a sweep faults.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Positions {
    /// Every one.
    All,
    /// Every one whose index is `part` modulo `parts`: the shares of one
    /// flight, swept by separate tests in parallel, together cover all.
    Share(u64, u64),
    /// The writer's messages while the lane is held: after the lane
    /// connection's first replies (its `BEGIN` batch with the lane row) and
    /// before its last (the lane's release).
    InLaneWriter,
}

/// The sweep: `fault` at every server message of the `flight` Commit that
/// `positions` selects.
pub fn sweep(tag: &str, fault: Fault, flight: Flight, positions: Positions) {
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    let started = Instant::now();
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    // The first service on a newly initialized Store sends a shorter flight
    // (it reserves nothing the Init left it); every later one sends the
    // same. Prime once, then count.
    let prime = run(&store, &proxy, "prime", flight, None);
    check(&store, "prime", flight, None, &prime);
    let clean = run(&store, &proxy, "clean", flight, None);
    assert!(
        matches!(clean.first, Verdict::Committed(_)),
        "{:?}",
        clean.first
    );
    let k = clean.window.seen;
    let kinds: String = clean.window.log.iter().map(|(_, kind)| *kind).collect();
    // A raced flight: the fused attempt's first error reply is its race
    // refusal, and exactly one fallback follows it.
    let race_refusal = (flight == Flight::Raced).then(|| {
        assert_eq!(
            clean.counts,
            (0, 1),
            "the clean raced Commit fell back once"
        );
        kinds.find('E').expect("the fused attempt's race refusal") as u64
    });
    if flight != Flight::Raced {
        let fused = u64::from(matches!(flight, Flight::Warm | Flight::First));
        assert_eq!(clean.counts, (fused, 0), "the clean Commit's path");
    }
    check(&store, "clean", flight, race_refusal, &clean);
    // The flight by connection: runs of messages, each tagged by the
    // connection's order within this flight.
    let mut order = Vec::new();
    let mut runs = String::new();
    let mut last = None;
    for (connection, kind) in &clean.window.log {
        if !order.contains(connection) {
            order.push(*connection);
        }
        if last != Some(*connection) {
            let tag = order.iter().position(|c| c == connection).unwrap();
            runs.push_str(&format!(" s{tag}:"));
            last = Some(*connection);
        }
        runs.push(*kind);
    }
    let connections: std::collections::BTreeSet<usize> =
        clean.window.log.iter().map(|(c, _)| *c).collect();
    println!(
        "FAULT_FLIGHT {{\"tag\":\"{tag}\",\"fault\":\"{fault:?}\",\"flight\":\"{flight:?}\",\"race_refusal\":{race_refusal:?},\"K\":{k},\"replies\":{},\"connections\":{},\"kinds\":\"{kinds}\",\"by_connection\":\"{}\"}}",
        kinds.matches('Z').count(),
        connections.len(),
        runs.trim()
    );
    assert!(k > 0, "the Commit sends something");
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    let mut unfired = Vec::new();
    let selected: Vec<u64> = match positions {
        Positions::All => (0..k).collect(),
        Positions::Share(part, parts) => (0..k).filter(|n| n % parts == part).collect(),
        Positions::InLaneWriter => {
            let log = &clean.window.log;
            let writer = log[0].0;
            let first_lane = log.iter().position(|(c, _)| *c != writer).expect("a lane");
            let lane_held = first_lane
                + log[first_lane..]
                    .iter()
                    .position(|(c, _)| *c == writer)
                    .expect("the writer inside the lane");
            let released = log.iter().rposition(|(c, _)| *c == writer).unwrap();
            (lane_held as u64..=released as u64).collect()
        }
    };
    println!(
        "FAULT_POSITIONS {{\"tag\":\"{tag}\",\"positions\":\"{positions:?}\",\"selected\":{},\"from\":{},\"to\":{}}}",
        selected.len(),
        selected.first().copied().unwrap_or_default(),
        selected.last().copied().unwrap_or_default()
    );
    let mut names = vec!["prime".to_owned(), "clean".to_owned(), "again".to_owned()];
    for n in selected {
        let name = format!("n{n}");
        names.push(name.clone());
        let outcome = run(&store, &proxy, &name, flight, Some((n, fault)));
        if outcome.window.fired.is_none() {
            unfired.push(n);
        }
        let reached: String = outcome
            .window
            .log
            .iter()
            .take(n as usize + 1)
            .map(|(_, kind)| *kind)
            .collect();
        assert_eq!(
            reached,
            kinds[..=n as usize],
            "{name}: the flight up to the fault is the clean one"
        );
        if let Some(fired) = &outcome.window.fired {
            if fault != Fault::Drop {
                assert_eq!(fired.killed, Some(true), "{name}: the backend was killed");
            }
        }
        let state = check(&store, &name, flight, race_refusal, &outcome);
        let class = match &outcome.first {
            Verdict::Committed(_) => "committed",
            Verdict::Refused(_) => "refused",
            Verdict::Unknown(_) => "unknown",
            Verdict::PublishedThenFailed(_) => "published_then_failed",
            Verdict::UpToDate => "up_to_date",
            Verdict::Conflicted => "conflicted",
        };
        println!(
            "FAULT_RUN {{\"n\":{n},\"at\":\"{}\",\"first\":\"{class}\",\"state\":\"{state}\",\"rebound\":{},\"counts\":\"{:?}\",\"delivered\":\"{}\",\"follow\":\"{:?}\"}}",
            outcome
                .window
                .fired
                .as_ref()
                .map(|f| format!("c{}:{}", f.connection, f.kind))
                .unwrap_or_default(),
            outcome.rebound,
            outcome.counts,
            outcome
                .window
                .fired
                .as_ref()
                .map(|f| f.delivered.concat())
                .unwrap_or_default(),
            outcome.follow
        );
        *states.entry(format!("{class}/{state}")).or_default() += 1;
    }
    println!(
        "FAULT_SUMMARY {{\"tag\":\"{tag}\",\"K\":{k},\"states\":{states:?},\"unfired\":{unfired:?},\"seconds\":{:.1}}}",
        started.elapsed().as_secs_f64()
    );
    // The flight is the same at the end: every position above was the
    // message it names.
    let again = run(&store, &proxy, "again", flight, None);
    check(&store, "again", flight, race_refusal, &again);
    assert_eq!(
        again
            .window
            .log
            .iter()
            .map(|(_, kind)| *kind)
            .collect::<String>(),
        kinds,
        "the flight is deterministic"
    );
    assert!(
        unfired.is_empty(),
        "every position of the flight is reached: {unfired:?}"
    );
    // No backend of this Store is left inside a transaction.
    let mut admin = store.admin();
    assert!(
        wait_until(Duration::from_secs(10), || {
            let open: i64 = admin
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() \
                     AND pid <> pg_backend_pid() AND state LIKE 'idle in transaction%'",
                    &[],
                )
                .unwrap()
                .get(0);
            open == 0
        }),
        "a backend is left idle in a transaction"
    );
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
    assert_eq!(store.duplicate_locators(), 0, "no object located twice");
    // Every Commit row is on its Branch's chain: none was published twice
    // or left behind by a failed attempt.
    let reached: usize = names
        .iter()
        .map(|name| store.chain(store.branch(name)).len())
        .sum();
    assert_eq!(
        store.commit_rows(),
        reached as i64,
        "Commit rows off every chain"
    );
    // The Store still publishes.
    let branch = store.fork("after");
    let service = store.open();
    let workspace = service.bind(branch);
    write(&workspace, "/after", b"after\n");
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    service.unmount(workspace);
    service.stop();
}
