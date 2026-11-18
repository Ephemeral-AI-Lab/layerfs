//! The call fixes of the #533 review (group calls): a folder scope bound to
//! its project allows only the calls it covers; `commit.get` pages to the
//! end; `branch.get` and `commit.get` keep 1.1.0's `NoSuch*` codes; a
//! scoped `branch.list` lists its namespace's Branches past every other.
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::Scope;
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
    layerfs_api::call_scoped(
        Some(home),
        scope,
        name,
        map(args),
        &mut |_| {},
        layerfs_api::Client::Rust,
    )
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
        .map(|i| i.get_text("branch").unwrap().to_owned())
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
        ("store.remove", &[("store", t("shop"))]),
        ("store.list", &[]),
        (
            "branch.remove",
            &[("project", t("shop/api")), ("branch", t("main"))],
        ),
        ("project.delete", &[("project", t("shop/api"))]),
        (
            "project.remove_file",
            &[("project", t("shop/api")), ("path", t("/web/a"))],
        ),
        ("store.compact", &[("store", t("shop"))]),
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
        ("sandbox.remove", &[("sandbox", t("s1"))]),
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
        let spec = layerfs_api::find(name).unwrap();
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
            &[("machine", t("box"))],
        );
        assert!(refused.unwrap_err().1.contains("out of scope"));
    }
    // the Store is still there: nothing ran
    ok(&home, "store.get", &[("store", t("shop"))]);
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
        let record = paths::sandbox_record(&home, name);
        write_record(
            &record,
            &SdkValue::map([
                ("name", t(name)),
                ("project", t(project)),
                ("machine", t("native")),
            ]),
        )
        .unwrap();
    }
    let scoped = scope("shop/api", Some("agent"));
    for (call_name, args) in [
        ("workspace.mount", map(&[("sandbox", t("theirs"))])),
        (
            "workspace.exec",
            map(&[("workspace", t("theirs:w")), ("command", t("true"))]),
        ),
        ("workspace.status", map(&[("workspace", t("theirs:w"))])),
        ("workspace.unmount", map(&[("workspace", t("nosuch:w"))])),
        ("workspace.commit", map(&[("workspace", t("no-colon"))])),
    ] {
        let refused = scoped
            .owns(&home, layerfs_api::find(call_name).unwrap(), &args)
            .unwrap_err();
        assert!(
            refused.message.contains("out of scope: sandbox"),
            "{call_name}"
        );
    }
    let mine = map(&[("workspace", t("mine:w"))]);
    scoped
        .owns(&home, layerfs_api::find("workspace.status").unwrap(), &mine)
        .unwrap();
    scoped
        .owns(
            &home,
            layerfs_api::find("workspace.mount").unwrap(),
            &map(&[("sandbox", t("mine"))]),
        )
        .unwrap();
    // through the call: refused before dispatch
    let refused = call(
        &home,
        Some(&scoped),
        "workspace.status",
        &[("workspace", t("theirs:w"))],
    );
    assert!(refused.unwrap_err().1.contains("out of scope: sandbox"));
    std::fs::remove_dir_all(home).unwrap();
}

/// The `next_cursor` of a page: absent from the short form when there is
/// no next page.
fn next_cursor(value: &SdkValue) -> Option<String> {
    value.get_text("next_cursor").map(str::to_owned)
}

#[test]
fn a_scoped_branch_list_pages_its_namespace_past_every_other() {
    let (home, folder) = project();
    let p = || ("project", t("shop/api"));
    // Branches that sort before the namespace fill the first page
    for n in 0..120 {
        ok(
            &home,
            "branch.fork",
            &[p(), ("name", t(&format!("a.{n:03}"))), ("from", t("main"))],
        );
    }
    for name in ["agent.x", "agent.y"] {
        ok(
            &home,
            "branch.fork",
            &[p(), ("name", t(name)), ("from", t("main"))],
        );
    }
    let first = ok(&home, "branch.list", &[p()]);
    assert!(
        !names(&first).iter().any(|n| n.starts_with("agent.")),
        "the namespace is past the first page"
    );
    let scoped = scope("shop/api", Some("agent"));
    let all = call(&home, Some(&scoped), "branch.list", &[p()]).unwrap();
    assert_eq!(names(&all), ["x", "y"]);
    assert_eq!(next_cursor(&all), None);
    let one = call(
        &home,
        Some(&scoped),
        "branch.list",
        &[p(), ("limit", SdkValue::Int(1))],
    )
    .unwrap();
    assert_eq!(names(&one), ["x"]);
    let cursor = next_cursor(&one).expect("a next page");
    let next = call(
        &home,
        Some(&scoped),
        "branch.list",
        &[p(), ("limit", SdkValue::Int(1)), ("cursor", t(&cursor))],
    )
    .unwrap();
    assert_eq!(names(&next), ["y"]);
    assert_eq!(next_cursor(&next), None);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
