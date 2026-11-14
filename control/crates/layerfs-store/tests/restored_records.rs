//! A restored Store's project records (#668) without a bucket, a daemon or
//! root: the record a restore writes, read back the way `sandbox.create`
//! reads a project (`layerfs_store::project::discovery::project`, then the
//! installed manifest).
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::provision::{ProviderKind, StoreProfile};
use layerfs_history::{BranchId, LayerStackId};
use layerfs_home::home::{paths, private_dir};
use layerfs_store::backup::restored::{records, Found, Main, Restored};
use layerfs_store::identity::{Identity, Kind};
use layerfs_store::project::add;
use layerfs_store::project::discovery;
use layerfs_store::project::record::ProjectRecord;
use std::path::{Path, PathBuf};

const STORE: &str = "box";
const PROJECT: &str = "proj";
const SQLITE: &str = "3.45.1";

/// A private home of this test's own under the system's temporary folder.
fn home(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "layerfs-restored-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    private_dir(&dir).expect("a private home");
    dir
}

/// The Store file a restore installs, as its staging folder holds it.
fn staged(home: &Path) -> PathBuf {
    let file = home.join("staged.sqlite");
    std::fs::write(&file, b"a restored Store file").expect("a staged Store file");
    file
}

fn identity() -> Identity {
    Identity {
        kind: Kind::Sqlite,
        binding: vec![7; 32],
        cursor_key: [9; 32],
    }
}

fn main_branch() -> Main {
    Main {
        stack: LayerStackId::from_authority([3; 16]),
        branch: BranchId::from_authority([4; 16]),
        root: [5; 32],
    }
}

/// What a restore of one project's Store hands [`records`].
fn restore(home: &Path, sqlite: &str) -> Restored {
    Restored {
        identity: identity(),
        file: staged(home),
        sqlite: sqlite.to_owned(),
    }
}

/// One project the restored Store holds, with its `main` Branch.
fn listed() -> Vec<Found> {
    vec![Found {
        name: PROJECT.to_owned(),
        stack: SdkValue::text("stack"),
        main: Some(main_branch()),
    }]
}

#[test]
fn a_restored_project_reads_as_a_sandbox_reads_it() {
    let home = home("reads");
    let restored = restore(&home, SQLITE);
    let written = records(&home, STORE, &restored, &listed())
        .expect("the records")
        .write(&home)
        .expect("written");
    assert_eq!(written, SdkValue::List(vec![SdkValue::text(PROJECT)]));

    // the read `sandbox.create` makes: the project record with its manifest,
    // then the installed manifest a Sandbox opens
    let project = discovery::project(Some(&home), Some(STORE), Some(PROJECT))
        .expect("a Sandbox finds the restored project");
    assert_eq!(project.manifest.provider, ProviderKind::Sqlite);
    assert_eq!(project.manifest.profile, StoreProfile::Disposable);
    assert_eq!(project.manifest.binding, vec![7; 32]);
    assert_eq!(project.manifest.cursor_key, [9; 32]);
    assert_eq!(project.manifest.stack, main_branch().stack.to_bytes());
    assert_eq!(project.manifest.branch, main_branch().branch.to_bytes());
    assert_eq!(project.manifest.root, [5; 32]);
    assert_eq!(
        project.manifest.bytes,
        std::fs::metadata(&restored.file)
            .expect("the Store file")
            .len()
    );
    assert_eq!(project.manifest.host_sqlite, SQLITE);
    assert_eq!(project.manifest.daemon_sqlite, None);
    let installed = project.installed.expect("an installed manifest");
    assert_eq!(installed.host_sqlite, SQLITE);
    assert_eq!(installed.daemon_sqlite.as_deref(), Some(SQLITE));
    // the record is `project.init`'s: its time is when it was restored
    let record = ProjectRecord::read(&home, STORE, PROJECT)
        .expect("the record")
        .expect("a record");
    assert!(record.created().is_some());
}

