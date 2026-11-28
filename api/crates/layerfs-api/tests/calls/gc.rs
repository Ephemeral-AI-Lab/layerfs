//! T11.1: the four GC calls in the API call table — their ids-only arguments,
//! typed refusals before any effect, and the result shapes of one reachable
//! refusal per call plus the three removal results and the §11 report.
use layerfs_api::Kind;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::PlaceAuthority;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::admin::open::args::OpenArgs;
use layerfs_store::admin::open::service::with_service;
use layerfs_store::admin::record::store::StoreRecord;
use layerfs_store_service::lookup;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-gc-calls-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

fn call(
    home: &Path,
    name: &str,
    fields: &[(&str, SdkValue)],
) -> Result<SdkValue, (String, String)> {
    layerfs_api::call(
        Some(crate::root::root_of(home)),
        name,
        map(fields),
        &mut |_| {},
    )
    .map_err(|refusal| (refusal.code, refusal.message))
}

fn ok(home: &Path, name: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, fields).unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"))
}

fn refused(home: &Path, name: &str, fields: &[(&str, SdkValue)]) -> (String, String) {
    call(home, name, fields).expect_err("the call is refused")
}

/// A home with Store `shop` and project `shop/api` imported from a folder.
fn project() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    layerfs_api::home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("base.txt"), "hello\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    (home, folder)
}

fn p() -> (&'static str, SdkValue) {
    ("project", t("shop/api"))
}

#[test]
fn the_four_gc_calls_take_ids_only() {
    let expected: &[(&str, &[(&str, Kind)])] = &[
        (
            "branch.remove",
            &[("project", Kind::Project), ("branch", Kind::Name)],
        ),
        ("project.delete", &[("project", Kind::Project)]),
        (
            "project.hide_path",
            &[("project", Kind::Project), ("path", Kind::Text)],
        ),
        ("store.compact", &[("store", Kind::Name)]),
    ];
    for &(name, args) in expected {
        let spec = layerfs_api::find(name).unwrap_or_else(|| panic!("{name} is missing"));
        let given: Vec<(&str, Kind)> = spec.args.iter().map(|arg| (arg.name, arg.kind)).collect();
        assert_eq!(given.as_slice(), args, "{name} arguments");
        assert!(spec.args.iter().all(|arg| !arg.optional), "{name} required");
        // ids only: no call takes a data option. `store.compact` alone
        // declares the shared `on_progress` function (#42 item 45).
        let options: Vec<(&str, Kind)> = spec.options.iter().map(|o| (o.name, o.kind)).collect();
        if name == "store.compact" {
            assert_eq!(options, [("on_progress", Kind::Function)], "{name}");
        } else {
            assert!(options.is_empty(), "{name} has no options");
        }
    }
}

