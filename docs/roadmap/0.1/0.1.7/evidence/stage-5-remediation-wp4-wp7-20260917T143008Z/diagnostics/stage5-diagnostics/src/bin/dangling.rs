//! Independent reviewer probe: can an acknowledged save hold a tree whose
//! inode values reference objects the Store does not contain?
//!
//! Public API only. Two arms build the SAME logical tree; the control arm also
//! emits the referenced objects, the dangling arm does not.

use std::collections::BTreeMap;

use layerfs_content::filesystem::input::{
    DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate,
};
use layerfs_content::filesystem::objects::FilesystemObjects;
use layerfs_content::filesystem::path::{LogicalPath, PathName};
use layerfs_content::filesystem::read::FilesystemRead;
use layerfs_content::filesystem::root::scope_for_seed;
use layerfs_content::filesystem::update::build_filesystem;
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

fn value(kind: InodeKind, content: ObjectId, metadata: ObjectId) -> InodeValue {
    InodeValue {
        kind,
        namespace_ref_count: 0,
        content_root: content,
        metadata_root: metadata,
    }
}

fn name(text: &str) -> PathName {
    PathName::new(text).expect("name")
}

struct Tree {
    root: layerfs_content::filesystem::root::FilesystemRootId,
    emitted: Vec<FinalizedObject>,
    file_content: ObjectId,
    file_metadata: ObjectId,
}

fn build(emit_referenced: bool) -> Tree {
    let directory = ObjectId::for_bytes(b"emitted-root-content");
    let directory_metadata = ObjectId::for_bytes(b"emitted-root-metadata");
    let mut file_content = ObjectId::for_bytes(b"file-content-root");
    let file_metadata = ObjectId::for_bytes(b"file-attribute-root");
    let mut bag = Bag::default();
    let mut sink = Sink::default();
    if emit_referenced {
        // The control arm really emits the object its file inode names.
        let target = layerfs_content::filesystem::symlink::SymlinkTarget::new(
            b"referenced-by-the-file-inode".to_vec(),
        )
        .expect("target");
        let mut objects = FilesystemObjects::new(&bag, &mut sink);
        file_content = layerfs_content::filesystem::symlink::emit_symlink(&mut objects, target)
            .expect("emit referenced object");
    }

    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("f"), Some(2))],
    }];
    let inodes = [
        InodeUpdate {
            serial: 1,
            value: value(InodeKind::Directory, directory, directory_metadata),
        },
        InodeUpdate {
            serial: 2,
            value: value(InodeKind::RegularFile, file_content, file_metadata),
        },
    ];
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x5a; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[1, 2],
        resources: FilesystemResources::default(),
    };
    let root = {
        let mut objects = FilesystemObjects::new(&bag, &mut sink);
        build_filesystem(&mut objects, &input, None)
            .expect("build")
            .root
    };
    // The build reads nothing: every referenced value root is caller-supplied.
    let mut emitted = Vec::new();
    for object in sink.objects {
        bag.objects
            .insert(object.id(), object.canonical().to_vec());
        emitted.push(object);
    }
    // Read the tree back in memory: does the file inode name the roots we supplied?
    let mut read = FilesystemRead::new(&bag, root).expect("read");
    let stat = read.stat(&LogicalPath::new("f").unwrap()).expect("stat");
    assert_eq!(stat.content_root, file_content);
    assert_eq!(stat.metadata_root, file_metadata);
    Tree {
        root,
        emitted,
        file_content,
        file_metadata,
    }
}

fn arm(label: &str, output: &str, emit_referenced: bool) {
    let _ = std::fs::remove_dir_all(output);
    std::fs::create_dir_all(output).expect("arm directory");
    let tree = build(emit_referenced);
    let path = format!("{output}/store.sqlite");
    let store = layerfs_telemetry::timer::Timing::disabled("probe.store", |timing| {
        layerfs_storage::Store::create(
            &path,
            layerfs_storage::StoragePolicy::frozen_default(),
            timing.child("create"),
        )
    })
    .0
    .expect("store");
    let emitted_ids: Vec<ObjectId> = tree.emitted.iter().map(|o| o.id()).collect();
    let outcome = layerfs_telemetry::timer::Timing::disabled("probe.save", |timing| {
        let mut operation = store.begin_save(timing.child("begin")).expect("begin");
        for object in tree.emitted {
            operation
                .accept(object, timing.child("accept"))
                .expect("accept");
        }
        operation.finish(timing.child("finish"))
    })
    .0;
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            println!("RESULT arm={label} save=Err({error:?})");
            return;
        }
    };
    println!(
        "RESULT arm={label} emitted_objects={} acknowledged={} inserted={} packs={} commits={}",
        emitted_ids.len(),
        outcome.acknowledged,
        outcome.inserted,
        outcome.packs_created,
        outcome.commits
    );
    let present = layerfs_telemetry::timer::Timing::disabled("probe.contains", |timing| {
        store.contains(&[tree.file_content, tree.file_metadata], timing.child("contains"))
    })
    .0;
    match present {
        Ok(present) => println!(
            "RESULT arm={label} file_value_roots_present_in_store={} (of 2)",
            present.len()
        ),
        Err(error) => println!("RESULT arm={label} contains=Err({error:?})"),
    }
    let read = layerfs_telemetry::timer::Timing::disabled("probe.read", |timing| {
        store.read_batch(&[tree.file_content], timing.child("read"))
    })
    .0;
    match read {
        Ok((values, _)) => println!(
            "RESULT arm={label} read_of_file_content_root=Ok(bytes={})",
            values.first().map(Vec::len).unwrap_or(0)
        ),
        Err(error) => println!("RESULT arm={label} read_of_file_content_root=Err({error:?})"),
    }
    println!("RESULT arm={label} root_id={}", tree.root.0);
}

fn main() {
    let output = std::env::args().nth(1).expect("output root");
    arm("dangling", &format!("{output}/dangling"), false);
    arm("control-emitted", &format!("{output}/control"), true);
}
