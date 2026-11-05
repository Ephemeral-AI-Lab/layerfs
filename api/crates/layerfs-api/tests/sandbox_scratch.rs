//! `sandbox.scratch` (#549): the operator pass over one machine's settled
//! Sandbox scratch. `--dry-run` lists exactly the roots a real pass deletes,
//! each with the record that owns it; an `Unresolved` Sandbox's scratch
//! survives the automatic pass and a running one is kept; nothing beside a
//! deleted root is touched.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_control::sandbox::scratch::Scratch;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh, private home.
fn tempdir(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "lfs-scratch-call-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// One `sandbox.scratch` call in `home`.
fn call(home: &Path, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), "sandbox.scratch", args, &mut |_| {})
        .map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, args).unwrap_or_else(|e| panic!("sandbox.scratch: {e:?}"))
}

/// The scratch root of Sandbox `name` on the native runtime.
fn root(home: &Path, name: &str) -> PathBuf {
    home.join("machine/native/run")
        .join(format!("lfs-x-{name}"))
        .join("tmp")
}

/// `path` with one command's throwaway file in it.
fn seed(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    let file = path.join("commands-left-this");
    std::fs::write(&file, "scratch").unwrap();
    file
}

/// One Sandbox record owning `root(home, name)`, in the state its `id` and
/// `op` derive.
fn record(
    home: &Path,
    name: &str,
    machine: &str,
    id: Option<&str>,
    op: Option<SdkValue>,
    generation: u64,
) -> PathBuf {
    let root = root(home, name);
    seed(&root);
    let mut fields = vec![
        ("name".to_owned(), t(name)),
        ("project".to_owned(), t("shop/api")),
        ("machine".to_owned(), t(machine)),
        ("created".to_owned(), SdkValue::Int(1)),
        (
            "scratch".to_owned(),
            Scratch::new(root.clone(), 1, generation)
                .unwrap()
                .to_record(),
        ),
    ];
    if let Some(id) = id {
        fields.push(("id".to_owned(), t(id)));
    }
    if let Some(op) = op {
        fields.push(("op".to_owned(), op));
    }
    write_record(&paths::sandbox_record(home, name), &SdkValue::Map(fields)).unwrap();
    root
}

/// A settled record: no id and no intent, so its state is `gone`.
fn gone(home: &Path, name: &str, machine: &str, generation: u64) -> PathBuf {
    record(home, name, machine, None, None, generation)
}

/// A running record: an id and no intent.
fn running(home: &Path, name: &str, machine: &str) -> PathBuf {
    record(home, name, machine, Some(name), None, 3)
}

/// A record whose close lost its answer: an uncertain intent.
fn unresolved(home: &Path, name: &str, machine: &str) -> PathBuf {
    let op = SdkValue::map([
        ("id", t("op-00000000000000aa")),
        ("name", t("sandbox.close")),
        ("phase", t("stop")),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("uncertain", SdkValue::Bool(true)),
    ]);
    record(home, name, machine, Some(name), Some(op), 4)
}

/// A Store, a Workspace and the runtime's non-scratch parts: none of them
/// is ever a scratch.
fn seed_bystanders(home: &Path) {
    for (path, body) in [
        (home.join("store/shop/store.sqlite"), "store"),
        (home.join("sandbox/keep/workspace/w.json"), "workspace"),
        (
            home.join("machine/native/run/lfs-x-keep/overlay/overlay.sqlite"),
            "overlay",
        ),
        (
            home.join("machine/native/run/lfs-x-keep/workspaces/1/file"),
            "workspace",
        ),
        (home.join("machine/native/run/lfs-x-keep/daemon.log"), "log"),
    ] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
}

/// Asserts every seeded bystander still exists.
fn bystanders(home: &Path) {
    for file in [
        home.join("store/shop/store.sqlite"),
        home.join("sandbox/keep/workspace/w.json"),
        home.join("machine/native/run/lfs-x-keep/overlay/overlay.sqlite"),
        home.join("machine/native/run/lfs-x-keep/workspaces/1/file"),
        home.join("machine/native/run/lfs-x-keep/daemon.log"),
    ] {
        assert!(file.is_file(), "{} must survive", file.display());
    }
}

