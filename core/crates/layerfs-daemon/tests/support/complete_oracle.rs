//! Independent expected names, portable metadata and every logical byte.
use super::{bytes, fixture};
use layerfs_content::{
    filesystem::{
        attributes::read::{read_portable, AttributeReadWork},
        FilesystemRead, LogicalPath, PathName,
    },
    object::inode_leaf::InodeKind,
    AuthenticatedObjects,
};
use layerfs_daemon::store::StoreOperation;
use layerfs_telemetry::timer::Timing;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

pub fn run(f: &fixture::Fixture, operation: &StoreOperation) {
    let start = Instant::now();
    let base = operation.workspace().base().unwrap();
    let mut reader = FilesystemRead::new(operation.client(), base.identity()).unwrap();
    assert_eq!(
        reader
            .read_portable(&LogicalPath::new("").unwrap())
            .unwrap(),
        f.root_metadata
    );
    match f.shape {
        fixture::Shape::Mixed => mixed(f, &mut reader, operation),
        fixture::Shape::Wide => wide(f, &mut reader, operation),
        fixture::Shape::Dense | fixture::Shape::Sparse => {
            let file = base
                .child(
                    base.root().root_inode().serial(),
                    &PathName::new("file").unwrap(),
                )
                .unwrap();
            let (len, pattern) = if matches!(f.shape, fixture::Shape::Dense) {
                (bytes::DENSE, bytes::Pattern::Dense)
            } else {
                (bytes::SPARSE, bytes::Pattern::Sparse)
            };
            let stat = base.stat(file.serial).unwrap();
            assert_eq!(stat.logical_len, len);
            assert_eq!(stat.metadata, f.file_metadata);
            let mut sink = bytes::Oracle::new(pattern);
            while sink.position < len {
                let n = (len - sink.position).min(bytes::WINDOW as u64) as u32;
                base.plan_read(file.serial, sink.position, n)
                    .unwrap()
                    .emit(&mut sink)
                    .unwrap();
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "full oracle exceeded existing independent-proof budget"
                );
            }
            assert_eq!(sink.position, len);
            assert_eq!(base.plan_read(file.serial, len, 1).unwrap().length(), 0);
            let entries = base
                .list(base.root().root_inode().serial(), None, 2, 1024)
                .unwrap();
            assert_eq!(
                entries.entries,
                vec![(PathName::new("file").unwrap(), file.serial)]
            );
            assert!(entries.continuation.is_none());
            println!(
                "Q1_FULL_BYTES pattern={pattern:?} compared={len} window={} metadata=exact",
                bytes::WINDOW
            );
        }
    }
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "independent proof budget"
    );
    assert!(operation.ports().failure().unwrap().is_none());
    println!(
        "Q1_ORACLE shape={:?} passed=true source_absent=true sealed_absent=true",
        f.shape
    );
}
fn mixed(f: &fixture::Fixture, reader: &mut FilesystemRead<'_>, operation: &StoreOperation) {
    for (path, metadata) in &f.metadata {
        let logical = LogicalPath::new(path).unwrap();
        let inode = reader.resolve(&logical).unwrap();
        assert_eq!(reader.read_portable(&logical).unwrap(), *metadata, "{path}");
        if fixture::DIRS.contains(&path.as_str()) {
            assert_eq!(inode.value.kind, InodeKind::Directory);
            let expected = f
                .metadata
                .iter()
                .filter_map(|(p, _)| {
                    let (parent, name) = p.rsplit_once('/').unwrap_or(("", p));
                    (parent == path && !p.is_empty()).then_some(name.to_owned())
                })
                .collect::<BTreeSet<_>>();
            let mut names = BTreeSet::new();
            let mut after = None;
            loop {
                let page = reader.list(&logical, after.as_ref(), 2, 1024).unwrap();
                for (name, _) in page.entries {
                    assert!(names.insert(name.as_str().to_owned()));
                }
                match page.continuation {
                    Some(next) => after = Some(next),
                    None => break,
                }
            }
            assert_eq!(names, expected, "{path}");
        } else if let Some((_, target)) = fixture::LINKS.iter().find(|(name, _)| *name == path) {
            assert_eq!(inode.value.kind, InodeKind::Symlink);
            assert_eq!(reader.readlink(&logical).unwrap().as_bytes(), *target);
        } else {
            assert_eq!(inode.value.kind, InodeKind::RegularFile);
            let expected = fixture::FILES
                .iter()
                .find(|(name, _)| *name == path)
                .map_or(fixture::FILES[1].1, |(_, body)| *body);
            let mut actual = Vec::new();
            Timing::disabled("q1.small.read", |scope| {
                layerfs_content::read_all(
                    operation.client(),
                    inode.value.content_root,
                    &mut actual,
                    scope.child("file"),
                )
            })
            .0
            .unwrap();
            assert_eq!(actual, expected, "{path}");
        }
    }
    let a = reader
        .resolve(&LogicalPath::new(".git/index").unwrap())
        .unwrap();
    let b = reader
        .resolve(&LogicalPath::new(".cache/index-alias").unwrap())
        .unwrap();
    let c = reader
        .resolve(&LogicalPath::new("output/index-copy").unwrap())
        .unwrap();
    assert_eq!(a.serial, b.serial);
    assert_ne!(a.serial, c.serial);
    assert_eq!(a.value.namespace_ref_count, 2);
    assert_eq!(c.value.namespace_ref_count, 1);
    assert_eq!(a.value.content_root, c.value.content_root);
    assert!(reader
        .resolve(&LogicalPath::new("external").unwrap())
        .is_err());
    println!("Q1_MIXED paths={} hard_links=exact separate_equal_file=distinct raw_targets=exact portable_metadata=all",f.metadata.len());
}
fn wide(f: &fixture::Fixture, reader: &mut FilesystemRead<'_>, operation: &StoreOperation) {
    let root = reader.root().root_inode().serial();
    let mut after = None;
    let mut n = 0;
    let mut windows = 0;
    loop {
        let page = reader.list_inode(root, after.as_ref(), 64, 32768).unwrap();
        let serials = page
            .entries
            .iter()
            .map(|(_, serial)| *serial)
            .collect::<Vec<_>>();
        let values = reader
            .lookup_inodes(&serials)
            .unwrap()
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>();
        let ids = values
            .iter()
            .flat_map(|v| [v.content_root, v.metadata_root])
            .collect::<Vec<_>>();
        operation.client().read_canonical_batch(&ids).unwrap();
        for ((name, _), value) in page.entries.iter().zip(&values) {
            assert_eq!(name.as_str(), fixture::name(n));
            assert_eq!(value.kind, InodeKind::RegularFile);
            assert_eq!(value.namespace_ref_count, 1);
            let metadata = read_portable(
                operation.client(),
                value.metadata_root,
                value.kind,
                &mut AttributeReadWork::default(),
            )
            .unwrap();
            assert_eq!(metadata, f.file_metadata);
            let mut actual = Vec::new();
            Timing::disabled("q1.wide.read", |scope| {
                layerfs_content::read_all(
                    operation.client(),
                    value.content_root,
                    &mut actual,
                    scope.child("file"),
                )
            })
            .0
            .unwrap();
            assert_eq!(actual, fixture::body(n));
            n += 1;
        }
        windows += 1;
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    assert_eq!(n, fixture::NAMES);
    println!("Q1_WIDE files={n} windows={windows} window=64 all_names_bytes_metadata=exact batched_inode_and_content=true");
}
