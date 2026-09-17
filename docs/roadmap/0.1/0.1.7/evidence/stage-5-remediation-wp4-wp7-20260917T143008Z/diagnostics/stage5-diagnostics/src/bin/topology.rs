//! Independent reviewer probe: whole-tree topology acceptance through the public API.

use std::collections::BTreeMap;

use layerfs_content::filesystem::identity::InodeScope;
use layerfs_content::filesystem::input::{
    DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate,
};
use layerfs_content::filesystem::objects::FilesystemObjects;
use layerfs_content::filesystem::path::{LogicalPath, PathName};
use layerfs_content::filesystem::read::FilesystemRead;
use layerfs_content::filesystem::root::{scope_for_seed, FilesystemRootId};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use layerfs_content::filesystem::update::{build_filesystem, update_filesystem};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::object::{AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId};

#[derive(Clone, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, Vec<u8>>,
}

impl AuthenticatedObjects for Bag {
    fn read_canonical_batch(
        &self,
        ids: &[ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.objects.get(id) {
                Some(bytes) => out.push(bytes.clone()),
                None => return Err(layerfs_content::ContentError::MissingObject),
            }
        }
        Ok(out)
    }
}

#[derive(Default)]
struct Sink {
    objects: Vec<FinalizedObject>,
}

impl FinalizedConsumer for Sink {
    fn accept(&mut self, object: FinalizedObject) -> layerfs_content::ContentResult<()> {
        self.objects.push(object);
        Ok(())
    }
}

fn value(kind: InodeKind, label: &[u8]) -> InodeValue {
    InodeValue {
        kind,
        namespace_ref_count: 0,
        content_root: ObjectId::for_bytes(label),
        metadata_root: ObjectId::for_bytes(b"empty-attributes"),
    }
}

fn name(text: &str) -> PathName {
    PathName::new(text).expect("name")
}

fn path(text: &str) -> LogicalPath {
    LogicalPath::new(text).expect("path")
}

fn absorb(bag: &mut Bag, sink: Sink) {
    for object in sink.objects {
        bag.objects.insert(object.id(), object.canonical().to_vec());
    }
}

/// root(1) directory holding d(2) directory holding e(3) directory; s(4) a symlink.
fn base(bag: &mut Bag) -> (FilesystemRootId, InodeScope) {
    let scope = scope_for_seed([0x5a; 32]);
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(2)), (name("s"), Some(4))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![(name("e"), Some(3))],
        },
        DirectoryUpdate {
            parent: 3,
            changes: vec![],
        },
    ];
    let inodes = [
        InodeUpdate {
            serial: 1,
            value: value(InodeKind::Directory, b"root"),
        },
        InodeUpdate {
            serial: 2,
            value: value(InodeKind::Directory, b"d"),
        },
        InodeUpdate {
            serial: 3,
            value: value(InodeKind::Directory, b"e"),
        },
        InodeUpdate {
            serial: 4,
            value: value(InodeKind::Symlink, b"target-root"),
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[1, 2, 3, 4],
        resources: FilesystemResources::default(),
    };
    let mut sink = Sink::default();
    let root = {
        let mut objects = FilesystemObjects::new(bag, &mut sink);
        build_filesystem(&mut objects, &input, None)
            .expect("base build")
            .root
    };
    absorb(bag, sink);
    (root, scope)
}

fn apply(
    bag: &mut Bag,
    root: FilesystemRootId,
    scope: InodeScope,
    directories: &[DirectoryUpdate],
    inodes: &[InodeUpdate],
    new_inodes: &[u64],
) -> Result<FilesystemRootId, String> {
    let input = FilesystemInput {
        base: Some(root),
        scope,
        root_serial: 1,
        directories,
        inodes,
        new_inodes,
        resources: FilesystemResources::default(),
    };
    let mut sink = Sink::default();
    let result = {
        let mut objects = FilesystemObjects::new(bag, &mut sink);
        update_filesystem(&mut objects, &input, None)
    };
    absorb(bag, sink);
    result.map(|r| r.root).map_err(|e| format!("{e:?}"))
}

