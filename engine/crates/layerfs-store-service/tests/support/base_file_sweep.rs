//! #810 F5: the fault sweep of a Commit whose Workspace reader opens base
//! files in one window (`commit_fault_base_files_*` binaries). One Workspace
//! Commit on PostgreSQL through [`FaultProxy`], first clean (to count its `K`
//! server messages), then once per message `N` in `0..K` with one fault at
//! `N`. Each run has its own Branch forked from the project's initial Layer
//! and its own Store service dialing through the proxy, so every run sends
//! the same flight: a warm-up Commit of `/warm`, then the armed Commit,
//! which edits `EDITED` base files in place and changes only the mode of
//! `KEPT` others, then one follow-up Commit with no new change. The armed
//! Commit's Workspace reader sends one message: the batched prefetch of the
//! edited files' content roots (one `ranges` read). Their lengths are one
//! batched demand too, answered from the locators the changes' own stats
//! read, so it sends nothing; the kept files are not read at all.
//!
//! After each run, from fresh sessions that do not go through the proxy
//! (`adversary::check_attempt`): the Branch is exactly at its old head or
//! exactly one Commit past it whose tree is the seed plus the edits; a
//! Commit answered `Committed` is that head; a definite refusal left the old
//! head (unless the follow-up published the change); after an unknown
//! outcome the follow-up is refused. At the end: the flight is the same as
//! the clean one, every position fired, the Store's audit reports nothing,
//! no backend is left idle in a transaction, no object is located twice, and
//! a new Commit through a direct service still publishes.
use super::adversary::{
    self, commit, random_bytes, seed_tree, verdict, wait_until, write, Node, Service, TestStore,
    Tree, Verdict,
};
use super::fault_proxy::{Fault, FaultProxy, Window};
use layerfs_store_service::write::mutation::Mutation;
use std::time::{Duration, Instant};

const LOCK_TIMEOUT_MS: u32 = 5_000;
const EDITED: usize = 6;
const KEPT: usize = 4;
/// Bytes of each base file (whole files).
const LENGTH: usize = 2_000;

fn seed() -> Vec<(String, Vec<u8>)> {
    (0..EDITED + KEPT)
        .map(|n| (format!("/base/f{n}"), random_bytes(n as u64 + 7, LENGTH)))
        .collect()
}

fn edit(n: usize) -> Vec<u8> {
    format!("edit{n}").into_bytes()
}

fn expected() -> Tree {
    let seed = seed();
    let pairs: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let mut tree = seed_tree(&pairs);
    for (n, (path, bytes)) in seed.iter().enumerate().take(EDITED) {
        let mut bytes = bytes.clone();
        bytes[..edit(n).len()].copy_from_slice(&edit(n));
        tree.insert(path.clone(), Node::File(bytes));
    }
    tree.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    tree
}

struct Run {
    window: Window,
    first: Verdict,
    follow: Verdict,
    old: Option<layerfs_history::CommitId>,
}

fn run(store: &TestStore, proxy: &FaultProxy, name: &str, target: Option<(u64, Fault)>) -> Run {
    let branch = store.fork(name);
    let (url, _) = store.postgres.as_ref().unwrap();
    let proxied = super::fault_proxy::proxied_url(url, proxy.port).0;
    let selection = store.with_lock_timeout(&store.selection_via(&proxied), LOCK_TIMEOUT_MS);
    let service = Service::open(store, selection);
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let old = match verdict(&commit(&workspace)) {
        Verdict::Committed(id) => Some(id),
        other => panic!("{name}: warm-up Commit: {other:?}"),
    };
    {
        let mut m = Mutation::new(&workspace).unwrap();
        m.now = adversary::NOW;
        for n in 0..EDITED + KEPT {
            let path = format!("/base/f{n}");
            if n < EDITED {
                let serial = m.resolve(&path).unwrap().serial;
                m.write_at(serial, 0, &edit(n)).unwrap();
            } else {
                m.chmod(&path, 0o600).unwrap();
            }
        }
        m.finish().unwrap();
    }
    proxy.arm(target);
    let first = verdict(&commit(&workspace));
    let window = proxy.disarm();
    let follow = verdict(&commit(&workspace));
    let _ = service.try_unmount(workspace);
    service.abandon();
    Run {
        window,
        first,
        follow,
        old,
    }
}

