//! A record that cannot be read (conformance R4 F5): the home's tree and
//! its doctors still list everything else, the bad record is a `broken`
//! node its kind's own `.record` check names, and a machine only a Sandbox
//! names is not there (its Sandbox's reference checks say so, as errors in
//! both the home's records check and the Sandbox's own).
use layerfs_api::home::{paths, write_entity};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-dur-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn call(home: &Path, name: &str, pairs: Vec<(&str, SdkValue)>) -> Result<SdkValue, SdkRefusal> {
    let args = pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    layerfs_api::call(Some(home), name, SdkValue::Map(args), &mut |_| {})
}

fn answer(home: &Path, name: &str, pairs: Vec<(&str, SdkValue)>) -> SdkValue {
    call(home, name, pairs).unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message))
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

fn named<'a>(nodes: &'a [SdkValue], word: &str, name: &str) -> Option<&'a SdkValue> {
    nodes.iter().find(|node| node.get_text(word) == Some(name))
}

/// The outcome of check `id` in a doctor's long answer's root node.
fn outcome<'a>(long: &'a SdkValue, id: &str) -> Option<&'a str> {
    let tree = long.get("tree")?;
    let row = list(tree, "checks")
        .iter()
        .find(|row| row.get_text("check") == Some(id))?;
    row.get_text("outcome")
}

fn garbage(path: PathBuf) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "{ not a record").unwrap();
}

#[test]
fn an_unreadable_record_is_a_broken_node_and_its_siblings_are_listed() {
    // a stand-in `docker` that is not running: the built-in docker machine
    // answers at once
    let tools = tempdir("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let docker = tools.join("docker");
    std::fs::write(&docker, "#!/bin/sh\necho 'Cannot connect' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![tools];
    dirs.extend(std::env::split_paths(&path));
    std::env::set_var("PATH", std::env::join_paths(dirs).unwrap());
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");

    let home = tempdir("home");
    answer(&home, "home.init", Vec::new());
    write_entity(
        &paths::store_record(&home, "cache"),
        &SdkValue::map([
            ("name", t("cache")),
            ("kind", t("sqlite")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::project_record(&home, "cache", "site"),
        &SdkValue::map([("name", t("site"))]),
    )
    .unwrap();
    // a Sandbox on machine `ghost`, which has no record
    write_entity(
        &paths::sandbox_record(&home, "orphan"),
        &SdkValue::map([
            ("name", t("orphan")),
            ("project", t("cache/site")),
            ("machine", t("ghost")),
            ("id", t("orphan")),
        ]),
    )
    .unwrap();
    garbage(paths::machine_record(&home, "lab"));
    garbage(paths::store_record(&home, "bad"));
    garbage(paths::sandbox_record(&home, "worn"));

    // the tree lists everything that reads, and the bad records it can place
    let got = answer(
        &home,
        "home.get",
        vec![("with", SdkValue::List(vec![t("tree")]))],
    );
    let tree = got.get("tree").unwrap_or_else(|| panic!("{got:?}"));
    let machines = list(tree, "machines");
    let lab = named(machines, "machine", "lab").unwrap_or_else(|| panic!("{tree:?}"));
    assert_eq!(lab.get_text("state"), Some("broken"));
    assert_eq!(lab.get_text("reason"), Some("failed"));
    let native = named(machines, "machine", layerfs_home::names::HOST_MACHINE).unwrap();
    assert!(
        named(list(native, "stores"), "store", "cache").is_some(),
        "{tree:?}"
    );
    let ghost = named(machines, "machine", "ghost").unwrap();
    assert!(named(list(ghost, "sandboxes"), "sandbox", "orphan").is_some());
    let bad = named(list(tree, "stores"), "store", "bad").unwrap_or_else(|| panic!("{tree:?}"));
    assert_eq!(bad.get_text("state"), Some("broken"));

    // the home doctor lists its siblings; `ghost` is not a machine of its own
    let long = answer(&home, "home.doctor", vec![("detail", SdkValue::Bool(true))]);
    let root = long.get("tree").unwrap();
    let checked = list(root, "machines");
    // the built-in `docker` machine's record is written by the first call
    // only in a build with the machine calls (`ensure_built_ins`)
    let mut expected = vec!["lab", layerfs_home::names::HOST_MACHINE];
    if cfg!(feature = "machine") {
        expected.push("docker");
    }
    for name in expected {
        assert!(
            named(checked, "machine", name).is_some(),
            "{name}: {root:?}"
        );
    }
    assert!(named(checked, "machine", "ghost").is_none(), "{root:?}");
    assert!(
        named(list(root, "stores"), "store", "bad").is_some(),
        "{root:?}"
    );
    let orphan = (list(&long, "findings").iter())
        .find(|f| f.get_text("sandbox") == Some("orphan"))
        .unwrap_or_else(|| panic!("{long:?}"));
    assert_eq!(orphan.get_text("severity"), Some("error"), "{orphan:?}");

    // each bad record is named by its kind's own record check
    let detail = || vec![("detail", SdkValue::Bool(true))];
    let with = |word: &'static str, name: &str| {
        let mut pairs = detail();
        pairs.push((word, t(name)));
        pairs
    };
    let lab = answer(&home, "machine.doctor", with("machine", "lab"));
    assert_eq!(outcome(&lab, "machine.record"), Some("error"), "{lab:?}");
    let bad = answer(&home, "store.doctor", with("store", "bad"));
    assert_eq!(outcome(&bad, "store.record"), Some("error"), "{bad:?}");
    let worn = answer(&home, "sandbox.doctor", with("sandbox", "worn"));
    assert_eq!(outcome(&worn, "sandbox.record"), Some("error"), "{worn:?}");
    // a sibling's doctor is not refused for them
    let cache = answer(&home, "store.doctor", with("store", "cache"));
    assert_eq!(outcome(&cache, "store.record"), Some("pass"), "{cache:?}");
    // a Sandbox's dangling machine: an error in its own check, and the
    // machine itself is not there
    let orphan = answer(&home, "sandbox.doctor", with("sandbox", "orphan"));
    assert_eq!(outcome(&orphan, "sandbox.machine_reference"), Some("error"));
    let ghost = call(&home, "machine.doctor", with("machine", "ghost")).unwrap_err();
    assert_eq!(ghost.code, "NotFound", "{}", ghost.message);
}
