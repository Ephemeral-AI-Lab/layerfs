//! `branch.list` pages past the old 64-item window and the default page of
//! 50, plain and under a folder scope: every page boundary is crossed, the
//! last page says so, and a bad page is refused (#30, #33). The Store's
//! paging is real: one Disposable home, Branches made by `branch.fork`.
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
            .map(|i| i.get_text("name").unwrap().to_owned())
            .collect(),
        other => panic!("no items: {other:?}"),
    }
}

/// Every page of `branch.list` at `size` (None: the default page), from the
/// first: the pages' names, with each page's `has_more` and `next_page_pos`
/// checked as it goes (the position moves strictly on; the last page has no
/// next).
fn walk(home: &Path, scope: Option<&Scope>, size: Option<i64>) -> Vec<Vec<String>> {
    let (mut pages, mut pos) = (Vec::new(), 0i64);
    loop {
        let mut args = vec![p()];
        if let Some(size) = size {
            args.push(("page_size", SdkValue::Int(size)));
        }
        if pos > 0 {
            args.push(("page_pos", SdkValue::Int(pos)));
        }
        let page = call(home, scope, "branch.list", &args).unwrap_or_else(|e| panic!("{e:?}"));
        let more = page.get("has_more") == Some(&SdkValue::Bool(true));
        pages.push(names(&page));
        match (more, page.get_int("next_page_pos")) {
            (true, Some(next)) => {
                assert!(next > pos, "page {} did not move on: {next}", pages.len());
                pos = next;
            }
            (false, None) => return pages,
            other => panic!("has_more and next_page_pos disagree: {other:?} {page:?}"),
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
                    ("from_branch", t("main")),
                ],
            );
        }
    }
    (home, folder)
}

#[test]
fn a_list_pages_past_the_old_64_and_the_default_50() {
    let (home, folder) = project();
    // the whole list at the largest page: main and 173 Branches (the reply
    // budget may still cut a page short, so the pages are joined)
    let every = walk(&home, None, Some(1000)).concat();
    assert_eq!(every.len(), 174, "{every:?}");
    // the default page is 50, and paging by it reaches every Branch once
    let by_default = walk(&home, None, None);
    assert_eq!(by_default[0].len(), 50);
    assert_eq!(by_default.len(), 4, "174 = 3 x 50 + 24");
    assert_eq!(by_default.concat(), every, "same items, same order");
    // sizes around the old 64 window and the default 50
    for size in [1, 49, 50, 51, 63, 64, 65, 173, 174, 175] {
        let pages = walk(&home, None, Some(size));
        assert_eq!(pages.concat(), every, "page_size {size}");
        assert!(pages
            .iter()
            .all(|page| !page.is_empty() && page.len() as i64 <= size));
        if size <= 65 {
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
    // default page: 50 then 20; the namespace's names are relative
    let by_default = walk(&home, Some(&scoped), None);
    assert_eq!(
        by_default.iter().map(Vec::len).collect::<Vec<_>>(),
        [50, 20]
    );
    assert_eq!(by_default.concat(), expected);
    for size in [1, 49, 50, 51, 63, 64, 65, 69, 70, 71, 1000] {
        let pages = walk(&home, Some(&scoped), Some(size));
        assert_eq!(pages.concat(), expected, "page_size {size}");
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
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_bad_page_is_refused_and_a_page_past_the_end_is_empty() {
    let (home, folder) = project();
    let scoped = Scope::parse(&map(&[
        ("project", t("shop/api")),
        ("folder", t("/web")),
        ("branch", t("agent")),
    ]))
    .unwrap();
    for scope in [None, Some(&scoped)] {
        for (what, args) in [
            ("page_size 0", vec![("page_size", SdkValue::Int(0))]),
            ("page_size 1001", vec![("page_size", SdkValue::Int(1001))]),
            ("page_size -1", vec![("page_size", SdkValue::Int(-1))]),
            ("page_pos -1", vec![("page_pos", SdkValue::Int(-1))]),
            ("page_pos text", vec![("page_pos", t("next"))]),
            ("page_size text", vec![("page_size", t("50"))]),
        ] {
            let mut given = vec![p()];
            given.extend(args);
            let refused = call(&home, scope, "branch.list", &given).unwrap_err();
            assert_eq!(refused.0, "InvalidInput", "{what}: {refused:?}");
        }
        // past the end: an empty last page, not an error
        let past = call(
            &home,
            scope,
            "branch.list",
            &[p(), ("page_pos", SdkValue::Int(100_000))],
        )
        .unwrap();
        assert!(names(&past).is_empty(), "{past:?}");
        assert_ne!(
            past.get("has_more"),
            Some(&SdkValue::Bool(true)),
            "{past:?}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
