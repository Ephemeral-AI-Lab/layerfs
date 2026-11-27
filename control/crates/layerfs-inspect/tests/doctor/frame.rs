//! The doctor frame (call surface 2.0): the one runner, with made-up kinds
//! (a budget that runs out is `timeout`, a failed parent blocks what is on
//! it without calling it, one progress event per finished thing, counts
//! and status), the one report's long form, and every kind's check ids
//! against calls.json `doctor.checks` through `doctor_checks_pending.json`.
use layerfs_home::events::Events;
use layerfs_home::json;
use layerfs_home::state::{BlockedBy, State};
use layerfs_inspect::doctor::budget::Checks;
use layerfs_inspect::doctor::report::{Coverage, Verdict};
use layerfs_inspect::{
    kinds, run_with, CheckId, Kind, KindDoctor, Located, Node, Options, Probe, Registry, Thing,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const SPEC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../core/docs/issues/call-surface-2-0/calls.json"
);
const PENDING: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/doctor_checks_pending.json"
);

fn read(path: &str) -> SdkValue {
    json::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

fn texts(value: &SdkValue, key: &str) -> Vec<String> {
    (list(value, key).iter())
        .map(|item| match item {
            SdkValue::Text(text) => text.clone(),
            other => panic!("not text: {other:?}"),
        })
        .collect()
}

fn keys(value: &SdkValue) -> Vec<&str> {
    match value {
        SdkValue::Map(fields) => fields.iter().map(|(key, _)| key.as_str()).collect(),
        _ => Vec::new(),
    }
}

// ---- made-up kinds ----

const MACHINE: &[CheckId] = &[CheckId::own("m.record"), CheckId::own("m.worker")];
const STORE: &[CheckId] = &[CheckId::own("s.record"), CheckId::own("s.slow")];
const BUCKET: &[CheckId] = &[CheckId::own("b.list"), CheckId::full("b.write")];

/// How many times a Store's checks ran.
static STORE_RUNS: AtomicUsize = AtomicUsize::new(0);

/// A machine named `bad` fails its worker check and blocks what is on it;
/// `quiet` gives a notice.
fn machine(probe: &Probe<'_>, node: &mut Node) {
    let mut checks = Checks::default();
    checks.pass("m.record", "on record");
    match probe.thing.name.as_str() {
        "bad" => {
            checks.fail("m.worker", "worker is from another build");
            checks.fix(
                "m.worker",
                "layerfs machine stop bad && layerfs machine start bad",
            );
            node.gate = Some(BlockedBy {
                thing: "machine",
                handle: "bad".to_owned(),
                reason: "outdated",
            });
        }
        "quiet" => checks.notice("m.worker", "a normal state worth saying"),
        _ => checks.pass("m.worker", "answers"),
    }
    checks.fill(node, probe.checks, probe.budget.expired());
}

/// A Store named `slow` outlives its budget before its second check.
fn store(probe: &Probe<'_>, node: &mut Node) {
    STORE_RUNS.fetch_add(1, Ordering::SeqCst);
    let mut checks = Checks::default();
    checks.pass("s.record", "on record");
    if probe.thing.name == "slow" {
        while !probe.budget.expired() {
            std::thread::sleep(Duration::from_millis(5));
        }
        checks.unknown("s.slow", "no answer before the budget ran out");
    }
    checks.fill(node, probe.checks, probe.budget.expired());
}

/// Makes nothing until its budget is gone.
fn silent(probe: &Probe<'_>, _: &mut Node) {
    while !probe.budget.expired() {
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn bucket(probe: &Probe<'_>, node: &mut Node) {
    let mut checks = Checks::default();
    checks.pass("b.list", "listed");
    if probe.full {
        checks.pass("b.write", "written and removed");
    }
    checks.fill(node, probe.checks, probe.budget.expired());
}

static KINDS: [KindDoctor; 4] = [
    KindDoctor {
        kind: Kind::Machine,
        checks: MACHINE,
        budget: Duration::from_secs(5),
        run: machine,
    },
    KindDoctor {
        kind: Kind::Store,
        checks: STORE,
        budget: Duration::from_millis(40),
        run: store,
    },
    KindDoctor {
        kind: Kind::Sandbox,
        checks: &[],
        budget: Duration::from_millis(30),
        run: silent,
    },
    KindDoctor {
        kind: Kind::Bucket,
        checks: BUCKET,
        budget: Duration::from_secs(5),
        run: bucket,
    },
];

/// Every machine holds Stores `a` and `slow` and Sandbox `sb`.
fn locate(_: &Path, thing: &Thing) -> Result<Located, SdkRefusal> {
    let children = match thing.kind {
        Kind::Machine => vec![
            (Thing::new(Kind::Store, "a"), State::Ready),
            (Thing::new(Kind::Store, "slow"), State::Ready),
            (Thing::new(Kind::Sandbox, "sb"), State::Ready),
        ],
        _ => Vec::new(),
    };
    Ok(Located {
        state: State::Ready,
        blocked: None,
        children,
    })
}

static REGISTRY: Registry = Registry {
    kinds: &KINDS,
    locate,
};

/// The report of `thing` and the progress events it sent.
fn doctor(thing: Thing, full: bool) -> (SdkValue, Vec<SdkValue>) {
    let mut events = Vec::new();
    let report = {
        let mut sink = |event: SdkValue| events.push(event);
        let mut stream = Events::new(&mut sink);
        let options = Options {
            full,
            ..Options::default()
        };
        run_with(
            &REGISTRY,
            Path::new("/nowhere"),
            thing,
            &options,
            &mut stream,
        )
        .unwrap()
    };
    (report.to_sdk(), events)
}

/// The node of `word` `name` under the report's tree.
fn child<'a>(report: &'a SdkValue, list_key: &str, word: &str, name: &str) -> &'a SdkValue {
    let tree = report.get("tree").unwrap();
    (list(tree, list_key).iter())
        .find(|node| node.get_text(word) == Some(name))
        .unwrap_or_else(|| panic!("no {word} {name}: {report:?}"))
}

#[test]
fn a_clean_parent_checks_each_thing_on_it_shallowly_under_its_own_budget() {
    let before = STORE_RUNS.load(Ordering::SeqCst);
    let (report, events) = doctor(Thing::new(Kind::Machine, "dev"), false);
    let doctor = &report;
    assert_eq!(
        keys(doctor),
        [
            "ok",
            "findings",
            "not_checked",
            "status",
            "counts",
            "observed_at",
            "tree"
        ]
    );
    // Store `slow` ran out of its own budget at its second check: that check
    // is `timeout`, never retried; its first check passed
    assert!(STORE_RUNS.load(Ordering::SeqCst) >= before + 2);
    assert_eq!(
        json::print(&SdkValue::List(list(doctor, "not_checked").to_vec())),
        concat!(
            r#"[{"store":"slow","check":"s.slow","blocked_by":null,"reason":"timeout"},"#,
            r#"{"sandbox":"sb","check":null,"blocked_by":null,"reason":"timeout"}]"#
        )
    );
    let slow = child(doctor, "stores", "store", "slow");
    assert_eq!(
        json::print(slow),
        concat!(
            r#"{"store":"slow","status":"not_checked","coverage":"shallow","checks":["#,
            r#"{"check":"s.record","outcome":"pass","from":null},"#,
            r#"{"check":"s.slow","outcome":"not_checked","from":null}],"#,
            r#""blocked_by":null,"reason":null}"#
        )
    );
    // a thing that gave nothing before its budget ran out was not checked
    let sandbox = child(doctor, "sandboxes", "sandbox", "sb");
    assert_eq!(sandbox.get_text("coverage"), Some("none"));
    assert_eq!(sandbox.get_text("reason"), Some("timeout"));
    // the tree: the machine's node with one list per kind on it
    let tree = doctor.get("tree").unwrap();
    assert_eq!(
        keys(tree),
        [
            "machine",
            "status",
            "coverage",
            "checks",
            "blocked_by",
            "reason",
            "stores",
            "sandboxes"
        ]
    );
    assert_eq!(doctor.get("ok"), Some(&SdkValue::Bool(false)));
    assert_eq!(doctor.get_text("status"), Some("not_checked"));
    assert_eq!(
        json::print(doctor.get("counts").unwrap()),
        r#"{"ok":2,"notice":0,"warning":0,"error":0,"not_checked":2}"#
    );
    // one progress event per finished thing, the node without child lists
    let shown: Vec<String> = (events.iter())
        .map(|event| {
            let event = event.get("progress").unwrap();
            assert_eq!(keys(event), ["phase", "done", "total", "node"]);
            format!(
                "{} {}/{} {}",
                event.get_text("phase").unwrap(),
                event.get_int("done").unwrap(),
                event.get_int("total").unwrap(),
                keys(event.get("node").unwrap()).len()
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            "self 1/4 6",
            "stores 2/4 6",
            "stores 3/4 6",
            "sandboxes 4/4 6"
        ]
    );
}

#[test]
fn a_failed_parent_blocks_what_is_on_it_without_trying_it() {
    let (report, events) = doctor(Thing::new(Kind::Machine, "bad"), false);
    // what is on it is a node that tried nothing
    let a = child(&report, "stores", "store", "a");
    assert_eq!(
        json::print(a),
        concat!(
            r#"{"store":"a","status":"not_checked","coverage":"none","checks":[],"#,
            r#""blocked_by":{"machine":"bad","reason":"outdated"},"reason":null}"#
        )
    );
    assert_eq!(
        json::print(&SdkValue::List(list(&report, "findings").to_vec())),
        concat!(
            r#"[{"machine":"bad","check":"m.worker","severity":"error","#,
            r#""message":"worker is from another build","#,
            r#""fix":"layerfs machine stop bad && layerfs machine start bad","#,
            r#""from":null,"evidence":null}]"#
        )
    );
    let open = list(&report, "not_checked");
    assert_eq!(open.len(), 3);
    for entry in open {
        assert_eq!(
            json::print(entry.get("blocked_by").unwrap()),
            r#"{"machine":"bad","reason":"outdated"}"#
        );
        assert_eq!(entry.get("check"), Some(&SdkValue::Null));
        assert_eq!(entry.get("reason"), Some(&SdkValue::Null));
    }
    assert_eq!(report.get_text("status"), Some("error"));
    assert_eq!(
        json::print(report.get("counts").unwrap()),
        r#"{"ok":0,"notice":0,"warning":0,"error":1,"not_checked":3}"#
    );
    assert_eq!(events.len(), 4, "one event per thing, blocked ones too");
    // and the blocked Stores ran no check: a blocked node is made, not run
    assert!(list(a, "checks").is_empty());
}

#[test]
fn a_blocked_store_s_checks_are_never_called() {
    // a registry whose only Store check counts its calls, under a machine
    // that blocks it
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn counted(_: &Probe<'_>, _: &mut Node) {
        CALLS.fetch_add(1, Ordering::SeqCst);
    }
    static ONLY: [KindDoctor; 2] = [
        KindDoctor {
            kind: Kind::Machine,
            checks: MACHINE,
            budget: Duration::from_secs(5),
            run: machine,
        },
        KindDoctor {
            kind: Kind::Store,
            checks: STORE,
            budget: Duration::from_secs(5),
            run: counted,
        },
    ];
    let registry = Registry {
        kinds: &ONLY,
        locate,
    };
    let mut sink = |_| {};
    let mut events = Events::new(&mut sink);
    let options = Options::default();
    let home = Path::new("/nowhere");
    let bad = Thing::new(Kind::Machine, "bad");
    let report = run_with(&registry, home, bad, &options, &mut events).unwrap();
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(report.tree.children.len(), 3);
    let dev = Thing::new(Kind::Machine, "dev");
    run_with(&registry, home, dev, &options, &mut events).unwrap();
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        2,
        "a ready machine's Stores run"
    );
}

#[test]
fn a_notice_is_a_finding_and_a_clean_report_is_ok() {
    let (report, _) = doctor(Thing::new(Kind::Bucket, "r2"), false);
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)));
    assert_eq!(report.get_text("status"), Some("ok"));
    let tree = report.get("tree").unwrap();
    assert_eq!(tree.get_text("coverage"), Some("shallow"));
    assert_eq!(list(tree, "checks").len(), 1, "no Full-only check");
    // the Full check
    let (full, _) = doctor(Thing::new(Kind::Bucket, "r2"), true);
    let tree = full.get("tree").unwrap();
    assert_eq!(tree.get_text("coverage"), Some("full"));
    assert_eq!(list(tree, "checks").len(), 2);
    // a notice makes the long form's `ok` false; the thing's status says it
    let registry = Registry {
        kinds: &KINDS,
        locate: |_, _| {
            Ok(Located {
                state: State::Ready,
                blocked: None,
                children: Vec::new(),
            })
        },
    };
    let mut sink = |_| {};
    let mut events = Events::new(&mut sink);
    let quiet = Thing::new(Kind::Machine, "quiet");
    let home = Path::new("/nowhere");
    let report = run_with(&registry, home, quiet, &Options::default(), &mut events).unwrap();
    assert!(!report.ok());
    assert_eq!(report.status(), Verdict::Notice);
    assert_eq!(report.tree.coverage, Coverage::Shallow);
    let long = report.to_sdk();
    assert_eq!(long.get("ok"), Some(&SdkValue::Bool(false)));
    assert_eq!(
        list(&long, "findings")[0].get_text("severity"),
        Some("notice")
    );
}

