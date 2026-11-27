//! Every CLI call is a new process: continuations cannot rely on process caches.
#[path = "../support/mod.rs"]
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_home::json;
use layerfs_protocol::sdk::SdkValue;
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
    let base = call(home, &["project", "stats", "q/p", "--detail"]);
    let layer = base.get_text("commit").unwrap();
    call(home, &["branch", "fork", "q/p", "dev", "main"]);
    // Three paths in the first Commit; this is also the first-Commit/base-Layer case.
    for (path, value) in [("/a", "one"), ("/b", "two"), ("/c", "three")] {
        let out = common::run_with_stdin(
            &mut layerfs(home, &["file", "write", "q/p", "dev", path]),
            value.as_bytes(),
        );
        assert!(out.status.success(), "{}", text(&out.stderr));
    }
    let saved = call(
        home,
        &["project", "stats", "q/p", "--branch", "dev", "--detail"],
    );
    let commit = saved.get_text("commit").unwrap();
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
    let published = call(home, &["project", "stats", "q/p", "--detail"]);
    let new_layer = published.get_text("commit").unwrap();
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
    let saved = call(
        home,
        &["project", "stats", "q/p", "--branch", "dev", "--detail"],
    );
    let tip = saved.get_text("commit").unwrap();
    let revisions = [
        "file", "history", "q/p", "dev", "/a", "--at", tip, "--limit", "1",
    ];
    let all = call(
        home,
        &[
            "file", "history", "q/p", "dev", "/a", "--at", tip, "--limit", "100",
        ],
    );
    // one Commit a page, each next page in a new process, by the cursor
    let mut page = call(home, &revisions);
    let mut commits = Vec::new();
    for _ in 0..10 {
        commits.extend(
            items(&page)
                .iter()
                .map(|i| i.get("commit").unwrap().clone()),
        );
        let Some(token) = page.get_text("next_cursor") else {
            break;
        };
        page = call(home, &[&revisions[..], &["--cursor", token]].concat());
    }
    let listed: Vec<SdkValue> = (items(&all).iter())
        .map(|i| i.get("commit").unwrap().clone())
        .collect();
    assert_eq!(commits, listed);
    assert!(listed.len() >= 4, "{all:?}");
    // full ids, and the first page pinned at `--at` by a prefix of it
    assert!(
        matches!(&listed[0], SdkValue::Text(id) if id.len() == 66),
        "{listed:?}"
    );
    let prefixed = call(
        home,
        &["file", "history", "q/p", "dev", "/a", "--at", &tip[..12]],
    );
    assert_eq!(items(&prefixed), items(&all));
    // a cursor given another path, traversal or call is refused
    let first = call(home, &revisions);
    let token = first.get_text("next_cursor").unwrap();
    invalid(
        home,
        &[
            "file", "history", "q/p", "dev", "/b", "--limit", "1", "--cursor", token,
        ],
    );
    invalid(
        home,
        &[
            &revisions[..],
            &["--cursor", token, "--traversal", "branch"],
        ]
        .concat(),
    );
    invalid(
        home,
        &["file", "list", "q/p", "dev", "/", "--cursor", token],
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
    // the URL's user is the Store's login, its password on stdin (a
    // trust-auth server takes any); the address is connected without it
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    let address = format!("{scheme}://{address}");
    let out = common::run_with_stdin(
        &mut layerfs(
            &home,
            &[
                "store",
                "connect",
                "q",
                &address,
                "--user",
                user,
                "--password-stdin",
                "--synchronous-commit",
                "off",
            ],
        ),
        b"unused\n",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    check(&home, &source);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}