#[test]
fn a_complete_record_is_left_as_it_is() {
    let home = home("kept");
    let restored = restore(&home, SQLITE);
    let (manifest, installed) = add::manifest(
        &identity(),
        &restored.file,
        main_branch().stack,
        main_branch().branch,
        main_branch().root,
        SQLITE,
    )
    .expect("the manifests");
    ProjectRecord::from_record(
        STORE,
        PROJECT,
        SdkValue::map([
            (
                "manifest",
                SdkValue::Bytes(manifest.encode().expect("a manifest")),
            ),
            (
                "installed",
                SdkValue::Bytes(installed.encode().expect("an installed manifest")),
            ),
            ("created", SdkValue::Int(11)),
            ("note", SdkValue::text("kept")),
        ]),
    )
    .write(&home)
    .expect("a complete record");
    let path = paths::project_record(&home, STORE, PROJECT);
    let before = std::fs::read(&path).expect("the record");
    records(&home, STORE, &restored, &listed())
        .expect("the records")
        .write(&home)
        .expect("written");
    assert_eq!(std::fs::read(&path).expect("the record"), before);
    let record = ProjectRecord::read(&home, STORE, PROJECT)
        .expect("the record")
        .expect("a record");
    assert_eq!(record.created(), Some(&SdkValue::Int(11)));
}

#[test]
fn an_incomplete_record_is_completed_and_keeps_its_own_fields() {
    let home = home("incomplete");
    ProjectRecord::from_record(
        STORE,
        PROJECT,
        SdkValue::map([
            ("name", SdkValue::text(PROJECT)),
            ("stack", SdkValue::text("stack")),
            ("created", SdkValue::Int(42)),
        ]),
    )
    .write(&home)
    .expect("a record from an older restore");
    let restored = restore(&home, SQLITE);
    records(&home, STORE, &restored, &listed())
        .expect("the records")
        .write(&home)
        .expect("written");
    let project = discovery::project(Some(&home), Some(STORE), Some(PROJECT))
        .expect("a Sandbox finds the completed project");
    assert!(project.installed.is_some());
    let record = ProjectRecord::read(&home, STORE, PROJECT)
        .expect("the record")
        .expect("a record");
    assert_eq!(record.created(), Some(&SdkValue::Int(42)));
}

#[test]
fn a_manifest_that_does_not_encode_refuses_the_restore() {
    // a backup's SQLite version is not checked when its metadata is read:
    // one a manifest cannot hold refuses, naming the project and the field,
    // rather than leaving the project without its manifest
    for (label, sqlite) in [("too long", "1".repeat(65)), ("empty", String::new())] {
        let home = home("refused");
        let restored = restore(&home, &sqlite);
        let refusal = records(&home, STORE, &restored, &listed())
            .err()
            .unwrap_or_else(|| panic!("{label}: the record was rebuilt"));
        assert_eq!(refusal.code, "Failed", "{label}");
        assert!(
            refusal.message.contains("project proj of Store box"),
            "{label}: {}",
            refusal.message
        );
        assert!(
            refusal.message.contains("its manifest does not encode"),
            "{label}: {}",
            refusal.message
        );
        // nothing was written: the restore refuses before it installs
        assert!(
            !paths::project_record(&home, STORE, PROJECT).exists(),
            "{label}"
        );
    }
}

#[test]
fn a_project_without_a_main_branch_keeps_the_listing_record() {
    let home = home("no-main");
    let restored = restore(&home, SQLITE);
    let found = [Found {
        name: PROJECT.to_owned(),
        stack: SdkValue::text("stack"),
        main: None,
    }];
    records(&home, STORE, &restored, &found)
        .expect("the records")
        .write(&home)
        .expect("written");
    let record = ProjectRecord::read(&home, STORE, PROJECT)
        .expect("the record")
        .expect("a record");
    assert!(record.to_record().get("manifest").is_none());
    // the read a Sandbox makes still refuses it: unchanged by this fix
    let refusal = discovery::project(Some(&home), Some(STORE), Some(PROJECT))
        .err()
        .expect("refused");
    assert_eq!(refusal.code, "NoSuchProject");
}

#[test]
fn an_entry_of_a_project_the_store_lacks_is_removed() {
    let home = home("stale");
    private_dir(&paths::project_dir(&home, STORE, "gone")).expect("a stale entry");
    let restored = restore(&home, SQLITE);
    records(&home, STORE, &restored, &listed())
        .expect("the records")
        .write(&home)
        .expect("written");
    assert!(!paths::project_dir(&home, STORE, "gone").exists());
}
