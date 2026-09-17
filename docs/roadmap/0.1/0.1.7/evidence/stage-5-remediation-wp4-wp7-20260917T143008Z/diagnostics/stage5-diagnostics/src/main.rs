//! Independent reviewer diagnostics over the frozen candidate PUBLIC API only.
//!
//! This client is external evidence: it does not modify, re-export or reach into
//! product source. Every probe prints one machine-readable RESULT line.

use std::collections::BTreeMap;

use layerfs_content::filesystem::identity::{InodeIdentity, InodeScope};
use layerfs_content::filesystem::input::{
    DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate,
};
use layerfs_content::filesystem::limits::MAXIMUM_PAGE_BYTES;
use layerfs_content::filesystem::objects::FilesystemObjects;
use layerfs_content::filesystem::path::{LogicalPath, PathName};
use layerfs_content::filesystem::read::FilesystemRead;
use layerfs_content::filesystem::root::{scope_for_seed, FilesystemRootId};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use layerfs_content::filesystem::update::build_filesystem;
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::object::{AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId};

/// In-memory provider/consumer used by both construction and read-back.
#[derive(Clone, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, (u8, Vec<u8>)>,
}

impl AuthenticatedObjects for Bag {
    fn read_canonical_batch(
        &self,
        ids: &[ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.objects.get(id) {
                Some((_, bytes)) => out.push(bytes.clone()),
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

struct Build {
    directories: Vec<DirectoryUpdate>,
    inodes: Vec<InodeUpdate>,
    new_inodes: Vec<u64>,
    root_serial: u64,
}

fn run_build(build: &Build, bag: &mut Bag) -> Result<FilesystemRootId, String> {
    let scope = scope_for_seed([0x5a; 32]);
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: build.root_serial,
        directories: &build.directories,
        inodes: &build.inodes,
        new_inodes: &build.new_inodes,
        resources: FilesystemResources::default(),
    };
    let mut sink = Sink::default();
    let result = {
        let mut objects = FilesystemObjects::new(bag, &mut sink);
        build_filesystem(&mut objects, &input, None)
    };
    for object in sink.objects {
        let id = object.id();
        let role = object.role().code();
        bag.objects.insert(id, (role, object.canonical().to_vec()));
    }
    result.map(|r| r.root).map_err(|e| format!("{e:?}"))
}

fn file_tree(entries: usize) -> Build {
    let mut changes = Vec::new();
    let mut inodes = vec![InodeUpdate {
        serial: 1,
        value: value(InodeKind::Directory, b"root"),
    }];
    let mut new_inodes = vec![1_u64];
    for index in 0..entries {
        let serial = index as u64 + 2;
        changes.push((name(&format!("f{index:05}")), Some(serial)));
        inodes.push(InodeUpdate {
            serial,
            value: value(InodeKind::RegularFile, format!("file-{index}").as_bytes()),
        });
        new_inodes.push(serial);
    }
    Build {
        directories: vec![DirectoryUpdate {
            parent: 1,
            changes,
        }],
        inodes,
        new_inodes,
        root_serial: 1,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("store") {
        let path = args.get(1).expect("store <path>");
        let opened = layerfs_telemetry::timer::Timing::disabled("diag.open", |timing| {
            layerfs_storage::Store::open(path, timing.child("open"))
        })
        .0;
        match opened {
            Ok(store) => println!(
                "RESULT store.open.old_schema=ACCEPTED policy={:?}",
                store.policy()
            ),
            Err(error) => println!("RESULT store.open.old_schema=REFUSED error={error:?}"),
        }
        return;
    }

    // P1: listing under a byte bound that cannot fit one row.
    let mut bag = Bag::default();
    let root = run_build(&file_tree(20), &mut bag).expect("build");
    let mut read = FilesystemRead::new(&bag, root).expect("read root");
    for bound in [1_usize, 14, 15, 16, 17, 4096] {
        let page = read.list(&LogicalPath::root(), None, 64, bound);
        match page {
            Ok(page) => println!(
                "RESULT list.max_bytes={bound} status=Ok entries={} continuation={:?}",
                page.entries.len(),
                page.continuation.as_ref().map(|n| n.as_str().to_owned())
            ),
            Err(error) => println!("RESULT list.max_bytes={bound} status=Err error={error:?}"),
        }
    }
    let full = read.list(&LogicalPath::root(), None, 64, MAXIMUM_PAGE_BYTES);
    println!(
        "RESULT list.full_page is_ok={} entries={} continuation_is_none={}",
        full.is_ok(),
        full.as_ref().map(|p| p.entries.len()).unwrap_or(0),
        full.as_ref().map(|p| p.continuation.is_none()).unwrap_or(false)
    );

    // P2: initial build with a disconnected directory cycle.
    let cycle = Build {
        directories: vec![
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("a"), Some(2))],
            },
            DirectoryUpdate {
                parent: 2,
                changes: vec![],
            },
            DirectoryUpdate {
                parent: 3,
                changes: vec![(name("x"), Some(4))],
            },
            DirectoryUpdate {
                parent: 4,
                changes: vec![(name("y"), Some(3))],
            },
        ],
        inodes: vec![
            InodeUpdate {
                serial: 1,
                value: value(InodeKind::Directory, b"root"),
            },
            InodeUpdate {
                serial: 2,
                value: value(InodeKind::Directory, b"a"),
            },
            InodeUpdate {
                serial: 3,
                value: value(InodeKind::Directory, b"c"),
            },
            InodeUpdate {
                serial: 4,
                value: value(InodeKind::Directory, b"d"),
            },
        ],
        new_inodes: vec![1, 2, 3, 4],
        root_serial: 1,
    };
    let mut bag2 = Bag::default();
    match run_build(&cycle, &mut bag2) {
        Ok(root) => {
            println!("RESULT build.disconnected_cycle=ACCEPTED root={}", root.0);
            if let Ok(mut r) = FilesystemRead::new(&bag2, root) {
                let listing = r.list(&LogicalPath::root(), None, 64, MAXIMUM_PAGE_BYTES);
                println!(
                    "RESULT build.disconnected_cycle.reachable_entries={:?}",
                    listing.map(|p| p.entries.len())
                );
            }
        }
        Err(error) => println!("RESULT build.disconnected_cycle=REFUSED error={error}"),
    }

    // P3: declared new inode that no binding retains.
    let orphan = Build {
        directories: vec![DirectoryUpdate {
            parent: 1,
            changes: vec![],
        }],
        inodes: vec![
            InodeUpdate {
                serial: 1,
                value: value(InodeKind::Directory, b"root"),
            },
            InodeUpdate {
                serial: 9,
                value: value(InodeKind::RegularFile, b"orphan"),
            },
        ],
        new_inodes: vec![1, 9],
        root_serial: 1,
    };
    let mut bag3 = Bag::default();
    match run_build(&orphan, &mut bag3) {
        Ok(root) => {
            let found =
                FilesystemRead::new(&bag3, root).and_then(|mut r| r.lookup_inodes(&[9]));
            println!(
                "RESULT build.orphan_declared_new=ACCEPTED root={} lookup_serial_9={found:?}",
                root.0
            );
        }
        Err(error) => println!("RESULT build.orphan_declared_new=REFUSED error={error}"),
    }

    // P4: name / path / symlink / serial boundaries.
    for length in [255_usize, 256] {
        let text = "n".repeat(length);
        println!(
            "RESULT boundary.name_bytes={length} status={}",
            match PathName::new(&text) {
                Ok(_) => "ACCEPTED".to_owned(),
                Err(e) => format!("REFUSED {e:?}"),
            }
        );
    }
    for length in [4095_usize, 4096, 4097] {
        let text = "p".repeat(length);
        println!(
            "RESULT boundary.path_bytes={length} status={}",
            match LogicalPath::new(&text) {
                Ok(_) => "ACCEPTED".to_owned(),
                Err(e) => format!("REFUSED {e:?}"),
            }
        );
    }
    for count in [256_usize, 257] {
        let mut text = String::new();
        for index in 0..count {
            if index > 0 {
                text.push('/');
            }
            text.push('c');
        }
        println!(
            "RESULT boundary.path_components={count} bytes={} status={}",
            text.len(),
            match LogicalPath::new(&text) {
                Ok(_) => "ACCEPTED".to_owned(),
                Err(e) => format!("REFUSED {e:?}"),
            }
        );
    }
    for length in [4096_usize, 4097] {
        println!(
            "RESULT boundary.symlink_target_bytes={length} status={}",
            match SymlinkTarget::new(vec![b't'; length]) {
                Ok(_) => "ACCEPTED".to_owned(),
                Err(e) => format!("REFUSED {e:?}"),
            }
        );
    }
    let scope = scope_for_seed([0x77; 32]);
    for serial in [i64::MAX as u64, i64::MAX as u64 + 1, 0] {
        println!(
            "RESULT boundary.inode_serial={serial} status={}",
            match InodeIdentity::new(scope, serial) {
                Ok(_) => "ACCEPTED".to_owned(),
                Err(e) => format!("REFUSED {e:?}"),
            }
        );
    }

    // P5: read-work counters after two stats.
    let mut bag6 = Bag::default();
    let root6 = run_build(&file_tree(20), &mut bag6).expect("build");
    let mut read6 = FilesystemRead::new(&bag6, root6).expect("read root");
    let _ = read6.stat(&LogicalPath::root()).expect("stat root");
    let _ = read6
        .stat(&LogicalPath::new("f00000").unwrap())
        .expect("stat file");
    let work = read6.work();
    println!(
        "RESULT read_work.two_stats directory.pages_read={} directory.read_waves={} inode.pages_read={}",
        work.directory.pages_read, work.directory.read_waves, work.inode.pages_read
    );
}
