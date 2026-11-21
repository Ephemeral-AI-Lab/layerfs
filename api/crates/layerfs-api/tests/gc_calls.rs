//! T11.1: the four GC calls in the API call table — their ids-only arguments,
//! typed refusals before any effect, and the result shapes of one reachable
//! refusal per call plus the three removal results and the §11 report.
use layerfs_api::Kind;
use layerfs_bridge::control::SdkValue;
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
    layerfs_api::call(Some(home), name, map(fields), &mut |_| {})
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
        Some(&home),
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
            Some(&home),
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
    use layerfs_store_service::history::admission;
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
    let branch = layerfs_bridge::sdk_values::unhex(id).expect("hex");
    let record = layerfs_store::record::StoreRecord::read(home, "shop")
        .unwrap()
        .unwrap();
    let args =
        layerfs_store::open_args::OpenArgs::in_root(home, "shop", Some(record.to_record().clone()));
    let (runtime, boot) = admission::owner_tokens(&format!("ws:{sandbox}"));
    let intent = GcIntent::shared(
        admission::op_id(),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch,
    )
    .named(Some("api"), Some(sandbox))
    .placed(Some([7; 32]), Some(sandbox), Some("native"))
    .owned(Some(&runtime), Some(&boot));
    layerfs_store::service::with_service(&args, |service| {
        let catalog = service.store.history();
        catalog
            .gc_register(&intent)
            .map_err(|error| layerfs_home::outcome::failed(error.to_string()))
    })
    .unwrap();
}

