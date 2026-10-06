//! Backed acquisition yields the root the whole-namespace constructor derives.
#![cfg(unix)]
mod support;
use layerfs_content::filesystem::{scope_for_seed, FilesystemRootId};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    build_filesystem, DirectoryUpdate, DiscardingConsumer, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, InodeUpdate, LogicalPath,
};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, MetadataExt},
    },
    path::PathBuf,
    sync::Arc,
};
use support::{
    memory_acquisition::MemoryAcquisition, memory_history::MemoryHistory,
    memory_metadata::MemoryMetadata, Fixture,
};

const WIDE: usize = 1300;

/// Acquisition order the contract fixes: breadth first, children by name bytes,
/// later paths of one native regular file sharing its first position.
fn expected_positions(fixture: &Fixture) -> BTreeMap<String, u64> {
    let mut positions = BTreeMap::from([(String::new(), 0)]);
    let mut first = BTreeMap::new();
    let mut pending = VecDeque::from([(fixture.source.clone(), String::new())]);
    let mut next = 1;
    while let Some((directory, logical)) = pending.pop_front() {
        let mut children: Vec<PathBuf> = fs::read_dir(&directory)
            .unwrap()
            .map(|child| child.unwrap().path())
            .collect();
        children.sort_by(|a, b| {
            let (a, b) = (a.file_name().unwrap(), b.file_name().unwrap());
            a.as_bytes().cmp(b.as_bytes())
        });
        for child in children {
            let name = child.file_name().unwrap().to_str().unwrap();
            let path = if logical.is_empty() {
                name.to_owned()
            } else {
                format!("{logical}/{name}")
            };
            let metadata = fs::symlink_metadata(&child).unwrap();
            let position = if metadata.is_file() {
                *first
                    .entry((metadata.dev(), metadata.ino()))
                    .or_insert(next)
            } else {
                next
            };
            next += 1;
            positions.insert(path.clone(), position);
            if metadata.is_dir() {
                pending.push_back((child, path));
            }
        }
    }
    positions
}

#[test]
fn wide_nested_aliased_root_equals_the_whole_namespace_constructor() {
    let fixture = Fixture::new(0);
    let source = &fixture.source;
    for directory in ["wide/zz-dir", "empty", "a/b/c", "a/empty-too"] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    // Created in descending order: name order is the backing's, not the host's.
    for n in (0..WIDE).rev() {
        let body = vec![(n % 251) as u8; n % 97 + 1];
        fs::write(source.join(format!("wide/n{n:05}")), body).unwrap();
    }
    fs::write(source.join("wide/zz-dir/inner"), b"inner").unwrap();
    fs::write(source.join("a/b/c/file"), b"deep").unwrap();
    symlink("n00003", source.join("wide/aa-link")).unwrap();
    symlink("../missing", source.join("a/dangling")).unwrap();
    fs::hard_link(source.join("wide/n00007"), source.join("a/alias")).unwrap();
    fs::hard_link(source.join("wide/n00007"), source.join("a/b/alias-two")).unwrap();
    let expected = expected_positions(&fixture);

    let store = support::storage(Arc::new(MemoryMetadata::default()));
    let history = MemoryHistory::default();
    let initialized = fixture.run(&store, &history);
    let work = initialized.namespace_work;
    assert_eq!(initialized.entries as usize, expected.len());
    assert_eq!(work.entries, expected.len());
    assert_eq!(work.directory_children, WIDE + 2);
    assert_eq!(work.regular_aliases, 2);
    assert_eq!(work.unique_files, WIDE + 2);
    assert_eq!(work.directory_bindings, expected.len() - 1);
    assert_eq!(work.wide_directories, 1);
    assert_eq!(work.child_window_rows, 512);
    // One working row per entry and one per native identity, all removed.
    assert_eq!(work.backing_rows, (expected.len() + WIDE + 2) as u64);
    assert!(work.backing_bytes > 0);
    let left: Vec<_> = fs::read_dir(&fixture.path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, [std::ffi::OsString::from("input")]);

    // The same source through read windows of 37 rows resumes every stream
    // many times and derives the identical root.
    let narrow = MemoryAcquisition::with_read_rows(37);
    let other = support::storage(Arc::new(MemoryMetadata::default()));
    let again = fixture.run_with(&other, &MemoryHistory::default(), &narrow);
    assert_eq!(again.root, initialized.root);
    assert_eq!(narrow.largest_windows().0, 37);
    assert!(narrow.operations().is_empty());
    assert!(again.namespace_work.read_units > 5 * work.read_units);

    // Read the published tree back and restate it as one whole-namespace input.
    let reader = store.reader().unwrap();
    let mut view = FilesystemRead::new(&reader, FilesystemRootId(initialized.root)).unwrap();
    let root_serial = initialized.root_serial;
    for (path, position) in &expected {
        let resolved = view.resolve(&LogicalPath::new(path).unwrap()).unwrap();
        assert_eq!(
            resolved.serial,
            root_serial + position,
            "position of {path}"
        );
    }
    let alias = view
        .resolve(&LogicalPath::new("a/b/alias-two").unwrap())
        .unwrap();
    assert_eq!(alias.value.kind, InodeKind::RegularFile);
    assert_eq!(alias.value.namespace_ref_count, 3);
    let empty = view.resolve(&LogicalPath::new("empty").unwrap()).unwrap();
    let other = view
        .resolve(&LogicalPath::new("a/empty-too").unwrap())
        .unwrap();
    assert_eq!(empty.value.content_root, other.value.content_root);
    assert_ne!(empty.serial, other.serial);

    let mut values: BTreeMap<u64, InodeValue> = BTreeMap::new();
    let mut directories = Vec::new();
    let mut pending = VecDeque::from([root_serial]);
    values.insert(root_serial, view.resolve_inode(root_serial).unwrap().value);
    while let Some(parent) = pending.pop_front() {
        let mut changes = Vec::new();
        let mut after = None;
        loop {
            let page = view
                .list_inode(parent, after.as_ref(), 100, 1 << 16)
                .unwrap();
            for (name, serial) in page.entries {
                let value = view.resolve_inode(serial).unwrap().value;
                if values.insert(serial, value).is_none() && value.kind == InodeKind::Directory {
                    pending.push_back(serial);
                }
                changes.push((name, Some(serial)));
            }
            match page.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        directories.push(DirectoryUpdate { parent, changes });
    }
    directories.sort_by_key(|directory| directory.parent);
    let wide = view.resolve(&LogicalPath::new("wide").unwrap()).unwrap();
    let listed = directories
        .iter()
        .find(|directory| directory.parent == wide.serial)
        .unwrap();
    assert_eq!(listed.changes.len(), WIDE + 2);
    let serials: Vec<u64> = values.keys().copied().collect();
    let inodes: Vec<InodeUpdate> = values
        .iter()
        .map(|(serial, value)| InodeUpdate {
            serial: *serial,
            value: InodeValue {
                namespace_ref_count: 0,
                // The constructor must derive every directory root and count.
                content_root: match value.kind {
                    InodeKind::Directory => value.metadata_root,
                    _ => value.content_root,
                },
                ..*value
            },
        })
        .collect();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([29; 32]),
        root_serial,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &serials,
        resources: FilesystemResources::default(),
    };
    let mut discard = DiscardingConsumer::new();
    let rebuilt = {
        let mut objects = FilesystemObjects::new(&reader, &mut discard);
        build_filesystem(&mut objects, &input, None).unwrap()
    };
    assert_eq!(rebuilt.root.0, initialized.root);
    println!(
        "S9_BACKED_ACQUISITION entries={} work={work:?}",
        initialized.entries
    );
}

