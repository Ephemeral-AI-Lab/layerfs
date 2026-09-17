//! Sealed reference parity: identical operations, identical canonical identities.
//!
//! The fixtures under `tests/fixtures/filesystem/` were produced by the pinned
//! reference implementation from fixed scopes, serials, names and synthetic
//! content/attribute roots. Each case here runs the same operation through the
//! replacement and compares the new root, the complete reachable object set with
//! its page shapes, and the read-back logical state. A passing round trip would
//! not be evidence; this comparison is.

#![allow(dead_code)]

mod support;

use std::collections::BTreeMap;

use layerfs_content::filesystem::directory::codec::decode_directory_page;
use layerfs_content::filesystem::inode::codec::decode_inode_page;
use layerfs_content::filesystem::{
    build_filesystem, update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemRead,
    FilesystemResources, FilesystemRootId, InodeUpdate, LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{ObjectId, ObjectRole};
use support::filesystem::{resources, synthetic, value, TreeStore};

mod manifest {
    include!("fixtures/filesystem/manifest.rs");
}

/// Maps one replacement logical role onto the reference grammar role code.
fn grammar_role(role: ObjectRole) -> u8 {
    match role {
        ObjectRole::DirectoryLeaf => 1,
        ObjectRole::DirectoryBranch => 2,
        ObjectRole::InodeLeaf => 7,
        ObjectRole::InodeBranch => 8,
        ObjectRole::AttributeLeaf => 9,
        ObjectRole::AttributeBranch => 10,
        ObjectRole::FilesystemRoot => 30,
        ObjectRole::Symlink => 31,
        other => panic!("unexpected tree role {other:?}"),
    }
}

/// Page shape of one tree object: `(role, level, count)`.
fn shape(store: &TreeStore, id: ObjectId) -> (u8, u8, u64) {
    use layerfs_content::filesystem::attributes::codec::decode_attribute_page;
    let role = store.role(id).expect("role");
    let canonical = store.canonical(id).expect("bytes");
    let code = grammar_role(role);
    match role {
        ObjectRole::DirectoryLeaf | ObjectRole::DirectoryBranch => {
            let page = decode_directory_page(canonical).expect("directory page");
            (
                code,
                page.level(),
                match &page {
                    layerfs_content::filesystem::directory::codec::DirectoryPage::Leaf {
                        entries,
                    } => entries.len() as u64,
                    layerfs_content::filesystem::directory::codec::DirectoryPage::Branch {
                        children,
                        ..
                    } => children.len() as u64,
                },
            )
        }
        ObjectRole::InodeLeaf | ObjectRole::InodeBranch => {
            let page = decode_inode_page(canonical).expect("inode page");
            (
                code,
                page.level(),
                match &page {
                    layerfs_content::filesystem::inode::codec::InodePage::Leaf { entries } => {
                        entries.len() as u64
                    }
                    layerfs_content::filesystem::inode::codec::InodePage::Branch {
                        children,
                        ..
                    } => children.len() as u64,
                },
            )
        }
        ObjectRole::AttributeLeaf | ObjectRole::AttributeBranch => {
            let page = decode_attribute_page(canonical).expect("attribute page");
            (
                code,
                page.level(),
                match &page {
                    layerfs_content::filesystem::attributes::codec::AttributePage::Leaf {
                        entries,
                        ..
                    } => entries.len() as u64,
                    layerfs_content::filesystem::attributes::codec::AttributePage::Branch {
                        children,
                        ..
                    } => children.len() as u64,
                },
            )
        }
        ObjectRole::FilesystemRoot | ObjectRole::Symlink => (code, 0, 0),
        other => panic!("unexpected tree role {other:?}"),
    }
}

/// Every tree object reachable from `root`, with its page shape.
fn reachable(store: &TreeStore, root: ObjectId) -> Vec<(ObjectId, u8, u8, u64)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut pending = vec![root];
    let mut output = Vec::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let (role, level, count) = shape(store, id);
        output.push((id, role, level, count));
        let canonical = store.canonical(id).expect("bytes");
        match store.role(id).expect("role") {
            ObjectRole::FilesystemRoot => {
                let root =
                    layerfs_content::filesystem::FilesystemRoot::decode(canonical).expect("root");
                pending.push(root.inode_table());
            }
            ObjectRole::InodeLeaf => {
                if let layerfs_content::filesystem::inode::codec::InodePage::Leaf { entries } =
                    decode_inode_page(canonical).expect("inode leaf")
                {
                    for (_, record) in entries {
                        if record.kind == InodeKind::Directory {
                            pending.push(record.content_root);
                        }
                    }
                }
            }
            ObjectRole::InodeBranch => {
                if let layerfs_content::filesystem::inode::codec::InodePage::Branch {
                    children,
                    ..
                } = decode_inode_page(canonical).expect("inode branch")
                {
                    pending.extend(children.into_iter().map(|(_, id)| id));
                }
            }
            ObjectRole::DirectoryLeaf => {}
            ObjectRole::DirectoryBranch => {
                if let layerfs_content::filesystem::directory::codec::DirectoryPage::Branch {
                    children,
                    ..
                } = decode_directory_page(canonical).expect("directory branch")
                {
                    pending.extend(children.into_iter().map(|(_, id)| id));
                }
            }
            _ => {}
        }
    }
    output.sort();
    output
}

