//! The call table and its dispatcher (#533 step 2): 83 calls in ten
//! families (the call surface 2.0), arguments checked before any effect, one test per family that
//! a valid call reaches its family function and a wrong one is refused.
use layerfs_api::{families, json, keys, size, Call, Kind};
use layerfs_protocol::sdk::SdkValue;
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
fn each_family_holds_its_own_calls_and_names_are_unique() {
    // which calls exist is the contract test's to say (`tests/contract.rs`
    // compares the table with the frozen spec); no count is pinned here
    // while the families move to the 2.0 call surface one package at a time
    let all: Vec<&Call> = layerfs_api::calls();
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
    assert_eq!(names.len(), all.len(), "call names are unique");
    // an optional positional is only ever followed by optional ones (D7)
    for c in &all {
        let first_optional = c.args.iter().position(|a| a.optional);
        if let Some(i) = first_optional {
            assert!(c.args[i..].iter().all(|a| a.optional), "{}", c.name());
        }
    }
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
        "store.move",
        map(&[
            ("store", t("s")),
            ("machine", t("m")),
            ("on_progress", t("f"))
        ])
    )
    .starts_with("InvalidInput"));
}

#[test]
fn project_remove_is_retired() {
    // spec 4: the hard delete leaves the table; a caller gets `InvalidInput`
    // naming it, never a silent hard delete
    assert!(layerfs_api::find("project.remove").is_none());
    assert_eq!(
        code("project.remove", map(&[("project", t("shop/api"))])),
        "InvalidInput: no call named `project.remove`"
    );
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
    assert!(code("branch.fork", map(&[("project", t("shop/api"))])).contains("`name` is required"));
    // the history calls moved to `commit`, with no alias
    for gone in ["branch.commits", "branch.get_commit", "branch.diff"] {
        assert!(layerfs_api::find(gone).is_none(), "{gone}");
        assert_eq!(
            code(gone, SdkValue::Null),
            format!("InvalidInput: no call named `{gone}`")
        );
    }
    // `from` is optional: main when left off
    let reached = code(
        "branch.fork",
        map(&[("project", t("shop/api")), ("name", t("fix"))]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
    let reached = code(
        "branch.fork",
        map(&[
            ("project", t("shop/api")),
            ("name", t("fix")),
            ("from", t("main")),
        ]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn branch_publish_declares_its_at_expected_and_force_options() {
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
            ("at", Kind::Name, true),
            ("expected", Kind::Name, true),
            ("force", Kind::Bool, true),
        ]
    );
    assert_eq!(
        call.help,
        "a Branch's head or Commit as main's next head, refusing a stale base"
    );
}

#[test]
fn branch_publish_expectations_are_plumbed() {
    for (name, value, why) in [
        ("at", SdkValue::Int(1), "`at` must be text"),
        ("at", SdkValue::Bool(true), "`at` must be text"),
        ("expected", SdkValue::Bool(true), "`expected` must be text"),
        ("expected", SdkValue::Int(1), "`expected` must be text"),
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
    // An option of the old form is not read as an argument of the call.
    let typo = code(
        "branch.publish",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("work")),
            ("expected_base", t("00")),
        ]),
    );
    assert!(
        typo.starts_with("InvalidInput: branch.publish: unknown argument `expected_base`"),
        "{typo}"
    );
    // A well-shaped expectation reaches the call instead of being refused by
    // argument checking.
    let reached = code(
        "branch.publish",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("work")),
            ("at", t("00")),
            ("expected", t("00")),
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
            ("limit", t("9"))
        ])
    )
    .contains("`limit` must be a whole number"));
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
    // a change names its fields as the single calls do (`from`, `to`), so
    // a folder scope sees its paths; the old names are refused
    let moved = SdkValue::List(vec![map(&[
        ("op", t("mv")),
        ("from_path", t("/a")),
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
    .contains("`from_path` is not a change field"));
    // `--mode` is octal text, as `file stat` answers it
    assert!(code(
        "file.mkdir",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("dev")),
            ("path", t("/d")),
            ("mode", t("644"))
        ])
    )
    .contains("`mode` is octal text"));
    // a window is 1 to 2000 lines; `--old-string` is required
    let read = |limit: &str| {
        code(
            "file.read",
            map(&[
                ("project", t("shop/api")),
                ("branch", t("main")),
                ("path", t("/a")),
                ("limit", SdkValue::Int(limit.parse().unwrap())),
            ]),
        )
    };
    assert!(
        read("2001").contains("`limit` is 1 to 2000"),
        "{}",
        read("2001")
    );
    assert!(code(
        "file.edit",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("dev")),
            ("path", t("/a")),
            ("new_string", t("x"))
        ])
    )
    .contains("file.edit: `old_string` is required"));
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
        "sandbox.remove",
        map(&[("sandbox", t("tests")), ("force", SdkValue::Bool(true))]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
    // ruling R1: the old names are gone, with no alias
    for gone in ["sandbox.close", "logs.sandbox"] {
        assert_eq!(
            code(gone, SdkValue::Null),
            format!("InvalidInput: no call named `{gone}`")
        );
    }
}

