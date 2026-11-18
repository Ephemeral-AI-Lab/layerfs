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
            "project.remove_file",
            &[("project", Kind::Project), ("path", Kind::Text)],
        ),
        ("store.compact", &[("name", Kind::Name)]),
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
            "project.remove_file",
            &[("project", t("shop/api")), ("path", SdkValue::Int(1))],
        ),
        ("store.compact", &[("name", SdkValue::Int(1))]),
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
            "project.remove_file",
            &[
                ("project", t("shop/api")),
                ("path", t("/base.txt")),
                ("force", SdkValue::Bool(true)),
            ],
        ),
        (
            "store.compact",
            &[("name", t("shop")), ("force", SdkValue::Bool(true))],
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
    // a live Branch is hidden by the marker and reported as `Removed`
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("work")), ("from_branch", t("main"))],
    );
    let removed = ok(&home, "branch.remove", &[p(), ("branch", t("work"))]);
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    // the project is the caller's <store>/<project>, not the Store's inner name
    assert_eq!(removed.get_text("project"), Some("shop/api"));
    assert_eq!(removed.get_text("branch"), Some("work"));
    assert!(removed.get_int("removed_ms").is_some_and(|ms| ms >= 0));
    let (code, message) = refused(&home, "branch.get", &[p(), ("branch", t("work"))]);
    assert_eq!(code, "NotFound", "{message}");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn project_remove_file_checks_its_path_grammar_then_reports_markers() {
    let (home, folder) = project();
    for bad in [
        "", "/", "//a", "a//b", "a/", "./a", "a/./b", ".", "..", "\\",
    ] {
        let (code, message) = refused(&home, "project.remove_file", &[p(), ("path", t(bad))]);
        assert_eq!(code, "InvalidInput", "{bad:?}: {message}");
    }
    // no effect: the file the bad calls named is still readable
    let read = ok(
        &home,
        "file.read",
        &[p(), ("branch", t("main")), ("path", t("/base.txt"))],
    );
    assert_eq!(read.get_text("text"), Some("hello\n"));

    let removed = ok(
        &home,
        "project.remove_file",
        &[p(), ("path", t("/base.txt"))],
    );
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    // the same full project name the caller passed
    assert_eq!(removed.get_text("project"), Some("shop/api"));
    assert!(removed.get_int("frontier_serial").is_some());
    assert!(removed.get_int("removed_ms").is_some_and(|ms| ms >= 0));
    // a repeat at the same frontier is UpToDate
    let repeat = ok(
        &home,
        "project.remove_file",
        &[p(), ("path", t("/base.txt"))],
    );
    assert_eq!(repeat.get_text("kind"), Some("UpToDate"));
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
    let removed = ok(&home, "project.delete", &[p()]);
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    assert_eq!(removed.get_text("project"), Some("shop/api"));
    assert!(removed.get_int("deleted_ms").is_some_and(|ms| ms >= 0));
    assert!(removed.get_bool("home_entry_removed"));
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
            map(&[("name", t("shop"))]),
            &mut |event| {
                let progress = event.get("progress").expect("a progress event").clone();
                assert_eq!(progress.get("done"), Some(&SdkValue::Null), "{progress:?}");
                assert_eq!(progress.get("total"), Some(&SdkValue::Null), "{progress:?}");
                phases.push(progress.get_text("phase").expect("phase").to_owned());
            },
        )
        .unwrap_or_else(|refusal| panic!("store.compact: {refusal:?}"));
        (report.get_text("path").expect("path").to_owned(), phases)
    };
    // nothing is dead yet: the run stops after its mark
    let (path, phases) = compact();
    assert_eq!(
        (path.as_str(), phases.as_slice()),
        ("none", ["mark".to_owned()].as_slice())
    );
    // a deleted project makes the next run a rebuild, through all four phases
    ok(&home, "project.delete", &[p()]);
    let (path, phases) = compact();
    assert_eq!(path, "rebuild");
    assert_eq!(phases, ["mark", "build", "verify", "install"]);
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn store_compact_refuses_an_unknown_store_and_reports_a_run() {
    let home = tempdir("compact-none");
    layerfs_api::home::open(&home).unwrap();
    let (code, message) = refused(&home, "store.compact", &[("name", t("ghost"))]);
    assert_eq!(code, "NotFound", "{message}");
    std::fs::remove_dir_all(home).unwrap();

    let (home, folder) = project();
    let report = ok(&home, "store.compact", &[("name", t("shop"))]);
    assert_eq!(report.get_text("kind"), Some("Compacted"));
    let path = report.get_text("path").expect("path");
    assert!(matches!(path, "none" | "rebuild"), "{path}");
    let generation = report.get("generation").expect("generation");
    let before = generation.get_int("before").unwrap();
    let after = generation.get_int("after").unwrap();
    if path == "none" {
        assert_eq!(before, after);
    } else {
        assert_eq!(after, before + 1);
    }
    for field in [
        "dead_bytes_bound",
        "dead_bytes_refined",
        "logical_discarded",
        "backend_reusable",
        "physical_shrink",
        "peak_temp_bytes",
        "estimate",
        "zero_reasons",
        "kept_waste",
        "residue",
    ] {
        assert!(report.get(field).is_some(), "missing {field}");
    }
    let kept = report.get("kept_waste").expect("kept_waste");
    for field in [
        "dead_in_retained_packs_bytes",
        "delta_bases",
        "inode_blocks_bytes",
    ] {
        assert!(kept.get(field).is_some(), "missing kept_waste.{field}");
    }
    let residue = report.get("residue").expect("residue");
    assert!(residue.get("bytes").is_some());
    assert!(residue.get("artifacts").is_some());
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
            "project.remove_file",
            &[("project", t("api")), ("path", t("/base.txt"))],
        ),
    ];
    for &(name, fields) in bare {
        let (code, message) = refused(&home, name, fields);
        assert_eq!(code, "InvalidInput", "{name}: {message}");
        assert!(message.contains("with its Store"), "{name}: {message}");
    }
    // store.compact takes a Store name; a project path is not one
    let (code, message) = refused(&home, "store.compact", &[("name", t("shop/api"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("store name"), "{message}");
    std::fs::remove_dir_all(home).unwrap();
}
