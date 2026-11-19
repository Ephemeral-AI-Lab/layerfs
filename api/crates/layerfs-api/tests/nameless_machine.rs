//! Records an earlier build left that this build does not reach (RV3 FAIL 1
//! and NOTE 7), over records alone: no provider tool, worker or VM.
//!
//! - A machine whose record has no 2.0 name (`vm_name`) that still holds a
//!   Store and a Sandbox: `machine remove` is refused `InUse` with the
//!   whole fix chain (the Sandbox's remove, the Store's, then the
//!   machine's), and running that chain removes every record, reaching
//!   nothing on the machine; each records-only answer says in `left` what
//!   may remain there.
//! - Every view of that machine's broken state (`machine get`, `machine
//!   list`, both with `--with live`, short and long, `home get --with
//!   tree` and `machine doctor`'s finding) shows the same chain as its fix, shrinking as each step runs
//!   (RT5 note 3); with nothing left on it the fix is `machine remove`.
//! - `native` has no provider name and is not such a record: its remove is
//!   still refused as built in.
//! - A Store record this build does not read goes alone, and `left` names
//!   the Store file it keeps with the line that removes it.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, write_entity, write_record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-nameless-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// The call's long form (`detail`).
fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    let detail = std::iter::once(("detail".to_owned(), SdkValue::Bool(true)));
    let args = SdkValue::Map(
        (args.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .chain(detail)
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {})
}

/// The texts of list `value`.
fn texts(value: Option<&SdkValue>) -> Vec<String> {
    match value {
        Some(SdkValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                SdkValue::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect(),
        other => panic!("not a list: {other:?}"),
    }
}

/// The call's short form.
fn short(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    let args = SdkValue::Map(
        (args.iter())
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).unwrap()
}

/// Machine `name`'s item of list answer `list`.
fn item<'a>(list: &'a SdkValue, name: &str) -> &'a SdkValue {
    let Some(SdkValue::List(items)) = list.get("items") else {
        panic!("no items in {list:?}")
    };
    (items.iter())
        .find(|item| item.get_text("machine") == Some(name))
        .unwrap_or_else(|| panic!("no {name} in {list:?}"))
}

/// The `fix` every view of machine `name`'s state shows, checked equal:
/// `machine get` and `machine list`, long and short, each with and
/// without `--with live`, the home's tree, and the doctor's
/// `machine.configuration` finding.
fn shown_fix(home: &Path, name: &str) -> String {
    let machine = ("machine", t(name));
    let live = ("with", SdkValue::List(vec![t("live")]));
    let mut seen = Vec::new();
    for asked in [vec![machine.clone()], vec![machine.clone(), live.clone()]] {
        for got in [
            call(home, "machine.get", &asked).unwrap(),
            short(home, "machine.get", &asked),
        ] {
            assert_eq!(got.get_text("state"), Some("broken"), "{got:?}");
            assert_eq!(got.get_text("reason"), Some("outdated"), "{got:?}");
            seen.push(got.get_text("fix").map(str::to_owned));
        }
    }
    for asked in [vec![], vec![live]] {
        for list in [
            call(home, "machine.list", &asked).unwrap(),
            short(home, "machine.list", &asked),
        ] {
            seen.push(item(&list, name).get_text("fix").map(str::to_owned));
        }
    }
    let tree = [("with", SdkValue::List(vec![t("tree")]))];
    let tree = call(home, "home.get", &tree).unwrap();
    let Some(SdkValue::List(machines)) = tree.get("tree").and_then(|tree| tree.get("machines"))
    else {
        panic!("no machines in {tree:?}")
    };
    let node = (machines.iter())
        .find(|node| node.get_text("machine") == Some(name))
        .unwrap_or_else(|| panic!("no {name} in {tree:?}"));
    seen.push(node.get_text("fix").map(str::to_owned));
    let doctor = call(home, "machine.doctor", &[machine]).unwrap();
    let Some(SdkValue::List(found)) = doctor.get("findings") else {
        panic!("no findings in {doctor:?}")
    };
    let found = (found.iter())
        .find(|f| f.get_text("check") == Some("machine.configuration"))
        .unwrap_or_else(|| panic!("no configuration finding in {doctor:?}"));
    assert!(
        (found.get_text("message").unwrap_or("")).contains("has no vm_name"),
        "{found:?}"
    );
    seen.push(found.get_text("fix").map(str::to_owned));
    let first = seen[0].clone().expect("a fix");
    for fix in &seen {
        assert_eq!(fix.as_deref(), Some(first.as_str()), "{seen:?}");
    }
    first
}

