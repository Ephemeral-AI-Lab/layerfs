//! Family 5 public native controls; prepared fixtures and explicit cleanup.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

type Tree = BTreeMap<Vec<u8>, Option<Vec<u8>>>;

fn dimension(name: &str) -> usize {
    std::env::var(name).unwrap().parse().unwrap()
}
fn oracle(extra: usize, deep: bool, unrelated: usize) -> Tree {
    let mut tree = Tree::new();
    for path in [
        "",
        "packages",
        "packages/old",
        "packages/new",
        "packages/old/subtree",
        "packages/old/subtree/child",
    ] {
        tree.insert(path.as_bytes().to_vec(), None);
    }
    tree.insert(
        b"packages/old/subtree/child/grand.txt".to_vec(),
        Some(b"grand-base".to_vec()),
    );
    tree.insert(
        b"packages/old/subtree/sibling.txt".to_vec(),
        Some(b"sibling-base".to_vec()),
    );
    for index in 0..extra {
        tree.insert(
            format!("packages/old/subtree/child/extra{index:03}.txt").into_bytes(),
            Some(b"x".to_vec()),
        );
    }
    if deep {
        let mut path = b"src".to_vec();
        tree.insert(path.clone(), None);
        for _ in 0..15 {
            path.push(b'/');
            path.extend_from_slice(&[b'd'; 250]);
            tree.insert(path.clone(), None);
        }
        path.extend_from_slice(b"/leaf");
        tree.insert(path, Some(b"deep-base".to_vec()));
    }
    if unrelated != 0 {
        tree.insert(b"unrelated".to_vec(), None);
        for index in 0..unrelated {
            tree.insert(
                format!("unrelated/f{index:03}").into_bytes(),
                Some(b"u".to_vec()),
            );
        }
    }
    tree
}
fn children(tree: &Tree, path: &[u8]) -> BTreeSet<Vec<u8>> {
    tree.keys()
        .filter_map(|name| {
            let suffix = if path.is_empty() {
                name.as_slice()
            } else {
                name.strip_prefix(path)?.strip_prefix(b"/")?
            };
            (!suffix.is_empty() && !suffix.contains(&b'/')).then(|| suffix.to_vec())
        })
        .collect()
}
fn canonical(f: &Fixture, root: Root, tree: &Tree) {
    for (path, bytes) in tree {
        let Response::Attributes { kind, mode, .. } = f.attributes(root, path) else {
            panic!("canonical attributes")
        };
        assert_eq!(
            (kind, mode),
            if bytes.is_some() {
                (1, 0o644)
            } else {
                (2, 0o755)
            }
        );
        if let Some(bytes) = bytes {
            assert_eq!(f.content_bytes(root, path), *bytes);
        } else {
            let mut after = Vec::new();
            let mut names = BTreeSet::new();
            loop {
                let Response::List {
                    entries,
                    continuation,
                } = f
                    .inspect(
                        root,
                        Inspect::List {
                            path: path.clone(),
                            after,
                            entries: 128,
                            bytes: 16384,
                        },
                    )
                    .unwrap()
                else {
                    panic!("canonical listing")
                };
                for (name, _) in entries {
                    assert!(names.insert(name));
                }
                match continuation {
                    Some(next) => after = next,
                    None => break,
                }
            }
            assert_eq!(names, children(tree, path));
        }
    }
}
fn lookup(f: &Fixture, path: &[u8], serials: &mut Vec<u64>) -> u64 {
    let mut serial = f.workspace.root().serial;
    for component in path
        .split(|byte| *byte == b'/')
        .filter(|name| !name.is_empty())
    {
        serial = f.lookup(serial, component).serial;
        serials.push(serial);
    }
    serial
}
fn pinned(f: &Fixture, token: &[u8; 33], tree: &Tree) {
    for (path, bytes) in tree {
        let mut serial = f.workspace.root().serial;
        for component in path
            .split(|byte| *byte == b'/')
            .filter(|name| !name.is_empty())
        {
            serial = f
                .workspace
                .view_lookup(token, serial, component, deadline())
                .unwrap()
                .serial;
        }
        if let Some(bytes) = bytes {
            assert_eq!(
                f.workspace
                    .view_read(token, serial, 0, bytes.len() + 1, deadline())
                    .unwrap()
                    .bytes,
                *bytes
            );
        } else {
            let mut after = None;
            let mut names = BTreeSet::new();
            loop {
                let page = f
                    .workspace
                    .view_list(token, serial, after.as_deref(), 128, deadline())
                    .unwrap();
                for (name, _) in page.entries {
                    assert!(names.insert(name));
                }
                after = page.continuation;
                if after.is_none() {
                    break;
                }
            }
            assert_eq!(names, children(tree, path));
        }
    }
}
fn finish(f: &Fixture, serials: Vec<u64>, token: &[u8; 33]) {
    assert!(matches!(
        f.workspace.release_view(token).unwrap(),
        layerfs_workspace::ViewRelease::Completed
    ));
    for serial in serials {
        f.workspace.forget(serial, u64::MAX, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    let backing = f.workspace.backing_status().unwrap();
    assert!(backing.accounting_complete);
    assert_eq!((backing.allocated_bytes, backing.reserved_bytes), (0, 0));
    println!("PHASE_B_NAMESPACE full_old_new_pin_tree=true known_commit=true cleanup=true");
}

#[test]
#[ignore = "Family5 explicit prepared-layout selection only"]
fn move_scaling() {
    let extra = dimension("LAYERFS_PHASE_B_EXTRA");
    let unrelated = dimension("LAYERFS_PHASE_B_UNRELATED");
    let resident = dimension("LAYERFS_PHASE_B_RESIDENT") != 0;
    let f = Fixture::with_extra_children(extra);
    let before_tree = oracle(extra, false, unrelated);
    canonical(&f, f.genesis, &before_tree);
    let token = [91; 33];
    f.workspace.pin_view(token, deadline()).unwrap();
    let mut serials = Vec::new();
    let old = lookup(&f, b"packages/old", &mut serials);
    let new = lookup(&f, b"packages/new", &mut serials);
    if resident {
        for path in before_tree
            .keys()
            .filter(|path| path.starts_with(b"packages/old/subtree"))
        {
            lookup(&f, path, &mut serials);
        }
    }
    for path in before_tree
        .keys()
        .filter(|path| path.starts_with(b"unrelated"))
    {
        lookup(&f, path, &mut serials);
    }
    let before = f.workspace.status().unwrap();
    let backing = f.workspace.backing_status().unwrap();
    let pages = f.workspace.metadata_status().unwrap();
    let start = Instant::now();
    f.workspace
        .rename(
            old,
            b"subtree",
            new,
            b"subtree-expanded",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let rename_ns = start.elapsed().as_nanos();
    let after = f.workspace.status().unwrap();
    let next_backing = f.workspace.backing_status().unwrap();
    let next_pages = f.workspace.metadata_status().unwrap();
    println!("PHASE_B_NAMESPACE_COST descendants={} resident_requested={resident} unrelated={unrelated} resident_nodes={} rename_ns={rename_ns} store_calls={} private_page_reads={} private_page_writes={} new_private_pages={} accounted_memory_delta={}", extra + 3, before.nodes, after.upstream_calls - before.upstream_calls, next_backing.metadata_reads - backing.metadata_reads, next_backing.metadata_writes - backing.metadata_writes, next_pages.allocated_pages as i128 - pages.allocated_pages as i128, after.accounted_bytes as i128 - before.accounted_bytes as i128);
    let moved = lookup(&f, b"packages/new/subtree-expanded", &mut serials);
    let directory = f.workspace.opendir(moved, ReferenceScope::Local).unwrap();
    let listing = f.workspace.readdir(directory, 0, 8, deadline()).unwrap();
    assert!(listing.entries().iter().any(|entry| entry.name == b"child"));
    assert!(listing
        .entries()
        .iter()
        .any(|entry| entry.name == b"sibling.txt"));
    assert_eq!(
        listing
            .entries()
            .iter()
            .find(|entry| entry.name == b"..")
            .unwrap()
            .serial,
        new
    );
    drop(listing);
    f.workspace.releasedir(directory).unwrap();
    let after_tree: Tree = before_tree
        .iter()
        .map(|(path, bytes)| {
            let name = path.strip_prefix(b"packages/old/subtree").map_or_else(
                || path.clone(),
                |tail| [b"packages/new/subtree-expanded".as_slice(), tail].concat(),
            );
            (name, bytes.clone())
        })
        .collect();
    let head = f.commit();
    for path in before_tree.keys() {
        let name = path.strip_prefix(b"packages/old/subtree").map_or_else(
            || path.clone(),
            |tail| [b"packages/new/subtree-expanded".as_slice(), tail].concat(),
        );
        let Response::Attributes {
            serial: before_serial,
            ..
        } = f.attributes(f.genesis, path)
        else {
            panic!("old serial")
        };
        let Response::Attributes {
            serial: after_serial,
            ..
        } = f.attributes(head, &name)
        else {
            panic!("new serial")
        };
        assert_eq!(before_serial, after_serial);
    }
    canonical(&f, head, &after_tree);
    canonical(&f, f.genesis, &before_tree);
    pinned(&f, &token, &before_tree);
    finish(&f, serials, &token);
}

#[test]
#[ignore = "Family5 explicit prepared deep-layout selection only"]
fn deep_move_back() {
    let resident = dimension("LAYERFS_PHASE_B_RESIDENT") != 0;
    let f = Fixture::with_layout(0, true);
    let tree = oracle(0, true, 0);
    canonical(&f, f.genesis, &tree);
    let token = [92; 33];
    f.workspace.pin_view(token, deadline()).unwrap();
    let mut serials = Vec::new();
    let source = lookup(&f, b"src", &mut serials);
    if resident {
        for path in tree.keys().filter(|path| path.starts_with(b"src")) {
            lookup(&f, path, &mut serials);
        }
    }
    let top = f.workspace.root().serial;
    let first = f
        .workspace
        .mkdir(top, &[b'a'; 255], 0o755, 0, deadline())
        .unwrap()
        .serial;
    let target = f
        .workspace
        .mkdir(first, &[b'b'; 63], 0o755, 0, deadline())
        .unwrap()
        .serial;
    serials.extend([first, target]);
    let before = f.workspace.status().unwrap();
    let start = Instant::now();
    f.workspace
        .rename(
            top,
            b"src",
            target,
            b"mmmmmmm",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let rename_ns = start.elapsed().as_nanos();
    let after = f.workspace.status().unwrap();
    let mut path = [
        vec![b'a'; 255],
        vec![b'/'],
        vec![b'b'; 63],
        b"/mmmmmmm".to_vec(),
    ]
    .concat();
    assert_eq!(lookup(&f, &path, &mut serials), source);
    for _ in 0..15 {
        path.push(b'/');
        path.extend_from_slice(&[b'd'; 250]);
    }
    path.extend_from_slice(b"/leaf");
    assert_eq!(path.len(), 4097);
    let leaf = lookup(&f, &path, &mut serials);
    let handle = f.open(leaf);
    assert_eq!(
        f.workspace
            .read(handle, 0, 10, deadline())
            .unwrap()
            .as_ref(),
        b"deep-base"
    );
    f.workspace.release(handle).unwrap();
    f.workspace
        .rename(
            target,
            b"mmmmmmm",
            target,
            b"mmmmmm",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    f.workspace
        .rename(
            target,
            b"mmmmmm",
            top,
            b"src",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    assert_eq!(lookup(&f, b"src", &mut serials), source);
    let mut next = tree.clone();
    next.insert(vec![b'a'; 255], None);
    next.insert([vec![b'a'; 255], vec![b'/'], vec![b'b'; 63]].concat(), None);
    canonical(&f, f.commit(), &next);
    pinned(&f, &token, &tree);
    println!("PHASE_B_NAMESPACE_DEEP path_bytes=4097 resident_requested={resident} resident_nodes={} rename_ns={rename_ns} store_calls={} move_back=true component_access=true", before.nodes, after.upstream_calls - before.upstream_calls);
    finish(&f, serials, &token);
}

#[test]
#[ignore = "Family5 explicit retained namespace selection only"]
fn retained_live_namespace() {
    let f = Fixture::new();
    let old_tree = oracle(0, false, 0);
    let mut serials = Vec::new();
    let old = lookup(&f, b"packages/old", &mut serials);
    let new = lookup(&f, b"packages/new", &mut serials);
    let grand = lookup(&f, b"packages/old/subtree/child/grand.txt", &mut serials);
    let handle = f.open(grand);
    let token = [93; 33];
    f.workspace.pin_view(token, deadline()).unwrap();
    f.workspace
        .rename(
            old,
            b"subtree",
            new,
            b"subtree",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let mut g1: Tree = old_tree
        .iter()
        .map(|(path, bytes)| {
            (
                path.strip_prefix(b"packages/old/subtree").map_or_else(
                    || path.clone(),
                    |tail| [b"packages/new/subtree".as_slice(), tail].concat(),
                ),
                bytes.clone(),
            )
        })
        .collect();
    f.gate.arm();
    let first = std::thread::scope(|scope| {
        let saving = scope.spawn(|| f.commit());
        f.gate.wait();
        let payload = f
            .workspace
            .own_payload(10, &mut b"grand-live".as_slice(), deadline())
            .unwrap();
        f.workspace
            .write_file(handle, 0, &payload, deadline())
            .unwrap();
        f.gate.release();
        saving.join().unwrap()
    });
    canonical(&f, first, &g1);
    g1.insert(
        b"packages/new/subtree/child/grand.txt".to_vec(),
        Some(b"grand-live".to_vec()),
    );
    canonical(&f, f.commit(), &g1);
    canonical(&f, f.genesis, &old_tree);
    pinned(&f, &token, &old_tree);
    f.workspace.release(handle).unwrap();
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 2);
    println!("PHASE_B_NAMESPACE_LIVE frozen_g1=true later_g2=true canonical_calls=2");
    finish(&f, serials, &token);
}
