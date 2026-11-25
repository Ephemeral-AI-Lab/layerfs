use adversary::{
    apply, check_attempt, commit, random_bytes, tree_of, verdict, write, Op, Service, TestStore,
    Verdict,
};
use fault_proxy::{Fault, FaultProxy};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flight {
    Warm,
    Cold,
    Raced,
}

/// A new directory of `files` small files and one 140 KiB file.
fn change(tag: &str, files: usize) -> Vec<Op> {
    let mut ops = vec![Op::Mkdir(format!("/{tag}"))];
    for n in 0..files {
        ops.push(Op::Write(
            format!("/{tag}/f{n}"),
            format!("{tag} {n}\n").into_bytes(),
        ));
    }
    ops.push(Op::Write(
        format!("/{tag}/big"),
        random_bytes(tag.len() as u64 * 31 + 7, 140 * 1024),
    ));
    ops
}

/// One run on Branch `name`; returns the messages the window saw and a
/// state label.
fn run(
    store: &TestStore,
    proxy: &FaultProxy,
    proxied: &str,
    name: &str,
    flight: Flight,
    fault: Option<(u64, Fault)>,
) -> (u64, String) {
    let branch = store.fork(name);
    let service = Service::open(
        store,
        store.with_lock_timeout(&store.selection_via(proxied), 5_000),
    );
    let workspace = service.bind(branch);
    let mut old = None;
    if flight != Flight::Cold {
        write(&workspace, "/warm", b"warm\n");
        match verdict(&commit(&workspace)) {
            Verdict::Committed(id) => old = Some(id),
            other => panic!("{name}: warm-up {other:?}"),
        }
    }
    if flight == Flight::Raced {
        let mover = store.open();
        let other = mover.bind(branch);
        write(&other, "/mover", b"mover\n");
        match verdict(&commit(&other)) {
            Verdict::Committed(id) => old = Some(id),
            v => panic!("{name}: mover {v:?}"),
        }
        mover.unmount(other);
        mover.stop();
    }
    // The raced flight's merge makes its message long: fewer files keep
    // the sweep and the final audit inside their bounds.
    let files = if flight == Flight::Raced { 40 } else { 150 };
    for op in change(name, files) {
        apply(&workspace, &op).unwrap();
    }
    // The intended tree: the Branch's head merged with this change.
    let mut expected = store.tree(branch);
    for (path, node) in tree_of(&workspace) {
        expected.insert(path, node);
    }
    proxy.arm(fault);
    let first = verdict(&commit(&workspace));
    let window = proxy.disarm();
    let follow = verdict(&commit(&workspace));
    let _ = service.try_unmount(workspace);
    service.abandon();
    let state = check_attempt(store, branch, old, &expected, &first, &follow, name);
    let failures = verify768::duplicate_allocations(store);
    assert!(
        failures.is_empty(),
        "{name}: first {first:?}, follow {follow:?}: allocation {failures:#?}"
    );
    let class = match first {
        Verdict::Committed(_) => "committed",
        Verdict::Refused(_) => "refused",
        Verdict::Unknown(_) => "unknown",
        _ => "other",
    };
    (window.seen, format!("{class}/{state}"))
}

fn sweep(tag: &str, flight: Flight, fault: Fault, part: u64, parts: u64) {
    let _slot = verify768::sweep_slot();
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    run(&store, &proxy, &proxied, "prime", flight, None);
    let (k, _) = run(&store, &proxy, &proxied, "clean", flight, None);
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    let mut direct = 0;
    for n in (0..k).filter(|n| n % parts == part) {
        let (_, state) = run(
            &store,
            &proxy,
            &proxied,
            &format!("n{n}"),
            flight,
            Some((n, fault)),
        );
        *states.entry(state).or_default() += 1;
        if n % 10 == part {
            direct += 1;
            let service = store.open();
            let branch = store.fork(&format!("direct{direct}"));
            let workspace = service.bind(branch);
            for op in change(&format!("direct{direct}"), 150) {
                apply(&workspace, &op).unwrap();
            }
            let answer = verdict(&commit(&workspace));
            assert!(
                matches!(answer, Verdict::Committed(_)),
                "direct after n{n}: {answer:?}"
            );
            service.unmount(workspace);
            service.stop();
            let failures = verify768::duplicate_allocations(&store);
            assert!(failures.is_empty(), "direct after n{n}: {failures:#?}");
        }
    }
    println!(
        "VERIFY_UNKNOWN {{\"tag\":\"{tag}\",\"flight\":\"{flight:?}\",\"fault\":\"{fault:?}\",\"K\":{k},\"states\":{states:?}}}"
    );
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
    let failures = verify768::amcheck(&store);
    assert!(failures.is_empty(), "amcheck: {failures:#?}");
}