#[test]
fn every_kind_s_check_ids_are_the_frozen_lists_or_pending() {
    let spec = read(SPEC);
    let frozen = spec.get("doctor").unwrap().get("checks").unwrap();
    let pending = read(PENDING);
    let pending = pending.get("kinds").unwrap();
    assert_eq!(kinds().len(), Kind::ALL.len());
    for kind in Kind::ALL {
        let word = kind.word();
        let doctor = kinds().iter().find(|doctor| doctor.kind == kind).unwrap();
        let said: Vec<(String, bool)> = (list(frozen, word).iter())
            .map(|check| {
                let id = check.get_text("check").unwrap().to_owned();
                (id, check.get("full_only") == Some(&SdkValue::Bool(true)))
            })
            .collect();
        let entry = pending
            .get(word)
            .unwrap_or_else(|| panic!("pending names no kind {word}"));
        let today: Vec<&str> = (doctor.checks.iter())
            .filter(|check| !said.iter().any(|(id, _)| id == check.id))
            .map(|check| check.id)
            .collect();
        let missing: Vec<&str> = (said.iter())
            .filter(|(id, _)| !doctor.checks.iter().any(|check| check.id == id))
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(
            today,
            texts(entry, "today"),
            "{word}: ids not in calls.json"
        );
        assert_eq!(
            missing,
            texts(entry, "missing"),
            "{word}: calls.json ids not given"
        );
        // the ids both name: the same Full-only mark, and the frozen order
        // unless pending says it still differs
        let common: Vec<&str> = (doctor.checks.iter())
            .filter(|check| said.iter().any(|(id, _)| id == check.id))
            .map(|check| check.id)
            .collect();
        for check in doctor.checks {
            if let Some((_, full)) = said.iter().find(|(id, _)| id == check.id) {
                assert_eq!(check.full_only, *full, "{word}: {}", check.id);
            }
        }
        let frozen_order: Vec<&str> = (said.iter())
            .map(|(id, _)| id.as_str())
            .filter(|id| common.contains(id))
            .collect();
        let differs = entry.get("order") == Some(&SdkValue::Bool(true));
        assert_eq!(common != frozen_order, differs, "{word}: order");
    }
}
