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
//!   `machine start` and `machine stop`, which need its VM, are refused
//!   with that same fix.
//! - A call on the Store recorded on it (`store compact`, `branch get`,
//!   `branch list`) is refused `Unavailable` with the `blocked_by` that
//!   `store get` shows for that Store and the same fix (LF10).
//! - `native` has no provider name and is not such a record: its remove is
//!   still refused as built in.
//! - A Store record this build does not read goes alone, and `left` names
//!   the Store file it keeps with the line that removes it.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, write_entity, write_record};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
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
    layerfs_api::call(Some(crate::root::root_of(home)), name, args, &mut |_| {})
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
    layerfs_api::call(Some(crate::root::root_of(home)), name, args, &mut |_| {}).unwrap()
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
    // what is on it is blocked for the reason its state says
    if let Some(SdkValue::List(blocked)) = doctor.get("not_checked") {
        for entry in blocked {
            let by = entry.get("blocked_by").and_then(|by| by.get_text("reason"));
            assert_eq!(by, Some("outdated"), "{entry:?}");
        }
    }
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
    let _env = crate::env_lock();
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
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([("created", SdkValue::Int(1))]),
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

    // a call that needs its VM is refused with the same chain, not the
    // bare remove that is itself refused
    let machine = [("machine", t("old"))];
    let start = call(&home, "machine.start", &machine).unwrap_err();
    assert_eq!(start.code, "Failed", "{start:?}");
    assert!(start.message.contains("has no vm_name"), "{start:?}");
    assert_eq!(start.detail.get_text("fix"), Some(chain), "{start:?}");
    let force = [("machine", t("old")), ("force", SdkValue::Bool(true))];
    let stop = call(&home, "machine.stop", &force).unwrap_err();
    assert_eq!(stop.detail.get_text("fix"), Some(chain), "{stop:?}");
    // so is a call that needs its worker, before anything is dialled, in
    // place of a `machine start` that is itself refused (RC-B F3)
    let stats = call(&home, "machine.stats", &machine).unwrap_err();
    assert_eq!(stats.code, "Failed", "{stats:?}");
    assert!(stats.message.contains("has no vm_name"), "{stats:?}");
    assert_eq!(stats.detail.get_text("fix"), Some(chain), "{stats:?}");
    // a call on the Store on it answers what `store get` says of the Store,
    // `blocked` by the machine, with the machine's fix (LF10)
    let store = call(&home, "store.get", &[("store", t("shop"))]).unwrap();
    assert_eq!(store.get_text("state"), Some("blocked"), "{store:?}");
    let blocked_by = store.get("blocked_by").expect("blocked_by").clone();
    assert_eq!(blocked_by.get_text("machine"), Some("old"), "{store:?}");
    assert_eq!(blocked_by.get_text("reason"), Some("outdated"), "{store:?}");
    let app = ("project", t("shop/app"));
    for (name, args) in [
        ("store.compact", vec![("store", t("shop"))]),
        ("branch.get", vec![app.clone(), ("branch", t("main"))]),
        ("branch.list", vec![app.clone()]),
    ] {
        let refused = call(&home, name, &args).unwrap_err();
        assert_eq!(refused.code, "Unavailable", "{name}: {refused:?}");
        let said = &refused.message;
        assert!(
            said.contains("Store shop is blocked by machine old"),
            "{said}"
        );
        assert!(said.contains("has no vm_name"), "{said}");
        assert_eq!(refused.detail.get_text("fix"), Some(chain), "{refused:?}");
        let next = refused
            .detail
            .get("next")
            .and_then(|next| next.get("blocked_by"));
        assert_eq!(next, Some(&blocked_by), "{name}: {refused:?}");
    }

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
    let rest = "layerfs store remove shop && layerfs machine remove old";
    assert_eq!(shown_fix(&home, "old"), rest);
    // a stop with no Sandbox left on it reaches the VM, and is refused so
    let stop = call(&home, "machine.stop", &machine).unwrap_err();
    assert_eq!(stop.detail.get_text("fix"), Some(rest), "{stop:?}");

    let store = step(&home, steps[1]);
    assert_eq!(store.get_text("outcome"), Some("removed"));
    let left = texts(store.get("left"));
    assert!(left[0].contains("stays on that machine's disk"), "{left:?}");
    assert!(!paths::store_dir(&home, "shop").exists());
    // nothing left on it: the bare remove, which now succeeds
    assert_eq!(shown_fix(&home, "old"), "layerfs machine remove old");
    let start = call(&home, "machine.start", &machine).unwrap_err();
    let bare = Some("layerfs machine remove old");
    assert_eq!(start.detail.get_text("fix"), bare, "{start:?}");

    let machine = step(&home, steps[2]);
    assert_eq!(machine.get_text("outcome"), Some("removed"));
    let message = machine.get_text("message").unwrap_or("");
    assert!(message.contains("only its record was removed"), "{message}");
    assert!(!paths::machine_dir(&home, "old").exists());

    // `native` keeps its built-in refusal
    let native = call(
        &home,
        "machine.remove",
        &[("machine", t(layerfs_home::names::HOST_MACHINE))],
    )
    .unwrap_err();
    assert_eq!(native.code, "InvalidInput", "{native:?}");
    assert!(native.message.contains("built in"), "{}", native.message);
    assert!(paths::machine_record(&home, layerfs_home::names::HOST_MACHINE).exists());

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