pub fn sweep(tag: &str, fault: Fault) {
    let seed = seed();
    let pairs: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let Some(store) = TestStore::postgres(tag, &pairs) else {
        return;
    };
    let started = Instant::now();
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let expected = expected();
    let check = |name: &str, run: &Run| {
        adversary::check_attempt(
            &store,
            store.branch(name),
            run.old,
            &expected,
            &run.first,
            &run.follow,
            &format!("{name}: fired {:?}", run.window.fired),
        )
    };
    // The first service on a new Store sends a shorter flight; prime once.
    let prime = run(&store, &proxy, "prime", None);
    check("prime", &prime);
    let clean = run(&store, &proxy, "clean", None);
    assert!(
        matches!(clean.first, Verdict::Committed(_)),
        "{:?}",
        clean.first
    );
    check("clean", &clean);
    let k = clean.window.seen;
    let kinds: String = clean.window.log.iter().map(|(_, kind)| *kind).collect();
    // The flight by connection, each tagged by its order in the flight: the
    // Workspace reader's batched demands come before the writer's Save.
    let mut order = Vec::new();
    let mut runs = String::new();
    let mut last = None;
    for (connection, kind) in &clean.window.log {
        if !order.contains(connection) {
            order.push(*connection);
        }
        if last != Some(*connection) {
            let at = order.iter().position(|c| c == connection).unwrap();
            runs.push_str(&format!(" s{at}:"));
            last = Some(*connection);
        }
        runs.push(*kind);
    }
    println!(
        "FAULT_FLIGHT {{\"tag\":\"{tag}\",\"fault\":\"{fault:?}\",\"K\":{k},\"kinds\":\"{kinds}\",\"by_connection\":\"{}\"}}",
        runs.trim()
    );
    assert!(
        order.len() >= 2,
        "the armed Commit's Workspace reader and writer both send"
    );
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    let mut unfired = Vec::new();
    let mut names = vec!["prime".to_owned(), "clean".to_owned(), "again".to_owned()];
    for n in 0..k {
        let name = format!("n{n}");
        names.push(name.clone());
        let outcome = run(&store, &proxy, &name, Some((n, fault)));
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
        assert_eq!(reached, kinds[..=n as usize], "{name}: the clean flight");
        if let (Some(fired), true) = (&outcome.window.fired, fault != Fault::Drop) {
            assert_eq!(fired.killed, Some(true), "{name}: the backend was killed");
        }
        let state = check(&name, &outcome);
        let class = match &outcome.first {
            Verdict::Committed(_) => "committed",
            Verdict::Refused(_) => "refused",
            Verdict::Unknown(_) => "unknown",
            Verdict::PublishedThenFailed(_) => "published_then_failed",
            Verdict::UpToDate => "up_to_date",
            Verdict::Conflicted => "conflicted",
        };
        println!(
            "FAULT_RUN {{\"n\":{n},\"at\":\"{}\",\"first\":\"{class}\",\"state\":\"{state}\",\"follow\":\"{:?}\"}}",
            outcome
                .window
                .fired
                .as_ref()
                .map(|f| format!("c{}:{}", f.connection, f.kind))
                .unwrap_or_default(),
            outcome.follow
        );
        *states.entry(format!("{class}/{state}")).or_default() += 1;
    }
    println!(
        "FAULT_SUMMARY {{\"tag\":\"{tag}\",\"K\":{k},\"states\":{states:?},\"unfired\":{unfired:?},\"seconds\":{:.1}}}",
        started.elapsed().as_secs_f64()
    );
    let again = run(&store, &proxy, "again", None);
    check("again", &again);
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
    assert!(unfired.is_empty(), "every position fired: {unfired:?}");
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
    let reached: usize = names
        .iter()
        .map(|name| store.chain(store.branch(name)).len())
        .sum();
    assert_eq!(
        store.commit_rows(),
        reached as i64,
        "Commit rows off every chain"
    );
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
