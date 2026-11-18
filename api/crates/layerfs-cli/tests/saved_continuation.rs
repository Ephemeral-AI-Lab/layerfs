//! Every CLI call is a new process: continuations cannot rely on process caches.
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

fn call(home: &Path, words: &[&str]) -> SdkValue {
    let out = run(&mut layerfs(home, words));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{words:?}: {}",
        text(&out.stderr)
    );
    json::parse(text(&out.stdout).trim()).unwrap()
}
fn items(value: &SdkValue) -> &[SdkValue] {
    let Some(SdkValue::List(items)) = value.get("items") else {
        panic!("items: {value:?}")
    };
    items
}
fn invalid(home: &Path, words: &[&str]) {
    let out = run(&mut layerfs(home, words));
    assert_eq!(
        out.status.code(),
        Some(125),
        "{words:?}: {}",
        text(&out.stderr)
    );
    assert_eq!(
        json::parse(text(&out.stderr).trim())
            .unwrap()
            .get_text("error"),
        Some("InvalidInput")
    );
}
fn check(home: &Path, source: &Path) {
    call(home, &["project", "init", "q/p", source.to_str().unwrap()]);
    let base = call(home, &["project", "stats", "q/p"]);
    let layer = base.get("at").unwrap().get_text("id").unwrap();
    call(home, &["branch", "fork", "q/p", "dev", "main"]);
    // Three paths in the first Commit; this is also the first-Commit/base-Layer case.
    for (path, value) in [("/a", "one"), ("/b", "two"), ("/c", "three")] {
        let out = common::run_with_stdin(
            &mut layerfs(home, &["file", "write", "q/p", "dev", path]),
            value.as_bytes(),
        );
        assert!(out.status.success(), "{}", text(&out.stderr));
    }
    let saved = call(home, &["project", "stats", "q/p", "dev"]);
    let commit = saved.get("at").unwrap().get_text("id").unwrap();
    // `commit` (dev's head, the third write) from `layer`; by default from
    // its parent, which lacks only the last write
    let changed = call(home, &["commit", "diff", "q/p", commit, "--from", layer]);
    assert_eq!(items(&changed).len(), 3);
    let parent = call(home, &["commit", "diff", "q/p", commit]);
    assert_eq!(items(&parent).len(), 1);
    assert_eq!(items(&parent)[0].get_text("path"), Some("/c"));
    // one path a page; each next page in a new process, by the cursor
    let query = ["commit", "diff", "q/p", "dev", "--limit", "1"];
    let first = call(home, &query);
    let token = first.get_text("next_cursor").unwrap();
    let second = call(home, &[&query[..], &["--cursor", token]].concat());
    assert_ne!(items(&first)[0].get("path"), items(&second)[0].get("path"));
    let third = call(
        home,
        &[
            &query[..],
            &["--cursor", second.get_text("next_cursor").unwrap()],
        ]
        .concat(),
    );
    assert_eq!(items(&third).len(), 1);
    assert_eq!(third.get("next_cursor"), None);
    // a cursor given other filters, or altered, is refused
    invalid(
        home,
        &[&query[..], &["--cursor", token, "--path", "/a"]].concat(),
    );
    invalid(
        home,
        &[&query[..], &["--cursor", token, "--from", "main"]].concat(),
    );
    let mut altered = token.to_owned();
    altered.insert(0, '!');
    invalid(home, &[&query[..], &["--cursor", &altered]].concat());
    call(home, &["branch", "publish", "q/p", "dev"]);
    let published = call(home, &["project", "stats", "q/p"]);
    let new_layer = published.get("at").unwrap().get_text("id").unwrap();
    assert_eq!(
        items(&call(
            home,
            &["commit", "diff", "q/p", new_layer, "--from", layer]
        ))
        .len(),
        3
    );
    // Additional path revisions and a frozen revision pin.
    for value in ["second", "third", "fourth"] {
        let out = common::run_with_stdin(
            &mut layerfs(home, &["file", "write", "q/p", "dev", "/a"]),
            value.as_bytes(),
        );
        assert!(out.status.success(), "{}", text(&out.stderr));
    }
    let saved = call(home, &["project", "stats", "q/p", "dev"]);
    let tip = saved.get("at").unwrap().get_text("id").unwrap();
    let revisions = [
        "file",
        "revision",
        "q/p",
        "dev",
        "/a",
        "--at-commit",
        tip,
        "--page-size",
        "1",
    ];
    let all = call(
        home,
        &[
            "file",
            "revision",
            "q/p",
            "dev",
            "/a",
            "--at-commit",
            tip,
            "--page-size",
            "100",
        ],
    );
    let mut page = call(home, &revisions);
    let mut refs = Vec::new();
    for _ in 0..10 {
        refs.extend(items(&page).iter().map(|i| i.get("ref").unwrap().clone()));
        assert!(
            page.get("scan")
                .unwrap()
                .get_int("candidates_examined")
                .unwrap()
                <= 5
        );
        let Some(token) = page.get_text("continuation") else {
            break;
        };
        page = call(home, &[&revisions[..], &["--continuation", token]].concat());
    }
    assert_eq!(
        refs,
        items(&all)
            .iter()
            .map(|i| i.get("ref").unwrap().clone())
            .collect::<Vec<_>>()
    );
    let first = call(home, &revisions);
    let token = first.get_text("continuation").unwrap();
    let changed_path = [
        "file",
        "revision",
        "q/p",
        "dev",
        "/b",
        "--at-commit",
        tip,
        "--page-size",
        "1",
        "--continuation",
        token,
    ];
    invalid(home, &changed_path);
    invalid(
        home,
        &[
            &revisions[..],
            &["--continuation", token, "--traversal", "branch"],
        ]
        .concat(),
    );
}
#[test]
fn sqlite_saved_queries_resume_in_new_processes() {
    let home = tempdir("continuation-home");
    let source = tempdir("continuation-source");
    call(&home, &["store", "create", "q"]);
    check(&home, &source);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}
#[test]
fn postgresql_saved_queries_resume_in_new_processes() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
        return;
    };
    let home = tempdir("pg-continuation-home");
    let source = tempdir("pg-continuation-source");
    // the URL's user becomes a saved login, its password on stdin (a
    // trust-auth server takes any); the address is connected without it
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    let out = common::run_with_stdin(
        &mut layerfs(
            &home,
            &[
                "store",
                "save-auth",
                "pg",
                "--user",
                user,
                "--password-stdin",
            ],
        ),
        b"unused\n",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let address = format!("{scheme}://{address}");
    call(
        &home,
        &[
            "store",
            "connect",
            "q",
            &address,
            "--auth",
            "pg",
            "--synchronous-commit",
            "off",
        ],
    );
    check(&home, &source);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}