/// `store move` off a nameless machine is refused as the Store's other
/// calls are (LF10; conformance v23 F3): `Unavailable` with the
/// `blocked_by` of `store get` and the machine's chain, nothing dialled.
#[test]
fn a_store_move_off_a_nameless_machine_is_unavailable_with_its_blocked_by() {
    let _env = crate::env_lock();
    let empty = tempdir("move-path");
    std::env::set_var("PATH", &empty);
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    std::env::set_var("HOME", &empty);
    std::env::set_var("LAYERFS_BUNDLE", empty.join("missing"));
    let home = tempdir("move-home");
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
    let store = call(&home, "store.get", &[("store", t("shop"))]).unwrap();
    let blocked_by = store.get("blocked_by").expect("blocked_by").clone();
    let refused = call(
        &home,
        "store.move",
        &[
            ("store", t("shop")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ],
    )
    .unwrap_err();
    assert_eq!(refused.code, "Unavailable", "{refused:?}");
    assert!(
        refused
            .message
            .contains("Store shop is blocked by machine old"),
        "{refused:?}"
    );
    assert_eq!(
        refused.detail.get_text("fix"),
        Some("layerfs store remove shop && layerfs machine remove old"),
        "{refused:?}"
    );
    let next = (refused.detail.get("next")).and_then(|next| next.get("blocked_by"));
    assert_eq!(next, Some(&blocked_by), "{refused:?}");
    let _ = std::fs::remove_dir_all(home);
}

/// A machine whose VM a live read found gone (`missing`, area 10) while a
/// Store and a Sandbox are recorded on it: before, `store remove` said
/// "start it", `machine start` could not, and `machine remove` was refused
/// `InUse` with no fix. Now every records view shows the chain, a Store call
/// is `Unavailable` with `blocked_by {machine, reason: missing}` and that
/// chain (LF10's shape), `machine start` is refused `Failed` with it, and
/// the chain's holder steps go by their records alone, reaching nothing.
#[test]
fn a_machine_whose_vm_is_gone_lets_its_holders_go_by_their_records() {
    let _env = crate::env_lock();
    let empty = tempdir("gone-path");
    std::env::set_var("PATH", &empty);
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    std::env::set_var("HOME", &empty);
    std::env::set_var("LAYERFS_BUNDLE", empty.join("missing"));
    let home = tempdir("gone");
    call(&home, "machine.list", &[]).unwrap();
    // what a live read writes when the provider holds no VM of its name
    write_entity(
        &paths::machine_record(&home, "lost"),
        &SdkValue::map([
            ("name", t("lost")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-00000000-lost")),
            ("state", t("stopped")),
            ("reason", t("missing")),
            (
                "error",
                t("its provider holds no VM named layerfs-00000000-lost"),
            ),
        ]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("lost")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([("created", SdkValue::Int(1))]),
    )
    .unwrap();
    write_record(
        &paths::sandbox_record(&home, "sb"),
        &SdkValue::map([
            ("name", t("sb")),
            ("id", t("sb")),
            ("machine", t("lost")),
            ("project", t("shop/app")),
        ]),
    )
    .unwrap();
    let chain =
        "layerfs sandbox remove sb && layerfs store remove shop && layerfs machine remove lost";
    let machine = [("machine", t("lost"))];
    for got in [
        call(&home, "machine.get", &machine).unwrap(),
        short(&home, "machine.get", &machine),
    ] {
        assert_eq!(got.get_text("reason"), Some("missing"), "{got:?}");
        assert_eq!(got.get_text("fix"), Some(chain), "{got:?}");
    }
    let listed = call(&home, "machine.list", &[]).unwrap();
    assert_eq!(item(&listed, "lost").get_text("fix"), Some(chain));

    let store = call(&home, "store.get", &[("store", t("shop"))]).unwrap();
    let blocked_by = store.get("blocked_by").expect("blocked_by").clone();
    assert_eq!(blocked_by.get_text("reason"), Some("missing"), "{store:?}");
    let compact = call(&home, "store.compact", &[("store", t("shop"))]).unwrap_err();
    assert_eq!(compact.code, "Unavailable", "{compact:?}");
    assert!(
        compact
            .message
            .contains("Store shop is blocked by machine lost (missing)"),
        "{compact:?}"
    );
    assert_eq!(compact.detail.get_text("fix"), Some(chain), "{compact:?}");
    let next = (compact.detail.get("next")).and_then(|next| next.get("blocked_by"));
    assert_eq!(next, Some(&blocked_by), "{compact:?}");

    let start = call(&home, "machine.start", &machine).unwrap_err();
    assert_eq!(start.code, "Failed", "{start:?}");
    assert!(start.message.contains("VM is gone"), "{start:?}");
    assert_eq!(start.detail.get_text("fix"), Some(chain), "{start:?}");
    let refused = call(&home, "machine.remove", &machine).unwrap_err();
    assert_eq!(refused.code, "InUse", "{refused:?}");
    assert_eq!(refused.detail.get_text("fix"), Some(chain), "{refused:?}");

    let steps: Vec<&str> = chain.split(" && ").collect();
    let sandbox = step(&home, steps[0]);
    assert_eq!(sandbox.get_text("outcome"), Some("already_gone"));
    let left = texts(sandbox.get("left"));
    assert!(left[0].contains("VM is gone"), "{left:?}");
    assert!(!paths::sandbox_record(&home, "sb").exists());
    let store = step(&home, steps[1]);
    assert_eq!(store.get_text("outcome"), Some("removed"));
    let left = texts(store.get("left"));
    assert!(left[0].contains("VM is gone"), "{left:?}");
    assert!(!paths::store_dir(&home, "shop").exists());
    // nothing left on it: its fix is its own remove, no longer refused
    // `InUse` (with no provider tool here it cannot delete a VM)
    let got = call(&home, "machine.get", &machine).unwrap();
    assert_eq!(got.get_text("fix"), Some("layerfs machine remove lost"));
    let remove = call(&home, "machine.remove", &machine);
    assert!(
        remove.as_ref().map_or_else(|r| r.code != "InUse", |_| true),
        "{remove:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(empty);
}
