//! #818 W6 (lane W): a file call plans its paths' base reads level by level
//! (PostgreSQL) and walks the directories it passes by kind only, and still
//! answers exactly as before. One `fs.apply` writes across directories at
//! several depths, creates a nested directory, copies and moves across
//! directories and overwrites a file; refusals keep their order and texts
//! (a name the grammar refuses before a parent that is a file, a parent
//! that is a file, a missing parent), each at its change's index; and a
//! list of 1,100 changes (past one plan of `PLAN_PATHS` = 1,024) commits
//! every change. Runs on SQLite, and on PostgreSQL with
//! `LAYERFS_TEST_POSTGRES_URL` (NOT_RUN without it). On PostgreSQL a drop
//! sweep checks that a plan read never refuses ahead of a change refused
//! before any read.
use crate::adversary;
use crate::fault_proxy;
use crate::rt;

use layerfs_protocol::sdk::{SdkCall, SdkRefusal, SdkValue};
use rt::{branch_arg as branch, path_arg as path, t, Opened, Store};

fn apply(opened: &Opened, changes: Vec<SdkValue>) -> Result<SdkValue, SdkRefusal> {
    let call = SdkCall {
        method: "fs.apply".into(),
        args: SdkValue::map([
            ("project", t(rt::PROJECT)),
            ("branch", t(rt::BRANCH)),
            ("changes", SdkValue::List(changes)),
        ]),
    };
    layerfs_store_service::families::call(&opened.service, &call)
}

fn change(op: &str, fields: Vec<(&'static str, SdkValue)>) -> SdkValue {
    let mut all = vec![("op".to_owned(), t(op))];
    all.extend(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value)),
    );
    SdkValue::Map(all.into_iter().collect())
}

fn write(to: &str, text: &str) -> SdkValue {
    change(
        "write",
        vec![("path", t(to)), ("bytes", SdkValue::Bytes(text.into()))],
    )
}

fn text(opened: &Opened, at: &str) -> String {
    let read = opened
        .call("fs.read", vec![branch(), path(at)])
        .unwrap_or_else(|refusal| panic!("read {at}: {refusal}"));
    read.get_text("text").unwrap_or_default().to_owned()
}

/// The refusal's code, message and change index.
fn refused(opened: &Opened, changes: Vec<SdkValue>) -> (String, String, Option<i64>) {
    let refusal = apply(opened, changes).expect_err("a refusal");
    let index = refusal.detail.get_int("index");
    (refusal.code, refusal.message, index)
}

fn scenario(store: &mut Store) {
    let source = store.dir.join("source");
    for dir in ["d1/d2/d3", "d4", "deep/a/b/c/d/e"] {
        std::fs::create_dir_all(source.join(dir)).unwrap();
    }
    for (file, body) in [
        ("a.txt", "a\n"),
        ("d1/x.txt", "x\n"),
        ("d1/d2/d3/y.txt", "y\n"),
        ("d4/z.txt", "z\n"),
        ("deep/a/b/c/d/e/f.txt", "f\n"),
    ] {
        std::fs::write(source.join(file), body).unwrap();
    }
    store.init(&source, &mut |_, _| {});
    let opened = store.open("planned");
    rt::fork_work(&opened);

    // Across directories and depths, in one call.
    let changes = vec![
        write("/d1/n.txt", "n1\n"),
        write("/d1/d2/d3/n.txt", "n3\n"),
        write("/deep/a/b/c/d/e/n.txt", "ne\n"),
        change("mkdir", vec![("path", t("/new/p/q"))]),
        change(
            "cp",
            vec![("from", t("/d1/x.txt")), ("to", t("/new/p/q/x.txt"))],
        ),
        change(
            "mv",
            vec![("from", t("/d4/z.txt")), ("to", t("/deep/a/z.txt"))],
        ),
        write("/a.txt", "A\n"),
    ];
    let done = apply(&opened, changes).expect("the apply commits");
    assert_eq!(done.get_text("kind"), Some("Committed"), "{done:?}");
    for (at, body) in [
        ("/d1/n.txt", "n1\n"),
        ("/d1/d2/d3/n.txt", "n3\n"),
        ("/deep/a/b/c/d/e/n.txt", "ne\n"),
        ("/new/p/q/x.txt", "x\n"),
        ("/deep/a/z.txt", "z\n"),
        ("/a.txt", "A\n"),
        ("/deep/a/b/c/d/e/f.txt", "f\n"),
    ] {
        assert_eq!(text(&opened, at), body, "{at}");
    }
    let moved = opened.call("fs.stat", vec![branch(), path("/d4/z.txt")]);
    assert!(moved.is_err(), "{moved:?}");

    // Refusals, each at its change, in the walk's order.
    let cases = [
        (
            vec![write("/d1/ok.txt", "ok\n"), write("/a.txt/b\\c", "x\n")],
            ("InvalidInput", "`/a.txt/b\\c` is not valid", 1),
        ),
        (
            vec![write("/d1/ok.txt", "ok\n"), write("/a.txt/x", "x\n")],
            ("NotDirectory", "/a.txt is not a folder", 1),
        ),
        (
            vec![write("/deep/a/b/c/d/e/f.txt/g/h", "x\n")],
            ("NotDirectory", "/deep/a/b/c/d/e/f.txt is not a folder", 0),
        ),
        (
            vec![write("/ghost/x.txt", "x\n")],
            ("NoSuchPath", "/ghost does not exist", 0),
        ),
        (
            vec![
                write("/d1/ok.txt", "ok\n"),
                write("/d1/ok2.txt", "ok\n"),
                write("/d1/d2/ghost/d/x.txt", "x\n"),
            ],
            ("NoSuchPath", "/d1/d2/ghost does not exist", 2),
        ),
        (
            vec![change("rm", vec![("path", t("/deep/a/b/ghost/x"))])],
            ("NoSuchPath", "/deep/a/b/ghost does not exist", 0),
        ),
    ];
    for (changes, (code, message, index)) in cases {
        assert_eq!(
            refused(&opened, changes),
            (code.to_owned(), message.to_owned(), Some(index)),
            "{message}"
        );
    }
    assert!(opened
        .call("fs.stat", vec![branch(), path("/d1/ok.txt")])
        .is_err());

    // Past one plan window: every change commits.
    let dirs = ["/d1", "/d4", "/deep/a/b", "/new/p"];
    let changes: Vec<SdkValue> = (0..1_100)
        .map(|n| write(&format!("{}/w{n:04}.txt", dirs[n % 4]), &format!("{n}\n")))
        .collect();
    let done = apply(&opened, changes).expect("1,100 changes commit");
    assert_eq!(done.get_text("kind"), Some("Committed"), "{done:?}");
    for n in [0, 1, 1_023, 1_024, 1_025, 1_099] {
        let at = format!("{}/w{n:04}.txt", dirs[n % 4]);
        assert_eq!(text(&opened, &at), format!("{n}\n"), "{at}");
    }
    opened.stop();
}