/// Runs one step of a fix line, `layerfs <family> <call> <name>`.
fn step(home: &Path, line: &str) -> SdkValue {
    let words: Vec<&str> = line.split_whitespace().collect();
    let ["layerfs", family, verb, name] = words.as_slice() else {
        panic!("not one call with one name: {line}");
    };
    call(home, &format!("{family}.{verb}"), &[(family, t(name))])
        .unwrap_or_else(|refusal| panic!("{line}: {refusal:?}"))
}

#[test]
fn a_nameless_machines_holders_and_the_machine_go_by_their_records() {
    // no provider tool and no bundle: anything that reached the machine
    // would fail
    let empty = tempdir("path");
    std::env::set_var("PATH", &empty);
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    std::env::set_var("HOME", &empty);
    std::env::set_var("LAYERFS_BUNDLE", empty.join("missing"));
    let home = tempdir("home");
    call(&home, "machine.list", &[]).unwrap();

    write_entity(
        &paths::machine_record(&home, "old"),
        &SdkValue::map([
            ("name", t("old")),
            ("platform", t("lima")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("old")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::sandbox_record(&home, "sb"),
        &SdkValue::map([
            ("name", t("sb")),
            ("id", t("sb")),
            ("machine", t("old")),
            ("project", t("shop/app")),
        ]),
    )
    .unwrap();

    // every view of its state shows the chain its remove is refused with
    let chain =
        "layerfs sandbox remove sb && layerfs store remove shop && layerfs machine remove old";
    assert_eq!(shown_fix(&home, "old"), chain);

    let refused = call(&home, "machine.remove", &[("machine", t("old"))]).unwrap_err();
    assert_eq!(refused.code, "InUse", "{refused:?}");
    assert!(
        refused.message.contains("no 2.0 name"),
        "{}",
        refused.message
    );
    let fix = refused
        .detail
        .get_text("fix")
        .expect("a fix chain")
        .to_owned();
    assert_eq!(fix, chain, "the refusal and the state disagree");
    let steps: Vec<&str> = fix.split(" && ").collect();

    let sandbox = step(&home, steps[0]);
    assert_eq!(sandbox.get_text("outcome"), Some("already_gone"));
    let left = texts(sandbox.get("left"));
    assert_eq!(
        left.len(),
        1,
        "the worker was asked for something: {left:?}"
    );
    assert!(left[0].contains("nothing on it was touched"), "{left:?}");
    assert!(!paths::sandbox_record(&home, "sb").exists());
    assert_eq!(
        shown_fix(&home, "old"),
        "layerfs store remove shop && layerfs machine remove old"
    );

    let store = step(&home, steps[1]);
    assert_eq!(store.get_text("outcome"), Some("removed"));
    let left = texts(store.get("left"));
    assert!(left[0].contains("stays on that machine's disk"), "{left:?}");
    assert!(!paths::store_dir(&home, "shop").exists());
    // nothing left on it: the bare remove, which now succeeds
    assert_eq!(shown_fix(&home, "old"), "layerfs machine remove old");

    let machine = step(&home, steps[2]);
    assert_eq!(machine.get_text("outcome"), Some("removed"));
    let message = machine.get_text("message").unwrap_or("");
    assert!(message.contains("only its record was removed"), "{message}");
    assert!(!paths::machine_dir(&home, "old").exists());

    // `native` keeps its built-in refusal
    let native = call(&home, "machine.remove", &[("machine", t("native"))]).unwrap_err();
    assert_eq!(native.code, "InvalidInput", "{native:?}");
    assert!(native.message.contains("built in"), "{}", native.message);
    assert!(paths::machine_record(&home, "native").exists());

    // a Store record this build does not read: its file is kept and named
    write_record(
        &paths::store_record(&home, "odd"),
        &SdkValue::map([("name", t("odd"))]),
    )
    .unwrap();
    std::fs::write(paths::store_file(&home, "odd"), b"kept").unwrap();
    let odd = call(&home, "store.remove", &[("store", t("odd"))]).unwrap();
    assert_eq!(odd.get_text("outcome"), Some("removed"));
    let left = texts(odd.get("left"));
    assert!(left[0].contains("was kept"), "{left:?}");
    assert!(left[0].contains("rm -f "), "{left:?}");
    assert!(paths::store_file(&home, "odd").exists());
    assert!(!paths::store_record(&home, "odd").exists());

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(empty);
}
