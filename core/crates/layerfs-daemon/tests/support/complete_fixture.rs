//! Native source creation and authenticated installation, retaining failures.
use super::{bytes, support};
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_daemon::{
    bootstrap::{open_store_observed, OpenedStore},
    install::receive_install,
    install_types::StoreSettings,
};
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sdk::{initialize, InitRequest};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{FileExt, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Mixed,
    Wide,
    Dense,
    Sparse,
}
pub const NAMES: usize = 100_000;
pub const FILES: [(&str, &[u8]); 8] = [
    (
        ".gitignore",
        b"node_modules/\n.cache/\noutput/\nignored.bin\n",
    ),
    (".git/index", b"git-index\0\xff"),
    (".git/objects/aa/object", b"git-object"),
    ("node_modules/pkg/index.js", b"module.exports=42"),
    (".cache/tool/data", b"cached\0data"),
    ("output/build/result", b"binary-output"),
    ("ignored.bin", b"\0\xff\x01\0"),
    ("empty-file", b""),
];
pub const LINKS: [(&str, &[u8]); 5] = [
    ("dependency-link", b"node_modules/pkg"),
    ("broken", b"missing"),
    ("self", b"self"),
    ("outside", b"../external"),
    ("opaque", b"x/\xff"),
];
pub const DIRS: [&str; 13] = [
    "",
    ".git",
    ".git/objects",
    ".git/objects/aa",
    "node_modules",
    "node_modules/pkg",
    ".cache",
    ".cache/tool",
    "output",
    "output/build",
    "empty",
    ".cache/empty",
    "output/empty",
];
pub struct Fixture {
    pub directory: PathBuf,
    pub opened: OpenedStore,
    pub manifest: layerfs_bridge::provision::StoreManifest,
    pub metadata: Vec<(String, PortableMetadata)>,
    pub file_metadata: PortableMetadata,
    pub root_metadata: PortableMetadata,
    pub shape: Shape,
}
pub fn metadata(path: &Path) -> PortableMetadata {
    let m = fs::symlink_metadata(path).unwrap();
    PortableMetadata {
        mode: if m.file_type().is_symlink() {
            0o777
        } else {
            m.mode() & if m.is_dir() { 0o1777 } else { 0o777 }
        },
        mtime_seconds: m.mtime(),
        mtime_nanoseconds: m.mtime_nsec() as u32,
    }
}
pub fn name(n: usize) -> String {
    format!("entry-{n:06}")
}
pub fn body(n: usize) -> Vec<u8> {
    format!("complete-native-entry:{n:06}\0").into_bytes()
}
fn fixed(file: &File) {
    file.set_permissions(fs::Permissions::from_mode(0o640))
        .unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_003, 456_789_123))
        .unwrap();
}
impl Fixture {
    pub fn new(shape: Shape) -> Self {
        let directory =
            std::env::temp_dir().join(format!("layerfs-q1-{}-{shape:?}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let source = directory.join("source");
        fs::create_dir(&source).unwrap();
        let mut expected = Vec::new();
        let file_metadata = match shape {
            Shape::Mixed => {
                for name in DIRS {
                    fs::create_dir_all(source.join(name)).unwrap();
                }
                for (name, body) in FILES {
                    let mut f = File::create(source.join(name)).unwrap();
                    f.write_all(body).unwrap();
                    fixed(&f);
                }
                fs::hard_link(source.join(".git/index"), source.join(".cache/index-alias"))
                    .unwrap();
                fs::hard_link(source.join(".git/index"), directory.join("outside-index")).unwrap();
                fs::write(source.join("output/index-copy"), FILES[1].1).unwrap();
                for (name, target) in LINKS {
                    std::os::unix::fs::symlink(
                        std::ffi::OsStr::from_bytes(target),
                        source.join(name),
                    )
                    .unwrap();
                }
                fs::write(directory.join("external"), b"must not enter root").unwrap();
                fs::set_permissions(source.join("empty"), fs::Permissions::from_mode(0o1750))
                    .unwrap();
                for name in DIRS
                    .into_iter()
                    .chain(FILES.map(|x| x.0))
                    .chain(LINKS.map(|x| x.0))
                    .chain([".cache/index-alias", "output/index-copy"])
                {
                    expected.push((name.to_owned(), metadata(&source.join(name))));
                }
                metadata(&source.join(".git/index"))
            }
            Shape::Wide => {
                for n in 0..NAMES {
                    let mut f = File::create(source.join(name(n))).unwrap();
                    f.write_all(&body(n)).unwrap();
                    fixed(&f);
                }
                metadata(&source.join(name(0)))
            }
            Shape::Dense | Shape::Sparse => {
                let mut f = File::create(source.join("file")).unwrap();
                if matches!(shape, Shape::Dense) {
                    let mut buf = vec![0; bytes::WINDOW];
                    let mut offset = 0;
                    while offset < bytes::DENSE {
                        let n = (bytes::DENSE - offset).min(buf.len() as u64) as usize;
                        bytes::fill(bytes::Pattern::Dense, offset, &mut buf[..n]);
                        f.write_all(&buf[..n]).unwrap();
                        offset += n as u64;
                    }
                } else {
                    f.set_len(bytes::SPARSE).unwrap();
                    for (at, data) in bytes::ISLANDS {
                        f.write_all_at(data, at).unwrap();
                    }
                }
                fixed(&f);
                let m = f.metadata().unwrap();
                let allocated = m.blocks() * 512;
                if matches!(shape, Shape::Dense) {
                    assert!(allocated >= bytes::DENSE);
                } else {
                    assert!(allocated < 2 * 1024 * 1024);
                }
                println!(
                    "Q1_NATIVE shape={shape:?} logical={} allocated={allocated}",
                    m.len()
                );
                metadata(&source.join("file"))
            }
        };
        let root_metadata = metadata(&source);
        let destination = directory.join("store.sqlite");
        let project = Timing::disabled("q1.init", |scope| {
            initialize(
                InitRequest {
                    source: source.clone(),
                    store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
                        .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
                    locator: destination.to_str().unwrap().into(),
                    policy: StoragePolicy::frozen_default(),
                    catalog: HistoryCatalogConfig {
                        binding_key: b"q1-installed".to_vec(),
                        cursor_key: [37; 32],
                        incarnation: 1,
                    },
                    stack: LayerStackId::from_authority([38; 16]),
                    stack_name: HistoryName::new("complete").unwrap(),
                    branch: BranchId::from_authority([39; 16]),
                    branch_name: HistoryName::new("main").unwrap(),
                    scope_seed: [40; 32],
                    deadline: Instant::now() + Duration::from_secs(30),
                },
                scope,
            )
        })
        .0
        .unwrap();
        println!(
            "Q1_INIT shape={shape:?} entries={} sealed_bytes={} host_sqlite={}",
            project.initialized.entries, project.store.bytes, project.manifest.host_sqlite
        );
        fs::remove_dir_all(source).unwrap();
        if matches!(shape, Shape::Wide) {
            fs::copy(&project.store.path, directory.join("prepared.sqlite")).unwrap();
            fs::write(
                directory.join("prepared.manifest"),
                project.manifest.encode().unwrap(),
            )
            .unwrap();
            fs::write(
                directory.join("expected-metadata.txt"),
                format!(
                    "{} {} {} {} {} {}\n",
                    file_metadata.mode,
                    file_metadata.mtime_seconds,
                    file_metadata.mtime_nanoseconds,
                    root_metadata.mode,
                    root_metadata.mtime_seconds,
                    root_metadata.mtime_nanoseconds
                ),
            )
            .unwrap();
        }

        let (mut channel, worker) = support::pair(move |channel| {
            receive_install(channel, &destination, StoreSettings::default())
        });
        let ack = layerfs_sdk::install(&project, &mut channel).unwrap();
        let installed = worker.join().unwrap();
        fs::remove_file(project.store.path).unwrap();
        println!(
            "Q1_INSTALL shape={shape:?} bytes={} records={} max_record={} daemon_sqlite={}",
            ack.work.sent_bytes,
            ack.work.sent_records,
            ack.work.buffer_bytes,
            installed.manifest.daemon_sqlite.as_deref().unwrap()
        );
        Self {
            directory,
            opened: installed.opened,
            manifest: installed.manifest,
            metadata: expected,
            file_metadata,
            root_metadata,
            shape,
        }
    }
    pub fn prepared_wide(prepared: &Path) -> Self {
        let directory =
            std::env::temp_dir().join(format!("layerfs-q1-{}-Wide-clone", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let mut manifest = layerfs_bridge::provision::StoreManifest::decode(
            &fs::read(prepared.join("prepared.manifest")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            manifest.profile,
            layerfs_bridge::provision::StoreProfile::Disposable
        );
        let expected = fs::read_to_string(prepared.join("expected-metadata.txt")).unwrap();
        let fields = expected.split_whitespace().collect::<Vec<_>>();
        assert_eq!(fields.len(), 6);
        let file_metadata = PortableMetadata {
            mode: fields[0].parse().unwrap(),
            mtime_seconds: fields[1].parse().unwrap(),
            mtime_nanoseconds: fields[2].parse().unwrap(),
        };
        let root_metadata = PortableMetadata {
            mode: fields[3].parse().unwrap(),
            mtime_seconds: fields[4].parse().unwrap(),
            mtime_nanoseconds: fields[5].parse().unwrap(),
        };
        let database = directory.join("store.sqlite");
        let mut source = File::open(prepared.join("prepared.sqlite")).unwrap();
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&database)
            .unwrap();
        let mut window = vec![0; bytes::WINDOW];
        let mut copied = 0;
        loop {
            let read = source.read(&mut window).unwrap();
            if read == 0 {
                break;
            }
            target.write_all(&window[..read]).unwrap();
            copied += read as u64;
        }
        drop((source, target));
        assert_eq!(copied, manifest.bytes);
        manifest.locator = database.to_str().unwrap().into();
        let opened = open_store_observed(
            PersistenceConfig::sqlite(&database)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            &manifest.binding,
            manifest.cursor_key,
            4,
            8 * 1024 * 1024,
            layerfs_storage::ReservationBlocks::default(),
        )
        .unwrap();
        println!("Q1_CLONE setup=clone bytes={copied} provider=buffered_byte_copy window=131072 original_retained=true no_native_source=true prepared={}",prepared.display());
        Self {
            directory,
            opened,
            manifest,
            metadata: Vec::new(),
            file_metadata,
            root_metadata,
            shape: Shape::Wide,
        }
    }
    pub fn cleanup(self) {
        drop(self.opened);
        fs::remove_dir_all(self.directory).unwrap();
    }
}