#[test]
fn sqlite_planned_paths_answer_as_before() {
    let mut store = Store::sqlite("w6-planned-sqlite");
    scenario(&mut store);
}

#[test]
fn postgresql_planned_paths_answer_as_before() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("w6planned") else {
        return;
    };
    scenario(&mut store);
}

/// The refusal order of a call whose first change is refused before any
/// read (here by its arguments: a relative path) while a later change
/// names base paths to plan (W6 review). The connection is dropped at each
/// server message of the call in turn. Every answer that names a change is
/// that first change's own refusal, at index 0, as without a plan. A
/// failure before the changes (the bind, the snapshot, the close) names
/// none. The call never commits.
#[test]
fn postgresql_unread_refusal_comes_before_the_plan() {
    use adversary::{Service, TestStore};
    use fault_proxy::{Fault, FaultProxy};
    let _turn = rt::postgresql_turn();
    let seed: &[(&str, &[u8])] = &[("/dir/inner", b"inner\n")];
    let Some(store) = TestStore::postgres("w6order", seed) else {
        return;
    };
    store.fork("work");
    let call = |service: &Service| {
        let changes = vec![
            write("relative.txt", "x\n"),
            write("/dir/deeper/new.txt", "y\n"),
        ];
        let fields = vec![("branch", t("work")), ("changes", SdkValue::List(changes))];
        service.call("fs.apply", fields)
    };
    let named = |refusal: &SdkRefusal| {
        let index = refusal.detail.get_int("index");
        (refusal.code.clone(), refusal.message.clone(), index)
    };
    let direct = store.open();
    let expected = named(&call(&direct).expect_err("the relative path is refused"));
    direct.stop();
    assert_eq!(expected.2, Some(0), "{expected:?}");

    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let selection = store.selection_via(&proxied);
    let service = Service::open(&store, selection.clone());
    proxy.arm(None);
    let clean = call(&service).expect_err("refused through the proxy");
    let window = proxy.disarm();
    service.stop();
    assert_eq!(named(&clean), expected);
    assert!(window.seen > 0, "the call sends something");
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    for n in 0..window.seen {
        let service = Service::open(&store, selection.clone());
        proxy.arm(Some((n, Fault::Drop)));
        let armed = call(&service);
        let fired = proxy.disarm().fired.is_some();
        assert!(fired, "n{n}: the fault fired");
        let refusal = armed.expect_err("the call never commits");
        let answer = named(&refusal);
        match answer.2 {
            Some(_) => assert_eq!(answer, expected, "n{n}: the first change's own refusal"),
            None => assert!(
                matches!(answer.0.as_str(), "Unknown" | "Failed"),
                "n{n}: a failure before the changes: {answer:?}"
            ),
        }
        *states
            .entry(format!("{} {:?}", answer.0, answer.2))
            .or_default() += 1;
        service.abandon();
    }
    println!("W6_ORDER_STATES K={} {states:?}", window.seen);
    assert!(
        store.no_open_transaction(),
        "no backend left in a transaction"
    );
}
