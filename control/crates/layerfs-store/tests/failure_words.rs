//! Store failures in words (LF2 review FAIL 2): a `PersistenceError`'s own
//! `Display` prints Rust `Debug` text ("pack persistence: Refused { status:
//! .. }"); `words::persistence` says each outcome in words, and the Init,
//! import and acquisition words use it.
use layerfs_history::HistoryError;
use layerfs_project::store::InitError;
use layerfs_project::ProjectError;
use layerfs_storage::port::acquisition::AcquisitionError;
use layerfs_storage::port::PersistenceError;
use layerfs_store::words;

fn no_debug(text: &str) {
    for debug in [
        "{",
        "}",
        "Some(",
        "Refused",
        "Busy",
        "Uncertain",
        "pack persistence",
    ] {
        assert!(!text.contains(debug), "Debug text {debug:?} in {text:?}");
    }
}

#[test]
fn every_persistence_outcome_reads_as_words() {
    let all = [
        PersistenceError::Busy,
        PersistenceError::BackendUnavailable,
        PersistenceError::Missing,
        PersistenceError::Refused {
            status: "schema exists".to_owned(),
        },
        PersistenceError::Malformed,
        PersistenceError::Uncertain,
    ];
    for error in &all {
        no_debug(&words::persistence(error));
    }
    let refused = PersistenceError::Refused {
        status: "schema exists".to_owned(),
    };
    assert_eq!(
        words::persistence(&refused),
        "the Store refused it: schema exists"
    );
    assert!(words::persistence(&PersistenceError::Uncertain).contains("not known"));
}

#[test]
fn init_import_and_acquisition_failures_use_the_same_words() {
    let busy = words::init_error(&InitError::Persistence(PersistenceError::Busy));
    assert_eq!(busy, words::persistence(&PersistenceError::Busy));
    let acquisition = AcquisitionError::Persistence(PersistenceError::Malformed);
    assert_eq!(
        words::project_error(&ProjectError::Acquisition(acquisition)),
        "the Store answered something malformed"
    );
}

#[test]
fn a_history_failure_keeps_the_history_crates_words() {
    // a moved head is said in words (its engine Display prints the moved
    // state as Debug); every other history failure as the history crate says
    assert_eq!(
        words::init_error(&InitError::History(HistoryError::Busy)),
        "busy"
    );
}

/// An older-format Store's refusal (#776): `OldStore` carries the version
/// refusal as its message; a daemon's `Store open: ...` text wrapping it
/// is read back to it, as `sandbox remove` reads a refusal; any other
/// refusal carries none. Its fix removes a sqlite Store and disconnects a
/// PostgreSQL one, which keeps its data.
///
/// The version refusal is the engine's own, from an open of a Store whose
/// header says schema version 2, so the test follows its wording.
#[test]
fn an_old_format_store_is_named_by_its_version_refusal() {
    use layerfs_home::outcome::refused;
    use layerfs_store::state::{outdated_fix, outdated_status, OLD_STORE};
    let Some(raised) = schema_2_open_refusal() else {
        return;
    };
    assert_eq!(raised.code, OLD_STORE, "{raised:?}");
    let version = raised.message.as_str();
    assert_eq!(
        layerfs_persistence::outdated_store(version),
        Some(version),
        "the message is the engine's version refusal itself"
    );
    assert_eq!(outdated_status(&raised).as_deref(), Some(version));
    let wrapped = format!(
        "Store open: physical Store I/O: pack persistence: Refused {{ status: {version:?} }}"
    );
    let daemon = refused("Failed", wrapped);
    assert_eq!(outdated_status(&daemon).as_deref(), Some(version));
    let other = refused(
        "Failed",
        "Store open: pack persistence: Refused { status: \"Closed\" }",
    );
    assert_eq!(outdated_status(&other), None);
    // schema version 2 has a step (#804): its fix is the upgrade
    assert_eq!(
        outdated_fix("shop", false, version),
        "layerfs store upgrade shop"
    );
    // a refusal no step takes keeps remove / disconnect
    let none = "SQLite Store schema version 1: not supported by this release";
    assert_eq!(
        outdated_fix("shop", false, none),
        "layerfs store remove shop"
    );
    assert_eq!(
        outdated_fix("my pg", true, none),
        "layerfs store disconnect 'my pg'"
    );
}

/// The refusal an open of sqlite Store `shop` gets once its header says
/// schema version 2 (`user_version` 36, at offset 60 of the closed file);
/// `None` where this filesystem cannot hold a directly opened Store.
fn schema_2_open_refusal() -> Option<layerfs_protocol::sdk::SdkRefusal> {
    use std::os::unix::fs::PermissionsExt;
    let home = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("layerfs-old-format-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    layerfs_home::home::open(&home).unwrap();
    let name = layerfs_home::names::StoreName::parse("shop").unwrap();
    if let Err(refusal) = layerfs_store::sqlite::create::create(&home, &name, None) {
        assert!(
            refusal.message.contains("does not carry SQLite locks"),
            "{refusal:?}"
        );
        eprintln!("NOT RUN: this filesystem cannot hold a directly opened Store");
        return None;
    }
    let file = layerfs_home::home::paths::store_file(&home, "shop");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes[60..64].copy_from_slice(&36u32.to_be_bytes());
    std::fs::write(&file, bytes).unwrap();
    let record = layerfs_store::record::StoreRecord::of(&home, "shop").unwrap();
    let open = layerfs_store::open_args::OpenArgs::in_root(
        &home,
        "shop",
        Some(record.to_record().clone()),
    );
    let refusal = layerfs_store::service::with_service(&open, |_| Ok(())).unwrap_err();
    let _ = std::fs::remove_dir_all(&home);
    Some(refusal)
}