#[test]
fn workspace_family() {
    assert!(
        code("workspace.exec", map(&[("workspace", t("tests:dev"))]))
            .contains("`command` is required")
    );
    // a Workspace by its name (#536 C3); the path form is gone (2.0)
    let reached = code("workspace.status", map(&[("workspace", t("tests:dev"))]));
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
    let refused = code(
        "workspace.status",
        map(&[("workspace", t("tests:/workspaces/1"))]),
    );
    assert!(refused.starts_with("InvalidInput"), "{refused}");
    let mount = layerfs_api::find("workspace.mount").unwrap();
    let options: Vec<_> = mount.options.iter().map(|o| (o.name, o.kind)).collect();
    assert_eq!(options, [("name", Kind::Name)]);
    // cancel (was exec_cancel): the Workspace, and an exec id to end one
    let cancel = layerfs_api::find("workspace.cancel").unwrap();
    let args: Vec<_> = cancel.args.iter().map(|a| (a.name, a.kind)).collect();
    assert_eq!(args, [("workspace", Kind::Text)]);
    let options: Vec<_> = cancel.options.iter().map(|o| (o.name, o.kind)).collect();
    assert_eq!(options, [("exec", Kind::Text)]);
    assert!(layerfs_api::find("workspace.exec_cancel").is_none());
    let exec = layerfs_api::find("workspace.exec").unwrap();
    assert!(!exec.options.iter().any(|o| o.name == "exec_id"));
    assert!(exec.options.iter().any(|o| o.name == "env" && o.repeat));
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
    assert!(code("store.move", map(&[("store", t("shop"))])).contains("`machine` is required"));
    // a required option is refused before the call runs
    for call in ["store.attach", "store.detach"] {
        assert!(code(call, map(&[("store", t("pg"))])).contains("`machine` is required"));
    }
    // the old names are gone, with no alias
    for gone in [
        "store.status",
        "store.stats",
        "store.push",
        "store.pull",
        "store.save_auth",
        "store.auths",
        "store.remove_auth",
        "store.update",
    ] {
        assert!(layerfs_api::find(gone).is_none(), "{gone}");
    }
    let reached = code("store.list", SdkValue::Null);
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn backup_family() {
    assert!(code("backup.create", map(&[("store", t("shop"))])).contains("`bucket` is required"));
    assert!(code(
        "backup.restore",
        map(&[("bucket", t("b")), ("store", t("shop"))])
    )
    .contains("`file` is required"));
    // the key's secret is an option; both key options are required
    let connect = map(&[("name", t("b")), ("url", t("http://127.0.0.1:9/b"))]);
    assert!(code("backup.connect", connect).contains("`access_key` is required"));
    let connect = map(&[
        ("name", t("b")),
        ("url", t("http://127.0.0.1:9/b")),
        ("access_key", t("AKID")),
    ]);
    assert!(code("backup.connect", connect).contains("`secret_key` is required"));
    // the table says so, and its JSON tells the front ends
    let table = json::parse(&layerfs_api::table_json()).unwrap();
    let entry = |name: &str| {
        let SdkValue::List(calls) = &table else {
            panic!("the table is a list")
        };
        let (family, call) = name.split_once('.').unwrap();
        calls
            .iter()
            .find(|c| c.get_text("family") == Some(family) && c.get_text("call") == Some(call))
            .cloned()
            .unwrap()
    };
    for (name, required) in [
        ("backup.connect", ["access_key", "secret_key"]),
        ("store.attach", ["machine", "machine"]),
        ("file.edit", ["old_string", "new_string"]),
    ] {
        let Some(SdkValue::List(options)) = entry(name).get("options").cloned() else {
            panic!("{name}: options")
        };
        for option in options {
            let want = required.contains(&option.get_text("name").unwrap());
            let said = option.get("required") == Some(&SdkValue::Bool(true));
            assert_eq!(said, want, "{name} {:?}", option.get_text("name"));
        }
    }
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
fn home_leftovers_and_prune_are_gone() {
    // spec v20: LayerFS removes leftovers itself; the two calls and the
    // name before them are gone, with no alias
    for gone in ["home.leftovers", "home.prune", "doctor.fix"] {
        assert!(layerfs_api::find(gone).is_none(), "{gone}");
        assert_eq!(
            code(gone, SdkValue::Null),
            format!("InvalidInput: no call named `{gone}`")
        );
    }
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
    // src/ holds lib.rs, the call machinery (dispatch/, layerfs-project#107),
    // the host worker (worker/) and families/ (#828), which holds its mod.rs
    // and one folder per family; a family folder holds one <call>.rs per
    // call, in it or in purpose subfolders (#828), and a mod.rs in each
    // folder, nothing else
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let names = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(&root), ["dispatch", "families", "lib.rs", "worker"]);
    let src = root.join("families");
    let mut expected: Vec<String> = families().iter().map(|f| f.name.to_owned()).collect();
    expected.push("mod.rs".to_owned());
    expected.sort();
    assert_eq!(names(&src), expected);
    // the call files under `dir`, each folder holding its mod.rs
    fn call_files(dir: &std::path::Path, files: &mut Vec<String>) {
        let mut has_mod = false;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            if path.is_dir() {
                call_files(&path, files);
            } else if name == "mod.rs" {
                has_mod = true;
            } else {
                files.push(name);
            }
        }
        assert!(has_mod, "{} has no mod.rs", dir.display());
    }
    for family in families() {
        let mut expected: Vec<String> = family
            .calls()
            .iter()
            .map(|c| format!("{}.rs", c.call))
            .collect();
        expected.sort();
        let mut files = Vec::new();
        call_files(&src.join(family.name), &mut files);
        files.sort();
        assert_eq!(files, expected, "{}", family.name);
    }
}

