//! A project's Store pushed to a remote and pulled into a fresh location
//! has identical history (#506 R): the same Branches, Commits and files.
//! The push is a consistent snapshot (a Commit after it is not pulled); a
//! pull refuses to replace a Store that is open and replaces it once closed.
//!
//! `folder_round_trip` runs anywhere Store installs run (Linux, as the
//! daemons' user). `minio_round_trip` is ignored by default: start a MinIO
//! with `engine/crates/layerfs-remote-storage/tests/minio_service.sh`, set
//! its `LAYERFS_REMOTE_*`, then run it with `--ignored`.
#![cfg(target_os = "linux")]
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_history::BranchId;
use layerfs_persistence::{PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile};
use layerfs_store_service::remote::storage::minio::MinioSettings;
use layerfs_store_service::remote::storage::pieces;
use layerfs_store_service::remote::{self, Provider, Remotes, STORE_OBJECT};
use std::fs;
use std::path::{Path, PathBuf};

const MIB: usize = 1024 * 1024;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// A directory on the target's filesystem, which carries the Store's locks.
fn scratch(label: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("layerfs-remote-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// Incompressible bytes, so the Store outgrows one multipart part.
fn noise(n: usize, mut state: u64) -> Vec<u8> {
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn write(f: &support::Fixture, branch: &str, path: &str, content: &str) {
    f.call(
        "fs.write",
        vec![
            ("branch", t(branch)),
            ("path", t(path)),
            ("content", t(content)),
        ],
    );
}

/// Everything the history calls report: Branches, each Branch's Commits and
/// the files read back (a refusal, such as `main` having no Commit list, is
/// compared as well).
fn history(f: &support::Fixture) -> Vec<String> {
    let mut seen = vec![format!("{:?}", f.call("project.list_branches", vec![]))];
    for branch in ["main", "feat", "work"] {
        seen.push(format!(
            "{:?}",
            f.try_call("branch.list_commits", vec![("branch", t(branch))])
        ));
        for path in ["/seed.txt", "/notes.txt"] {
            seen.push(format!(
                "{:?}",
                f.try_call("fs.read", vec![("branch", t(branch)), ("path", t(path))])
            ));
        }
    }
    seen
}

/// Opens the Store at `store` as the SDK fixture opens an installed one.
fn open(store: &Path, directory: &Path) -> support::Fixture {
    fs::create_dir_all(directory).unwrap();
    support::Fixture::open(installed::Fixture {
        directory: directory.to_owned(),
        config: PersistenceConfig::sqlite(store)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        branch: BranchId::from_authority([24; 16]),
        count: 2,
    })
}

fn round_trip(label: &str, provider: Provider, big: usize) {
    let work = scratch(label);
    let source = support::Fixture::new(label, |root| {
        fs::write(root.join("seed.txt"), b"seed\n").unwrap();
        fs::write(root.join("big.bin"), noise(big, 0x9e37_79b9_7f4a_7c15)).unwrap();
        2
    });
    source.fork("feat");
    source.fork("work");
    write(&source, "work", "/notes.txt", "work one\n");
    write(&source, "work", "/notes.txt", "work two\n");
    write(&source, "feat", "/notes.txt", "feat one\n");
    write(&source, "feat", "/seed.txt", "seed on feat\n");
    let before = history(&source);

    let driver = provider.driver().unwrap();
    let pushed = remote::push(&source.service.opened, driver.as_ref(), &work).unwrap();
    assert!(pushed.bytes as usize > big, "{pushed:?}");
    assert!(pushed.cleanup.is_none());
    // the snapshot is removed; the remote holds the file and its sha256
    assert_eq!(fs::read_dir(&work).unwrap().count(), 0);
    let stat = driver.stat(STORE_OBJECT).unwrap().unwrap();
    assert_eq!(stat.bytes, pushed.bytes);
    assert_eq!(stat.metadata[pieces::SHA256], pieces::hex(&pushed.sha256));
    // a Commit after the push is not in the pushed Store
    write(&source, "work", "/notes.txt", "after the push\n");
    assert_ne!(history(&source), before);

    // a project volume; the pull creates its global/ directory
    fs::create_dir(work.join("volume")).unwrap();
    let destination = work.join("volume/global/store.sqlite");
    let pulled = remote::pull(
        driver.as_ref(),
        &destination,
        installed::BINDING,
        installed::CURSOR,
    )
    .unwrap();
    assert_eq!((pulled.bytes, pulled.sha256), (pushed.bytes, pushed.sha256));
    assert!(!pulled.replaced);
    let opened = open(&destination, &work.join("first"));
    assert_eq!(history(&opened), before);

    // an open local Store is not replaced; the verified download is kept
    let refused = remote::pull(
        driver.as_ref(),
        &destination,
        installed::BINDING,
        installed::CURSOR,
    )
    .unwrap_err();
    assert!(refused.cause.contains("is open"), "{refused}");
    let left = refused.left.expect("the temporary pulled file");
    assert!(left.ends_with("store.sqlite.pulling"));
    fs::remove_file(left).unwrap();
    write(
        &opened,
        "feat",
        "/notes.txt",
        "local change, replaced by the pull\n",
    );
    let host = opened.close();

    // once closed, the pull replaces it
    let again = remote::pull(
        driver.as_ref(),
        &destination,
        installed::BINDING,
        installed::CURSOR,
    )
    .unwrap();
    assert!(again.replaced);
    let reopened = open(&destination, &work.join("second"));
    assert_eq!(history(&reopened), before);
    reopened.close();
    drop(host);

    // a Store of another project does not open with this binding
    fs::create_dir_all(work.join("other")).unwrap();
    let other = work.join("other/global/store.sqlite");
    let refused = remote::pull(driver.as_ref(), &other, b"another", [1; 32]).unwrap_err();
    assert!(refused.cause.contains("does not open"), "{refused}");
    assert!(!other.exists());

    pieces::delete(driver.as_ref(), STORE_OBJECT).unwrap();
    source.cleanup();
    fs::remove_dir_all(work).unwrap();
}

#[test]
fn folder_round_trip() {
    let folder = scratch("folder-remote");
    round_trip("folder", Provider::LocalFolder(folder.clone()), MIB);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
#[ignore = "needs MinIO: layerfs-remote-storage/tests/minio_service.sh"]
fn minio_round_trip() {
    let mut settings = MinioSettings::from_env().expect("LAYERFS_REMOTE_* of a running MinIO");
    settings.prefix = format!("tests/remote-store-{}", std::process::id());
    // a Store above one 5 MiB part goes up as a multipart upload
    settings.part_size = 5 * MIB as u64;
    let provider = Provider::Minio(settings);

    // the project's settings keep a credential reference, never the secret
    let work = scratch("settings");
    let mut remotes = Remotes::default();
    remotes.set("origin", provider.clone()).unwrap();
    remotes.save(&work.join("remotes")).unwrap();
    let text = fs::read_to_string(work.join("remotes")).unwrap();
    let secret = std::env::var("LAYERFS_REMOTE_SECRET_KEY").unwrap();
    assert!(!text.contains(&secret));
    assert!(text.contains("credentials env:LAYERFS_REMOTE"));
    let loaded = Remotes::load(&work.join("remotes")).unwrap();
    assert_eq!(loaded, remotes);
    fs::remove_dir_all(work).unwrap();

    round_trip("minio", loaded.get("origin").unwrap().clone(), 12 * MIB);
}