/// Native inputs for one sealed case.
struct Case<'a> {
    manifest: &'a manifest::FixtureCase,
}

impl Case<'_> {
    fn updates(&self) -> Vec<DirectoryUpdate> {
        let mut by_parent: BTreeMap<u64, Vec<(PathName, Option<u64>)>> = BTreeMap::new();
        for change in self.manifest.changes {
            let binding = if change.binding < 0 {
                None
            } else {
                Some(change.binding as u64)
            };
            by_parent
                .entry(change.parent)
                .or_default()
                .push((PathName::new(change.name).expect("name"), binding));
        }
        for update in self.manifest.finals {
            if update.content == "directory" {
                by_parent.entry(update.serial).or_default();
            }
        }
        by_parent
            .into_iter()
            .map(|(parent, mut changes)| {
                changes.sort_by(|left, right| left.0.cmp(&right.0));
                DirectoryUpdate { parent, changes }
            })
            .collect()
    }

    fn inodes(&self) -> Vec<InodeUpdate> {
        self.manifest
            .finals
            .iter()
            .map(|record| InodeUpdate {
                serial: record.serial,
                value: value(
                    kind_of(record.kind),
                    synthetic(record.content),
                    synthetic(record.metadata),
                ),
            })
            .collect()
    }

    fn new_inodes(&self, known: &[u64]) -> Vec<u64> {
        self.manifest
            .finals
            .iter()
            .map(|record| record.serial)
            .filter(|serial| !known.contains(serial))
            .collect()
    }
}

fn kind_of(code: u8) -> InodeKind {
    match code {
        1 => InodeKind::RegularFile,
        2 => InodeKind::Directory,
        3 => InodeKind::Symlink,
        other => panic!("unexpected kind {other}"),
    }
}

fn root_serial(case: &manifest::FixtureCase) -> u64 {
    case.finals
        .iter()
        .find(|record| record.serial == 1)
        .map(|record| record.serial)
        .unwrap_or(1)
}

/// Builds one sealed construction case and checks its identity and page shapes.
fn build_case(case: &manifest::FixtureCase) -> TreeStore {
    let mut store = TreeStore::new();
    let case_view = Case { manifest: case };
    let updates = case_view.updates();
    let inodes = case_view.inodes();
    let new_inodes = case_view.new_inodes(&[]);
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x5a; 32]),
        root_serial: root_serial(case),
        directories: &updates,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let result = support::filesystem::with_objects(&mut store, |objects| {
        build_filesystem(objects, &input, None)
    })
    .unwrap_or_else(|error| panic!("case {}: {error}", case.name));
    let expected = ObjectId::from_str_checked(case.root);
    assert_eq!(
        result.root.0, expected,
        "case {}: root identity differs from the reference",
        case.name
    );
    store
}

trait FromStrChecked {
    fn from_str_checked(value: &str) -> ObjectId;
}

impl FromStrChecked for ObjectId {
    fn from_str_checked(value: &str) -> ObjectId {
        value.parse().expect("fixture identity")
    }
}

fn check_case(case: &manifest::FixtureCase, base_store: Option<&TreeStore>) {
    let built = build_case(case);
    let expected = ObjectId::from_str_checked(case.root);
    let sealed = case
        .objects
        .iter()
        .map(|object| {
            (
                ObjectId::from_str_checked(object.id),
                object.role,
                object.level,
                object.count,
            )
        })
        .collect::<Vec<_>>();
    let mut observed = reachable(&built, expected);
    observed.sort();
    assert_eq!(
        observed, sealed,
        "case {}: reachable object set or page shapes differ",
        case.name
    );
    let _ = base_store;
    let _ = resources();
}

