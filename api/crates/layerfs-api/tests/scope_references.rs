//! A folder scope confines `branch.fork` and `branch.publish` references:
//! `from_commit` and `commit` pass the same check as every other Commit or
//! Branch reference, and a refusal comes before any effect (#30, #33).
use layerfs_api::home;
use layerfs_api::Scope;
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-scope-refs-{tag}-{}-{}",
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

fn p() -> (&'static str, SdkValue) {
    ("project", t("shop/api"))
}

fn scope() -> Scope {
    Scope::parse(&map(&[
        ("project", t("shop/api")),
        ("folder", t("/web")),
        ("branch", t("agent")),
    ]))
    .unwrap()
}

/// Names of every Branch of the project, read unscoped.
fn branches(home: &Path) -> Vec<String> {
    let all = ok(
        home,
        "branch.list",
        &[p(), ("page_size", SdkValue::Int(1000))],
    );
    match all.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|i| i.get_text("name").unwrap().to_owned())
            .collect(),
        other => panic!("no items: {other:?}"),
    }
}

/// The id of the one Commit `branch` has.
fn commit_of(home: &Path, branch: &str) -> String {
    let list = ok(home, "commit.list", &[p(), ("branch", t(branch))]);
    match list.get("items") {
        Some(SdkValue::List(items)) if items.len() == 1 => {
            items[0].get_text("id").unwrap().to_owned()
        }
        other => panic!("{branch}: one Commit expected: {other:?}"),
    }
}

/// The base Layer of `main`: what a publish moves.
fn main_base(home: &Path) -> String {
    let main = ok(home, "branch.get", &[p(), ("branch", t("main"))]);
    main.get_text("base").unwrap().to_owned()
}

/// A home with project `shop/api`; Branch `other` (outside the scope's
/// namespace `agent`) and `agent.work` (inside) each hold one Commit.
fn project() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[p(), ("folder", t(&folder.display().to_string()))],
    );
    for branch in ["other", "agent.work"] {
        ok(
            &home,
            "branch.fork",
            &[p(), ("name", t(branch)), ("from_branch", t("main"))],
        );
        ok(
            &home,
            "file.write",
            &[
                p(),
                ("branch", t(branch)),
                ("path", t("/f")),
                ("content", t(&format!("{branch}\n"))),
                ("message", t(&format!("write {branch}"))),
            ],
        );
    }
    (home, folder)
}

fn cleanup(home: PathBuf, folder: PathBuf) {
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn fork_refuses_every_reference_outside_the_namespace_before_any_effect() {
    let (home, folder) = project();
    let scoped = scope();
    let before = branches(&home);
    let other = commit_of(&home, "other");
    // `from_commit` is checked like `from_branch`: a Branch name is composed
    // (`agent.other` does not exist), a malformed reference is out of scope
    let forks: &[(&str, &[(&str, SdkValue)])] = &[
        (
            "from_commit naming an outside Branch",
            &[
                ("name", t("x")),
                ("from_branch", t("work")),
                ("from_commit", t("other")),
            ],
        ),
        (
            "from_commit outside the Branch grammar",
            &[
                ("name", t("x")),
                ("from_branch", t("work")),
                ("from_commit", t("Other")),
            ],
        ),
        (
            "from_commit of the wrong length",
            &[
                ("name", t("x")),
                ("from_branch", t("work")),
                ("from_commit", t(&other[..65])),
            ],
        ),
        (
            "from_branch outside",
            &[("name", t("x")), ("from_branch", t("other"))],
        ),
        (
            "from_branch outside the grammar",
            &[("name", t("x")), ("from_branch", t("../other"))],
        ),
        (
            "name outside the grammar",
            &[("name", t("A/b")), ("from_branch", t("work"))],
        ),
        (
            "name too long once composed",
            &[("name", t(&"n".repeat(60))), ("from_branch", t("work"))],
        ),
    ];
    for (n, (what, args)) in forks.iter().enumerate() {
        let mut given = vec![p()];
        given.extend(args.iter().cloned());
        let refused = call(&home, Some(&scoped), "branch.fork", &given).expect_err(what);
        assert_eq!(refused.0, "InvalidInput", "{what}: {refused:?}");
        // the two composed Branch names (cases 0 and 3) name no Branch and
        // are refused by the Store; the others never reach it
        if n != 0 && n != 3 {
            assert!(
                refused.1.contains("out of scope: branch"),
                "{what}: {refused:?}"
            );
        }
        assert_eq!(branches(&home), before, "{what}: a Branch appeared");
    }
    cleanup(home, folder);
}

#[test]
fn fork_inside_the_namespace_still_works() {
    let (home, folder) = project();
    let scoped = scope();
    let work = commit_of(&home, "agent.work");
    // from a Branch of the namespace
    call(
        &home,
        Some(&scoped),
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("a")),
            ("from_branch", t("work")),
        ],
    )
    .unwrap();
    // `from_commit` as a Branch name is composed, like `from_branch`
    call(
        &home,
        Some(&scoped),
        "branch.fork",
        &[
            p(),
            ("name", t("b")),
            ("from_branch", t("work")),
            ("from_commit", t("work")),
        ],
    )
    .unwrap();
    // `from_commit` as a Commit id of the namespace stays as given
    call(
        &home,
        Some(&scoped),
        "branch.fork",
        &[
            p(),
            ("name", t("c")),
            ("from_branch", t("work")),
            ("from_commit", t(&work)),
        ],
    )
    .unwrap();
    let names = branches(&home);
    for made in ["agent.a", "agent.b", "agent.c"] {
        assert!(names.iter().any(|n| n == made), "{made} in {names:?}");
    }
    cleanup(home, folder);
}

