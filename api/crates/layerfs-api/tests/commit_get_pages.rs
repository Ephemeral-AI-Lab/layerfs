//! `commit.list` pages a Branch's Commits to the end by its cursor, and
//! `commit.get` reads any of them by id or prefix without naming the
//! Branch; an absent Commit or Branch is refused `NotFound` (their `NoSuch*`
//! codes fold into it since the call surface 2.0). #533 review, group calls.
use layerfs_api::home;
use layerfs_protocol::provision::install::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-commit-get-{tag}-{}-{}",
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

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    layerfs_api::call(Some(home), name, map(args), &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn items(value: &SdkValue) -> Vec<SdkValue> {
    match value.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("no items: {other:?}"),
    }
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
fn commit_list_pages_to_the_end_and_an_absent_commit_or_branch_is_not_found() {
    let (home, folder) = project();
    let p = || ("project", t("shop/api"));
    let detail = || ("detail", SdkValue::Bool(true));
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("fix")), ("from", t("main"))],
    );
    let mut oldest = String::new();
    for n in 0..60 {
        ok(
            &home,
            "file.write",
            &[
                p(),
                ("branch", t("fix")),
                ("path", t(&format!("/f{n}"))),
                ("content", t("x\n")),
                ("message", t(&format!("write {n}"))),
            ],
        );
        if n == 0 {
            let first = ok(&home, "commit.list", &[p(), ("branch", t("fix"))]);
            oldest = items(&first)[0].get_text("commit").unwrap().to_owned();
        }
    }
    assert_eq!(oldest.len(), 66, "a full id");
    // pages of 25 by the cursor: 25, 25, 10, newest first, the last with no
    // next; writes after the first page do not move what the later ones list
    let (mut pages, mut cursor) = (Vec::<Vec<String>>::new(), None::<String>);
    loop {
        let mut args = vec![p(), ("branch", t("fix")), ("limit", SdkValue::Int(25))];
        if let Some(cursor) = &cursor {
            args.push(("cursor", t(cursor)));
        }
        let page = ok(&home, "commit.list", &args);
        let rows = items(&page);
        pages.push(
            rows.iter()
                .map(|c| c.get_text("commit").unwrap().to_owned())
                .collect(),
        );
        if pages.len() == 1 {
            ok(
                &home,
                "file.write",
                &[
                    p(),
                    ("branch", t("fix")),
                    ("path", t("/late")),
                    ("content", t("y\n")),
                ],
            );
        }
        match page.get_text("next_cursor") {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
        assert!(pages.len() < 10, "a list that never ends");
    }
    assert_eq!(pages.iter().map(Vec::len).collect::<Vec<_>>(), [25, 25, 10]);
    assert_eq!(pages[2].last(), Some(&oldest));
    // the cursor is this Branch's: another Branch with it is refused, and
    // so is another `--at`
    let cursor = cursor.expect("the second page's cursor");
    for other in [("branch", t("main")), ("at", t(&oldest))] {
        let mut args = vec![p(), ("branch", t("fix")), ("cursor", t(&cursor))];
        args.retain(|(name, _)| *name != other.0);
        args.push(other);
        let refused = call(&home, "commit.list", &args);
        assert_eq!(refused.unwrap_err().0, "InvalidInput");
    }
    // by id or a prefix, with no Branch named
    for given in [oldest.as_str(), &oldest[..8], &oldest[..12]] {
        let found = ok(&home, "commit.get", &[p(), ("commit", t(given)), detail()]);
        assert_eq!(found.get_text("commit"), Some(oldest.as_str()), "{given}");
        assert_eq!(found.get_text("branch"), Some("fix"));
        assert_eq!(found.get_text("message"), Some("write 0"));
    }
    let short = ok(&home, "commit.get", &[p(), ("commit", t(&oldest[..8]))]);
    assert_eq!(short.get_text("branch"), Some("fix"), "{short:?}");
    // `--branch` must hold it
    ok(
        &home,
        "commit.get",
        &[p(), ("commit", t(&oldest)), ("branch", t("fix"))],
    );
    let refused = call(
        &home,
        "commit.get",
        &[p(), ("commit", t(&oldest)), ("branch", t("main"))],
    );
    assert_eq!(refused.unwrap_err().0, "NotFound");
    let refused = call(&home, "commit.get", &[p(), ("commit", t(&oldest[..7]))]);
    assert_eq!(refused.unwrap_err().0, "InvalidInput");

    let nosuch = format!("{}{}", &oldest[..2], "f".repeat(64));
    let refused = call(&home, "commit.get", &[p(), ("commit", t(&nosuch))]);
    assert_eq!(refused.unwrap_err().0, "NotFound");
    let refused = call(&home, "branch.get", &[p(), ("branch", t("nosuch"))]);
    assert_eq!(refused.unwrap_err().0, "NotFound");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
