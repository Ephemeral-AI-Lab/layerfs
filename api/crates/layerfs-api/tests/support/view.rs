//! The decoded view of a filesystem root (#355 M1): one line per path with
//! its kind, content, mode, mtime and hard-link group, in path order. Two
//! merges are equivalent when their views are equal, whatever their root
//! identities.

use layerfs_content::compare::tree::{inode_changes, join, list_directory, TreeTable};
use layerfs_content::compare::DifferenceWork;
use layerfs_content::file::read_all_bounded;
use layerfs_content::filesystem::attributes::{read_portable, AttributeReadWork};
use layerfs_content::filesystem::{FilesystemRead, FilesystemRootId, LogicalPath, Resolved};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::object::{AuthenticatedObjects, ObjectId};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::Storage;
use layerfs_telemetry::timer::Timing;
use std::collections::BTreeMap;
use std::path::Path;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        text.push_str(&format!("{byte:02x}"));
        text
    })
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// A read-only opener of one Store file, for views.
pub struct Viewer {
    storage: Storage,
}

impl Viewer {
    pub fn open(store: &Path, binding: &[u8], cursor: [u8; 32]) -> Self {
        let config = PersistenceConfig::sqlite(store)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable);
        let handles = Handles::open_read_only(config, binding, cursor).unwrap();
        Self {
            storage: Storage::new(handles.storage).unwrap(),
        }
    }

    /// The view of the root named by `root` (hex). Without `mtime`, mtimes
    /// are left out (fixtures written at different times).
    pub fn view(&self, root: &str, mtime: bool) -> Vec<String> {
        let reader = self.storage.reader().unwrap();
        view(&reader, root, mtime)
    }

    /// The inode bound at `path` in the root named by `root` (hex).
    fn resolve(&self, root: &str, path: &str) -> Resolved {
        let reader = self.storage.reader().unwrap();
        let root = FilesystemRootId(ObjectId::from_bytes(&unhex(root)).unwrap());
        let path = LogicalPath::new(path.trim_start_matches('/')).unwrap();
        FilesystemRead::new(&reader, root)
            .unwrap()
            .resolve(&path)
            .unwrap()
    }

    /// The link count stored for the inode bound at `path` in the root
    /// named by `root` (hex).
    pub fn links(&self, root: &str, path: &str) -> u64 {
        self.resolve(root, path).value.namespace_ref_count
    }

    /// The bytes of the regular file bound at `path` in the root named by
    /// `root` (hex), as text.
    pub fn text(&self, root: &str, path: &str) -> String {
        let reader = self.storage.reader().unwrap();
        let file = self.resolve(root, path).value;
        assert_eq!(file.kind, InodeKind::RegularFile, "{path}");
        let mut bytes = Vec::new();
        Timing::disabled("view.text", |timing| {
            read_all_bounded(
                &reader,
                file.content_root,
                u64::MAX,
                &mut bytes,
                timing.child("read"),
            )
        })
        .0
        .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    /// Serials whose value differs between two roots (hex): what a merge by
    /// serial decides for that side.
    pub fn changed(&self, from: &str, to: &str) -> u64 {
        let reader = self.storage.reader().unwrap();
        let open = |root: &str| {
            let root = FilesystemRootId(ObjectId::from_bytes(&unhex(root)).unwrap());
            TreeTable::open(&reader, root).unwrap()
        };
        let mut count = 0;
        let mut work = DifferenceWork::default();
        inode_changes(&reader, &open(from), &open(to), &mut work, |_, _, _| {
            count += 1;
            Ok(())
        })
        .unwrap();
        count
    }
}

pub fn view(reader: &dyn AuthenticatedObjects, root: &str, mtime: bool) -> Vec<String> {
    let root = FilesystemRootId(ObjectId::from_bytes(&unhex(root)).unwrap());
    let tree = TreeTable::open(reader, root).unwrap();
    let mut lines: BTreeMap<String, (u64, String)> = BTreeMap::new();
    let mut groups: BTreeMap<u64, Vec<String>> = BTreeMap::new();
    let mut stack = vec![("/".to_owned(), tree.root_serial())];
    while let Some((path, serial)) = stack.pop() {
        let value = tree.value(reader, serial).unwrap().expect("bound serial");
        let metadata = read_portable(
            reader,
            value.metadata_root,
            value.kind,
            &mut AttributeReadWork::default(),
        )
        .unwrap();
        let time = if mtime {
            format!(
                " mtime={}.{:09}",
                metadata.mtime_seconds, metadata.mtime_nanoseconds
            )
        } else {
            String::new()
        };
        let (kind, content) = match value.kind {
            InodeKind::Directory => ("dir", String::new()),
            InodeKind::RegularFile => ("file", hex(value.content_root.as_bytes())),
            _ => ("symlink", hex(value.content_root.as_bytes())),
        };
        lines.insert(
            path.clone(),
            (
                serial,
                format!("{path} {kind} {content} mode={:o}{time}", metadata.mode),
            ),
        );
        groups.entry(serial).or_default().push(path.clone());
        if value.kind == InodeKind::Directory {
            list_directory(reader, value.content_root, |name, child| {
                stack.push((join(&path, name), child));
                Ok(true)
            })
            .unwrap();
        }
    }
    lines
        .into_values()
        .map(|(serial, line)| {
            let group = &groups[&serial];
            if group.len() > 1 {
                let first = group.iter().min().expect("one path");
                format!("{line} link={first}")
            } else {
                line
            }
        })
        .collect()
}