#[test]
fn wrong_kinds_and_unknown_options_are_refused_before_any_effect() {
    let home = tempdir("types");
    layerfs_api::home::open(&home).unwrap();
    let wrong: &[(&str, &[(&str, SdkValue)])] = &[
        (
            "branch.remove",
            &[("project", t("shop/api")), ("branch", SdkValue::Int(1))],
        ),
        ("project.delete", &[("project", SdkValue::Int(1))]),
        (
            "project.hide_path",
            &[("project", t("shop/api")), ("path", SdkValue::Int(1))],
        ),
        ("store.compact", &[("store", SdkValue::Int(1))]),
    ];
    for &(name, fields) in wrong {
        let (code, message) = refused(&home, name, fields);
        assert_eq!(code, "InvalidInput", "{name}: {message}");
        assert!(message.starts_with(name), "{name}: {message}");
    }
    let unknown: &[(&str, &[(&str, SdkValue)])] = &[
        (
            "branch.remove",
            &[
                ("project", t("shop/api")),
                ("branch", t("main")),
                ("force", SdkValue::Bool(true)),
            ],
        ),
        (
            "project.delete",
            &[("project", t("shop/api")), ("force", SdkValue::Bool(true))],
        ),
        (
            "project.hide_path",
            &[
                ("project", t("shop/api")),
                ("path", t("/base.txt")),
                ("force", SdkValue::Bool(true)),
            ],
        ),
        (
            "store.compact",
            &[("store", t("shop")), ("force", SdkValue::Bool(true))],
        ),
    ];
    for &(name, fields) in unknown {
        let (code, message) = refused(&home, name, fields);
        assert_eq!(code, "InvalidInput", "{name}: {message}");
        assert!(
            message.contains("unknown argument `force`"),
            "{name}: {message}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn branch_remove_refuses_reserved_and_absent_branches() {
    let (home, folder) = project();
    let (code, message) = refused(&home, "branch.remove", &[p(), ("branch", t("main"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    // nothing ran: main is still there
    ok(&home, "branch.get", &[p(), ("branch", t("main"))]);
    let (code, message) = refused(&home, "branch.remove", &[p(), ("branch", t("ghost"))]);
    assert_eq!(code, "NotFound", "{message}");
    // a live Branch is hidden by the marker and reported as `removed`
    for name in ["work", "work2"] {
        ok(
            &home,
            "branch.fork",
            &[p(), ("name", t(name)), ("from", t("main"))],
        );
    }
    let removed = ok(&home, "branch.remove", &[p(), ("branch", t("work"))]);
    assert_eq!(removed, SdkValue::map([("ok", SdkValue::Bool(true))]));
    let removed = ok(
        &home,
        "branch.remove",
        &[
            p(),
            ("branch", t("work2")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(removed.get_text("outcome"), Some("removed"));
    assert_eq!(removed.get_text("branch"), Some("work2"));
    assert!(removed
        .get_text("removed_at")
        .is_some_and(|at| at.ends_with('Z')));
    let (code, message) = refused(&home, "branch.get", &[p(), ("branch", t("work"))]);
    assert_eq!(code, "NotFound", "{message}");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn project_hide_path_checks_its_path_grammar_then_reports_markers() {
    let (home, folder) = project();
    for bad in [
        "", "/", "//a", "a//b", "a/", "./a", "a/./b", ".", "..", "\\",
    ] {
        let (code, message) = refused(&home, "project.hide_path", &[p(), ("path", t(bad))]);
        assert_eq!(code, "InvalidInput", "{bad:?}: {message}");
    }
    // no effect: the file the bad calls named is still readable
    let read = ok(
        &home,
        "file.read",
        &[p(), ("branch", t("main")), ("path", t("/base.txt"))],
    );
    assert_eq!(read.get_text("text"), Some("hello\n"));

    let detail = ("detail", SdkValue::Bool(true));
    let removed = ok(
        &home,
        "project.hide_path",
        &[p(), ("path", t("/base.txt")), detail.clone()],
    );
    assert_eq!(removed.get_text("outcome"), Some("hidden"));
    assert_eq!(removed.get("unchanged"), Some(&SdkValue::Bool(false)));
    assert_eq!(removed.get_text("path"), Some("/base.txt"));
    assert!(removed.get_int("frontier_serial").is_some());
    assert!(removed
        .get_text("hidden_at")
        .is_some_and(|at| at.ends_with('Z')));
    // a repeat at the same frontier is unchanged; its short form says so
    let short = ok(&home, "project.hide_path", &[p(), ("path", t("/base.txt"))]);
    assert_eq!(short, SdkValue::map([("unchanged", SdkValue::Bool(true))]));
    let repeat = ok(
        &home,
        "project.hide_path",
        &[p(), ("path", t("/base.txt")), detail],
    );
    assert_eq!(repeat.get_text("outcome"), Some("unchanged"));
    assert_eq!(
        repeat.get_int("frontier_serial"),
        removed.get_int("frontier_serial")
    );
    // the marker hides it from reads
    let (code, message) = refused(
        &home,
        "file.read",
        &[p(), ("branch", t("main")), ("path", t("/base.txt"))],
    );
    assert_eq!(code, "NotFound", "{message}");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn project_delete_reports_removal_and_retires_the_entry() {
    let (home, folder) = project();
    let (code, message) = refused(&home, "project.delete", &[("project", t("shop/ghost"))]);
    assert_eq!(code, "NotFound", "{message}");
    let removed = ok(
        &home,
        "project.delete",
        &[p(), ("detail", SdkValue::Bool(true))],
    );
    assert_eq!(removed.get_text("outcome"), Some("deleted"));
    assert_eq!(removed.get("ok"), Some(&SdkValue::Bool(true)));
    assert!(removed
        .get_text("deleted_at")
        .is_some_and(|at| at.ends_with('Z')));
    assert_eq!(removed.get("home_entry_kept"), Some(&SdkValue::Bool(false)));
    let (code, _) = refused(&home, "project.get", &[p()]);
    assert_eq!(code, "NotFound");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// A project name a `project.delete` retired is still held: a second
/// `project.init` is `NameTaken` with the `deleted` holder, refused before
/// the folder is scanned.
#[test]
fn project_init_after_delete_is_nametaken_deleted() {
    let (home, folder) = project();
    ok(&home, "project.delete", &[p()]);
    let refusal = layerfs_api::call(
        Some(crate::root::root_of(&home)),
        "project.init",
        map(&[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal.code, "Exists", "{}", refusal.message);
    let holder = refusal.detail.get("holder").expect("holder detail");
    assert_eq!(holder.get_text("kind"), Some("project"), "{refusal:?}");
    assert_eq!(holder.get_text("name"), Some("api"));
    assert_eq!(holder.get_text("state"), Some("deleted"));
    assert_eq!(refusal.detail.get_text("source"), Some("store"));
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// `store.compact` sends each phase as a progress event before it answers:
/// `{ progress: { phase, done, total } }` with null counts.
#[test]
fn store_compact_sends_each_phase_as_a_progress_event() {
    let (home, folder) = project();
    let compact = || {
        let mut phases = Vec::new();
        let report = layerfs_api::call(
            Some(crate::root::root_of(&home)),
            "store.compact",
            map(&[("store", t("shop")), ("detail", SdkValue::Bool(true))]),
            &mut |event| {
                let progress = event.get("progress").expect("a progress event").clone();
                assert_eq!(progress.get("done"), Some(&SdkValue::Null), "{progress:?}");
                assert_eq!(progress.get("total"), Some(&SdkValue::Null), "{progress:?}");
                phases.push(progress.get_text("phase").expect("phase").to_owned());
            },
        )
        .unwrap_or_else(|refusal| panic!("store.compact: {refusal:?}"));
        (
            report.get_text("outcome").expect("outcome").to_owned(),
            phases,
        )
    };
    // nothing is dead yet: the run stops after its mark
    let (outcome, phases) = compact();
    assert_eq!(
        (outcome.as_str(), phases.as_slice()),
        ("unchanged", ["mark".to_owned()].as_slice())
    );
    // a deleted project makes the next run a rebuild, through all four phases
    ok(&home, "project.delete", &[p()]);
    let (outcome, phases) = compact();
    assert_eq!(outcome, "compacted");
    assert_eq!(phases, ["mark", "build", "verify", "install"]);
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// Registers a mount intent of Workspace `[7; 32]` on a new Branch
/// `sandbox` of `shop/api`, placed with Sandbox `sandbox` and owned by this
/// live process, as a daemon's mount registers one.
fn plant_mount(home: &Path, sandbox: &str) {
    use layerfs_history::{GcIntent, GcIntentKind, GcScopeKind};
    use layerfs_store_service::history::gc_admission;
    ok(home, "branch.fork", &[p(), ("name", t(sandbox))]);
    let got = ok(
        home,
        "branch.get",
        &[
            p(),
            ("branch", t(sandbox)),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    let id = got.get_text("id").expect("the Branch's id");
    let branch = layerfs_protocol::sdk::unhex(id).expect("hex");
    let record = layerfs_store::admin::record::store::StoreRecord::read(home, "shop")
        .unwrap()
        .unwrap();
    let args = layerfs_store::admin::open::args::OpenArgs::in_root(
        home,
        "shop",
        Some(record.to_record().clone()),
    );
    let (runtime, boot) = gc_admission::owner_tokens(&format!("ws:{sandbox}"));
    let intent = GcIntent::shared(
        gc_admission::op_id(),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch,
    )
    .named(Some("api"), Some(sandbox))
    .placed(
        Some([7; 32]),
        Some(sandbox),
        Some(layerfs_home::names::HOST_MACHINE),
    )
    .owned(Some(&runtime), Some(&boot));
    layerfs_store::admin::open::service::with_service(&args, |service| {
        let catalog = service.store.history();
        catalog
            .gc_register(&intent)
            .map_err(|error| layerfs_home::reply::outcome::failed(error.to_string()))
    })
    .unwrap();
}

/// A live-owned mount intent of a Sandbox this home no longer records (one
/// `sandbox remove` forgot without stopping its daemon) is not ended by
/// anything that cannot prove its owner gone (RV5 FAIL-1): `store compact`
/// and `branch remove` stay `InUse` (mounts active). The Store doctor reports
/// it as the `store.mount_holders` warning of the lean doctor, never as a
/// leftover it releases itself (V20-4).
#[test]
fn a_live_mount_intent_of_a_forgotten_sandbox_keeps_holding_the_store() {
    let (home, folder) = project();
    plant_mount(&home, "gone");
    let doctor = ok(&home, "store.doctor", &[("store", t("shop"))]);
    let reported = format!("{doctor:?}");
    assert!(reported.contains("store.mount_holders"), "{doctor:?}");
    assert!(!reported.contains("leftovers"), "{doctor:?}");
    for _ in 0..2 {
        let (code, message) = refused(&home, "store.compact", &[("store", t("shop"))]);
        assert_eq!(code, "InUse", "{message}");
    }
    let (code, message) = refused(&home, "branch.remove", &[p(), ("branch", t("gone"))]);
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("InUse", "mounts active: 1 holder(s)")
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// Records native Sandbox `sb1` of project `project`, started: a daemon
/// whose process has already ended (a child of this test), so nothing runs
/// a daemon, and its Workspace `[7; 32]` of Branch `sb1` mounted at
/// `/w/sb1`.
#[cfg(target_os = "macos")]
fn ended_native_sandbox(home: &Path, project: &str) {
    use layerfs_home::disk::{paths, write_record};
    use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let process = layerfs_os::process::LocalProcess::observe(child.id()).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    let id = layerfs_home::disk::opened(home).unwrap().id;
    let name = format!("{}sb1", layerfs_home::names::provider_prefix(&id));
    let run = paths::run_root(home).join(&name);
    std::fs::create_dir_all(&run).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "mac".to_owned(),
        name,
        container: "ab".repeat(32),
        runtime: "process",
        uid: layerfs_home::lock::flock::euid(),
        gid: layerfs_home::lock::flock::egid(),
        directory: run.display().to_string(),
        process: (
            process.pid,
            process.start,
            format!("{:032x}", process.boot.unwrap()),
        ),
        port: 9,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    });
    write_record(
        &paths::sandbox_record(home, "sb1"),
        &map(&[
            ("name", t("sb1")),
            ("project", t(project)),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("id", t("sb1")),
            ("runtime", t("process")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
    let workspace = "07".repeat(32);
    let records = paths::sandbox_dir(home, "sb1").join("workspace");
    std::fs::create_dir_all(&records).unwrap();
    WorkspaceRecord::mounted(workspace.clone(), None, "sb1", "/w/sb1", 1)
        .write(&records.join(format!("{workspace}.json")))
        .unwrap();
}

/// `sandbox.remove sb1 --force`'s `unmounted` and `left`.
#[cfg(target_os = "macos")]
fn force_removed(home: &Path) -> (Vec<String>, Vec<String>) {
    let mut sink = |_| {};
    let mut events = layerfs_home::reply::events::Events::new(&mut sink);
    let request = layerfs_sandbox::family::RemoveRequest {
        name: "sb1".to_owned(),
        force: true,
    };
    match layerfs_sandbox::family::remove(
        home,
        &mut events,
        &request,
        layerfs_workspace::route::dispatch::call,
    ) {
        Ok(layerfs_sandbox::family::Removed::Stopped { unmounted, left }) => (unmounted, left),
        Ok(_) => panic!("Sandbox sb1 was not stopped"),
        Err(refusal) => panic!("sandbox.remove: {refusal:?}"),
    }
}

/// A forced remove of a native Sandbox ends, once its runtime is stopped,
/// the mount intent each of its Workspaces held (RV5 FAIL-2): `left` is
/// empty, and `branch remove` and `store compact` are admitted after it.
#[cfg(target_os = "macos")]
#[test]
fn a_forced_native_remove_ends_its_workspaces_mount_intents() {
    let (home, folder) = project();
    plant_mount(&home, "sb1");
    let (code, message) = refused(&home, "store.compact", &[("store", t("shop"))]);
    assert_eq!(code, "InUse", "{message}");
    ended_native_sandbox(&home, "shop/api");
    let (unmounted, left) = force_removed(&home);
    assert_eq!(unmounted, ["sb1:/w/sb1"]);
    assert!(left.is_empty(), "{left:?}");
    ok(&home, "branch.remove", &[p(), ("branch", t("sb1"))]);
    ok(&home, "store.compact", &[("store", t("shop"))]);
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// A mount intent a forced native remove cannot end (here its Store cannot
/// be opened) is a sentence of `left` saying why, what it refuses and what
/// ends it on this computer, not an answer that claims it ended.
#[cfg(target_os = "macos")]
#[test]
fn a_mount_intent_a_forced_remove_cannot_end_is_left() {
    let (home, folder) = project();
    ended_native_sandbox(&home, "ghost/api");
    let (_, left) = force_removed(&home);
    assert_eq!(
        left,
        ["Workspace /w/sb1's mount intent was not ended (no Store named ghost): `layerfs branch \
          remove` of its Branch and `layerfs store compact` are refused while it remains; nothing \
          on this computer can prove its owner gone, so it stays"]
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// The Branch id [`handed_back`] names, set by its test before the close.
#[cfg(all(target_os = "macos", feature = "machine"))]
static HANDED_BRANCH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// A Sandbox's own side closing it on a machine that could not dial its
/// PostgreSQL Store's URL: its runtime is stopped and the one Workspace
/// mount intent it could not end goes back to the caller (`ending`: the
/// Workspace's kernel path and its bound Workspace and Branch ids).
#[cfg(all(target_os = "macos", feature = "machine"))]
fn handed_back(
    _host: layerfs_sandbox::route::machine::Host,
    _call: layerfs_sandbox::route::machine::Call,
) -> layerfs_home::reply::outcome::Outcome {
    let branch = HANDED_BRANCH.get().expect("the Branch id").clone();
    Ok(map(&[
        ("kind", t("Closed")),
        ("sandbox", t("sb1")),
        ("unknown", SdkValue::List(vec![t("/w/sb1")])),
        ("left", SdkValue::List(Vec::new())),
        (
            "ending",
            SdkValue::List(vec![map(&[
                ("path", t("/w/sb1")),
                ("workspace", t(&"07".repeat(32))),
                ("branch", t(&branch)),
            ])]),
        ),
    ]))
}

/// A close whose side ran without a PostgreSQL Store URL (its machine
/// cannot dial it, `route`) leaves the mount intents of the Workspaces it
/// drained to its caller, which runs PostgreSQL Store calls in its own
/// process: the caller ends each once with its own URL, so `branch remove`
/// and `store compact` are admitted after it and nothing is `left`.
/// `LAYERFS_TEST_POSTGRES_URL` names a server where a database of the
/// test's own is made and dropped.
#[cfg(all(target_os = "macos", feature = "machine"))]
#[test]
fn a_close_without_a_store_url_has_its_caller_end_the_mount_intents() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
        return;
    };
    let database = format!("gccalls_close_{}", std::process::id());
    let admin = |sql: &str| {
        let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        client.batch_execute(sql).unwrap();
    };
    admin(&format!("DROP DATABASE IF EXISTS {database} WITH (FORCE)"));
    admin(&format!("CREATE DATABASE {database}"));
    let _dropped = DroppedDatabase(url.clone(), database.clone());
    let config: postgres::Config = url.parse().unwrap();
    let user = config.get_user().unwrap_or("postgres").to_owned();
    let address = match config.get_hosts().first() {
        Some(postgres::config::Host::Tcp(host)) => host.clone(),
        other => panic!("a TCP test server: {other:?}"),
    };
    let port = config.get_ports().first().copied().unwrap_or(5432);
    let home = tempdir("pg-close");
    layerfs_api::home::open(&home).unwrap();
    let folder = tempdir("pg-close-folder");
    std::fs::write(folder.join("base.txt"), "hello\n").unwrap();
    // the URL's user becomes a saved login (a trust-auth server takes any
    // password); the address is connected without it
    let clean = format!("postgresql://{address}:{port}/{database}?sslmode=disable");
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&clean)),
            ("user", t(&user)),
            ("password", t("unused")),
        ],
    );
    ok(
        &home,
        "project.init",
        &[
            ("project", t("pg/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    let pg = ("project", t("pg/api"));
    ok(&home, "branch.fork", &[pg.clone(), ("name", t("sb1"))]);
    let got = ok(
        &home,
        "branch.get",
        &[
            pg.clone(),
            ("branch", t("sb1")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    let branch = got.get_text("id").expect("the Branch's id").to_owned();
    HANDED_BRANCH.set(branch.clone()).unwrap();
    // the mount intent Workspace [7; 32]'s daemon registered on Branch sb1
    {
        use layerfs_history::{GcIntent, GcIntentKind, GcScopeKind};
        use layerfs_store_service::history::gc_admission;
        let record = layerfs_store::admin::record::store::StoreRecord::read(&home, "pg")
            .unwrap()
            .unwrap();
        let args = layerfs_store::admin::open::args::OpenArgs::of_record(&home, "pg", &record)
            .unwrap()
            .with_project("api");
        let (runtime, boot) = gc_admission::owner_tokens("ws:sb1");
        let intent = GcIntent::shared(
            gc_admission::op_id(),
            GcIntentKind::Mount,
            GcScopeKind::Branch,
            layerfs_protocol::sdk::unhex(&branch).unwrap(),
        )
        .named(Some("api"), Some("sb1"))
        .placed(
            Some([7; 32]),
            Some("sb1"),
            Some(layerfs_home::names::HOST_MACHINE),
        )
        .owned(Some(&runtime), Some(&boot));
        layerfs_store::admin::open::service::with_service(&args, |service| {
            (service.store.history().gc_register(&intent))
                .map_err(|error| layerfs_home::reply::outcome::failed(error.to_string()))
        })
        .unwrap();
    }
    let (code, message) = refused(&home, "branch.remove", &[pg.clone(), ("branch", t("sb1"))]);
    assert_eq!(code, "InUse", "{message}");
    // Sandbox sb1 on native, its daemon recorded where the close runs
    let daemon = folder.join("layerfs-daemon");
    std::fs::write(&daemon, "").unwrap();
    ok(
        &home,
        "machine.update",
        &[
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("daemon", t(&daemon.display().to_string())),
        ],
    );
    layerfs_home::disk::write_record(
        &layerfs_home::disk::paths::sandbox_record(&home, "sb1"),
        &map(&[
            ("v", SdkValue::Int(1)),
            ("name", t("sb1")),
            ("project", t("pg/api")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("created", SdkValue::Int(1)),
            ("id", t("sb1")),
        ]),
    )
    .unwrap();
    let record = layerfs_sandbox::record::SandboxRecord::read(&home, "sb1")
        .unwrap()
        .unwrap();
    let mut sink = |_| {};
    let mut events = layerfs_home::reply::events::Events::new(&mut sink);
    let close = layerfs_sandbox::route::machine::Call::Close {
        force: true,
        held: None,
    };
    let answer = layerfs_sandbox::route::machine::send::on_machine(
        &home,
        &mut events,
        &record,
        close,
        handed_back,
    )
    .unwrap();
    ok(&home, "branch.remove", &[pg.clone(), ("branch", t("sb1"))]);
    ok(&home, "store.compact", &[("store", t("pg"))]);
    // the caller's answer: nothing left, and the handed-back field consumed
    assert_eq!(answer.get("ending"), None, "{answer:?}");
    assert_eq!(answer.get("left"), Some(&SdkValue::List(Vec::new())));
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

/// Drops database `.1` of the server `.0` names when the test ends, passed
/// or not.
#[cfg(all(target_os = "macos", feature = "machine"))]
struct DroppedDatabase(String, String);

#[cfg(all(target_os = "macos", feature = "machine"))]
impl Drop for DroppedDatabase {
    fn drop(&mut self) {
        if let Ok(mut client) = postgres::Client::connect(&self.0, postgres::NoTls) {
            let _ =
                client.batch_execute(&format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.1));
        }
    }
}

#[test]
fn store_compact_refuses_an_unknown_store_and_reports_a_run() {
    let home = tempdir("compact-none");
    layerfs_api::home::open(&home).unwrap();
    let (code, message) = refused(&home, "store.compact", &[("store", t("ghost"))]);
    assert_eq!(code, "NotFound", "{message}");
    std::fs::remove_dir_all(home).unwrap();

    let (home, folder) = project();
    // the short form: the bytes freed alone
    let short = ok(&home, "store.compact", &[("store", t("shop"))]);
    assert_eq!(
        layerfs_home::json::print(&short),
        r#"{"freed_bytes":0}"#,
        "nothing is dead yet"
    );
    let report = ok(
        &home,
        "store.compact",
        &[("store", t("shop")), ("detail", SdkValue::Bool(true))],
    );
    assert_eq!(report.get_text("store"), Some("shop"));
    let outcome = report.get_text("outcome").expect("outcome");
    assert!(matches!(outcome, "unchanged" | "compacted"), "{outcome}");
    let before = report.get_int("generation_before").unwrap();
    let after = report.get_int("generation_after").unwrap();
    if outcome == "unchanged" {
        assert_eq!(before, after);
    } else {
        assert_eq!(after, before + 1);
    }
    for field in [
        "freed_bytes",
        "dead_bytes",
        "discarded_bytes",
        "reusable_bytes",
        "peak_temp_bytes",
        "estimate",
        "zero_reasons",
        "kept_waste",
        "delta_base_references",
        "delta_base_references_count",
        "residue_bytes",
        "residue_artifacts",
    ] {
        assert!(report.get(field).is_some(), "missing {field}");
    }
    assert_eq!(report.get("residue_reason"), Some(&SdkValue::Null));
    let kept = report.get("kept_waste").expect("kept_waste");
    for field in [
        "dead_in_retained_packs_bytes",
        "delta_bases_bytes",
        "inode_blocks_bytes",
    ] {
        assert!(kept.get(field).is_some(), "missing kept_waste.{field}");
    }
    // the run settled: the Store's record keeps no operation
    let record =
        layerfs_api::home::read_record(&layerfs_api::home::paths::store_record(&home, "shop"))
            .unwrap()
            .unwrap();
    assert_eq!(record.get("op"), None, "{record:?}");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_bare_project_name_is_refused_by_every_project_call() {
    let home = tempdir("bare-names");
    layerfs_api::home::open(&home).unwrap();
    let bare: &[(&str, &[(&str, SdkValue)])] = &[
        (
            "branch.remove",
            &[("project", t("api")), ("branch", t("main"))],
        ),
        ("project.delete", &[("project", t("api"))]),
        (
            "project.hide_path",
            &[("project", t("api")), ("path", t("/base.txt"))],
        ),
    ];
    for &(name, fields) in bare {
        let (code, message) = refused(&home, name, fields);
        assert_eq!(code, "InvalidInput", "{name}: {message}");
        assert!(message.contains("with its Store"), "{name}: {message}");
    }
    // store.compact takes a Store name; a project path is not one
    let (code, message) = refused(&home, "store.compact", &[("store", t("shop/api"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("store name"), "{message}");
    std::fs::remove_dir_all(home).unwrap();
}

/// `store.repair_mount_holders` is the explicit two-step operator repair:
/// preview answers the typed holder view and writes nothing; release needs
/// a preceding settle; `settle_legacy` marks only rows with no owner or place
/// evidence; release retains the row as the durable done audit, and the audit
/// action reads it back in bounded pages.
#[test]
fn store_repair_mount_holders_is_an_explicit_two_step_operator_call() {
    let (home, folder) = project();
    let record = StoreRecord::read(&home, "shop").unwrap().unwrap();
    let open = OpenArgs::of_record(&home, "shop", &record).unwrap();
    // The fixture's main Branch is the Branch the project recorded, read from
    // the name index. `store.repair_mount_holders` is what must resolve the
    // name `main` through `branch.list` (spec 4.4: a listing names
    // the recorded Branch, not a null main id).
    let (stack, branch) = with_service(&open, |service| {
        let stack = lookup::project(&service.store, "api")?;
        let main = layerfs_history::HistoryName::new("main").expect("a valid Branch name");
        let branch = service
            .store
            .history()
            .branch_named(stack.id, &main)
            .map_err(lookup::history)?
            .expect("the project records its main Branch")
            .id;
        Ok((stack.id, branch))
    })
    .unwrap();
    let live = GcIntent::shared(
        [0x21; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("api"), Some("main"))
    .placed(Some([0x22; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:1"))
    .authorized(PlaceAuthority {
        machine_incarnation: Some("worker:1".to_owned()),
        sandbox_generation: Some("generation:1".to_owned()),
    });
    let legacy = GcIntent::shared(
        [0x31; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("api"), Some("main"));
    with_service(&open, |service| {
        service.store.history().gc_register(&live).unwrap();
        service.store.history().gc_register(&legacy).unwrap();
        Ok(())
    })
    .unwrap();

    let live_op = "21".repeat(16);
    let legacy_op = "31".repeat(16);
    let repair = |action: &str, op: Option<&str>, legacy: bool| {
        let mut fields = vec![
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("action", t(action)),
        ];
        if let Some(op) = op {
            fields.push(("op", t(op)));
        }
        if legacy {
            fields.push(("legacy", SdkValue::Bool(true)));
        }
        fields.push(("operator", t("alice")));
        // the long form: every key, a null `next_cursor` included
        fields.push(("detail", SdkValue::Bool(true)));
        fields
    };

    // Preview answers the typed view, settle is a separate step, and release
    // needs the settled state.
    let preview = ok(
        &home,
        "store.repair_mount_holders",
        &repair("preview", None, false),
    );
    let Some(SdkValue::List(holders)) = preview.get("items") else {
        panic!("preview holders: {preview:?}");
    };
    assert_eq!(holders.len(), 2, "{preview:?}");
    assert_eq!(preview.get_text("action"), Some("preview"));
    assert_eq!(preview.get("next_cursor"), Some(&SdkValue::Null));
    // Preview is read-only: the Branch's intents are as registered, and no
    // retained row was written.
    let intents = with_service(&open, |service| {
        (service.store.history())
            .gc_intents(GcScopeKind::Branch, branch.as_slice())
            .map_err(lookup::history)
    })
    .unwrap();
    assert_eq!(intents.len(), 2, "{intents:?}");
    assert!(
        intents.iter().all(|intent| !intent.uncertain),
        "{intents:?}"
    );
    let audit = ok(
        &home,
        "store.repair_mount_holders",
        &repair("audit", None, false),
    );
    assert!(
        matches!(audit.get("items"), Some(SdkValue::List(rows)) if rows.is_empty()),
        "preview wrote a row: {audit:?}"
    );
    let live_row = holders
        .iter()
        .find(|row| row.get_text("operation") == Some(live_op.as_str()))
        .expect("the live holder");
    assert_eq!(live_row.get_text("status"), Some("live"));
    assert_eq!(live_row.get_text("machine_incarnation"), Some("worker:1"));
    assert_eq!(
        live_row.get_text("sandbox_generation"),
        Some("generation:1")
    );
    assert_eq!(live_row.get("legacy"), Some(&SdkValue::Bool(false)));
    // nothing proves its owner gone: the row still looks live
    assert_eq!(live_row.get_text("liveness"), Some("live"));
    assert_eq!(live_row.get("safe"), Some(&SdkValue::Bool(false)));
    assert!(live_row.get("reason").is_none(), "state words, no prose");
    let legacy_row = holders
        .iter()
        .find(|row| row.get_text("operation") == Some(legacy_op.as_str()))
        .expect("the legacy holder");
    assert_eq!(legacy_row.get("legacy"), Some(&SdkValue::Bool(true)));

    let (code, message) = refused(
        &home,
        "store.repair_mount_holders",
        &repair("release", Some(&live_op), false),
    );
    assert_eq!(code, "Conflict", "{message}");
    // `legacy` may not touch a row with owner and place evidence.
    let (code, message) = refused(
        &home,
        "store.repair_mount_holders",
        &repair("settle", Some(&live_op), true),
    );
    assert_eq!(code, "InvalidInput", "{message}");

    // a holder that still looks live is settled only when forced
    let (code, message) = refused(
        &home,
        "store.repair_mount_holders",
        &repair("settle", Some(&live_op), false),
    );
    assert_eq!(code, "InUse", "{message}");
    let mut forced = repair("settle", Some(&live_op), false);
    forced.push(("force", SdkValue::Bool(true)));
    let settled = ok(&home, "store.repair_mount_holders", &forced);
    assert_eq!(settled.get_text("status"), Some("uncertain"), "{settled:?}");
    let preview = ok(
        &home,
        "store.repair_mount_holders",
        &repair("preview", None, false),
    );
    let Some(SdkValue::List(holders)) = preview.get("items") else {
        panic!("preview holders: {preview:?}");
    };
    let settled_row = holders
        .iter()
        .find(|row| row.get_text("operation") == Some(live_op.as_str()))
        .expect("the settled holder");
    assert_eq!(settled_row.get_text("status"), Some("uncertain"));
    ok(
        &home,
        "store.repair_mount_holders",
        &repair("release", Some(&live_op), false),
    );

    let settled = ok(
        &home,
        "store.repair_mount_holders",
        &repair("settle_legacy", None, false),
    );
    assert_eq!(settled.get_int("settled"), Some(1));
    ok(
        &home,
        "store.repair_mount_holders",
        &repair("release", Some(&legacy_op), false),
    );
    let preview = ok(
        &home,
        "store.repair_mount_holders",
        &repair("preview", None, false),
    );
    assert!(
        matches!(preview.get("items"), Some(SdkValue::List(rows)) if rows.is_empty()),
        "{preview:?}"
    );

    // Both releases retained their row as the durable operator audit, and
    // the three previews wrote none. Admission still filters every done row.
    let audit = ok(
        &home,
        "store.repair_mount_holders",
        &repair("audit", None, false),
    );
    let Some(SdkValue::List(rows)) = audit.get("items") else {
        panic!("audit items: {audit:?}");
    };
    let released: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.get_text("evidence")
                .is_some_and(|evidence| evidence.starts_with("action=release;"))
        })
        .collect();
    assert_eq!(released.len(), 2, "{audit:?}");
    assert_eq!(rows.len(), 2, "only the releases are retained: {audit:?}");
    let released = released
        .iter()
        .find(|row| row.get_text("operation") == Some(live_op.as_str()))
        .expect("the released live op");
    assert_eq!(released.get_text("kind"), Some("mount"));
    assert_eq!(released.get_text("project"), Some("api"));
    assert_eq!(released.get_text("branch"), Some("main"));
    assert_eq!(released.get_text("machine"), Some("box"));
    assert_eq!(released.get_text("sandbox"), Some("box"));
    let evidence = released.get_text("evidence").expect("evidence");
    assert!(
        evidence.contains(&format!("action=release;operator=alice;op={live_op}")),
        "{evidence}"
    );
    assert_eq!(audit.get("next_cursor"), Some(&SdkValue::Null), "{audit:?}");

    // Paging is by `limit` and `cursor`: one row a page, the cursor carries
    // the project, Branch and action, and another action's cursor is refused.
    let mut first = repair("audit", None, false);
    first.push(("limit", SdkValue::Int(1)));
    let first = ok(&home, "store.repair_mount_holders", &first);
    let Some(cursor) = first.get_text("next_cursor").map(str::to_owned) else {
        panic!("a first page of one names the next: {first:?}");
    };
    let mut second = repair("audit", None, false);
    second.push(("limit", SdkValue::Int(1)));
    second.push(("cursor", t(&cursor)));
    let second = ok(&home, "store.repair_mount_holders", &second);
    let op_of = |page: &SdkValue| match page.get("items") {
        Some(SdkValue::List(rows)) if rows.len() == 1 => {
            rows[0].get_text("operation").map(str::to_owned)
        }
        other => panic!("one row: {other:?}"),
    };
    assert_ne!(op_of(&first), op_of(&second), "the second page moves on");
    assert_eq!(
        second.get("next_cursor"),
        Some(&SdkValue::Null),
        "{second:?}"
    );
    let mut crossed = repair("preview", None, false);
    crossed.push(("cursor", t(&cursor)));
    let (code, message) = refused(&home, "store.repair_mount_holders", &crossed);
    assert_eq!(code, "InvalidInput", "{message}");
    let mut too_many = repair("audit", None, false);
    too_many.push(("limit", SdkValue::Int(i64::from(u32::MAX))));
    let (code, message) = refused(&home, "store.repair_mount_holders", &too_many);
    assert_eq!(code, "InvalidInput", "{message}");

    // A refused call changes nothing.
    let (code, message) = refused(
        &home,
        "store.repair_mount_holders",
        &repair("inspect", None, false),
    );
    assert_eq!(code, "InvalidInput", "{message}");
    let (code, message) = refused(
        &home,
        "store.repair_mount_holders",
        &repair("settle", None, false),
    );
    assert_eq!(code, "InvalidInput", "{message}");
    let unknown = "99".repeat(16);
    let (code, message) = refused(
        &home,
        "store.repair_mount_holders",
        &repair("release", Some(&unknown), false),
    );
    assert_eq!(code, "NotFound", "{message}");

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}
