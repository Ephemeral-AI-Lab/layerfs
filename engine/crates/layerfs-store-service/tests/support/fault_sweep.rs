//! The fault sweep the `commit_fault_*` binaries run: one Workspace Commit
//! on PostgreSQL through [`FaultProxy`], first clean (to count the `K`
//! server messages of its flight), then once per message `N` in `0..K`
//! with one fault injected at `N`. Each run has its own Branch, forked from
//! the project's initial Layer, and its own Store service dialing through
//! the proxy, so every run sends the same flight. The [`Flight`] selects
//! the Commit: a warm or first one (fused since #768 stage 2), a forced one
//! (the separate path with its held-lane connection), or one whose head
//! another Workspace moved first (the fused attempt's race refusal and the
//! fallback on the separate path, in one call), or a warm one that also adds
//! families of similar files (#810: the Save's publication of the families'
//! first members before the fused message), or one whose change touches
//! two base directories nothing read before (their root pages one grouped
//! read, #810 F3).
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
//!   (the client then never saw a definite race: an unknown outcome);
//! * the Workspace is never locked out: its follow-up Commit is refused
//!   `InFlight` only while a retained Commit waits for its resolution
//!   (`BoundWorkspace::has_retained`), never with nothing to resolve;
//! * a connection dropped after the race refusal reached the client but
//!   before the rest of the fused message's replies did (the session is
//!   quarantined with the race in hand) does not fall back on that session:
//!   the call answers a definite refusal.
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
/// A spread flight's seed: two more directories, which its armed Commit
/// touches and nothing before it reads.
const SPREAD_SEED: &[(&str, &[u8])] = &[
    ("/seed", b"seed\n"),
    ("/dir/inner", b"inner\n"),
    ("/one/inner", b"one\n"),
    ("/two/inner", b"two\n"),
];
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
    /// A warm Commit that also adds families of similar files: its Save
    /// holds each family's later members until one publication of the
    /// families' first members, before the fused message (#810).
    Similar,
    /// A warm Commit whose change touches two base directories nothing
    /// read before (`/one`, `/two`): their root pages are one grouped read,
    /// one locate message for both (#810 F3).
    Spread,
    /// A warm Commit over a wide seed ([`WIDE_DIRECTORIES`] directories of
    /// [`WIDE_FILES`] files) that also rewrites one file and adds one in
    /// every directory: its reads before the lane go in batched windows
    /// (#810: the Save's leaf reads, the difference walk's read-ahead, the
    /// directory windows of the binding hints).
    Wide,
}

/// Directories of the [`Flight::Wide`] seed.
const WIDE_DIRECTORIES: usize = 12;
/// Files in each of them.
const WIDE_FILES: usize = 10;

/// The seed of `flight`'s Store.
fn seed(flight: Flight) -> Vec<(String, Vec<u8>)> {
    let files = if flight == Flight::Spread {
        SPREAD_SEED
    } else {
        SEED
    };
    let mut out: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
        .collect();
    if flight == Flight::Wide {
        for d in 0..WIDE_DIRECTORIES {
            for f in 0..WIDE_FILES {
                let path = format!("/w/d{d:02}/f{f:02}");
                out.push((path.clone(), format!("{path} seed\n").into_bytes()));
            }
        }
    }
    out
}

/// The armed Commit's change besides `/fault`: in a [`Flight::Wide`] one,
/// a rewrite of `f05` and a new file in every directory.
fn wide_change(flight: Flight) -> Vec<(String, Vec<u8>)> {
    if flight != Flight::Wide {
        return Vec::new();
    }
    (0..WIDE_DIRECTORIES)
        .flat_map(|d| [format!("/w/d{d:02}/f05"), format!("/w/d{d:02}/new")])
        .map(|path| {
            let bytes = format!("{path} changed\n").into_bytes();
            (path, bytes)
        })
        .collect()
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
    /// Whether the Workspace kept a retained Commit after the armed one.
    retained: bool,
    /// Whether the follow-up Commit was refused `InFlight`.
    follow_in_flight: bool,
}

