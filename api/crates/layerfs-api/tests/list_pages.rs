//! `branch.list` pages by its cursor past the old 64-item window and the
//! default page of 100, plain and under a folder scope: every page boundary
//! is crossed, the last page has no `next_cursor`, and a bad limit or cursor
//! is refused (#30, #33; 2.0 call surface). The Store's paging is real: one
//! Normal home, Branches made by `branch.fork`.
use layerfs_api::home;
use layerfs_api::Scope;
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-list-pages-{tag}-{}-{}",
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

fn names(page: &SdkValue) -> Vec<String> {
    match page.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|i| i.get_text("branch").unwrap().to_owned())
            .collect(),
        other => panic!("no items: {other:?}"),
    }
}

/// Every page of `branch.list` at `size` (None: the default page), from the
/// first, each next page by the cursor the page before answered; the last
/// page has none.
fn walk(home: &Path, scope: Option<&Scope>, size: Option<i64>) -> Vec<Vec<String>> {
    let (mut pages, mut cursor) = (Vec::new(), None::<String>);
    loop {
        let mut args = vec![p()];
        if let Some(size) = size {
            args.push(("limit", SdkValue::Int(size)));
        }
        if let Some(cursor) = &cursor {
            args.push(("cursor", t(cursor)));
        }
        let page = call(home, scope, "branch.list", &args).unwrap_or_else(|e| panic!("{e:?}"));
        pages.push(names(&page));
        match page.get_text("next_cursor") {
            Some(next) => {
                assert_ne!(
                    cursor.as_deref(),
                    Some(next),
                    "page {} did not move on",
                    pages.len()
                );
                cursor = Some(next.to_owned());
            }
            None => return pages,
        }
        assert!(pages.len() < 1000, "a list that never ends");
    }
}

/// A home with project `shop/api`, 100 Branches `a.NNN` that sort before the
/// namespace, then 70 `agent.NNN` (the scope's), then 3 `z.NNN`.
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
    for (prefix, count) in [("a", 100), ("agent", 70), ("z", 3)] {
        for n in 0..count {
            ok(
                &home,
                "branch.fork",
                &[
                    p(),
                    ("name", t(&format!("{prefix}.{n:03}"))),
                    ("from", t("main")),
                ],
            );
        }
    }
    (home, folder)
}

#[test]
fn a_list_pages_past_the_old_64_and_the_default_100() {
    let (home, folder) = project();
    // the whole list at the largest page: main and 173 Branches (the reply
    // budget may still cut a page short, so the pages are joined)
    let every = walk(&home, None, Some(1000)).concat();
    assert_eq!(every.len(), 174, "{every:?}");
    // the default page is 100, and paging by it reaches every Branch once
    let by_default = walk(&home, None, None);
    assert_eq!(
        by_default.iter().map(Vec::len).collect::<Vec<_>>(),
        [100, 74]
    );
    assert_eq!(by_default.concat(), every, "same items, same order");
    // sizes around the old 64 window and the default 100
    for size in [1, 49, 50, 51, 63, 64, 65, 99, 100, 101, 173, 174, 175] {
        let pages = walk(&home, None, Some(size));
        assert_eq!(pages.concat(), every, "limit {size}");
        assert!(pages
            .iter()
            .all(|page| !page.is_empty() && page.len() as i64 <= size));
        // a page also ends early at 64 KiB of rows
        if size <= 101 {
            let full = pages.len() - 1;
            assert!(pages[..full].iter().all(|page| page.len() as i64 == size));
        }
    }
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_scoped_list_pages_its_namespace_across_the_others() {
    let (home, folder) = project();
    let scoped = Scope::parse(&map(&[
        ("project", t("shop/api")),
        ("folder", t("/web")),
        ("branch", t("agent")),
    ]))
    .unwrap();
    let expected: Vec<String> = (0..70).map(|n| format!("{n:03}")).collect();
    // one default page; the namespace's names are relative
    let by_default = walk(&home, Some(&scoped), None);
    assert_eq!(by_default, [expected.clone()]);
    for size in [1, 49, 50, 51, 63, 64, 65, 69, 70, 71, 1000] {
        let pages = walk(&home, Some(&scoped), Some(size));
        assert_eq!(pages.concat(), expected, "limit {size}");
        let full = pages.len() - 1;
        assert!(pages[..full].iter().all(|page| page.len() as i64 == size));
    }
    // a namespace with no Branch is one empty final page
    let none = Scope::parse(&map(&[
        ("project", t("shop/api")),
        ("folder", t("/web")),
        ("branch", t("nothing")),
    ]))
    .unwrap();
    assert_eq!(walk(&home, Some(&none), None), [Vec::<String>::new()]);
    // a prefix narrows a page; the cursor carries it
    let page = ok(
        &home,
        "branch.list",
        &[p(), ("prefix", t("z.")), ("limit", SdkValue::Int(2))],
    );
    assert_eq!(names(&page), ["z.000", "z.001"]);
    let cursor = page.get_text("next_cursor").unwrap().to_owned();
    let rest = ok(&home, "branch.list", &[p(), ("cursor", t(&cursor))]);
    assert_eq!(names(&rest), ["z.002"]);
    let kept = ok(
        &home,
        "branch.list",
        &[p(), ("exclude_prefix", t("a")), ("limit", SdkValue::Int(3))],
    );
    assert_eq!(names(&kept), ["main", "z.000", "z.001"]);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_bad_limit_or_cursor_is_refused_and_a_cursor_past_the_end_is_empty() {
    let (home, folder) = project();
    let scoped = Scope::parse(&map(&[
        ("project", t("shop/api")),
        ("folder", t("/web")),
        ("branch", t("agent")),
    ]))
    .unwrap();
    let first = ok(
        &home,
        "branch.list",
        &[p(), ("prefix", t("a.")), ("limit", SdkValue::Int(1))],
    );
    let made = first.get_text("next_cursor").unwrap().to_owned();
    let other_call = layerfs_home::cursor::make("commit.list", t("a.000"), &[]);
    for scope in [None, Some(&scoped)] {
        for (what, args) in [
            ("limit 0", vec![("limit", SdkValue::Int(0))]),
            ("limit 1001", vec![("limit", SdkValue::Int(1001))]),
            ("limit -1", vec![("limit", SdkValue::Int(-1))]),
            ("limit text", vec![("limit", t("50"))]),
            ("a cursor that is no cursor", vec![("cursor", t("next"))]),
            ("another call's cursor", vec![("cursor", t(&other_call))]),
            (
                "a cursor with another prefix",
                vec![("cursor", t(&made)), ("prefix", t("z."))],
            ),
        ] {
            let mut given = vec![p()];
            given.extend(args);
            let refused = call(&home, scope, "branch.list", &given).unwrap_err();
            assert_eq!(refused.0, "InvalidInput", "{what}: {refused:?}");
        }
        // past the end: an empty last page, not an error
        let past = layerfs_home::cursor::make("branch.list", t("zzz"), &[]);
        let past = call(&home, scope, "branch.list", &[p(), ("cursor", t(&past))]).unwrap();
        assert!(names(&past).is_empty(), "{past:?}");
        assert_eq!(past.get_text("next_cursor"), None, "{past:?}");
    }
    // the same filter given again is accepted
    let again = ok(
        &home,
        "branch.list",
        &[
            p(),
            ("cursor", t(&made)),
            ("prefix", t("a.")),
            ("limit", SdkValue::Int(1)),
        ],
    );
    assert_eq!(names(&again), ["a.001"]);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
