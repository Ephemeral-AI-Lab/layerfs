//! A project's state, its answers and its cursor (call surface 2.0):
//! `project.list` and `project.get` derive `blocked` from the records of a
//! project's Store and that Store's machine with no probe, `busy` from the
//! `op` an Init or a delete keeps in the project's record, and `broken`
//! `unresolved` from one whose owner is gone. The records-only cases run on
//! any host; the cases that open a Store run where a sqlite Store opens.
use layerfs_api::home::{self, paths, write_record};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lfs-project-state-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn private_home(tag: &str) -> PathBuf {
    let home = tempdir(tag);
    home::open(&home).unwrap();
    home
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {})
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message))
}

fn items(list: &SdkValue) -> Vec<SdkValue> {
    match list.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("{other:?}"),
    }
}

/// The keys of a map, in order.
fn keys(value: &SdkValue) -> Vec<String> {
    match value {
        SdkValue::Map(fields) => fields.iter().map(|(key, _)| key.clone()).collect(),
        other => panic!("{other:?}"),
    }
}

/// Project `name` of Store `store`'s record: its name and `fields`.
fn project_record(home: &Path, store: &str, name: &str, fields: Vec<(&str, SdkValue)>) {
    let mut record = vec![("name".to_owned(), t(name))];
    record.extend(fields.into_iter().map(|(k, v)| (k.to_owned(), v)));
    write_record(
        &paths::project_record(home, store, name),
        &SdkValue::Map(record),
    )
    .unwrap();
}

/// A recorded operation `call` at `phase`, owned by process `pid` (0 is no
/// process; 1 is one that lives and is not this one).
fn op(call: &str, phase: &str, pid: i64, uncertain: bool) -> SdkValue {
    SdkValue::map([
        ("id", t("op-0000000000000001")),
        ("name", t(call)),
        ("phase", t(phase)),
        ("args", SdkValue::Null),
        ("owner", SdkValue::map([("pid", SdkValue::Int(pid))])),
        ("started", SdkValue::Int(1_000)),
        ("updated", SdkValue::Int(1_000)),
        ("uncertain", SdkValue::Bool(uncertain)),
    ])
}

/// A home with sqlite Store `shop` on machine `gone`, which the home does
/// not record: the Store is blocked by it, `missing`, so no Store is asked.
fn shop_on_a_missing_machine(tag: &str) -> PathBuf {
    let home = private_home(tag);
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("gone")),
        ]),
    )
    .unwrap();
    home
}