#[cfg(all(feature = "machine", target_os = "macos"))]
#[test]
fn native_machine_images_reports_host_userland_without_a_bundle() {
    let args = map(&[
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ("detail", SdkValue::Bool(true)),
    ]);
    let (outcome, events) = run("machine.images", args);
    let result = outcome.unwrap();
    assert_eq!(result.get_text("mode"), Some("host"));
    assert_eq!(result.get("items"), Some(&SdkValue::List(vec![])));
    // the host's Docker is read only with `--with docker`
    assert_eq!(result.get("images"), None);
    assert!(events.is_empty());
    // no image on macOS native: a load or prune is refused, naming why
    for (call, args) in [
        (
            "machine.load_image",
            map(&[
                ("machine", t(layerfs_home::names::HOST_MACHINE)),
                ("image", t("alpine:3")),
            ]),
        ),
        (
            "machine.prune",
            map(&[
                ("machine", t(layerfs_home::names::HOST_MACHINE)),
                ("unused", SdkValue::Bool(true)),
            ]),
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
    let load = layerfs_api::find("machine.load_image").unwrap();
    assert_eq!(load.args[1].kind, Kind::Image);
    assert!(load.options.iter().any(|a| a.name == "on_progress"));
    let prune = layerfs_api::find("machine.prune").unwrap();
    assert!(prune.args[1].optional);
    assert_eq!(prune.options[0].name, "unused");
}

#[test]
fn the_four_gc_calls_take_only_ids() {
    // T11.1: the four GC calls, each one resource identifier and no data
    // option; `store.compact` alone declares `on_progress` (#42 item 45).
    use layerfs_api::Kind;
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
        assert_eq!(given.as_slice(), args, "{name}");
        assert!(spec.args.iter().all(|arg| !arg.optional), "{name}");
        let options: Vec<(&str, Kind)> = spec.options.iter().map(|o| (o.name, o.kind)).collect();
        if name == "store.compact" {
            assert_eq!(options, [("on_progress", Kind::Function)], "{name}");
        } else {
            assert!(options.is_empty(), "{name} takes no option");
        }
    }
}

#[test]
fn the_call_log_calls_moved_to_home_with_no_alias() {
    // ruling R1: an old name is gone, not kept beside the new one
    for gone in ["logs.list", "logs.reset"] {
        assert!(layerfs_api::find(gone).is_none(), "{gone}");
        assert_eq!(
            code(gone, SdkValue::Null),
            format!("InvalidInput: no call named `{gone}`")
        );
    }
    let home: Vec<_> = (families().iter())
        .find(|family| family.name == "home")
        .unwrap()
        .calls()
        .iter()
        .map(|call| call.call)
        .collect();
    assert_eq!(home, ["init", "get", "log", "clear_log", "doctor",]);
}

#[test]
fn a_migrated_entry_declares_its_forms_and_the_others_declare_none() {
    use layerfs_api::{Paging, Profile};
    let migrated: Vec<String> = layerfs_api::calls()
        .into_iter()
        .filter(|call| call.form.is_some())
        .map(|call| call.name())
        .collect();
    let machine = [
        "machine.list",
        "machine.get",
        "machine.images",
        "machine.load_image",
        "machine.prune",
        "machine.stats",
        "machine.files",
        "machine.log",
        "machine.start",
        "machine.stop",
        "machine.create",
        "machine.remove",
        "machine.update",
        "machine.doctor",
    ];
    let home = [
        "home.init",
        "home.get",
        "home.log",
        "home.clear_log",
        "home.doctor",
    ];
    let branch = [
        "branch.list",
        "branch.get",
        "branch.fork",
        "branch.merge",
        "branch.publish",
        "branch.remove",
    ];
    let commit = ["commit.list", "commit.get", "commit.diff"];
    let file = [
        "file.read",
        "file.stat",
        "file.list",
        "file.glob",
        "file.grep",
        "file.history",
        "file.blame",
        "file.write",
        "file.edit",
        "file.rm",
        "file.mkdir",
        "file.mv",
        "file.cp",
        "file.apply",
    ];
    let project = [
        "project.init",
        "project.list",
        "project.get",
        "project.stats",
        "project.doctor",
        "project.export",
        "project.delete",
        "project.hide_path",
    ];
    let sandbox = [
        "sandbox.create",
        "sandbox.list",
        "sandbox.get",
        "sandbox.remove",
        "sandbox.files",
        "sandbox.log",
        "sandbox.doctor",
    ];
    let workspace = [
        "workspace.list",
        "workspace.mount",
        "workspace.exec",
        "workspace.cancel",
        "workspace.commit",
        "workspace.unmount",
        "workspace.status",
    ];
    let store = [
        "store.list",
        "store.get",
        "store.create",
        "store.connect",
        "store.disconnect",
        "store.attach",
        "store.detach",
        "store.move",
        "store.remove",
        "store.compact",
        "store.upgrade",
        "store.doctor",
    ];
    let after = [
        "backup.connect",
        "backup.disconnect",
        "backup.list",
        "backup.create",
        "backup.restore",
        "backup.doctor",
    ];
    // in family order: home, project, branch, commit, file, sandbox, …
    assert_eq!(
        migrated,
        [
            &home[..],
            &project[..],
            &branch[..],
            &commit[..],
            &file[..],
            &sandbox[..],
            &workspace[..],
            &machine[..],
            &store[..],
            &after[..],
        ]
        .concat()
    );
    for call in layerfs_api::calls() {
        if let Some(form) = call.form {
            // every short key is a long key, and no key is listed twice
            for key in form.always.iter().chain(form.sometimes) {
                assert!(form.long.contains(key), "{}: {key}", call.name());
                assert!(
                    !(form.always.contains(key) && form.sometimes.contains(key)),
                    "{}: {key}",
                    call.name()
                );
            }
            // a paged call answers items (maps, or `file.glob`'s paths)
            // and a cursor
            if call.paging == Paging::Cursor {
                assert!(form.part("items").is_some(), "{}", call.name());
                assert!(form.sometimes.contains(&"next_cursor"), "{}", call.name());
                assert!(call.arg("limit").is_some() && call.arg("cursor").is_some());
            }
        } else {
            // not migrated: nothing of the new surface is declared yet
            assert!(call.with.is_empty(), "{}", call.name());
            assert_eq!(call.paging, Paging::None, "{}", call.name());
            assert_eq!(call.profile, Profile::Full, "{}", call.name());
        }
    }
    let get = layerfs_api::find("home.get").unwrap();
    let sections: Vec<_> = get.with.iter().map(|section| section.name).collect();
    assert_eq!(sections, ["platforms", "tree"]);
}

#[test]
fn the_global_options_are_taken_out_before_the_check_on_every_call() {
    // `detail` is accepted by a call that is not migrated and changes nothing
    let reached = code("backup.list", map(&[("detail", SdkValue::Bool(true))]));
    assert!(reached.starts_with("ok "), "{reached}");
    assert!(code("backup.list", map(&[("detail", t("yes"))]))
        .starts_with("InvalidInput: backup.list: `detail` must be true or false"));
    // a section the entry does not declare is refused, naming what it takes
    let refused = code(
        "home.get",
        map(&[("with", SdkValue::List(vec![t("live")]))]),
    );
    assert_eq!(
        refused,
        "InvalidInput: home.get: `with` names `live`; this call takes platforms, tree"
    );
    let refused = code("backup.list", map(&[("with", t("live"))]));
    assert_eq!(
        refused,
        "InvalidInput: backup.list: `with` names `live`, and this call takes no section"
    );
    let refused = code("store.list", map(&[("with", t("live"))]));
    assert_eq!(
        refused,
        "InvalidInput: store.list: `with` names `live`; this call takes stats, format"
    );
    // an empty list asks for nothing
    let reached = code("store.list", map(&[("with", SdkValue::List(vec![]))]));
    assert!(reached.starts_with("ok "), "{reached}");
}

#[test]
fn every_argument_the_old_name_list_redacted_is_flagged_on_its_entry() {
    // the dispatcher's name list is gone: until its family is migrated, an
    // argument of one of its names keeps its flag, so no value it hid
    // enters the call log
    use layerfs_api::Redact;
    let bytes = [
        "command", "message", "old", "new", "password", "url", "pattern",
    ];
    let paths = [
        "path",
        "left_path",
        "right_path",
        "from_path",
        "to_path",
        "path_prefix",
    ];
    for call in layerfs_api::calls() {
        for arg in call.args.iter().chain(call.options) {
            if arg.kind != Kind::Text {
                continue;
            }
            if bytes.contains(&arg.name) {
                assert_eq!(arg.redact, Redact::Bytes, "{} {}", call.name(), arg.name);
            } else if paths.contains(&arg.name) {
                assert_eq!(arg.redact, Redact::Path, "{} {}", call.name(), arg.name);
            }
        }
    }
}

/// The guest worker's build (no `machine` feature) refuses each machine call
/// that runs on the host, naming it.
#[cfg(not(feature = "machine"))]
#[test]
fn a_guest_build_refuses_a_machine_call_naming_it() {
    for (name, args) in [
        ("machine.list", SdkValue::Null),
        ("machine.stop", map(&[("machine", t("dev"))])),
    ] {
        let (outcome, _) = run(name, args);
        assert_eq!(
            outcome.unwrap_err(),
            ("Unsupported".to_owned(), format!("{name} runs on the host"))
        );
    }
}
