//! The call fixes of the #533 review (group calls): a folder scope bound to
//! its project allows only the calls it covers; `commit.get` pages to the
//! end; `branch.get` and `commit.get` keep 1.1.0's `NoSuch*` codes; a
//! scoped `branch.list` filters every page, not the first.
use layerfs_api::calls::{self, Scope};
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-calls-scope-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn map(args: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    )
}

fn call(
    home: &Path,
    scope: Option<&Scope>,
    name: &str,
    args: &[(&str, SdkValue)],
) -> Result<SdkValue, (String, String)> {
    calls::call_scoped(Some(home), scope, name, map(args), &mut |_| {})
        .map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, None, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn items(value: &SdkValue) -> Vec<SdkValue> {
    match value.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("no items: {other:?}"),
    }
}

fn names(value: &SdkValue) -> Vec<String> {
    items(value)
        .iter()
        .map(|i| i.get_text("name").unwrap().to_owned())
        .collect()
}

fn scope(project: &str, branch: Option<&str>) -> Scope {
    let mut fields = vec![("project", t(project)), ("folder", t("/web"))];
    if let Some(branch) = branch {
        fields.push(("branch", t(branch)));
    }
    Scope::parse(&map(&fields)).unwrap()
}

/// A home with Store `shop` and project `shop/api` Inited from a folder.
fn project() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
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

#[test]
fn a_scope_names_its_project() {
    let refused = Scope::parse(&map(&[("folder", t("/web"))])).unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
    assert!(refused.message.contains("scope.project"), "{refused:?}");
    let bad = Scope::parse(&map(&[("project", t("api")), ("folder", t("/web"))]));
    assert_eq!(bad.unwrap_err().code, "InvalidInput");
    assert_eq!(scope("shop/api", None).project, "shop/api");
}

