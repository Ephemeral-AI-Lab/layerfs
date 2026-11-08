//! Namespace resume follows component ordering through directories and gaps.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_store_service::{
    history::lookup,
    read::{snapshot, walk::Walker},
};

#[test]
fn every_saved_boundary_resumes_the_same_namespace_suffix() {
    let fixture = support::Fixture::new("saved-walk", |source| {
        for dir in ["a/sub", "a-/deep", "b", "z"] {
            std::fs::create_dir_all(source.join(dir)).unwrap();
        }
        for file in ["a/1", "a/sub/2", "a/sub/3", "a-/deep/4", "b/5", "top"] {
            std::fs::write(source.join(file), b"x\n").unwrap();
        }
        12
    });
    let store = &fixture.service.store;
    let stack = lookup::project(store, "installed").unwrap();
    let reader = snapshot::reader(store, &stack);
    let snap = snapshot::resolve(store, &stack, &reader, "main").unwrap();
    let collect = |root, after| {
        let mut walk = Walker::after(&reader, &snap, root, after).unwrap();
        let mut paths = Vec::new();
        while let Some((path, _)) = walk.next().unwrap() {
            paths.push(path);
        }
        paths
    };
    let all = collect("/", None);
    for (index, path) in all.iter().enumerate() {
        assert_eq!(collect("/", Some(path)), all[index + 1..], "after {path}");
    }
    for missing in ["/a/0", "/a/sub/0", "/a/missing/x", "/y/x", "/zz/missing"] {
        let expected: Vec<_> = all
            .iter()
            .filter(|p| p.split('/').cmp(missing.split('/')).is_gt())
            .cloned()
            .collect();
        assert_eq!(
            collect("/", Some(missing)),
            expected,
            "after missing {missing}"
        );
    }
    assert_eq!(collect("/a", Some("/a")), collect("/a", None));
    assert!(collect("/top", Some("/top")).is_empty());
}