/// Children that fill many read and write windows before the one directory
/// is placed, with target bytes far larger than any window could hold.
const VERY_WIDE: usize = 17_000;

#[test]
fn very_wide_directory_is_ordered_through_many_windows() {
    let fixture = Fixture::new(0);
    let wide = fixture.source.join("wide");
    fs::create_dir(&wide).unwrap();
    // Opaque dangling targets near the host path limit; never traversed.
    let long = "missing/".repeat(120);
    for n in 0..VERY_WIDE {
        let name = (n * 7919) % VERY_WIDE;
        symlink(format!("{long}{name}"), wide.join(format!("s{name:05}"))).unwrap();
    }
    let store = support::storage(Arc::new(MemoryMetadata::default()));
    let history = MemoryHistory::default();
    let initialized = fixture.run(&store, &history);
    let work = initialized.namespace_work;
    assert_eq!(initialized.entries as usize, VERY_WIDE + 2);
    assert_eq!(work.directory_children, VERY_WIDE);
    assert_eq!(work.directory_bindings, VERY_WIDE + 1);
    assert_eq!((work.jobs, work.unique_files), (0, 0));
    assert_eq!(work.wide_directories, 1);
    assert_eq!(work.backing_rows, VERY_WIDE as u64 + 2);
    assert!(work.read_window_rows <= 512 && work.write_window_rows <= 4096);
    assert!(work.write_window_bytes <= 1024 * 1024);

    let reader = store.reader().unwrap();
    let mut view = FilesystemRead::new(&reader, FilesystemRootId(initialized.root)).unwrap();
    let parent = view.resolve(&LogicalPath::new("wide").unwrap()).unwrap();
    assert_eq!(parent.serial, initialized.root_serial + 1);
    let (mut after, mut listed) = (None, 0usize);
    loop {
        let page = view
            .list_inode(parent.serial, after.as_ref(), 500, 1 << 16)
            .unwrap();
        for (name, serial) in page.entries {
            assert_eq!(name.as_bytes(), format!("s{listed:05}").as_bytes());
            assert_eq!(serial, parent.serial + 1 + listed as u64);
            listed += 1;
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    assert_eq!(listed, VERY_WIDE);
    let last = format!("wide/s{:05}", VERY_WIDE - 1);
    let last = view.resolve(&LogicalPath::new(&last).unwrap()).unwrap();
    assert_eq!(last.value.kind, InodeKind::Symlink);
    println!("S9_BACKED_ACQUISITION_WIDE work={work:?}");
}