fn main() {
    // T1: two declared-new directories bound to each other (a cycle with no base).
    {
        let mut bag = Bag::default();
        let (root, scope) = base(&mut bag);
        let directories = [
            DirectoryUpdate {
                parent: 10,
                changes: vec![(name("a"), Some(11))],
            },
            DirectoryUpdate {
                parent: 11,
                changes: vec![(name("b"), Some(10))],
            },
        ];
        let inodes = [
            InodeUpdate {
                serial: 10,
                value: value(InodeKind::Directory, b"n1"),
            },
            InodeUpdate {
                serial: 11,
                value: value(InodeKind::Directory, b"n2"),
            },
        ];
        match apply(&mut bag, root, scope, &directories, &inodes, &[10, 11]) {
            Ok(root) => {
                println!("RESULT T1.update_new_dir_cycle=ACCEPTED root={}", root.0);
                if let Ok(mut r) = FilesystemRead::new(&bag, root) {
                    println!(
                        "RESULT T1.reachable_root_entries={:?}",
                        r.list(&LogicalPath::root(), None, 64, 8192)
                            .map(|p| p.entries.len())
                    );
                    println!(
                        "RESULT T1.lookup_inode_10={:?}",
                        r.lookup_inodes(&[10]).map(|v| v[0].map(|x| x.namespace_ref_count))
                    );
                }
            }
            Err(error) => println!("RESULT T1.update_new_dir_cycle=REFUSED error={error}"),
        }
    }

    // T2: an EXISTING directory gains a second parent inside the effective tree.
    {
        let mut bag = Bag::default();
        let (root, scope) = base(&mut bag);
        let directories = [DirectoryUpdate {
            parent: 2,
            changes: vec![(name("e2"), Some(3))],
        }];
        match apply(&mut bag, root, scope, &directories, &[], &[]) {
            Ok(root) => {
                println!("RESULT T2.dir_second_parent=ACCEPTED root={}", root.0);
                if let Ok(mut r) = FilesystemRead::new(&bag, root) {
                    for p in ["d/e", "d/e2"] {
                        println!(
                            "RESULT T2.stat {}={:?}",
                            p,
                            r.stat(&path(p))
                                .map(|s| (s.kind.code(), s.namespace_ref_count))
                        );
                    }
                    println!(
                        "RESULT T2.lookup_serial_3={:?}",
                        r.lookup_inodes(&[3]).map(|v| v[0].map(|x| x.namespace_ref_count))
                    );
                }
            }
            Err(error) => println!("RESULT T2.dir_second_parent=REFUSED error={error}"),
        }
    }

    // T3: an EXISTING symlink gains a second parent.
    {
        let mut bag = Bag::default();
        let (root, scope) = base(&mut bag);
        let directories = [DirectoryUpdate {
            parent: 2,
            changes: vec![(name("s2"), Some(4))],
        }];
        match apply(&mut bag, root, scope, &directories, &[], &[]) {
            Ok(root) => {
                println!("RESULT T3.symlink_second_parent=ACCEPTED root={}", root.0);
                if let Ok(mut r) = FilesystemRead::new(&bag, root) {
                    println!(
                        "RESULT T3.lookup_serial_4={:?}",
                        r.lookup_inodes(&[4]).map(|v| v[0].map(|x| x.namespace_ref_count))
                    );
                    println!(
                        "RESULT T3.readlink d/s2={:?}",
                        r.readlink(&path("d/s2")).map(|t| t.as_bytes().to_vec())
                    );
                }
            }
            Err(error) => println!("RESULT T3.symlink_second_parent=REFUSED error={error}"),
        }
    }

    // T4: control - this batch's own duplicate binding must still be refused.
    {
        let mut bag = Bag::default();
        let (root, scope) = base(&mut bag);
        let directories = [DirectoryUpdate {
            parent: 2,
            changes: vec![(name("e2"), Some(3)), (name("e3"), Some(3))],
        }];
        match apply(&mut bag, root, scope, &directories, &[], &[]) {
            Ok(root) => println!("RESULT T4.same_batch_duplicate=ACCEPTED root={}", root.0),
            Err(error) => println!("RESULT T4.same_batch_duplicate=REFUSED error={error}"),
        }
    }

    let _ = SymlinkTarget::new(vec![b'x']);
}