/// `field` as `(sandbox, root)` pairs, in report order.
fn pairs(value: &SdkValue, field: &str) -> Vec<(String, String)> {
    let Some(SdkValue::List(items)) = value.get(field) else {
        panic!("{field} is not a list: {value:?}");
    };
    items
        .iter()
        .map(|item| {
            (
                item.get_text("sandbox").unwrap().to_owned(),
                item.get_text("root").unwrap().to_owned(),
            )
        })
        .collect()
}

/// `kept` as `(sandbox, root, why)` triples.
fn kept(value: &SdkValue) -> Vec<(String, String, String)> {
    let Some(SdkValue::List(items)) = value.get("kept") else {
        panic!("kept is not a list: {value:?}");
    };
    items
        .iter()
        .map(|item| {
            (
                item.get_text("sandbox").unwrap().to_owned(),
                item.get_text("root").unwrap().to_owned(),
                item.get_text("why").unwrap().to_owned(),
            )
        })
        .collect()
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

#[test]
fn dry_run_lists_exactly_what_the_real_pass_deletes() {
    let home = tempdir("pass");
    home::open(&home).unwrap();
    seed_bystanders(&home);
    let a = gone(&home, "a", "native", 1);
    let b = gone(&home, "b", "native", 2);
    let live = running(&home, "c", "native");
    let stuck = unresolved(&home, "d", "native");
    // another machine's settled scratch is not this pass's to delete
    let elsewhere = gone(&home, "e", "dev", 5);

    let dry = ok(
        &home,
        &[("machine", t("native")), ("dry_run", SdkValue::Bool(true))],
    );
    assert_eq!(dry.get_text("machine"), Some("native"));
    assert_eq!(dry.get("dry_run"), Some(&SdkValue::Bool(true)));
    let planned = vec![("a".to_owned(), text(&a)), ("b".to_owned(), text(&b))];
    assert_eq!(pairs(&dry, "planned"), planned);
    assert!(
        pairs(&dry, "deleted").is_empty(),
        "a dry run deletes nothing: {dry:?}"
    );
    assert_eq!(
        kept(&dry),
        vec![
            ("c".to_owned(), text(&live), "live".to_owned()),
            ("d".to_owned(), text(&stuck), "unresolved".to_owned()),
        ]
    );
    for path in [&a, &b, &live, &stuck, &elsewhere] {
        assert!(
            path.is_dir(),
            "a dry run deletes nothing: {}",
            path.display()
        );
    }
    bystanders(&home);

    let applied = ok(&home, &[("machine", t("native"))]);
    assert_eq!(applied.get("dry_run"), Some(&SdkValue::Bool(false)));
    assert_eq!(
        pairs(&applied, "deleted"),
        planned,
        "a real pass deletes exactly the plan"
    );
    assert!(pairs(&applied, "planned").is_empty());
    assert_eq!(
        kept(&applied),
        kept(&dry),
        "both passes share the predicate"
    );
    assert!(!a.exists() && !b.exists());
    for path in [&live, &stuck, &elsewhere] {
        assert!(path.is_dir(), "{} must survive", path.display());
    }
    bystanders(&home);

    // a second pass has nothing left to plan or delete
    let again = ok(
        &home,
        &[("machine", t("native")), ("dry_run", SdkValue::Bool(true))],
    );
    assert!(pairs(&again, "planned").is_empty(), "{again:?}");
}

#[test]
fn the_pass_needs_a_machine_name() {
    let home = tempdir("machine");
    home::open(&home).unwrap();
    let (code, message) = call(&home, &[]).unwrap_err();
    assert_eq!(code, "InvalidInput");
    assert!(message.contains("is required"), "{message}");
    let (code, message) = call(&home, &[("machine", t(""))]).unwrap_err();
    assert_eq!(code, "InvalidInput");
    assert!(message.contains("a machine name is required"), "{message}");
}