#[test]
fn construction_matches_the_sealed_reference_roots_and_pages() {
    for case in manifest::CASES.iter().filter(|case| case.base.is_empty()) {
        check_case(case, None);
    }
}

#[test]
fn updates_match_the_sealed_reference_operation() {
    for case in manifest::CASES.iter().filter(|case| !case.base.is_empty()) {
        let base = manifest::CASES
            .iter()
            .find(|candidate| candidate.name == case.base)
            .expect("base case");
        let base_store = build_case(base);
        let case_view = Case { manifest: case };
        let updates = case_view.updates();
        let inodes = case_view.inodes();
        let known = base_store_serials(base);
        let new_inodes = case_view.new_inodes(&known);
        let mut store = base_store.clone();
        let input = FilesystemInput {
            base: Some(FilesystemRootId(ObjectId::from_str_checked(base.root))),
            scope: layerfs_content::filesystem::scope_for_seed([0x5a; 32]),
            root_serial: root_serial(case),
            directories: &updates,
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: FilesystemResources::default(),
        };
        let result = support::filesystem::with_objects(&mut store, |objects| {
            update_filesystem(objects, &input, None)
        })
        .unwrap_or_else(|error| panic!("case {}: {error}", case.name));
        let expected = ObjectId::from_str_checked(case.root);
        assert_eq!(
            result.root.0, expected,
            "case {}: updated root identity differs from the reference",
            case.name
        );
        let sealed = case
            .objects
            .iter()
            .map(|object| {
                (
                    ObjectId::from_str_checked(object.id),
                    object.role,
                    object.level,
                    object.count,
                )
            })
            .collect::<Vec<_>>();
        let mut observed = reachable(&store, expected);
        observed.sort();
        assert_eq!(
            observed, sealed,
            "case {}: reachable object set or page shapes differ",
            case.name
        );
        // Read-back parity: the final logical state must match the sealed finals.
        let mut read = FilesystemRead::new(&store, FilesystemRootId(expected)).expect("read root");
        let mut seen = BTreeMap::new();
        collect_bindings(&mut read, &LogicalPath::root(), &mut seen).expect("walk");
        for record in case.finals {
            let entry = seen.get(&record.serial).unwrap_or_else(|| {
                panic!("case {}: serial {} unreachable", case.name, record.serial)
            });
            if std::env::var("LAYERFS_DEBUG_CASE").ok().as_deref() == Some(case.name) {
                eprintln!(
                    "record serial {} kind {} count {} vs seen kind {} count {}",
                    record.serial,
                    record.kind,
                    record.count,
                    entry.kind.code(),
                    entry.namespace_ref_count
                );
            }
            assert_eq!(
                entry.kind.code(),
                record.kind,
                "case {}: serial {} kind",
                case.name,
                record.serial
            );
            assert_eq!(
                entry.namespace_ref_count, record.count,
                "case {}: serial {} reference count",
                case.name, record.serial
            );
            if record.content != "directory" {
                assert_eq!(
                    entry.content_root,
                    synthetic(record.content),
                    "case {}: serial {} content root",
                    case.name,
                    record.serial
                );
            }
            assert_eq!(
                entry.metadata_root,
                synthetic(record.metadata),
                "case {}: serial {} metadata root",
                case.name,
                record.serial
            );
        }
        for serial in case.removed {
            assert!(
                !seen.contains_key(serial),
                "case {}: removed serial {serial} is still reachable",
                case.name
            );
        }
    }
}

fn base_store_serials(case: &manifest::FixtureCase) -> Vec<u64> {
    case.finals.iter().map(|record| record.serial).collect()
}

/// Walks a filesystem and records every reachable inode.
fn collect_bindings(
    read: &mut FilesystemRead<'_>,
    path: &LogicalPath,
    seen: &mut BTreeMap<u64, layerfs_content::object::inode_leaf::InodeValue>,
) -> layerfs_content::ContentResult<()> {
    use layerfs_content::object::inode_leaf::InodeKind;
    let resolved = read.resolve(path)?;
    if seen.insert(resolved.serial, resolved.value).is_some() {
        return Ok(());
    }
    if resolved.value.kind != InodeKind::Directory {
        return Ok(());
    }
    let mut after = None;
    loop {
        let page = read.list(path, after.as_ref(), 64, 8192)?;
        for (name, serial) in &page.entries {
            let child = path.join(name);
            let child_value = read.resolve(&child)?;
            assert_eq!(child_value.serial, *serial, "binding/serial mismatch");
            collect_bindings(read, &child, seen)?;
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => return Ok(()),
        }
    }
}
