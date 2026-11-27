//! The fault sweep the `sdk_read_fault_rh_*` binaries run (#818 lane RH):
//! one SDK read call on PostgreSQL through [`FaultProxy`], first clean (to
//! count the `K` server messages it causes), then once per message `N` in
//! `0..K` with one fault injected at `N`. The call is `fs.stat` of a visible
//! path on Branch `work` of a project that holds a `project.remove_file`
//! marker, on a newly opened Store service (cold: the control reader
//! connects and prepares inside the window), so the window holds every
//! message R1 changed: the Branch-by-name read and the marker read, each one
//! fenced statement now, with their prepares. Since R5 the read's context
//! is one statement: the sweep runs the call twice over, naming the Branch
//! (`read_context_named`) and then the Branch's head Commit by its full
//! identity (`read_context_node`).
//!
//! After each run:
//!
//! * the armed call answered exactly the clean answer, or a refusal whose
//!   code says the read failed (`Unknown` or `Failed`): a lost reply never
//!   reads as an absent Branch, path or marker, nor as another answer;
//! * the same service answers the call again exactly (a quarantined control
//!   reader reopens on its next read, once);
//! * a fresh service, not through the proxy, answers it exactly.
//!
//! At the end no backend of the Store is left idle in a transaction.
use super::adversary::{self, Service, TestStore};
use super::fault_proxy::{Fault, FaultProxy};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_persistence::StoreSelection;

const SEED: &[(&str, &[u8])] = &[
    ("/seed", b"seed\n"),
    ("/dir/inner", b"inner\n"),
    ("/dir/gone", b"gone\n"),
];
const BRANCH: &str = "work";
const PATH: &str = "/dir/inner";

/// The read's snapshot argument: `branch` or `commit`, and its value.
type Named<'a> = (&'static str, &'a str);

fn stat(service: &Service, (argument, value): Named<'_>) -> Result<SdkValue, SdkRefusal> {
    service.call(
        "fs.stat",
        vec![
            (argument, SdkValue::text(value)),
            ("path", SdkValue::text(PATH)),
        ],
    )
}

fn outcome(result: &Result<SdkValue, SdkRefusal>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(refusal) => format!("refused {}", refusal.code),
    }
}

/// The sweep: `fault` at every server message of the cold `fs.stat`.
pub fn sweep(tag: &str, fault: Fault) {
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    store.fork(BRANCH);
    // The marker (not swept): on a direct service.
    let direct = store.open();
    let removed = direct
        .call(
            "project.remove_file",
            vec![("path", SdkValue::text("/dir/gone"))],
        )
        .unwrap();
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    let expected = stat(&direct, ("branch", BRANCH)).expect("the direct answer");
    let head = expected
        .get("at")
        .and_then(|at| at.get_text("id"))
        .expect("the head Commit's id")
        .to_owned();
    let by_id = stat(&direct, ("commit", &head)).expect("the direct answer by id");
    direct.stop();

    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let selection = store.selection_via(&proxied);
    for (shape, named, expected) in [
        ("named", ("branch", BRANCH), expected),
        ("node", ("commit", head.as_str()), by_id),
    ] {
        let tag = format!("{tag}_{shape}");
        sweep_one(&tag, fault, &store, &proxy, &selection, named, expected);
    }
    let _ = adversary::tmp;
}

/// One call shape's sweep: the clean run, then `fault` at every message.
fn sweep_one(
    tag: &str,
    fault: Fault,
    store: &TestStore,
    proxy: &FaultProxy,
    selection: &StoreSelection,
    named: Named<'_>,
    expected: SdkValue,
) {
    // The clean run: the flight's message count.
    let service = Service::open(store, selection.clone());
    proxy.arm(None);
    let clean = stat(&service, named);
    let window = proxy.disarm();
    assert_eq!(clean.as_ref().ok(), Some(&expected), "the clean answer");
    service.stop();
    let k = window.seen;
    let kinds: String = window.log.iter().map(|(_, kind)| *kind).collect();
    println!(
        "READ_FAULT_FLIGHT {{\"tag\":\"{tag}\",\"fault\":\"{fault:?}\",\"K\":{k},\"kinds\":\"{kinds}\"}}"
    );
    assert!(k > 0, "the call sends something");

    let mut states = std::collections::BTreeMap::<String, u64>::new();
    for n in 0..k {
        let name = format!("n{n}");
        let service = Service::open(store, selection.clone());
        proxy.arm(Some((n, fault)));
        let armed = stat(&service, named);
        let window = proxy.disarm();
        assert!(window.fired.is_some(), "{name}: the fault fired");
        match &armed {
            Ok(answer) => assert_eq!(answer, &expected, "{name}: an answer is the clean one"),
            Err(refusal) => assert!(
                matches!(refusal.code.as_str(), "Unknown" | "Failed"),
                "{name}: a lost reply refuses as a failed read, not {}: {}",
                refusal.code,
                refusal.message
            ),
        }
        let again = stat(&service, named);
        assert_eq!(
            again.as_ref().ok(),
            Some(&expected),
            "{name}: the same service answers again: {}",
            outcome(&again)
        );
        service.abandon();
        *states.entry(outcome(&armed)).or_default() += 1;
    }
    let fresh = store.open();
    assert_eq!(
        stat(&fresh, named).ok(),
        Some(expected),
        "a fresh service answers"
    );
    fresh.stop();
    println!("READ_FAULT_STATES {{\"tag\":\"{tag}\",\"K\":{k},\"states\":{states:?}}}");
    assert!(
        store.no_open_transaction(),
        "no backend left in a transaction"
    );
}
