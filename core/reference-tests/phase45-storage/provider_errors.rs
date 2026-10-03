//! The provider bridge keeps failure classes distinguishable from absence.
//!
//! A caller-authorized value root is allowed to name an object the Store does
//! not hold, so `MissingObject` must remain the answer for absence and for
//! nothing else: a corrupt pack, a record above the publication watermark or a
//! capacity refusal reaches C1 as `ContentError::ProviderFailure` instead.

mod support;

use layerfs_content::{AuthenticatedObjects, ContentError, ObjectId};
use support::{construct_file, create_store, noise, open_store, save_all, TempDir};

/// Corrupts the pack header magic of the pack holding `object`.
///
/// The header is validated before any group body is touched
/// (`pack::layout::parse_header`), so every object the pack holds becomes
/// unreadable - a store-integrity failure, never absence.
fn tamper_pack_of(path: &std::path::Path, object: ObjectId) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let pack_id: i64 = connection
        .query_row(
            "SELECT pack_id FROM objects WHERE object_id = ?1",
            rusqlite::params![object.as_bytes()],
            |row| row.get(0),
        )
        .expect("the object's locator");
    let mut data = support::read_pack_row(&connection, pack_id);
    data[0] ^= 0x01;
    support::write_pack_row(&connection, pack_id, &data);
}

/// The highest published pack and the highest pack present.
fn pack_state(path: &std::path::Path) -> (i64, i64) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let ceiling: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs WHERE completed = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let highest: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    (ceiling, highest)
}

#[test]
fn a_corrupt_pack_is_not_reported_as_absence() {
    let dir = TempDir::new("provider_corrupt");
    let path = dir.store_path("provider_corrupt");
    let root;
    {
        let store = create_store(&path);
        let (objects, file_root, _) = construct_file(&noise(600_000));
        save_all(&store, &objects).expect("save");
        root = file_root;
    }
    tamper_pack_of(&path, root);

    let store = open_store(&path);
    let provider = layerfs_storage::StoreProvider::new(&store);
    match provider.read_canonical_batch(&[root]) {
        Err(ContentError::ProviderFailure { what }) => {
            assert!(!what.is_empty(), "the refusing class is named");
        }
        Err(ContentError::MissingObject) => {
            panic!("a corrupt pack reached C1 as absence")
        }
        Err(other) => panic!("expected a provider failure, got {other}"),
        Ok(_) => panic!("corrupted bytes were served"),
    }
}

#[test]
fn an_object_the_store_does_not_hold_is_absence() {
    let dir = TempDir::new("provider_absent");
    let path = dir.store_path("provider_absent");
    {
        let store = create_store(&path);
        let (objects, _, _) = construct_file(&noise(600_000));
        save_all(&store, &objects).expect("save");
    }
    let absent = ObjectId::for_bytes(b"layerfs/provider-errors/absent");

    let store = open_store(&path);
    let provider = layerfs_storage::StoreProvider::new(&store);
    match provider.read_canonical_batch(&[absent]) {
        Err(ContentError::MissingObject) => {}
        Err(ContentError::ProviderFailure { .. }) => {
            panic!("absence was reported as a provider failure")
        }
        Err(other) => panic!("expected absence, got {other}"),
        Ok(_) => panic!("an unsaved object was served"),
    }
}

#[test]
fn a_record_above_the_publication_watermark_is_not_reported_as_absence() {
    let dir = TempDir::new("provider_unpublished");
    let path = dir.store_path("provider_unpublished");
    {
        let store = create_store(&path);
        let (objects, _, _) = construct_file(&noise(600_000));
        save_all(&store, &objects).expect("save");
    }
    let (_, highest) = pack_state(&path);
    let unpublished = ObjectId::for_bytes(b"layerfs/provider-errors/unpublished");
    {
        // A pack above the watermark with a locator for the object, exactly as
        // an unfinished save's early-committed packs would leave them.
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute("INSERT INTO saves(active_slot) VALUES(1)", [])
            .unwrap();
        let save = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![highest + 1, vec![0u8; 32], save],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO objects \
                 (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) \
                 VALUES (?1, 1, 100, ?2, 0, 0, ?3)",
                rusqlite::params![unpublished.as_bytes(), highest + 1, save],
            )
            .unwrap();
    }

    let store = open_store(&path);
    let provider = layerfs_storage::StoreProvider::new(&store);
    match provider.read_canonical_batch(&[unpublished]) {
        Err(ContentError::ProviderFailure { what }) => {
            assert_eq!(what, "record outside the publication scope");
        }
        Err(ContentError::MissingObject) => {
            panic!("an unpublished record reached C1 as absence")
        }
        Err(other) => panic!("expected a visibility refusal, got {other}"),
        Ok(_) => panic!("an unpublished object was served"),
    }
}
