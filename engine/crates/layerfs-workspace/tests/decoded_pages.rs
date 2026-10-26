//! Decoded inode and directory pages kept by the shared cache (#364 item 3):
//! a kept page skips the copy and decode, never an answer or a check.
mod common;

use common::name;
use layerfs_content::filesystem::inode::{lookup, InodeReadWork, InodeTable};
use layerfs_workspace::{BaseView, CanonicalCache, CanonicalClient, ClientWork};
use std::sync::Arc;

fn client(f: &common::Fixture, bytes: usize, decoded: usize) -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::with_cache(
        Arc::new(f.store.clone()),
        Some(Arc::new(f.store.clone())),
        Arc::new(CanonicalCache::with_decoded(bytes, decoded)),
    ))
}
fn work(client: &CanonicalClient) -> ClientWork {
    client.diagnostics().unwrap()
}
const NAMES: [&str; 7] = [
    ".git",
    "alias",
    "cache",
    "file",
    "node_modules",
    "output",
    "symlink",
];

#[test]
fn a_kept_page_answers_without_a_canonical_acquisition() {
    let f = common::fixture_with(Vec::new());
    let client = client(&f, 1 << 20, 1 << 20);
    let base = BaseView::open(client.clone(), f.root, f.scope).unwrap();
    let first = base.child(1, &name("file")).unwrap();
    let before = work(&client);
    assert!(before.decoded.misses > 0);
    assert_eq!(before.decoded.pages as u64, before.decoded.misses);
    let again = base.child(1, &name("file")).unwrap();
    let after = work(&client);
    assert_eq!(again.serial, first.serial);
    assert_eq!(again.value, first.value);
    assert!(after.decoded.hits > before.decoded.hits);
    assert_eq!(after.decoded.misses, before.decoded.misses);
    assert_eq!(
        (after.cache_hits, after.cache_misses),
        (before.cache_hits, before.cache_misses)
    );
}

#[test]
fn every_answer_is_the_same_with_and_without_kept_pages() {
    let f = common::fixture_with(vec![7; 300_000]);
    let kept = BaseView::open(client(&f, 1 << 20, 1 << 20), f.root, f.scope).unwrap();
    let plain = BaseView::open(client(&f, 1 << 20, 0), f.root, f.scope).unwrap();
    // Twice, so the second pass of `kept` answers from kept pages.
    for _ in 0..2 {
        for serial in 1..=9 {
            let (a, b) = (kept.inode(serial), plain.inode(serial));
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
            if let (Ok(a), Ok(b)) = (kept.stat(serial), plain.stat(serial)) {
                assert_eq!(
                    (a.value, a.metadata, a.logical_len),
                    (b.value, b.metadata, b.logical_len)
                );
            }
        }
        for parent in [1, 4, 5, 6, 7] {
            for text in NAMES.iter().chain(&["index", "pkg", "absent"]) {
                let (a, b) = (
                    kept.child(parent, &name(text)),
                    plain.child(parent, &name(text)),
                );
                assert_eq!(format!("{a:?}"), format!("{b:?}"));
            }
            let directory = kept.inode(parent).unwrap().value;
            assert_eq!(
                kept.entries(directory).unwrap(),
                plain.entries(directory).unwrap()
            );
            let (a, b) = (
                kept.list(parent, None, 2, 4096),
                plain.list(parent, None, 2, 4096),
            );
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

#[test]
fn the_budget_holds_its_charge_and_evicts_the_oldest() {
    let f = common::fixture_with(Vec::new());
    // Room for about one small page.
    const BUDGET: usize = 600;
    let kept = client(&f, 1 << 20, BUDGET);
    let base = BaseView::open(kept.clone(), f.root, f.scope).unwrap();
    for _ in 0..3 {
        for parent in [1, 4, 5, 6, 7] {
            base.list(parent, None, 16, 4096).unwrap();
            base.inode(parent).unwrap();
        }
    }
    let decoded = work(&kept).decoded;
    assert!(decoded.charged <= BUDGET);
    assert!(decoded.evictions > 0);
    assert!(decoded.pages >= 1);
    // Nothing kept: every page is decoded.
    let none = client(&f, 1 << 20, 0);
    let base = BaseView::open(none.clone(), f.root, f.scope).unwrap();
    base.list(1, None, 16, 4096).unwrap();
    base.list(1, None, 16, 4096).unwrap();
    let decoded = work(&none).decoded;
    assert_eq!((decoded.hits, decoded.pages, decoded.charged), (0, 0, 0));
    assert!(decoded.misses >= 2);
}

/// A directory page kept from a directory read is not an inode page: an
/// inode read of it is refused exactly as without the cache.
#[test]
fn a_kept_page_never_answers_a_read_of_another_grammar() {
    let f = common::fixture_with(Vec::new());
    let read = |decoded: usize| {
        let reader = client(&f, 1 << 20, decoded);
        let base = BaseView::open(reader.clone(), f.root, f.scope).unwrap();
        let directory = base.inode(1).unwrap().value.content_root;
        base.list(1, None, 16, 4096).unwrap();
        let table = InodeTable {
            root: directory,
            root_serial: 1,
        };
        let answer = lookup(reader.as_ref(), table, 2, &mut InodeReadWork::default());
        format!("{answer:?}")
    };
    let (kept, plain) = (read(1 << 20), read(0));
    assert!(kept.starts_with("Err"), "{kept}");
    assert_eq!(kept, plain);
}