#[test]
fn a_project_of_a_blocked_store_answers_blocked_from_records_alone() {
    let home = shop_on_a_missing_machine("blocked");
    project_record(
        &home,
        "shop",
        "api",
        vec![("created", SdkValue::Int(7_000))],
    );
    project_record(&home, "shop", "web", vec![]);
    let by = SdkValue::map([("machine", t("gone")), ("reason", t("missing"))]);

    let list = ok(&home, "project.list", &[]);
    let rows = items(&list);
    assert_eq!(rows.len(), 2, "{list:?}");
    for (row, name) in rows.iter().zip(["shop/api", "shop/web"]) {
        assert_eq!(keys(row), ["project", "state", "blocked_by"], "{row:?}");
        assert_eq!(row.get_text("project"), Some(name));
        assert_eq!(row.get_text("state"), Some("blocked"));
        assert_eq!(row.get("blocked_by"), Some(&by));
    }
    assert_eq!(list.get("next_cursor"), None, "one page");

    // the long form: every key, `commit` null (the Store was not read) and
    // `size` null for a blocked project
    let long = ok(
        &home,
        "project.list",
        &[
            ("detail", SdkValue::Bool(true)),
            ("with", SdkValue::List(vec![t("stats")])),
        ],
    );
    let row = &items(&long)[0];
    assert_eq!(
        keys(row),
        [
            "project",
            "state",
            "op",
            "blocked_by",
            "reason",
            "fix",
            "store",
            "engine",
            "machine",
            "commit",
            "created_at",
            "size"
        ]
    );
    assert_eq!(row.get_text("engine"), Some("sqlite"));
    assert_eq!(row.get_text("machine"), Some("gone"));
    assert_eq!(row.get("commit"), Some(&SdkValue::Null));
    assert_eq!(row.get("size"), Some(&SdkValue::Null));
    assert_eq!(row.get_text("created_at"), Some("1970-01-01T00:00:07Z"));

    let got = ok(&home, "project.get", &[("project", t("shop/api"))]);
    assert_eq!(
        got,
        SdkValue::map([
            ("store", t("shop")),
            ("state", t("blocked")),
            ("blocked_by", by.clone()),
        ])
    );
    let missing = call(&home, "project.get", &[("project", t("shop/none"))]).unwrap_err();
    assert_eq!(missing.code, "NotFound", "{missing:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_page_ends_at_its_limit_and_its_cursor_carries_the_store_filter_only() {
    let home = shop_on_a_missing_machine("cursor");
    for name in ["a", "b", "c"] {
        project_record(&home, "shop", name, vec![]);
    }
    let page = |args: &[(&str, SdkValue)]| ok(&home, "project.list", args);
    let names = |list: &SdkValue| -> Vec<String> {
        (items(list).iter())
            .filter_map(|row| row.get_text("project").map(str::to_owned))
            .collect()
    };
    let limit = ("limit", SdkValue::Int(2));
    let first = page(&[("store", t("shop")), limit.clone()]);
    assert_eq!(names(&first), ["shop/a", "shop/b"]);
    let cursor = first.get("next_cursor").cloned().expect("a cursor");
    // the cursor carries the filter: given alone, or with the same one
    let rest = page(&[("cursor", cursor.clone())]);
    assert_eq!(names(&rest), ["shop/c"]);
    assert_eq!(rest.get("next_cursor"), None);
    let same = page(&[("store", t("shop")), ("cursor", cursor.clone())]);
    assert_eq!(names(&same), ["shop/c"]);
    // another filter, an altered cursor or a bad limit is refused
    let refused = |args: &[(&str, SdkValue)]| call(&home, "project.list", args).unwrap_err().code;
    assert_eq!(
        refused(&[("store", t("other")), ("cursor", cursor.clone())]),
        "InvalidInput"
    );
    assert_eq!(
        refused(&[("cursor", t("bm90LWEtY3Vyc29y"))]),
        "InvalidInput"
    );
    assert_eq!(refused(&[("limit", SdkValue::Int(0))]), "InvalidInput");
    assert_eq!(refused(&[("limit", SdkValue::Int(1001))]), "InvalidInput");
    // the old paging and the old positional are gone, with no alias
    assert_eq!(refused(&[("page_size", SdkValue::Int(1))]), "InvalidInput");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_recorded_init_or_delete_makes_the_project_busy_or_broken() {
    let home = shop_on_a_missing_machine("ops");
    let mine = std::process::id() as i64;
    project_record(
        &home,
        "shop",
        "busy",
        vec![("op", op("project.delete", "mark", mine, false))],
    );
    project_record(
        &home,
        "shop",
        "lost",
        vec![("op", op("project.delete", "mark", 0, true))],
    );
    project_record(
        &home,
        "shop",
        "stopped",
        vec![("op", op("project.init", "install", 0, false))],
    );
    // a gone owner's certain delete holds nothing: the Store's block shows
    project_record(
        &home,
        "shop",
        "stale",
        vec![("op", op("project.delete", "mark", 0, false))],
    );
    let state = |name: &str| {
        ok(
            &home,
            "project.get",
            &[("project", t(&format!("shop/{name}")))],
        )
    };

    let busy = state("busy");
    assert_eq!(busy.get_text("state"), Some("busy"), "{busy:?}");
    assert_eq!(
        busy.get("op"),
        Some(&SdkValue::map([
            ("name", t("delete")),
            ("phase", t("mark")),
            ("started_at", t("1970-01-01T00:00:01Z")),
        ]))
    );
    for name in ["lost", "stopped"] {
        let broken = state(name);
        assert_eq!(broken.get_text("state"), Some("broken"), "{broken:?}");
        assert_eq!(broken.get_text("reason"), Some("unresolved"));
        let fix = format!("layerfs project delete shop/{name}");
        assert_eq!(broken.get_text("fix"), Some(fix.as_str()));
    }
    assert_eq!(state("stale").get_text("state"), Some("blocked"));

    // the listing says the same, row by row
    let list = ok(&home, "project.list", &[]);
    let words: Vec<_> = (items(&list).iter())
        .map(|row| row.get_text("state").unwrap_or("ready").to_owned())
        .collect();
    assert_eq!(words, ["busy", "broken", "blocked", "broken"], "{list:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_delete_is_refused_busy_while_another_live_process_holds_the_project() {
    let home = shop_on_a_missing_machine("held");
    // process 1 lives and is not this one
    project_record(
        &home,
        "shop",
        "api",
        vec![("op", op("project.init", "install", 1, false))],
    );
    let refused = call(&home, "project.delete", &[("project", t("shop/api"))]).unwrap_err();
    assert_eq!(refused.code, "Busy", "{refused:?}");
    // nothing changed: the record still holds the other process's Init
    let record = home::read_record(&paths::project_record(&home, "shop", "api"))
        .unwrap()
        .unwrap();
    assert_eq!(
        record.get("op").and_then(|op| op.get_text("name")),
        Some("project.init")
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// The cases that open a Store: a sqlite Store on `native`.
#[cfg(target_os = "linux")]
mod with_a_store {
    use super::*;

    fn shop_with_api(tag: &str) -> (PathBuf, PathBuf, String) {
        let home = private_home(tag);
        let folder = tempdir(&format!("{tag}-folder"));
        std::fs::write(folder.join("README.md"), "hello\n").unwrap();
        ok(&home, "store.create", &[("name", t("shop"))]);
        let made = ok(
            &home,
            "project.init",
            &[
                ("project", t("shop/api")),
                ("folder", t(&folder.display().to_string())),
            ],
        );
        assert_eq!(keys(&made), ["commit"], "{made:?}");
        let commit = made.get_text("commit").unwrap().to_owned();
        assert_eq!(commit.len(), 66);
        (home, folder, commit)
    }

    /// A Store of schema version 2 (#776 R1: before each Commit recorded
    /// its attempt) is refused at open by its version: `store get --with
    /// stats` answers `Unsupported` (raised `OldStore`) with that refusal
    /// as its message and `store upgrade` as its fix (#804: schema 2 has a step), its projects are
    /// `blocked` by it `outdated`, and its doctor fails `store.schema`
    /// with the same fix, its project's check waiting on it `outdated`.
    /// None says `Failed` or Debug text.
    #[test]
    fn an_old_format_store_answers_outdated_with_its_fix() {
        let (home, folder, _) = shop_with_api("outdated");
        layerfs_api::close_idle(std::time::Duration::ZERO);
        rusqlite::Connection::open(paths::store_file(&home, "shop"))
            .unwrap()
            .execute_batch("PRAGMA user_version=36")
            .unwrap();
        let version = "SQLite Store schema version 2 (before each Commit recorded its attempt, \
#776): not supported by this release and not migrated; create a new Store";
        let with = |what: &str| ("with", SdkValue::List(vec![t(what)]));
        let stats = call(
            &home,
            "store.get",
            &[
                ("store", t("shop")),
                with("stats"),
                ("detail", SdkValue::Bool(true)),
            ],
        )
        .unwrap_err();
        assert_eq!(
            (stats.code.as_str(), stats.message.as_str()),
            ("Unsupported", version),
            "{stats:?}"
        );
        assert_eq!(stats.detail.get_text("code"), Some("OldStore"), "{stats:?}");
        assert_eq!(
            stats.detail.get_text("fix"),
            Some("layerfs store upgrade shop")
        );
        let by = SdkValue::map([("store", t("shop")), ("reason", t("outdated"))]);
        let got = ok(&home, "project.get", &[("project", t("shop/api"))]);
        assert_eq!(got.get_text("state"), Some("blocked"), "{got:?}");
        assert_eq!(got.get("blocked_by"), Some(&by), "{got:?}");
        let row = &items(&ok(&home, "project.list", &[]))[0];
        assert_eq!(row.get("blocked_by"), Some(&by), "{row:?}");
        let doctor = format!("{:?}", ok(&home, "store.doctor", &[("store", t("shop"))]));
        let said = [
            "store.schema",
            "layerfs store upgrade shop",
            version,
            "(\"reason\", Text(\"outdated\"))",
        ];
        for said in said {
            assert!(doctor.contains(said), "{said}: {doctor}");
        }
        assert!(!doctor.contains("Text(\"failed\")"), "{doctor}");
        layerfs_api::close_idle(std::time::Duration::ZERO);
        std::fs::remove_dir_all(home).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_ready_project_answers_main_s_head_and_its_size() {
        let (home, folder, commit) = shop_with_api("ready");
        let got = ok(&home, "project.get", &[("project", t("shop/api"))]);
        assert_eq!(
            got,
            SdkValue::map([
                ("store", t("shop")),
                ("state", t("ready")),
                ("commit", t(&commit)),
            ])
        );
        let list = ok(
            &home,
            "project.list",
            &[
                ("detail", SdkValue::Bool(true)),
                ("with", SdkValue::List(vec![t("stats")])),
            ],
        );
        let row = &items(&list)[0];
        assert_eq!(row.get_text("state"), Some("ready"), "{row:?}");
        assert_eq!(row.get_text("commit"), Some(commit.as_str()));
        assert_eq!(
            row.get_text("machine"),
            Some(layerfs_home::names::HOST_MACHINE)
        );
        assert_eq!(row.get_int("size"), Some(6), "README.md's six bytes");
        assert!(row
            .get_text("created_at")
            .is_some_and(|at| at.ends_with('Z')));
        // without `--with stats` the short row is the project alone
        let short = ok(&home, "project.list", &[]);
        assert_eq!(items(&short), [SdkValue::map([("project", t("shop/api"))])]);
        // `export --at` takes a prefix and exports that Commit
        let out = tempdir("ready-out").join("api");
        let exported = ok(
            &home,
            "project.export",
            &[
                ("project", t("shop/api")),
                ("destination", t(&out.display().to_string())),
                ("at", t(&commit[..12])),
                ("detail", SdkValue::Bool(true)),
            ],
        );
        assert_eq!(exported.get_text("commit"), Some(commit.as_str()));
        assert_eq!(exported.get_int("file_count"), Some(1));
        assert_eq!(exported.get_int("size"), Some(6));
        std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
        layerfs_api::close_idle(std::time::Duration::ZERO);
        std::fs::remove_dir_all(home).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn the_store_s_list_hides_what_it_does_not_hold_and_a_left_intent_is_unresolved() {
        let (home, folder, _) = shop_with_api("hidden");
        // a record the Store does not list: hidden, and NotFound to get
        project_record(&home, "shop", "ghost", vec![]);
        let names: Vec<_> = (items(&ok(&home, "project.list", &[])).iter())
            .filter_map(|row| row.get_text("project").map(str::to_owned))
            .collect();
        assert_eq!(names, ["shop/api"]);
        let ghost = call(&home, "project.get", &[("project", t("shop/ghost"))]).unwrap_err();
        assert_eq!(ghost.code, "NotFound", "{ghost:?}");
        // and `project delete`, the fix `project doctor` names for it,
        // removes its entry
        let deleted = ok(&home, "project.delete", &[("project", t("shop/ghost"))]);
        assert_eq!(deleted, SdkValue::map([("ok", SdkValue::Bool(true))]));
        assert!(!paths::project_dir(&home, "shop", "ghost").exists());

        // a delete's intent left behind on a project the Store still lists
        let intent = paths::store_dir(&home, "shop")
            .join("delete")
            .join("api.json");
        std::fs::create_dir_all(intent.parent().unwrap()).unwrap();
        std::fs::write(&intent, "{}").unwrap();
        let broken = ok(&home, "project.get", &[("project", t("shop/api"))]);
        assert_eq!(broken.get_text("state"), Some("broken"), "{broken:?}");
        assert_eq!(broken.get_text("reason"), Some("unresolved"));
        std::fs::remove_file(&intent).unwrap();

        // an Init that stopped part-way and never reached the Store: listed
        // broken, and `project delete` removes its entry
        project_record(
            &home,
            "shop",
            "half",
            vec![("op", op("project.init", "install", 0, true))],
        );
        let half = ok(&home, "project.get", &[("project", t("shop/half"))]);
        assert_eq!(half.get_text("reason"), Some("unresolved"), "{half:?}");
        let deleted = ok(&home, "project.delete", &[("project", t("shop/half"))]);
        assert_eq!(deleted, SdkValue::map([("ok", SdkValue::Bool(true))]));
        assert!(!paths::project_dir(&home, "shop", "half").exists());

        // a delete answers in its row's shape and the project is gone
        let deleted = ok(
            &home,
            "project.delete",
            &[("project", t("shop/api")), ("detail", SdkValue::Bool(true))],
        );
        assert_eq!(
            keys(&deleted),
            ["ok", "home_entry_kept", "outcome", "deleted_at"]
        );
        assert_eq!(deleted.get_text("outcome"), Some("deleted"));
        let gone = call(&home, "project.get", &[("project", t("shop/api"))]).unwrap_err();
        assert_eq!(gone.code, "NotFound");
        layerfs_api::close_idle(std::time::Duration::ZERO);
        std::fs::remove_dir_all(home).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn an_init_claims_its_name_at_once_and_a_refusal_leaves_no_entry() {
        let (home, folder, _) = shop_with_api("claim");
        // a name the home already holds is refused before anything is read
        let again = call(
            &home,
            "project.init",
            &[
                ("project", t("shop/api")),
                ("folder", t(&folder.display().to_string())),
            ],
        )
        .unwrap_err();
        assert_eq!(again.code, "Exists", "{again:?}");
        // a folder that is not there is refused before anything is claimed
        let missing = tempdir("claim-missing").join("absent");
        let refused = call(
            &home,
            "project.init",
            &[
                ("project", t("shop/web")),
                ("folder", t(&missing.display().to_string())),
            ],
        )
        .unwrap_err();
        assert_eq!(refused.code, "InvalidInput", "{refused:?}");
        assert!(!paths::project_dir(&home, "shop", "web").exists());
        // a name the Store holds but the home forgot: the Init claims the
        // entry, the Store refuses it, and the claimed entry is removed
        let kept = tempdir("claim-kept").join("api");
        std::fs::rename(paths::project_dir(&home, "shop", "api"), &kept).unwrap();
        let taken = call(
            &home,
            "project.init",
            &[
                ("project", t("shop/api")),
                ("folder", t(&folder.display().to_string())),
            ],
        )
        .unwrap_err();
        assert_eq!(taken.code, "Exists", "{taken:?}");
        assert!(!paths::project_dir(&home, "shop", "api").exists());
        std::fs::rename(&kept, paths::project_dir(&home, "shop", "api")).unwrap();
        std::fs::remove_dir_all(kept.parent().unwrap()).unwrap();
        // the finished record holds no `op`
        let record = home::read_record(&paths::project_record(&home, "shop", "api"))
            .unwrap()
            .unwrap();
        assert!(record.get("op").is_none(), "{record:?}");
        layerfs_api::close_idle(std::time::Duration::ZERO);
        std::fs::remove_dir_all(home).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
    }
}
