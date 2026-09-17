//! Candidate arm of the Stage 5 component comparison (C1 primitives only).
//!
//! This driver uses the *corresponding* candidate primitives, not the complete
//! filesystem operation: the sorted directory merge, the sorted inline-inode
//! merge and root encoding. Both arms must therefore exclude the same work —
//! normalization, deriving final reference counts, topology validation, ordering
//! preparation and persistence — and receive the same prepared, sorted inputs.
//!
//! Prints the base identity, the updated identity, the page partition and the
//! actual nanosecond timings so the comparison driver can check identity before
//! comparing any number.

use std::collections::BTreeMap;

use layerfs_content::filesystem::directory::update::apply_bindings;
use layerfs_content::filesystem::inode::update::apply_inode_values;
use layerfs_content::filesystem::objects::FilesystemObjects;
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::sorted::finish::DirectoryRoot;
use layerfs_content::filesystem::sorted::MAXIMUM_SCRATCH_BYTES;
use layerfs_content::filesystem::{profile_id, scope_for_seed, FilesystemRoot, InodeScope};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use std::time::Instant;

/// The strictly sorted directory bindings the comparison arms receive.
type PreparedDirectory = Vec<(PathName, Option<u64>)>;
/// The strictly sorted typed inode rows the comparison arms receive.
type PreparedInodes = Vec<(u64, Option<InodeValue>)>;

/// In-memory provider and consumer for one arm.
#[derive(Clone, Debug, Default)]
struct Bag {
    objects: BTreeMap<ObjectId, (ObjectRole, Vec<u8>)>,
}

impl Bag {
    fn get(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        self.objects
            .get(&id)
            .map(|(_, bytes)| bytes.clone())
            .ok_or(ContentError::MissingObject)
    }
}

impl FinalizedConsumer for Bag {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let (id, role, bytes, _) = object.into_parts();
        self.objects.insert(id, (role, bytes));
        Ok(())
    }
}

impl AuthenticatedObjects for Bag {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter().map(|id| self.get(*id)).collect()
    }
}

fn name_of(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/stage5-fixture/{label}").as_bytes())
}

fn record(serial: u64) -> InodeValue {
    InodeValue {
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

/// Builds the shared base tree through the candidate primitives, untimed.
fn build_base(files: usize) -> (Bag, DirectoryRoot, ObjectId, InodeScope) {
    let scope = scope_for_seed([0x5a; 32]);
    let mut bag = Bag::default();
    let reader = bag.clone();
    let mut sink = Bag::default();
    let (directory, table) = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let (directory, _) = apply_bindings(
            &mut objects,
            None,
            (1..=files as u64)
                .map(|serial| Ok((name_of(&format!("e{serial:05}")), Some(serial))))
                .collect::<Vec<_>>()
                .into_iter(),
            MAXIMUM_SCRATCH_BYTES,
            &mut |_, _| Ok(()),
        )
        .expect("base directory");
        let mut rows: Vec<ContentResult<(u64, Option<InodeValue>)>> = vec![Ok((
            1_u64,
            Some(InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: synthetic("component/root-meta"),
            }),
        ))];
        rows.extend((1..=files as u64).map(|serial| Ok((serial + 1, Some(record(serial))))));
        let (table, _) =
            apply_inode_values(&mut objects, None, rows.into_iter(), MAXIMUM_SCRATCH_BYTES)
                .expect("base table");
        (directory, table)
    };
    bag.objects.extend(sink.objects);
    (bag, directory, table, scope)
}

/// The prepared, strictly sorted change batches both arms receive.
fn prepared_changes(files: usize, changes: usize) -> (PreparedDirectory, PreparedInodes) {
    let mut directory: PreparedDirectory = Vec::new();
    let mut inodes: PreparedInodes = Vec::new();
    for index in 0..changes {
        let serial = (index as u64 % files as u64) + 1;
        directory.push((name_of(&format!("e{serial:05}")), None));
        directory.push((name_of(&format!("r{index:05}")), Some(serial)));
        inodes.push((serial + 1, Some(record(serial))));
    }
    directory.sort_by(|left, right| left.0.cmp(&right.0));
    directory.dedup_by(|left, right| left.0 == right.0);
    inodes.sort_by_key(|row| row.0);
    inodes.dedup_by_key(|row| row.0);
    (directory, inodes)
}

fn main() {
    let config = parse();
    let (bag, base_directory, base_table, scope) = build_base(config.files);
    let (directory_changes, inode_changes) = prepared_changes(config.files, config.changes);
    let base_root = FilesystemRoot::new(profile_id(), scope, 1, base_table)
        .expect("root")
        .encode()
        .expect("encode");
    println!("arm candidate");
    println!("files {} changes {}", config.files, config.changes);
    println!("sampled_changes {}", directory_changes.len());
    println!("base_directory {}", base_directory.0);
    println!("base_table {base_table}");
    println!("base_root {}", ObjectId::for_bytes(&base_root));

    // Timed region: the same three primitive steps in both arms.
    for sample in 0..config.samples {
        let started = Instant::now();
        let reader = bag.clone();
        let mut sink = Bag::default();
        let (updated_directory, table) = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            let (updated, _) = apply_bindings(
                &mut objects,
                Some(base_directory),
                directory_changes.iter().cloned().map(Ok),
                MAXIMUM_SCRATCH_BYTES,
                &mut |_, _| Ok(()),
            )
            .expect("directory merge");
            // The root inode's final value names the directory root the merge just
            // produced. Both arms build this row the same way inside the timed
            // region, so neither is credited for deriving reference counts.
            let mut rows: Vec<ContentResult<(u64, Option<InodeValue>)>> = inode_changes
                .iter()
                .cloned()
                .map(Ok)
                .collect::<Vec<ContentResult<(u64, Option<InodeValue>)>>>();
            rows.push(Ok((
                1_u64,
                Some(InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: updated.0,
                    metadata_root: synthetic("component/root-meta"),
                }),
            )));
            rows.sort_by_key(|row| row.as_ref().map(|row| row.0).unwrap_or(0));
            let (table, _) = apply_inode_values(
                &mut objects,
                Some(base_table),
                rows.into_iter(),
                MAXIMUM_SCRATCH_BYTES,
            )
            .expect("inode merge");
            (updated, table)
        };
        let root = FilesystemRoot::new(profile_id(), scope, 1, table)
            .expect("root")
            .encode()
            .expect("encode");
        let elapsed = started.elapsed();
        println!(
            "sample {sample} elapsed_ns {} updated_directory {} table {} root {}",
            elapsed.as_nanos(),
            updated_directory.0,
            table,
            ObjectId::for_bytes(&root)
        );
    }
    println!("prepared_objects {}", bag.objects.len());
}
