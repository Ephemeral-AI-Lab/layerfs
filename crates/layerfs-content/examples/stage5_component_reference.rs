//! Reference arm of the Stage 5 component comparison (v0.1.6 public primitives).
//!
//! The same prepared, sorted inputs as the candidate arm, driven through the
//! reference's public sorted directory merge, sorted inline-inode merge and root
//! encoding. Nothing private is transplanted: this is the pinned reference
//! implementation's own API. Both arms exclude the same work (normalization,
//! deriving final reference counts, topology validation, ordering preparation and
//! persistence).
//!
//! Run: `cargo +1.85.1 run --locked -p layerfs-content --example stage5_component_reference -- --files 2000 --changes 200`

use std::collections::BTreeMap;
use std::time::Instant;

use layerfs_content::filesystem::{build_initial_directory_sorted, namespace};
use layerfs_content::object::access::ObjectStore;
use layerfs_content::tree::batch::{
    compact_inode_table_apply_sorted, compact_inode_table_from_sorted,
    directory_apply_sorted_with_budget, SORTED_TREE_UPDATE_SCRATCH_BYTES,
};
use layerfs_content::tree::compact::{self, InodeSerial};
use layerfs_content::tree::directory::codec::encode_namespace_root;
use layerfs_content::tree::directory::DirectoryStateRoot;
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1, InodeTableRoot};
use layerfs_content::tree::NamespaceRootV1;
use layerfs_content::{CanonicalName, CoreError, CoreResult, ObjectId};

/// Plain in-memory reference store.
#[derive(Clone, Default)]
struct MemoryStore {
    objects: BTreeMap<ObjectId, Vec<u8>>,
}

impl ObjectStore for MemoryStore {
    fn compact_namespace(&self) -> bool {
        true
    }

    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.objects.get(&id).cloned().ok_or(CoreError::MissingObject)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        self.objects.insert(id, canonical.to_vec());
        Ok(id)
    }
}

fn name_of(value: &str) -> CanonicalName {
    CanonicalName::new(value).expect("name")
}

fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/stage5-fixture/{label}").as_bytes())
}

fn record(serial: u64) -> InodeRecordV1 {
    InodeRecordV1 {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: synthetic(&format!("component/content-{serial:05}")),
        metadata_root: synthetic("component/meta"),
    }
}

struct Config {
    files: usize,
    changes: usize,
    samples: usize,
}

fn parse() -> Config {
    let mut config = Config {
        files: 2_000,
        changes: 200,
        samples: 1,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        match argument.as_str() {
            "--files" => config.files = value,
            "--changes" => config.changes = value,
            "--samples" => config.samples = value.max(1),
            other => panic!("unexpected argument {other}"),
        }
    }
    config
}

/// Builds the shared base tree through the reference primitives, untimed.
fn build_base(files: usize) -> (MemoryStore, DirectoryStateRoot, InodeTableRoot) {
    let mut store = MemoryStore::default();
    let directory = build_initial_directory_sorted(
        &mut store,
        (1..=files as u64).map(|serial| (name_of(&format!("e{serial:05}")), inode(serial))),
    )
    .expect("base directory");
    let mut rows = vec![(
        serial(1),
        InodeRecordV1 {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: directory.0,
            metadata_root: synthetic("component/root-meta"),
        },
    )];
    rows.extend((1..=files as u64).map(|value| (serial(value + 1), record(value))));
    let (table, _) = compact_inode_table_from_sorted(
        &mut store,
        rows.into_iter().map(Ok),
        SORTED_TREE_UPDATE_SCRATCH_BYTES,
    )
    .expect("base table");
    (store, directory, table)
}

fn serial(value: u64) -> InodeSerial {
    InodeSerial::new(value).expect("serial")
}

fn inode(value: u64) -> InodeId {
    serial(value).inode_key()
}

/// The prepared, strictly sorted change batches both arms receive.
fn prepared_changes(
    files: usize,
    changes: usize,
) -> (
    Vec<(CanonicalName, Option<InodeId>)>,
    Vec<(InodeSerial, Option<InodeRecordV1>)>,
) {
    let mut directory = Vec::new();
    let mut inodes = Vec::new();
    for index in 0..changes {
        let value = (index as u64 % files as u64) + 1;
        directory.push((name_of(&format!("e{value:05}")), None));
        directory.push((name_of(&format!("r{index:05}")), Some(inode(value))));
        inodes.push((serial(value + 1), Some(record(value))));
    }
    directory.sort_by(|left, right| left.0.cmp(&right.0));
    directory.dedup_by(|left, right| left.0 == right.0);
    inodes.sort_by_key(|row| row.0);
    inodes.dedup_by_key(|row| row.0);
    (directory, inodes)
}

fn main() {
    let config = parse();
    let (store, base_directory, base_table) = build_base(config.files);
    let (directory_changes, inode_changes) = prepared_changes(config.files, config.changes);
    let scope = compact::scope_for_seed([0x5a; 32]);
    let base_root = store
        .objects
        .get(&base_table.0)
        .map(|_| ())
        .map(|_| {
            encode_namespace_root(NamespaceRootV1 {
                scope: Some(scope),
                profile_id: compact::profile_id(),
                root_directory_inode: inode(1),
                inode_table_root: base_table.0,
            })
            .expect("root")
        })
        .expect("base table exists");
    println!("arm reference");
    println!("files {} changes {}", config.files, config.changes);
    println!("sampled_changes {}", directory_changes.len());
    println!("prepared_objects {}", store.objects.len());
    println!("base_directory {}", base_directory.0);
    println!("base_table {}", base_table.0);
    println!("base_root {}", ObjectId::for_bytes(&base_root));

    for sample in 0..config.samples {
        let started = Instant::now();
        let mut working = store.clone();
        let (updated_directory, _) = directory_apply_sorted_with_budget(
            &mut working,
            base_directory,
            directory_changes.iter().cloned().map(Ok),
            SORTED_TREE_UPDATE_SCRATCH_BYTES,
        )
        .expect("directory merge");
        let mut rows = inode_changes.clone();
        rows.push((
            serial(1),
            Some(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: updated_directory.0,
                metadata_root: synthetic("component/root-meta"),
            }),
        ));
        rows.sort_by_key(|row| row.0);
        let (table, _) = compact_inode_table_apply_sorted(
            &mut working,
            base_table,
            rows.into_iter().map(Ok),
            SORTED_TREE_UPDATE_SCRATCH_BYTES,
        )
        .expect("inode merge");
        let root = encode_namespace_root(NamespaceRootV1 {
            scope: Some(scope),
            profile_id: compact::profile_id(),
            root_directory_inode: inode(1),
            inode_table_root: table.0,
        })
        .expect("root");
        let elapsed = started.elapsed();
        let _ = namespace(&working, ObjectId::for_bytes(&root));
        println!(
            "sample {sample} elapsed_ns {} updated_directory {} table {} root {}",
            elapsed.as_nanos(),
            updated_directory.0,
            table.0,
            ObjectId::for_bytes(&root)
        );
    }
}