/// A live-owned mount intent of a Sandbox this home no longer records (one
/// `sandbox remove` forgot without stopping its daemon) is not ended by
/// anything that cannot prove its owner gone (RV5 FAIL-1): `store compact`
/// and `branch remove` stay `InUse`. Doctor identifies the blocker and
/// suggests recovery without releasing the admission.
#[test]
fn a_live_mount_intent_of_a_forgotten_sandbox_keeps_holding_the_store() {
    let (home, folder) = project();
    plant_mount(&home, "gone");
    let doctor = ok(&home, "store.doctor", &[("store", t("shop"))]);
    let Some(SdkValue::List(findings)) = doctor.get("findings") else {
        panic!("missing findings: {doctor:?}");
    };
    let holder = findings
        .iter()
        .find(|finding| finding.get_text("check") == Some("store.mount_holders"))
        .expect("historical mount holder finding");
    assert_eq!(holder.get_text("severity"), Some("warning"));
    assert!(
        holder.get_text("message").unwrap().contains("gone"),
        "{holder:?}"
    );
    assert!(
        holder.get_text("fix").unwrap().contains("compact"),
        "{holder:?}"
    );
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
    use layerfs_home::home::{paths, write_record};
    use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let process = layerfs_bridge::process::LocalProcess::observe(child.id()).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    let id = layerfs_home::home::opened(home).unwrap().id;
    let name = format!("{}sb1", layerfs_home::names::provider_prefix(&id));
    let run = paths::run_root(home).join(&name);
    std::fs::create_dir_all(&run).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "mac".to_owned(),
        name,
        container: "ab".repeat(32),
        runtime: "native",
        uid: layerfs_home::home::flock::euid(),
        gid: layerfs_home::home::flock::egid(),
        directory: run.display().to_string(),
        process: (
            process.pid,
            process.start,
            format!("{:032x}", process.boot.unwrap()),
        ),
        port: 9,
        image: "local".to_owned(),
        labels: Vec::new(),
    });
    write_record(
        &paths::sandbox_record(home, "sb1"),
        &map(&[
            ("name", t("sb1")),
            ("project", t(project)),
            ("machine", t("native")),
            ("id", t("sb1")),
            ("runtime", t("native")),
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

/// Missing control keys say nothing about the runtime or its Store holders.
#[cfg(target_os = "macos")]
#[test]
fn reclaim_missing_keys_preserves_the_remove_retry() {
    let (home, folder) = project();
    plant_mount(&home, "sb1");
    ended_native_sandbox(&home, "shop/api");
    let dir = layerfs_home::home::paths::sandbox_dir(&home, "sb1");
    // Reopen probes the recorded process identity. It never starts a daemon.
    layerfs_machine::record::MachineRecord::from_record(
        "native",
        map(&[("daemon", t("/usr/bin/true"))]),
    )
    .write(&home)
    .unwrap();
    let args = [("sandbox", t("sb1")), ("detail", SdkValue::Bool(true))];
    let (_, message) = refused(&home, "sandbox.remove", &args);
    assert!(message.contains("daemon keys"), "{message}");
    assert!(dir.join("record.json").is_file());
    assert!(dir
        .join("workspace")
        .join(format!("{}.json", "07".repeat(32)))
        .is_file());
    assert_eq!(
        refused(&home, "store.compact", &[("store", t("shop"))]).0,
        "InUse"
    );
    layerfs_home::secrets::save_daemon_keys(&dir, &map(&[])).unwrap();
    let removed = ok(&home, "sandbox.remove", &args);
    assert_eq!(removed.get_text("outcome"), Some("already_gone"));
    assert!(!dir.exists());
    ok(&home, "project.delete", &[p()]);
    ok(&home, "store.compact", &[("store", t("shop"))]);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

/// Reclaim runs on the runtime's machine, including a Docker worker. A failed
/// branch lookup must leave the binding available to the next attempt.
#[test]
fn reclaim_on_runtime_host_preserves_failed_binding_then_releases_holder() {
    use layerfs_home::home::{paths, write_record};
    use layerfs_sandbox::host::{Host, Message};
    use layerfs_sandbox::record::WorkspaceRecord;
    let (home, folder) = project();
    plant_mount(&home, "sb1");
    let dir = paths::sandbox_dir(&home, "sb1");
    let daemon = map(&[("container", t("runtime-id"))]);
    write_record(&dir.join("record.json"), &map(&[("daemon", daemon)])).unwrap();
    let id = "07".repeat(32);
    let ws = dir.join("workspace").join(format!("{id}.json"));
    let bind = |branch| {
        WorkspaceRecord::mounted(id.clone(), None, branch, "/w/sb1", 1)
            .write(&ws)
            .unwrap()
    };
    bind("missing");
    let record = layerfs_store::record::StoreRecord::read(&home, "shop")
        .unwrap()
        .unwrap();
    let open = layerfs_store::open_args::OpenArgs::of_record(&home, "shop", &record).unwrap();
    let host = Host {
        home: home.clone(),
        store: "shop".into(),
        project: "api".into(),
        sandbox: Some("sb1".into()),
        store_record: Some(record),
        store_path: open.store_path,
        deploy: None,
        postgresql: None,
        message: Message::Absent,
        started: None,
    };
    let reclaim = || layerfs_sandbox::lifecycle::reclaim(&host, "runtime-id", false, &[], &[]);
    let failure = reclaim().unwrap_err();
    assert!(failure.message.contains("missing"), "{failure:?}");
    assert!(dir.join("record.json").is_file());
    assert!(ws.is_file());
    assert_eq!(
        refused(&home, "store.compact", &[("store", t("shop"))]).0,
        "InUse"
    );
    std::fs::remove_file(&ws).unwrap();
    let held = [String::from("/w/sb1")];
    let retry = |bindings: &[(String, String, String)]| {
        layerfs_sandbox::lifecycle::reclaim(&host, "runtime-id", false, &held, bindings)
    };
    assert!(retry(&[])
        .unwrap_err()
        .message
        .contains("missing Workspace binding"));
    assert!(dir.join("record.json").is_file());
    let forwarded = [(held[0].clone(), id.clone(), String::from("sb1"))];
    assert_eq!(
        retry(&forwarded).unwrap().get_text("kind"),
        Some("Reclaimed")
    );
    assert!(!dir.exists());
    ok(&home, "project.delete", &[p()]);
    ok(&home, "store.compact", &[("store", t("shop"))]);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

/// `sandbox.remove sb1 --force`'s `unmounted` and `left`.
#[cfg(target_os = "macos")]
fn force_removed(home: &Path) -> (Vec<String>, Vec<String>) {
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let request = layerfs_sandbox::family::RemoveRequest {
        name: "sb1".to_owned(),
        force: true,
    };
    match layerfs_sandbox::family::remove(home, &mut events, &request) {
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
    let dir = layerfs_home::home::paths::sandbox_dir(&home, "sb1");
    assert!(dir.join("record.json").is_file());
    assert!(dir
        .join("workspace")
        .join(format!("{}.json", "07".repeat(32)))
        .is_file());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
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
        layerfs_api::json::print(&short),
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
