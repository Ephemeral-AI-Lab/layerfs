//! Public-API diagnostic only; no performance or whole-stage acceptance claim.
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
impl FinalizedConsumer for Objects {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let (id, _, bytes, _) = object.into_parts();
        self.0.borrow_mut().insert(id, bytes);
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
    assert_eq!((backing.held_bytes(), backing.peak_bytes()), (0, 0), "observed-defect assertion");
    drop(run);
    backing.release().unwrap();

    let operation = parent.join("operation");
    std::fs::create_dir(&operation).unwrap();
    let mut refused = RefuseRelease { inner: FileBacking::new(&operation), releases: 0, runs: 0 };
    let reader = Objects::default();
    let mut output = reader.clone();
    let mut objects = FilesystemObjects::new(&reader, &mut output);
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
    let result = build_filesystem(&mut objects, &input, Some(&mut refused));
    println!("operation: success={} runs_created={} release_calls={} files_bytes_after_return={} result={:?}",
        result.is_ok(), refused.runs, refused.releases, file_bytes(&operation), result);
    assert!(result.is_ok(), "observed-defect assertion: operation currently bypasses failed cleanup");
    assert!(refused.runs > 0, "must exercise actual run-backed ordering");
    assert_eq!(refused.releases, 0, "observed-defect assertion: cleanup never called");
    assert!(file_bytes(&operation) > 0);
    assert!(refused.release().is_err(), "control: cleanup capability actually rejects");
    refused.inner.release().unwrap();
}
