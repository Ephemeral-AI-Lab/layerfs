//! #818 W6 (lane W): a file call plans its paths' base reads level by level
//! (PostgreSQL) and walks the directories it passes by kind only, and still
//! answers exactly as before. One `fs.apply` writes across directories at
//! several depths, creates a nested directory, copies and moves across
//! directories and overwrites a file; refusals keep their order and texts
//! (a name the grammar refuses before a parent that is a file, a parent
//! that is a file, a missing parent), each at its change's index; and a
//! list of 1,100 changes (past one plan of `PLAN_PATHS` = 1,024) commits
//! every change. Runs on SQLite, and on PostgreSQL with
//! `LAYERFS_TEST_POSTGRES_URL` (NOT_RUN without it).
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
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
    layerfs_store_service::methods::call(&opened.service, &call)
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