fn expected(flight: Flight, fault: &[u8]) -> Tree {
    let mut tree = Tree::new();
    tree.insert("/seed".into(), Node::File(b"seed\n".to_vec()));
    tree.insert("/dir".into(), Node::Dir);
    tree.insert("/dir/inner".into(), Node::File(b"inner\n".to_vec()));
    if flight.warm() {
        tree.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    }
    if flight == Flight::Similar {
        tree.insert("/warm_ring".into(), Node::File(ring_warming(fault)));
    }
    if flight == Flight::Raced {
        tree.insert("/other".into(), Node::File(b"other\n".to_vec()));
    }
    if flight == Flight::Wide {
        tree.insert("/w".into(), Node::Dir);
        for (path, bytes) in seed(flight).into_iter().skip(SEED.len()) {
            let directory = path.rsplit_once('/').unwrap().0.to_owned();
            tree.insert(directory, Node::Dir);
            tree.insert(path, Node::File(bytes));
        }
        for (path, bytes) in wide_change(flight) {
            tree.insert(path, Node::File(bytes));
        }
    }
    tree.insert("/fault".into(), Node::File(fault.to_vec()));
    if flight == Flight::Similar {
        for (path, bytes) in similar_files(fault) {
            tree.insert(path, Node::File(bytes));
        }
    }
    if flight == Flight::Spread {
        for dir in ["one", "two"] {
            tree.insert(format!("/{dir}"), Node::Dir);
            tree.insert(
                format!("/{dir}/inner"),
                Node::File(format!("{dir}\n").into_bytes()),
            );
            tree.insert(format!("/{dir}/fault"), Node::File(fault.to_vec()));
        }
    }
    tree
}

/// The `Similar` flight's warm-up file: 400 bytes unique to the run, whose
/// selection loads the Store's similarity ring before the armed Commit, so
/// the armed flight never reads the ring table (which grows run by run).
fn ring_warming(fault: &[u8]) -> Vec<u8> {
    let mut warm = fault.to_vec();
    warm.extend_from_slice(b"warm");
    similar_files(&warm).swap_remove(0).1
}

/// Families of similar files of the `Similar` flight, unique to the run
/// whose `/fault` holds `fault`: three families of three 400-byte members
/// sharing all but their last line.
fn similar_files(fault: &[u8]) -> Vec<(String, Vec<u8>)> {
    let seed = fault.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    });
    let mut files = Vec::new();
    for family in 0..3_u64 {
        let mut state = (seed ^ family).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let shared: Vec<u8> = (0..400)
            .map(|n| {
                if n % 64 == 63 {
                    return b'\n';
                }
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                b'a' + (state % 26) as u8
            })
            .collect();
        for member in 0..3 {
            let mut bytes = shared.clone();
            bytes.extend_from_slice(format!("member {member}\n").as_bytes());
            files.push((format!("/similar_{family}_{member}"), bytes));
        }
    }
    files
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
        if flight == Flight::Similar {
            write(&workspace, "/warm_ring", &ring_warming(&content(name)));
        }
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
    for (path, bytes) in wide_change(flight) {
        write(&workspace, &path, &bytes);
    }
    write(&workspace, "/fault", &content(name));
    if flight == Flight::Similar {
        for (path, bytes) in similar_files(&content(name)) {
            write(&workspace, &path, &bytes);
        }
    }
    if flight == Flight::Spread {
        write(&workspace, "/one/fault", &content(name));
        write(&workspace, "/two/fault", &content(name));
    }
    let before = service.service.store.fused_counts();
    proxy.arm(target);
    let first = verdict(&commit(&workspace));
    let window = proxy.disarm();
    let after = service.service.store.fused_counts();
    let counts = (after.0 - before.0, after.1 - before.1);
    let retained = workspace.has_retained();
    let followed = commit(&workspace);
    let follow_in_flight = matches!(
        &followed,
        Err(failure) if matches!(failure.error, layerfs_store_service::store::CommitError::InFlight)
    );
    let follow = verdict(&followed);
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
        retained,
        follow_in_flight,
    }
}

fn proxied(store: &TestStore, proxy: &FaultProxy) -> String {
    let (url, _) = store.postgres.as_ref().unwrap();
    super::fault_proxy::proxied_url(url, proxy.port).0
}

/// Where a raced flight's fused attempt stands in the clean flight: the
/// race refusal, and the last reply of its message (the `COMMIT`'s
/// `ReadyForQuery`, which follows the first `CommandComplete` after it).
#[derive(Clone, Copy, Debug)]
pub struct Raced {
    pub refusal: u64,
    pub fused_end: u64,
}

