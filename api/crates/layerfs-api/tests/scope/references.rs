//! A folder scope confines `branch.fork` and `branch.publish` references
//! (#30, #33): Branch names are composed into the namespace and checked by
//! the scope, and a Commit given as `at` must be in the history of the
//! namespace's Branch, which the Store checks. Every refusal comes before
//! any effect.
use layerfs_api::home;
use layerfs_api::Scope;
use layerfs_protocol::sdk::SdkValue;
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
    let all = ok(home, "branch.list", &[p(), ("limit", SdkValue::Int(1000))]);
    match all.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|i| i.get_text("branch").unwrap().to_owned())
            .collect(),
        other => panic!("no items: {other:?}"),
    }
}

/// The id of the one Commit `branch` has.
fn commit_of(home: &Path, branch: &str) -> String {
    let list = ok(home, "commit.list", &[p(), ("branch", t(branch))]);
    match list.get("items") {
        Some(SdkValue::List(items)) if items.len() == 1 => {
            items[0].get_text("commit").unwrap().to_owned()
        }
        other => panic!("{branch}: one Commit expected: {other:?}"),
    }
}

/// `main`'s head: what a publish moves.
fn main_head(home: &Path) -> String {
    let main = ok(home, "branch.get", &[p(), ("branch", t("main"))]);
    main.get_text("commit").unwrap().to_owned()
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
            &[p(), ("name", t(branch)), ("from", t("main"))],
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

/// A refused fork: what it shows, its arguments and the expected code, or
/// the scope's message.
type Case<'a> = (&'a str, &'a [(&'a str, SdkValue)], &'a str);

#[test]
fn fork_refuses_every_reference_outside_the_namespace_before_any_effect() {
    let (home, folder) = project();
    let scoped = scope();
    let before = branches(&home);
    let other = commit_of(&home, "other");
    let forks: &[Case<'_>] = &[
        // the Store: `other`'s Commit is not in `agent.work`'s history
        (
            "at naming an outside Commit",
            &[("name", t("x")), ("from", t("work")), ("at", t(&other))],
            "NotFound",
        ),
        (
            "at by a prefix of an outside Commit",
            &[
                ("name", t("x")),
                ("from", t("work")),
                ("at", t(&other[..12])),
            ],
            "NotFound",
        ),
        (
            "at that is no Commit id",
            &[("name", t("x")), ("from", t("work")), ("at", t("Other"))],
            "InvalidInput",
        ),
        // composed to `agent.other`, which names no Branch
        (
            "from outside",
            &[("name", t("x")), ("from", t("other"))],
            "NotFound",
        ),
        // the scope, before dispatch
        (
            "from outside the grammar",
            &[("name", t("x")), ("from", t("../other"))],
            "out of scope: branch",
        ),
        (
            "name outside the grammar",
            &[("name", t("A/b")), ("from", t("work"))],
            "out of scope: branch",
        ),
        (
            "name too long once composed",
            &[("name", t(&"n".repeat(60))), ("from", t("work"))],
            "out of scope: branch",
        ),
    ];
    for (what, args, expected) in forks {
        let mut given = vec![p()];
        given.extend(args.iter().cloned());
        let refused = call(&home, Some(&scoped), "branch.fork", &given).expect_err(what);
        match expected.strip_prefix("out of scope") {
            Some(_) => {
                assert_eq!(refused.0, "InvalidInput", "{what}: {refused:?}");
                assert!(refused.1.contains(expected), "{what}: {refused:?}");
            }
            None => assert_eq!(refused.0, *expected, "{what}: {refused:?}"),
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
    let fork = |name: &str, extra: &[(&str, SdkValue)]| {
        let mut given = vec![p(), ("name", t(name)), ("from", t("work"))];
        given.extend(extra.iter().cloned());
        call(&home, Some(&scoped), "branch.fork", &given).unwrap()
    };
    // from a Branch of the namespace, its head or one of its Commits
    fork("a", &[]);
    let at = fork("b", &[("at", t(&work))]);
    assert_eq!(at.get_text("commit"), Some(work.as_str()), "{at:?}");
    fork("c", &[("at", t(&work[..10]))]);
    // `from` left off is the namespace's own `main`, which does not exist
    let refused = call(
        &home,
        Some(&scoped),
        "branch.fork",
        &[p(), ("name", t("d"))],
    )
    .unwrap_err();
    assert_eq!(refused.0, "NotFound", "{refused:?}");
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
    let head = main_head(&home);
    let other = commit_of(&home, "other");
    for (what, at, code) in [
        ("an outside Commit", other.as_str(), "NotFound"),
        ("no Commit id", "Other", "InvalidInput"),
    ] {
        let refused = call(
            &home,
            Some(&scoped),
            "branch.publish",
            &[p(), ("branch", t("work")), ("at", t(at))],
        )
        .expect_err(what);
        assert_eq!(refused.0, code, "{what}: {refused:?}");
        assert_eq!(main_head(&home), head, "{what}: main moved");
    }
    // `branch` outside the namespace is composed, so it names no Branch
    let refused = call(
        &home,
        Some(&scoped),
        "branch.publish",
        &[p(), ("branch", t("other"))],
    )
    .unwrap_err();
    assert_eq!(refused.0, "NotFound", "{refused:?}");
    let refused = call(
        &home,
        Some(&scoped),
        "branch.publish",
        &[p(), ("branch", t("../other"))],
    )
    .unwrap_err();
    assert!(refused.1.contains("out of scope: branch"), "{refused:?}");
    assert_eq!(main_head(&home), head, "main moved");
    cleanup(home, folder);
}

#[test]
fn publish_inside_the_namespace_still_works() {
    let (home, folder) = project();
    let scoped = scope();
    let head = main_head(&home);
    let work = commit_of(&home, "agent.work");
    let added = call(
        &home,
        Some(&scoped),
        "branch.publish",
        &[p(), ("branch", t("work")), ("at", t(&work[..12]))],
    )
    .unwrap();
    let published = added.get_text("commit").expect("the new publication");
    assert_ne!(main_head(&home), head, "main took the new publication");
    assert_eq!(main_head(&home), published);
    cleanup(home, folder);
}

#[test]
fn the_json_form_refuses_like_the_table_call() {
    let (home, folder) = project();
    let scope_text = r#"{"project":"shop/api","folder":"/web","branch":"agent"}"#;
    let args = r#"{"project":"shop/api","name":"x","from":"../work"}"#;
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
        &[p(), ("name", t("x")), ("from", t("../work"))],
    )
    .unwrap_err();
    assert!(
        answer.contains(&format!("\"error\":\"{}\"", direct.0)),
        "{answer}"
    );
    assert!(answer.contains("out of scope: branch"), "{answer}");
    assert!(direct.1.contains("out of scope: branch"), "{direct:?}");
    assert!(!branches(&home).iter().any(|n| n == "agent.x"));
    cleanup(home, folder);
}