#[test]
fn a_scope_refuses_every_call_it_does_not_cover() {
    let home = tempdir("uncovered");
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let scoped = scope("shop/api", Some("agent"));
    let uncovered: &[(&str, &[(&str, SdkValue)])] = &[
        ("store.remove", &[("name", t("shop"))]),
        ("store.list", &[]),
        ("project.remove", &[("project", t("shop/api"))]),
        (
            "project.init",
            &[("project", t("shop/x")), ("folder", t("/tmp"))],
        ),
        ("project.list", &[]),
        (
            "backup.restore",
            &[("store", t("shop")), ("bucket", t("b"))],
        ),
        ("backup.list", &[]),
        ("sandbox.close", &[("name", t("s1"))]),
        (
            "sandbox.create",
            &[
                ("project", t("shop/api")),
                ("name", t("s1")),
                ("machine", t("native")),
            ],
        ),
        ("sandbox.list", &[]),
    ];
    for (name, args) in uncovered {
        let spec = calls::find(name).unwrap();
        // fill the call's required arguments so the table's check passes
        let mut given: Vec<(&str, SdkValue)> = args.to_vec();
        for arg in spec.args.iter().filter(|a| !a.optional) {
            if !given.iter().any(|(n, _)| *n == arg.name) {
                given.push((arg.name, t("x")));
            }
        }
        let refused = call(&home, Some(&scoped), name, &given).unwrap_err();
        assert_eq!(refused.0, "InvalidInput", "{name}: {refused:?}");
        assert!(refused.1.contains("out of scope"), "{name}: {refused:?}");
    }
    #[cfg(feature = "machine")]
    {
        let refused = call(
            &home,
            Some(&scoped),
            "machine.remove",
            &[("name", t("box"))],
        );
        assert!(refused.unwrap_err().1.contains("out of scope"));
    }
    // the Store is still there: nothing ran
    ok(&home, "store.get", &[("name", t("shop"))]);
    // history stays refused, and another project is out of scope
    let history = call(
        &home,
        Some(&scoped),
        "commit.list",
        &[("project", t("shop/api")), ("branch", t("main"))],
    );
    assert!(history.unwrap_err().1.contains("out of scope: history"));
    let other = call(
        &home,
        Some(&scoped),
        "file.stat",
        &[
            ("project", t("shop/other")),
            ("branch", t("main")),
            ("path", t("/web/a")),
        ],
    );
    assert!(other.unwrap_err().1.contains("out of scope: project"));
    let other = call(
        &home,
        Some(&scoped),
        "branch.list",
        &[("project", t("shop/other"))],
    );
    assert!(other.unwrap_err().1.contains("out of scope: project"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_scoped_workspace_call_needs_a_sandbox_of_its_project() {
    let home = tempdir("sandbox");
    home::open(&home).unwrap();
    for (name, project) in [("mine", "shop/api"), ("theirs", "shop/other")] {
        let record = paths::sandbox_dir(&home, "native", name).join("record.json");
        write_record(
            &record,
            &SdkValue::map([("name", t(name)), ("project", t(project))]),
        )
        .unwrap();
    }
    let scoped = scope("shop/api", Some("agent"));
    for (call_name, args) in [
        ("workspace.mount", map(&[("sandbox", t("theirs"))])),
        (
            "workspace.exec",
            map(&[("ws", t("theirs:/w")), ("command", t("true"))]),
        ),
        ("workspace.status", map(&[("ws", t("theirs:/w"))])),
        ("workspace.unmount", map(&[("ws", t("nosuch:/w"))])),
        ("workspace.commit", map(&[("ws", t("no-colon"))])),
    ] {
        let refused = scoped.owns(&home, call_name, &args).unwrap_err();
        assert!(
            refused.message.contains("out of scope: sandbox"),
            "{call_name}"
        );
    }
    let mine = map(&[("ws", t("mine:/w"))]);
    scoped.owns(&home, "workspace.status", &mine).unwrap();
    scoped
        .owns(&home, "workspace.mount", &map(&[("sandbox", t("mine"))]))
        .unwrap();
    // through the call: refused before dispatch
    let refused = call(
        &home,
        Some(&scoped),
        "workspace.status",
        &[("ws", t("theirs:/w"))],
    );
    assert!(refused.unwrap_err().1.contains("out of scope: sandbox"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_scoped_branch_list_reads_every_page() {
    let (home, folder) = project();
    let p = || ("project", t("shop/api"));
    // Branches that sort before the namespace fill the first page
    for n in 0..120 {
        ok(
            &home,
            "branch.fork",
            &[
                p(),
                ("name", t(&format!("a.{n:03}"))),
                ("from_branch", t("main")),
            ],
        );
    }
    for name in ["agent.x", "agent.y"] {
        ok(
            &home,
            "branch.fork",
            &[p(), ("name", t(name)), ("from_branch", t("main"))],
        );
    }
    let first = ok(
        &home,
        "branch.list",
        &[p(), ("page_size", SdkValue::Int(1000))],
    );
    assert!(
        !names(&first).iter().any(|n| n.starts_with("agent.")),
        "the namespace is past the first page"
    );
    let scoped = scope("shop/api", Some("agent"));
    let all = call(&home, Some(&scoped), "branch.list", &[p()]).unwrap();
    assert_eq!(names(&all), ["x", "y"]);
    assert_eq!(all.get("has_more"), Some(&SdkValue::Bool(false)));
    let one = call(
        &home,
        Some(&scoped),
        "branch.list",
        &[p(), ("page_size", SdkValue::Int(1))],
    )
    .unwrap();
    assert_eq!(names(&one), ["x"]);
    assert_eq!(one.get("has_more"), Some(&SdkValue::Bool(true)));
    assert_eq!(one.get_int("next_page_pos"), Some(1));
    let next = call(
        &home,
        Some(&scoped),
        "branch.list",
        &[
            p(),
            ("page_size", SdkValue::Int(1)),
            ("page_pos", SdkValue::Int(1)),
        ],
    )
    .unwrap();
    assert_eq!(names(&next), ["y"]);
    assert_eq!(next.get("has_more"), Some(&SdkValue::Bool(false)));
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