/// Checks the invariants of one run on Branch `name`; returns the state
/// label for the summary.
fn check(
    store: &TestStore,
    name: &str,
    flight: Flight,
    raced: Option<Raced>,
    run: &Run,
) -> &'static str {
    let race_refusal = raced.map(|raced| raced.refusal);
    let label = format!(
        "{name}: fired {:?}, fused counts {:?}, first {:?}",
        run.window.fired, run.counts, run.first
    );
    assert!(
        !run.follow_in_flight || run.retained,
        "{label}: the follow-up was refused InFlight with no retained Commit to resolve"
    );
    let (fused, fallbacks) = run.counts;
    assert!(
        fused + fallbacks <= 1,
        "{label}: one fused Commit or one fallback"
    );
    match flight {
        Flight::Warm | Flight::First | Flight::Similar | Flight::Spread | Flight::Wide => {
            assert_eq!(fallbacks, 0, "{label}: nothing raced")
        }
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
            // A drop among the fused message's later replies: the race is in
            // hand but the session lost a reply. No fallback on it, and a
            // definite refusal (nothing was written).
            if let (Some(fired), Some(raced)) = (&run.window.fired, raced) {
                let lost_after_race = fired.at > raced.refusal
                    && fired.at <= raced.fused_end
                    && fired.delivered.is_empty()
                    && fired.killed.is_none();
                if lost_after_race {
                    assert_eq!(fallbacks, 0, "{label}: no fallback on a lost session");
                    assert!(
                        matches!(run.first, Verdict::Refused(_)),
                        "{label}: a race refusal in hand is definite"
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
    /// A raced flight: the fused message's replies after its race refusal.
    RaceReplies,
    /// A raced flight: the writer's messages after the fused message, up
    /// to the first message of a connection opened after it: the fallback's
    /// publication of the held tail, its hint rows, its head read and what
    /// follows on the writer before anything else connects.
    Fallback,
}

/// The sweep: `fault` at every server message of the `flight` Commit that
/// `positions` selects.
pub fn sweep(tag: &str, fault: Fault, flight: Flight, positions: Positions) {
    let seeded = seed(flight);
    let seeded: Vec<(&str, &[u8])> = seeded
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let Some(store) = TestStore::postgres(tag, &seeded) else {
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
    // refusal, and exactly one fallback follows it. The fused message ends
    // at its COMMIT's answer: the first CommandComplete after the refusal
    // (every request between them answers an error), then ReadyForQuery.
    let raced = (flight == Flight::Raced).then(|| {
        assert_eq!(
            clean.counts,
            (0, 1),
            "the clean raced Commit fell back once"
        );
        let refusal = kinds.find('E').expect("the fused attempt's race refusal");
        let end = refusal
            + kinds[refusal..]
                .find('C')
                .expect("the fused COMMIT's answer")
            + 1;
        assert_eq!(&kinds[end..=end], "Z", "the fused message's last reply");
        Raced {
            refusal: refusal as u64,
            fused_end: end as u64,
        }
    });
    let race_refusal = raced.map(|raced| raced.refusal);
    if flight != Flight::Raced {
        let fused = u64::from(matches!(
            flight,
            Flight::Warm | Flight::First | Flight::Similar | Flight::Spread | Flight::Wide
        ));
        assert_eq!(clean.counts, (fused, 0), "the clean Commit's path");
    }
    check(&store, "clean", flight, raced, &clean);
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
        Positions::RaceReplies => {
            let raced = raced.expect("a raced flight");
            (raced.refusal + 1..=raced.fused_end).collect()
        }
        Positions::Fallback => {
            let raced = raced.expect("a raced flight");
            let log = &clean.window.log;
            let writer = log[raced.refusal as usize].0;
            let before: std::collections::BTreeSet<usize> = log[..=raced.fused_end as usize]
                .iter()
                .map(|(c, _)| *c)
                .collect();
            let end = log
                .iter()
                .enumerate()
                .skip(raced.fused_end as usize + 1)
                .find(|(_, (c, _))| !before.contains(c))
                .map_or(log.len(), |(at, _)| at);
            (raced.fused_end + 1..end as u64)
                .filter(|n| log[*n as usize].0 == writer)
                .collect()
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
        let state = check(&store, &name, flight, raced, &outcome);
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
    check(&store, "again", flight, raced, &again);
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