#[test]
fn publish_refuses_a_commit_outside_the_namespace_before_any_effect() {
    let (home, folder) = project();
    let scoped = scope();
    let base = main_base(&home);
    for (what, commit) in [
        ("an outside Branch name", "other"),
        ("outside the Branch grammar", "Other"),
    ] {
        let refused = call(
            &home,
            Some(&scoped),
            "branch.publish",
            &[p(), ("branch", t("work")), ("commit", t(commit))],
        )
        .expect_err(what);
        assert_eq!(refused.0, "InvalidInput", "{what}: {refused:?}");
        assert_eq!(main_base(&home), base, "{what}: main moved");
    }
    let refused = call(
        &home,
        Some(&scoped),
        "branch.publish",
        &[p(), ("branch", t("work")), ("commit", t("Other"))],
    )
    .unwrap_err();
    assert!(refused.1.contains("out of scope: branch"), "{refused:?}");
    // `branch` outside the namespace is composed, so it names no Branch
    let refused = call(
        &home,
        Some(&scoped),
        "branch.publish",
        &[p(), ("branch", t("other"))],
    )
    .unwrap_err();
    assert_eq!(refused.0, "NoSuchBranch", "{refused:?}");
    assert_eq!(main_base(&home), base, "main moved");
    cleanup(home, folder);
}

#[test]
fn publish_inside_the_namespace_still_works() {
    let (home, folder) = project();
    let scoped = scope();
    let base = main_base(&home);
    let work = commit_of(&home, "agent.work");
    let added = call(
        &home,
        Some(&scoped),
        "branch.publish",
        &[p(), ("branch", t("work")), ("commit", t(&work))],
    )
    .unwrap();
    assert_eq!(added.get_text("kind"), Some("Added"), "{added:?}");
    assert_ne!(main_base(&home), base, "main took the new Layer");
    cleanup(home, folder);
}

#[test]
fn the_json_form_refuses_like_the_table_call() {
    let (home, folder) = project();
    let scope_text = r#"{"project":"shop/api","folder":"/web","branch":"agent"}"#;
    let args = r#"{"project":"shop/api","name":"x","from_branch":"work","from_commit":"Other"}"#;
    let answer = layerfs_api::call_json(
        Some(&home),
        Some(scope_text),
        "branch.fork",
        args,
        &mut |_| {},
        layerfs_api::Client::Rust,
    );
    let direct = call(
        &home,
        Some(&scope()),
        "branch.fork",
        &[
            p(),
            ("name", t("x")),
            ("from_branch", t("work")),
            ("from_commit", t("Other")),
        ],
    )
    .unwrap_err();
    assert!(
        answer.contains(&format!("\"code\":\"{}\"", direct.0)),
        "{answer}"
    );
    assert!(answer.contains("out of scope: branch"), "{answer}");
    assert!(direct.1.contains("out of scope: branch"), "{direct:?}");
    assert!(!branches(&home).iter().any(|n| n == "agent.x"));
    cleanup(home, folder);
}
