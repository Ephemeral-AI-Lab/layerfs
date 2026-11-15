//! The call table and its dispatcher (#533 step 2): 76 calls in eleven
//! families, arguments checked before any effect, one test per family that
//! a valid call reaches its family function and a wrong one is refused.
use layerfs_api::{families, json, keys, size, Call, Kind};
use layerfs_bridge::control::SdkValue;
use std::path::Path;

fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    )
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// One call in a private temporary home, its events kept.
fn run(name: &str, args: SdkValue) -> (Result<SdkValue, (String, String)>, Vec<SdkValue>) {
    let home = tempdir();
    let mut events = Vec::new();
    let outcome = layerfs_api::call(Some(&home), name, args, &mut |event| events.push(event));
    let _ = std::fs::remove_dir_all(&home);
    (outcome.map_err(|r| (r.code, r.message)), events)
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-calls-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn code(name: &str, args: SdkValue) -> String {
    match run(name, args).0 {
        Ok(value) => format!("ok {}", json::print(&value)),
        Err((code, message)) => format!("{code}: {message}"),
    }
}

#[test]
fn the_table_has_eleven_families_and_76_calls() {
    let all: Vec<&Call> = layerfs_api::calls();
    assert_eq!(all.len(), 76);
    // each family holds only its own calls, and has at least one
    for family in families() {
        assert!(!family.calls().is_empty(), "{}", family.name);
        assert!(
            family.calls().iter().all(|c| c.family == family.name),
            "{}",
            family.name
        );
    }
    let mut names: Vec<_> = all.iter().map(|c| c.name()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 76, "call names are unique");
    // an optional positional is only ever followed by optional ones (D7)
    for c in &all {
        let first_optional = c.args.iter().position(|a| a.optional);
        if let Some(i) = first_optional {
            assert!(c.args[i..].iter().all(|a| a.optional), "{}", c.name());
        }
    }
    let counts: Vec<usize> = families().iter().map(|f| f.calls().len()).collect();
    assert_eq!(counts, [6, 8, 3, 15, 4, 6, 12, 12, 5, 4, 1]);
}

#[test]
fn unknown_calls_and_arguments_are_refused_before_any_effect() {
    assert!(code("fs.read", SdkValue::Null).starts_with("InvalidInput: no call named `fs.read`"));
    assert!(code(
        "project.get",
        map(&[("project", t("shop/api")), ("bogus", t("x"))])
    )
    .starts_with("InvalidInput: project.get: unknown argument `bogus`"));
    // a function option never crosses into the library
    assert!(code(
        "store.push",
        map(&[
            ("name", t("s")),
            ("machine", t("m")),
            ("on_progress", t("f"))
        ])
    )
    .starts_with("InvalidInput"));
}

#[test]
fn project_family() {
    assert!(code("project.get", map(&[("project", t("api"))]))
        .starts_with("InvalidInput: project `api` must be written with its Store"));
    assert!(
        code("project.init", map(&[("project", t("shop/api"))])).contains("`folder` is required")
    );
    assert!(code("project.export", map(&[("project", t("shop/api"))]))
        .contains("`destination` is required"));
    assert!(code(
        "project.export",
        map(&[
            ("project", t("shop/api")),
            ("destination", t("/tmp/out")),
            ("commit", t("00"))
        ])
    )
    .starts_with("InvalidInput: project.export: unknown argument `commit`"));
    assert!(code("project.get", map(&[("project", t("shop/API"))])).starts_with("InvalidInput"));
    let reached = code("project.get", map(&[("project", t("shop/api"))]));
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn branch_family() {
    assert!(code(
        "branch.fork",
        map(&[("project", t("shop/api")), ("name", t("fix"))])
    )
    .contains("`from_branch` is required"));
    let reached = code(
        "branch.fork",
        map(&[
            ("project", t("shop/api")),
            ("name", t("fix")),
            ("from_branch", t("main")),
        ]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn branch_publish_declares_its_expected_base_options() {
    // The shell, Node and Python front ends build their arguments from this
    // table, so these options (and their shapes) are the CLI's and SDK's.
    let call = layerfs_api::calls()
        .into_iter()
        .find(|call| call.family == "branch" && call.call == "publish")
        .expect("branch.publish is in the table");
    let declared: Vec<(&str, Kind, bool)> = call
        .options
        .iter()
        .map(|option| (option.name, option.kind, option.optional))
        .collect();
    assert_eq!(
        declared,
        vec![
            ("expected_base", Kind::Name, true),
            ("expected_head", Kind::Name, true),
            ("force", Kind::Bool, true),
        ]
    );
    assert_eq!(
        call.help,
        "a Branch's head or Commit as main's next Layer, refusing a stale base"
    );
}

#[test]
fn branch_publish_expectations_are_plumbed() {
    for (name, value, why) in [
        (
            "expected_base",
            SdkValue::Int(1),
            "`expected_base` must be text",
        ),
        (
            "expected_base",
            SdkValue::Bool(true),
            "`expected_base` must be text",
        ),
        (
            "expected_head",
            SdkValue::Bool(true),
            "`expected_head` must be text",
        ),
        (
            "expected_head",
            SdkValue::Int(1),
            "`expected_head` must be text",
        ),
        ("force", t("yes"), "`force` must be true or false"),
        ("force", SdkValue::Int(1), "`force` must be true or false"),
    ] {
        let refused = code(
            "branch.publish",
            map(&[
                ("project", t("shop/api")),
                ("branch", t("work")),
                (name, value),
            ]),
        );
        assert!(refused.contains(why), "{name}: {refused}");
    }
    // A misspelled option is not read as an argument of the call.
    let typo = code(
        "branch.publish",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("work")),
            ("expectedBase", t("00")),
        ]),
    );
    assert!(
        typo.starts_with("InvalidInput: branch.publish: unknown argument `expectedBase`"),
        "{typo}"
    );
    // A well-shaped expectation reaches the call instead of being refused by
    // argument checking.
    let reached = code(
        "branch.publish",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("work")),
            ("expected_base", t("00")),
            ("expected_head", t("00")),
            ("force", SdkValue::Bool(true)),
        ]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn commit_family() {
    assert!(code(
        "commit.list",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("page_size", t("9"))
        ])
    )
    .contains("`page_size` must be a whole number"));
    let reached = code(
        "commit.list",
        map(&[("project", t("shop/api")), ("branch", t("main"))]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn file_family() {
    assert!(code(
        "file.write",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a"))
        ])
    )
    .contains("`content` is required"));
    assert!(code(
        "file.write",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a")),
            ("content", SdkValue::Int(1))
        ])
    )
    .contains("`content` must be text or bytes"));
    // a change names its paths as the table does, so a folder scope sees them
    let moved = SdkValue::List(vec![map(&[
        ("op", t("mv")),
        ("from", t("/a")),
        ("to", t("/b")),
    ])]);
    assert!(code(
        "file.apply",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("changes", moved)
        ])
    )
    .contains("`from` is not a change field"));
    let reached = code(
        "file.read",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a")),
        ]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn sandbox_family() {
    assert!(
        code("sandbox.create", map(&[("project", t("shop/api"))])).contains("`name` is required")
    );
    // `machine` is optional (#536 §10): the create reaches the library
    let reached = code(
        "sandbox.create",
        map(&[("project", t("shop/api")), ("name", t("tests"))]),
    );
    assert!(!reached.contains("`machine`"), "{reached}");
    let reached = code(
        "sandbox.close",
        map(&[("name", t("tests")), ("force", SdkValue::Bool(true))]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn workspace_family() {
    assert!(
        code("workspace.exec", map(&[("ws", t("tests:/workspaces/1"))]))
            .contains("`command` is required")
    );
    let reached = code("workspace.status", map(&[("ws", t("tests:/workspaces/1"))]));
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
    // a Workspace by its name (#536 C3)
    let reached = code("workspace.status", map(&[("ws", t("tests:dev"))]));
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
    let mount = layerfs_api::find("workspace.mount").unwrap();
    let options: Vec<_> = mount.options.iter().map(|o| (o.name, o.kind)).collect();
    assert_eq!(options, [("name", Kind::Name)]);
    // exec_cancel (layerfs-project#113): the Workspace, the exec id and a grace
    let cancel = layerfs_api::find("workspace.exec_cancel").unwrap();
    let args: Vec<_> = cancel.args.iter().map(|a| (a.name, a.kind)).collect();
    assert_eq!(args, [("ws", Kind::Text), ("exec_id", Kind::Text)]);
    let options: Vec<_> = cancel.options.iter().map(|o| (o.name, o.kind)).collect();
    assert_eq!(options, [("grace_ms", Kind::Int)]);
    let exec = layerfs_api::find("workspace.exec").unwrap();
    assert!(exec.options.iter().any(|o| o.name == "exec_id"));
    assert!(code(
        "workspace.exec_cancel",
        map(&[("ws", t("tests:/workspaces/1"))])
    )
    .contains("`exec_id` is required"));
}

#[test]
fn machine_family() {
    assert!(code(
        "machine.create",
        map(&[
            ("name", t("dev")),
            ("platform", t("lima")),
            ("memory", t("8Q"))
        ])
    )
    .contains("`memory` is a size"));
    let reached = code("machine.list", SdkValue::Null);
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn store_family() {
    assert!(code("store.push", map(&[("name", t("shop"))])).contains("`machine` is required"));
    let reached = code("store.list", SdkValue::Null);
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn backup_family() {
    assert!(code("backup.create", map(&[("store", t("shop"))])).contains("`bucket` is required"));
    let reached = code("backup.list", SdkValue::Null);
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn json_round_trips_every_value() {
    let value = map(&[
        ("a", SdkValue::Int(-12)),
        ("b", t("x\"y\\z\n\u{1}é😀")),
        (
            "c",
            SdkValue::List(vec![
                SdkValue::Null,
                SdkValue::Bool(true),
                SdkValue::Bool(false),
            ]),
        ),
        ("d", SdkValue::Bytes(vec![0xff, 0x00, 0x10])),
        ("e", SdkValue::Bytes(b"plain".to_vec())),
    ]);
    let text = json::print(&value);
    let back = json::parse(&text).unwrap();
    let expect = map(&[
        ("a", SdkValue::Int(-12)),
        ("b", t("x\"y\\z\n\u{1}é😀")),
        (
            "c",
            SdkValue::List(vec![
                SdkValue::Null,
                SdkValue::Bool(true),
                SdkValue::Bool(false),
            ]),
        ),
        ("d", SdkValue::Bytes(vec![0xff, 0x00, 0x10])),
        ("e", t("plain")),
    ]);
    assert_eq!(back, expect);
    assert_eq!(json::parse("\"\\ud83d\\ude00\"").unwrap(), t("😀"));
    for bad in [
        "1.5",
        "{\"a\":1,\"a\":2}",
        "[1,",
        "nul",
        "\"\\ud83d\"",
        "1 2",
    ] {
        assert!(json::parse(bad).is_err(), "{bad}");
    }
    assert_eq!(
        json::unbase64(&json::base64(b"any carnal pleas")).unwrap(),
        b"any carnal pleas"
    );
}

#[test]
fn sizes_and_keys() {
    assert_eq!(size("512"), Some(512));
    assert_eq!(size("64K"), Some(64 << 10));
    assert_eq!(size("8G"), Some(8 << 30));
    assert_eq!(size("8T"), None);
    assert_eq!(keys::snake_name("exitCode"), "exit_code");
    assert_eq!(keys::snake_name("nextPagePos"), "next_page_pos");
    assert_eq!(keys::snake_name("/src/README.md"), "/src/README.md");
    assert_eq!(keys::snake_name("Makefile"), "Makefile");
    assert_eq!(keys::camel_name("start_line"), "startLine");
    assert!(layerfs_api::calls()
        .into_iter()
        .flat_map(|c| c.args.iter().chain(c.options))
        .all(|a| a.name == a.name.to_lowercase()));
    assert!(layerfs_api::calls()
        .into_iter()
        .flat_map(|c| c.options)
        .any(|a| a.kind == Kind::Function));
}

#[test]
fn doctor_family() {
    assert!(code("doctor.fix", SdkValue::Null).contains("`id` is required"));
    assert!(code(
        "doctor.fix",
        map(&[("id", t("native:image-part:00")), ("path", t("/etc"))])
    )
    .starts_with("InvalidInput: doctor.fix: unknown argument `path`"));
    // reached: an id that names nothing is not found, and one of a kind
    // that is only counted is refused
    let reached = code("doctor.fix", map(&[("id", t("native:run-folder:../x"))]));
    assert!(reached.starts_with("NotFound: "), "{reached}");
    let lock = format!("native:image-lock:{}.2", "ab".repeat(32));
    let reached = code("doctor.fix", map(&[("id", t(&lock))]));
    assert!(reached.starts_with("InvalidInput: "), "{reached}");
    assert!(reached.contains("only reported"), "{reached}");
}

#[test]
fn an_explicit_home_must_be_absolute() {
    let mut events = Vec::new();
    let refused = layerfs_api::call(
        Some(Path::new("relative")),
        "machine.list",
        SdkValue::Null,
        &mut |e| events.push(e),
    )
    .unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
}

#[test]
fn the_source_is_one_file_per_call_under_its_family() {
    // src/ holds lib.rs, the call machinery (calls/, layerfs-project#107)
    // and one folder per family; each family folder holds its mod.rs and
    // one <call>.rs per call, nothing else
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let names = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    };
    let mut expected: Vec<String> = families().iter().map(|f| f.name.to_owned()).collect();
    expected.push("lib.rs".to_owned());
    expected.push("calls".to_owned());
    expected.sort();
    assert_eq!(names(&src), expected);
    for family in families() {
        let mut files: Vec<String> = family
            .calls()
            .iter()
            .map(|c| format!("{}.rs", c.call))
            .collect();
        files.push("mod.rs".to_owned());
        if family.name == "doctor" {
            // the report itself (explicit options and its run) stays
            // outside the table and the home dispatcher, beside the family's
            // one call
            files.extend(["options.rs".to_owned(), "run.rs".to_owned()]);
        }
        files.sort();
        assert_eq!(names(&src.join(family.name)), files, "{}", family.name);
    }
}

#[cfg(all(feature = "machine", target_os = "macos"))]
#[test]
fn native_machine_images_reports_host_userland_without_a_bundle() {
    let (outcome, events) = run("machine.images", map(&[("name", t("native"))]));
    let result = outcome.unwrap();
    assert_eq!(result.get_text("mode"), Some("host"));
    assert_eq!(result.get("default"), Some(&SdkValue::Null));
    assert_eq!(result.get("images"), Some(&SdkValue::List(vec![])));
    assert_eq!(result.get("docker"), Some(&SdkValue::Null));
    assert_eq!(result.get("available"), Some(&SdkValue::List(vec![])));
    assert!(events.is_empty());
    // no image on macOS native: a load or prune is refused, naming why
    for (call, args) in [
        (
            "machine.load",
            map(&[("name", t("native")), ("image", t("alpine:3"))]),
        ),
        (
            "machine.prune",
            map(&[("name", t("native")), ("unused", SdkValue::Bool(true))]),
        ),
    ] {
        let home = tempdir();
        let error = layerfs_api::call(Some(&home), call, args, &mut |_| {}).unwrap_err();
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(error.code, "Unsupported", "{call}");
        assert_eq!(error.detail.get_text("reason"), Some("host"), "{call}");
    }
}

#[test]
fn sandbox_images_are_named_by_the_image_kind() {
    let create = layerfs_api::find("sandbox.create").unwrap();
    let image = create.options.iter().find(|a| a.name == "image").unwrap();
    assert_eq!(image.kind, Kind::Image);
    assert!(create.options.iter().all(|a| a.name != "image_archive"));
    let load = layerfs_api::find("machine.load").unwrap();
    assert_eq!(load.args[1].kind, Kind::Image);
    assert!(load.options.iter().any(|a| a.name == "on_progress"));
    let prune = layerfs_api::find("machine.prune").unwrap();
    assert!(prune.args[1].optional);
    assert_eq!(prune.options[0].name, "unused");
}
