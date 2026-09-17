//! Independent reviewer probe: measured growth of the record-backed update path.
//!
//! Same logical operation at increasing size, with the ordering pending map
//! bounded so the operation spills, and a no-spill control arm. Public API only.

use std::collections::BTreeMap;
use std::time::Instant;

use layerfs_content::filesystem::identity::InodeScope;
use layerfs_content::filesystem::input::{
    DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate,
};
use layerfs_content::filesystem::objects::FilesystemObjects;
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::filesystem::root::{scope_for_seed, FilesystemRootId};
use layerfs_content::filesystem::update::{build_filesystem, update_filesystem};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::object::{AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId};

#[derive(Clone, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, (Vec<u8>, ObjectId)>,
}

impl AuthenticatedObjects for Bag {
    fn read_canonical_batch(
        &self,
        ids: &[ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.objects.get(id) {
                Some((bytes, _)) => out.push(bytes.clone()),
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

fn absorb(bag: &mut Bag, sink: Sink) {
    for object in sink.objects {
        let id = object.id();
        let role = object.role().code();
        let references = object.references().first().copied().unwrap_or(id);
        bag.objects.insert(id, (object.canonical().to_vec(), references));
        let _ = role;
    }
}

fn build_base(entries: usize, bag: &mut Bag) -> (FilesystemRootId, InodeScope) {
    let scope = scope_for_seed([0x5a; 32]);
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
            value: value(InodeKind::RegularFile, format!("payload-{index}").as_bytes()),
        });
        new_inodes.push(serial);
    }
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &[DirectoryUpdate {
            parent: 1,
            changes,
        }],
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let mut sink = Sink::default();
    let root = {
        let mut objects = FilesystemObjects::new(bag, &mut sink);
        build_filesystem(&mut objects, &input, None).expect("base build").root
    };
    absorb(bag, sink);
    (root, scope)
}

struct Arm {
    label: &'static str,
    maximum_pending_records: usize,
}

fn main() {
    let output = std::env::args().nth(1).expect("output directory");
    for entries in [200_usize, 400, 800, 1600] {
        for arm in [
            Arm {
                label: "no-spill",
                maximum_pending_records: 1_000_000,
            },
            Arm {
                label: "spilling",
                maximum_pending_records: 64,
            },
        ] {
            let directory = format!("{output}/e{entries}-{}", arm.label);
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("arm directory");
            let mut bag = Bag::default();
            let (root, scope) = build_base(entries, &mut bag);
            // Rename every name to a freshly allocated serial.
            let mut changes = Vec::new();
            let mut inodes = Vec::new();
            let mut new_inodes = Vec::new();
            for index in 0..entries {
                let serial = entries as u64 + index as u64 + 2;
                changes.push((name(&format!("f{index:05}")), Some(serial)));
                inodes.push(InodeUpdate {
                    serial,
                    value: value(InodeKind::RegularFile, format!("renamed-{index}").as_bytes()),
                });
                new_inodes.push(serial);
            }
            let resources = FilesystemResources {
                maximum_pending_records: arm.maximum_pending_records,
                ..FilesystemResources::default()
            };
            let input = FilesystemInput {
                base: Some(root),
                scope,
                root_serial: 1,
                directories: &[DirectoryUpdate {
                    parent: 1,
                    changes,
                }],
                inodes: &inodes,
                new_inodes: &new_inodes,
                resources,
            };
            let mut backing = FileBacking::new(&directory);
            let mut sink = Sink::default();
            let started = Instant::now();
            let result = {
                let mut objects = FilesystemObjects::new(&bag, &mut sink);
                update_filesystem(&mut objects, &input, Some(&mut backing))
            };
            let elapsed = started.elapsed();
            match result {
                Ok(result) => {
                    let work = result.counters.references;
                    println!(
                        "RESULT arm={} entries={} elapsed_ns={} rows_touched={} rows_spilled={} runs_created={} merges={} rows_read={} rows_written={} serials_scanned={} peak_live_runs={} peak_run_bytes={} backing_peak={} backing_held_after={}",
                        arm.label,
                        entries,
                        elapsed.as_nanos(),
                        work.rows_touched,
                        work.rows_spilled,
                        work.runs.runs_created,
                        work.runs.merges,
                        work.runs.rows_read,
                        work.runs.rows_written,
                        work.serials_scanned,
                        work.runs.peak_live_runs,
                        work.runs.peak_run_bytes,
                        backing.peak_bytes(),
                        backing.held_bytes(),
                    );
                }
                Err(error) => println!(
                    "RESULT arm={} entries={} status=Err error={error:?}",
                    arm.label, entries
                ),
            }
        }
    }
}
