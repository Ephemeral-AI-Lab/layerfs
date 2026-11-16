//! The install step of `backup.restore` (#570 P1, review P2-1). The restore
//! checks `store/<name>` when it starts, but the download between that check
//! and the install is a long window: the folder can appear, or lose its
//! privacy, while it runs, and `home::private_dir` keeps an existing folder's
//! mode. The install therefore checks the target's folder again, under the
//! home lock, before it links the Store into it: a folder that is not the
//! daemon's private one is refused with `store.create`'s code and remedy and
//! nothing is installed; a private one, and an absent one (made `0700`), are
//! installed. These tests run in an ordinary user's process: no bucket, no
//! install permission, no timing.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::home::{self, paths};
use layerfs_store::backup::restore::install;
use layerfs_store::record::StoreRecord;
use layerfs_store::Related;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// A home whose own folder is private, as `backup.restore` requires.
fn scratch(label: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "layerfs-restore-install-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&home)
        .unwrap();
    home::layout::create(&home).unwrap();
    home::private_dir(&home.join("store")).unwrap();
    home
}

/// The `InUse` checks a new Store's install never asks.
fn unused(home: &Path, name: &str) -> Result<(), SdkRefusal> {
    panic!(
        "a new Store's install asks no user: {} in {}",
        name,
        home.display()
    );
}

fn related() -> Related {
    Related {
        store_idle: unused,
        can_remove_project: unused,
    }
}

/// The downloaded Store, staged as `backup.restore` leaves it before the
/// install.
fn staged(home: &Path) -> PathBuf {
    let folder = home.join("store/.restore-fixture");
    home::private_dir(&folder).unwrap();
    let file = folder.join("store.sqlite");
    std::fs::write(&file, b"the downloaded Store\n").unwrap();
    file
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// `store/<name>` made `0755` while the restore downloads: the install
/// refuses it, with `store.create`'s `Unsupported` and `chmod 700` remedy,
/// before it links the Store or writes the record. The user's folder keeps
/// its mode, nothing of the restore is installed and the staged file stays
/// for the restore to remove with its staging folder; once the folder is
/// private again the same install goes through.
#[test]
fn a_folder_made_unprivate_during_the_download_is_refused_before_the_install() {
    let home = scratch("window");
    let store = "shop";
    let directory = paths::store_dir(&home, store);
    let staged = staged(&home);
    let record = StoreRecord::restored(store, b"binding", &[7; 32]);
    let _lock = home::lock::take(&home).unwrap();

    // the restore's own first check saw no folder; the download ran
    assert!(!directory.exists());
    std::fs::create_dir(&directory).unwrap();
    set_mode(&directory, 0o755);

    let refusal = install(&home, store, "bucket", None, &record, &staged, related())
        .expect_err("a 0755 folder is not the daemon's private directory");
    assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
    assert!(
        refusal.message.contains(&directory.display().to_string()),
        "{}",
        refusal.message
    );
    assert!(
        refusal.message.contains("0755") && refusal.message.contains("0700"),
        "{}",
        refusal.message
    );
    assert!(
        refusal
            .message
            .contains(&format!("chmod 700 {}", directory.display())),
        "{}",
        refusal.message
    );
    assert_eq!(mode(&directory), 0o755, "the folder's mode is not changed");
    assert!(!paths::store_file(&home, store).exists());
    assert!(!paths::store_record(&home, store).exists());
    assert_eq!(
        std::fs::read(&staged).unwrap(),
        b"the downloaded Store\n",
        "nothing was linked out of the staging folder"
    );

    // the folder made private again: this install is the one that goes in
    set_mode(&directory, 0o700);
    install(&home, store, "bucket", None, &record, &staged, related()).unwrap();
    assert_eq!(mode(&directory), 0o700);
    assert_eq!(
        std::fs::read(paths::store_file(&home, store)).unwrap(),
        b"the downloaded Store\n"
    );
    let written = StoreRecord::read(&home, store).unwrap().unwrap();
    assert_eq!(written.binding(), Some(&b"binding"[..]));
    let _ = std::fs::remove_dir_all(&home);
}

/// A Store this restore replaces lost its privacy during the download: its
/// own folder is refused before the replacement's checks run, so the download
/// is not installed over the Store either.
#[test]
fn a_replaced_stores_folder_is_checked_again_before_the_install() {
    let home = scratch("replacing");
    let store = "shop";
    let directory = paths::store_dir(&home, store);
    let staged = staged(&home);
    let existing = StoreRecord::restored(store, b"binding", &[9; 32]);
    existing.write(&home).unwrap();
    set_mode(&directory, 0o755);
    let _lock = home::lock::take(&home).unwrap();

    let refusal = install(
        &home,
        store,
        "bucket",
        Some(&existing),
        &existing,
        &staged,
        related(),
    )
    .expect_err("a 0755 folder is not the daemon's private directory");
    assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
    assert!(refusal.message.contains("chmod 700"), "{}", refusal.message);
    // the guard's refusal, not the replacement's own: the Store was not
    // reached, so it still holds what it had and its folder its mode
    assert!(
        !refusal.message.contains("changed since"),
        "{}",
        refusal.message
    );
    assert!(!paths::store_file(&home, store).exists());
    assert_eq!(mode(&directory), 0o755);
    assert_eq!(
        StoreRecord::read(&home, store).unwrap().unwrap().binding(),
        Some(&b"binding"[..])
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// A folder the download left absent is made `0700` by the install, which is
/// the restore's own new-Store path: the Store file and its record are
/// written, and the folder is one every opener accepts.
#[test]
fn an_absent_folder_is_made_private_by_the_install() {
    let home = scratch("absent");
    let store = "shop";
    let staged = staged(&home);
    let record = StoreRecord::restored(store, b"binding", &[3; 32]);
    let _lock = home::lock::take(&home).unwrap();
    assert!(!paths::store_dir(&home, store).exists());

    install(&home, store, "bucket", None, &record, &staged, related()).unwrap();
    assert_eq!(mode(&paths::store_dir(&home, store)), 0o700);
    assert_eq!(
        std::fs::read(paths::store_file(&home, store)).unwrap(),
        b"the downloaded Store\n"
    );
    assert!(paths::store_record(&home, store).is_file());
    let _ = std::fs::remove_dir_all(&home);
}
