//! Records an earlier build left that this build does not reach (RV3 FAIL 1
//! and NOTE 7), over records alone: no provider tool, worker or VM.
//!
//! - A machine whose record has no 2.0 name (`vm_name`) that still holds a
//!   Store and a Sandbox: `machine remove` is refused `InUse` with the
//!   whole fix chain (the Sandbox's remove, the Store's, then the
//!   machine's), and running that chain removes every record, reaching
//!   nothing on the machine; each records-only answer says in `left` what
//!   may remain there.
//! - `native` has no provider name and is not such a record: its remove is
//!   still refused as built in.
//! - A Store record this build does not read goes alone, and `left` names
//!   the Store file it keeps with the line that removes it.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, write_record};
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

    write_record(
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
    assert_eq!(
        fix,
        "layerfs sandbox remove sb && layerfs store remove shop && layerfs machine remove old"
    );
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

    let store = step(&home, steps[1]);
    assert_eq!(store.get_text("outcome"), Some("removed"));
    let left = texts(store.get("left"));
    assert!(left[0].contains("stays on that machine's disk"), "{left:?}");
    assert!(!paths::store_dir(&home, "shop").exists());

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
