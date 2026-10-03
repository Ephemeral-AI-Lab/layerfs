#![allow(dead_code)]
use layerfs_content::{
    filesystem::attributes::read::{read_portable, AttributeReadWork},
    read_all, FilesystemRead, LogicalPath,
};
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId};
use layerfs_project::{init, InitRequest, Initialized};
use layerfs_storage::{
    port::{MetadataStore, ObjectStore},
    Storage,
};
use layerfs_telemetry::timer::Timing;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub mod memory_history;
#[path = "../../../layerfs-storage/tests/support/memory_metadata.rs"]
pub mod memory_metadata;
pub mod memory_objects;
pub struct Fixture {
    pub path: PathBuf,
    pub source: PathBuf,
    pub expected: BTreeMap<String, Vec<u8>>,
}
impl Fixture {
    pub fn new(count: usize) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-project-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = path.join("input");
        fs::create_dir_all(&source).unwrap();
        let mut expected = BTreeMap::new();
        for n in 0..count {
            let name = format!("d{}/f{n:04}", n % 10);
            let body = (0..(n % 17 + 1) * 128)
                .map(|i| (i + n) as u8)
                .collect::<Vec<_>>();
            fs::create_dir_all(source.join(format!("d{}", n % 10))).unwrap();
            fs::write(source.join(&name), &body).unwrap();
            expected.insert(name, body);
        }
        Self {
            path,
            source,
            expected,
        }
    }
    pub fn run(&self, storage: &Storage, history: &dyn HistoryCatalog) -> Initialized {
        Timing::disabled("project.init", |scope| {
            init(
                storage,
                history,
                InitRequest {
                    source: &self.source,
                    scratch_parent: &self.path,
                    stack: LayerStackId::from_authority([19; 16]),
                    name: HistoryName::new("main").unwrap(),
                    scope_seed: [29; 32],
                    deadline: Instant::now() + Duration::from_secs(60),
                },
                scope,
            )
        })
        .0
        .unwrap()
    }
    pub fn verify_namespace(
        &self,
        storage: &Storage,
        history: &dyn HistoryCatalog,
        initialized: &Initialized,
    ) {
        let reader = storage.reader().unwrap();
        let root = initialized.root;
        let stack = history.layer_stack(initialized.stack.id).unwrap().unwrap();
        assert_eq!(history.layer(stack.head_layer).unwrap().unwrap().root, root);
        let mut filesystem = FilesystemRead::new(
            &reader,
            layerfs_content::filesystem::root::FilesystemRootId(root),
        )
        .unwrap();
        let mut seen = BTreeMap::new();
        let mut pending = vec![String::new()];
        while let Some(path) = pending.pop() {
            let logical = LogicalPath::new(&path).unwrap();
            let inode = filesystem.resolve(&logical).unwrap().value;
            let portable = read_portable(
                &reader,
                inode.metadata_root,
                inode.kind,
                &mut AttributeReadWork::default(),
            )
            .unwrap();
            let metadata = fs::symlink_metadata(self.source.join(&path)).unwrap();
            assert_eq!(portable.mode, metadata.mode() & 0o7777);
            assert_eq!(portable.mtime_seconds, metadata.mtime());
            assert_eq!(portable.mtime_nanoseconds as i64, metadata.mtime_nsec());
            if metadata.is_dir() {
                let mut after = None;
                loop {
                    let page = filesystem
                        .list(&logical, after.as_ref(), 128, 16384)
                        .unwrap();
                    for (name, _) in page.entries {
                        pending.push(if path.is_empty() {
                            name.as_str().to_owned()
                        } else {
                            format!("{path}/{}", name.as_str())
                        });
                    }
                    match page.continuation {
                        Some(next) => after = Some(next),
                        None => break,
                    }
                }
            } else {
                let mut body = Vec::new();
                Timing::disabled("verify", |scope| {
                    read_all(&reader, inode.content_root, &mut body, scope.child("file"))
                })
                .0
                .unwrap();
                assert!(seen.insert(path.clone(), body.clone()).is_none());
                assert_eq!(body, self.expected[&path]);
            }
        }
        assert_eq!(seen, self.expected);
        assert_eq!(initialized.entries, self.expected.len() as u64 + 11);
        assert!(!fs::read_dir(&self.path).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("ordering-")));
    }
    pub fn service_config(&self) -> layerfs_metadata::PgConfig {
        let mut c = layerfs_metadata::PgConfig::from_env().unwrap();
        c.schema = format!(
            "lfs302_project_{}_{}",
            std::process::id(),
            self.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .split('-')
                .next_back()
                .unwrap()
        );
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
pub fn storage(metadata: Arc<dyn MetadataStore>, objects: Arc<dyn ObjectStore>) -> Storage {
    Storage::new(metadata, objects).unwrap()
}
