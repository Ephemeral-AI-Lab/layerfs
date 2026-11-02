//! `commit.get` pages a Branch's Commits to the end, past the reply budget
//! of one page; `branch.get` and `commit.get` keep 1.1.0's `NoSuch*` codes
//! (S §2.9). #533 review, group calls.
use layerfs_api::calls;
use layerfs_api::home;
use layerfs_bridge::control::SdkValue;
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
    calls::call(Some(home), name, map(args), &mut |_| {}).map_err(|r| (r.code, r.message))
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
fn commit_get_pages_to_the_end_and_codes_stay_no_such() {
    let (home, folder) = project();
    let p = || ("project", t("shop/api"));
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("fix")), ("from_branch", t("main"))],
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
            oldest = items(&first)[0].get_text("id").unwrap().to_owned();
        }
    }
    // the 60th newest Commit is past the first page (the reply budget)
    let page = ok(
        &home,
        "commit.list",
        &[
            p(),
            ("branch", t("fix")),
            ("page_size", SdkValue::Int(1000)),
        ],
    );
    assert!(items(&page).len() < 60, "{}", items(&page).len());
    assert!(!items(&page)
        .iter()
        .any(|c| c.get_text("id") == Some(oldest.as_str())));
    let found = ok(
        &home,
        "commit.get",
        &[p(), ("branch", t("fix")), ("commit", t(&oldest))],
    );
    assert_eq!(found.get_text("id"), Some(oldest.as_str()));
    let short = ok(
        &home,
        "commit.get",
        &[p(), ("branch", t("fix")), ("commit", t(&oldest[..12]))],
    );
    assert_eq!(short.get_text("id"), Some(oldest.as_str()));

    let nosuch = "f".repeat(66);
    let refused = call(
        &home,
        "commit.get",
        &[p(), ("branch", t("fix")), ("commit", t(&nosuch))],
    );
    assert_eq!(refused.unwrap_err().0, "NoSuchCommit");
    let refused = call(&home, "branch.get", &[p(), ("branch", t("nosuch"))]);
    assert_eq!(refused.unwrap_err().0, "NoSuchBranch");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
