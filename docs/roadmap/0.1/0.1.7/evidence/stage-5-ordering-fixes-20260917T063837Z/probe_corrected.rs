//! Corrected replay of the audit's defect probe: the same public-API client with
//! its three observed-defect assertions replaced by the required behaviour. It
//! passes only when ordering bytes are owned and reported, checked cleanup gates
//! success, and real ordering work reaches the returned counters.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking, OrderingRun};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, InodeUpdate, PathName,
};
use layerfs_content::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};

#[derive(Clone, Default)]
struct Objects(Rc<RefCell<BTreeMap<ObjectId, Vec<u8>>>>);
impl AuthenticatedObjects for Objects {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter().map(|id| {
            let value = self.0.borrow().get(id).cloned().ok_or(ContentError::MissingObject)?;
            if ObjectId::for_bytes(&value) != *id { return Err(ContentError::IdentityMismatch); }
            Ok(value)
        }).collect()
    }
}
#[derive(Clone, Default)]
struct RootCount(Rc<RefCell<u64>>);
impl FinalizedConsumer for RootCount {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if object.role() == layerfs_content::ObjectRole::FilesystemRoot {
            *self.0.borrow_mut() += 1;
        }
        Ok(())
    }
}

struct RefuseRelease { inner: FileBacking, releases: usize, runs: usize }
impl OrderingBacking for RefuseRelease {
    fn create_run(&mut self) -> ContentResult<Box<dyn OrderingRun>> {
        self.runs += 1;
        self.inner.create_run()
    }
    fn held_bytes(&self) -> u64 { self.inner.held_bytes() }
    fn peak_bytes(&self) -> u64 { self.inner.peak_bytes() }
    fn release(&mut self) -> ContentResult<()> {
        self.releases += 1;
        Err(ContentError::ResourceUnavailable { what: "diagnostic refused cleanup" })
    }
}

fn file_bytes(path: &Path) -> u64 {
    std::fs::read_dir(path).unwrap().map(|entry| entry.unwrap().metadata().unwrap().len()).sum()
}

fn main() {
    let parent = std::path::PathBuf::from(std::env::args_os().nth(1).expect("fresh output dir"));
    std::fs::create_dir(&parent).unwrap();
    let direct = parent.join("direct");
    std::fs::create_dir(&direct).unwrap();
    let mut backing = FileBacking::new(&direct);
    let mut run = backing.create_run().unwrap();
    run.append(&[7; 88]).unwrap();
    run.flush().unwrap();
    println!("direct: actual_bytes={} run_len={} held_bytes={} peak_bytes={}",
        file_bytes(&direct), run.len(), backing.held_bytes(), backing.peak_bytes());
    assert_eq!(file_bytes(&direct), 88);
    // Required behaviour: the physical owner counts what it owns.
    assert_eq!((backing.held_bytes(), backing.peak_bytes()), (88, 88), "required-accounting assertion");
    drop(run);
    assert_eq!(backing.held_bytes(), 0, "required: a dropped run returns its bytes");
    assert!(!backing.owns_storage(), "required: a dropped run removes its file");
    backing.release().unwrap();

    let operation = parent.join("operation");
    std::fs::create_dir(&operation).unwrap();
    let mut refused = RefuseRelease { inner: FileBacking::new(&operation), releases: 0, runs: 0 };
    let reader = Objects::default();
    let mut output = RootCount::default();
    let placeholder = ObjectId::for_bytes(b"component-only opaque content root");
    let directories = [DirectoryUpdate { parent: 1, changes: vec![
        (PathName::new("a").unwrap(), Some(2)), (PathName::new("b").unwrap(), Some(3)),
    ] }];
    let inodes = [1, 2, 3].map(|serial| InodeUpdate { serial, value: InodeValue {
        kind: if serial == 1 { InodeKind::Directory } else { InodeKind::RegularFile },
        namespace_ref_count: 0, content_root: placeholder, metadata_root: placeholder,
    } });
    let input = FilesystemInput { base: None, scope: scope_for_seed([7; 32]), root_serial: 1,
        directories: &directories, inodes: &inodes, new_inodes: &[1, 2, 3],
        resources: FilesystemResources { maximum_pending_records: 1, ..Default::default() },
    };
    let published = output.clone();
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut output);
        build_filesystem(&mut objects, &input, Some(&mut refused))
    };
    println!("operation: success={} runs_created={} release_calls={} files_bytes_after_return={} result={:?}",
        result.is_ok(), refused.runs, refused.releases, file_bytes(&operation), result);
    // Required behaviour: a refused cleanup fails the operation, cleanup is
    // attempted exactly once, nothing stays owned, and no root is published.
    assert!(result.is_err(), "required: a failed cleanup must fail the operation");
    assert!(refused.runs > 0, "must exercise actual run-backed ordering");
    assert_eq!(refused.releases, 1, "required: cleanup is attempted exactly once");
    assert_eq!(file_bytes(&operation), 0, "required: no run bytes survive the attempt");
    assert!(refused.inner.cleanup_failed(), "required: the failed cleanup is visible");
    assert_eq!(*published.0.borrow(), 0, "required: no root after failed cleanup");
}
