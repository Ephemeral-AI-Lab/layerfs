//! Payloads the codec cannot shrink: stored verbatim, framed when it can.
//!
//! A payload record's tag says which form it is, and the reader dispatches on
//! exactly that byte: `FULL_TAG` and `PREFIX_TAG` carry a codec frame,
//! `STORED_TAG` carries the payload itself. These cases drive both directions of
//! that grammar through a real Store - save, close, reopen, read - and check the
//! width claim the treatment is registered on: a stored record costs the payload
//! plus its tag, where the frame it replaces cost more than the payload it
//! describes.

mod support;

use layerfs_content::{ConstructionPolicy, FinalizedObject, ObjectRole};
use layerfs_storage::encoding::delta::record::{parse, FULL_TAG, PREFIX_TAG, STORED_TAG};
use layerfs_storage::pack::layout::{
    group_view, parse_header, PackLane, VERSION_NATIVE, VERSION_NATIVE_STORED, VERSION_SINGLETON,
    VERSION_SINGLETON_STORED, VERSION_WHOLE_FILE, VERSION_WHOLE_FILE_GROUPED,
};
use layerfs_storage::{StoragePolicy, Store};
use support::{
    construct_file, create_store, disabled, noise, open_store, read_objects, repeat, save_all,
    Collected, TempDir,
};

/// The pack row one object's locator names, truncated to the pack it declares.
///
/// A pack row is allocated at its lane's capacity so an append can write in
/// place, so the row is longer than the pack until `truncate_pack` reads the
/// declared length out of the control area.
fn pack_of_object(path: &std::path::Path, root: layerfs_content::ObjectId) -> Vec<u8> {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    support::truncate_pack(
        connection
            .query_row(
                "SELECT p.data FROM objects o JOIN object_packs p ON p.pack_id = o.pack_id \
                 WHERE o.object_id = ?1",
                [root.to_bytes().to_vec()],
                |row| row.get(0),
            )
            .expect("pack row"),
    )
}

fn version(pack: &[u8]) -> u32 {
    u32::from_le_bytes(pack[8..12].try_into().expect("pack version"))
}

/// One record of a compact whole-file pack, as the lane stores it.
///
/// The lane's directory is starts-only, so the group's own body carries the record
/// boundaries: one count and one end offset per record, then the compact records
/// with their two length fields dropped.
fn whole_file_record(pack: &[u8], group: usize, record: usize) -> Vec<u8> {
    let header = parse_header(pack).expect("pack header");
    assert_eq!(header.lane, PackLane::WholeFile);
    let view = group_view(pack, header, group).expect("group view");
    layerfs_storage::encoding::framed_record(&pack[view.start..view.end], record)
        .expect("group record")
        .to_vec()
}

#[test]
fn a_payload_the_codec_cannot_shrink_is_stored_verbatim_and_reads_back() {
    let dir = TempDir::new("stored-noise");
    let path = dir.store_path("stored-noise");
    // Larger than the probe width, so the decision comes from the bounded prefix
    // rather than from a frame the codec already produced.
    let raw = noise(96_000);
    let (collected, root, _) = construct_file(&raw);
    assert_eq!(collected.objects().len(), 1);
    let canonical = collected.objects()[0].2.clone();
    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.inserted, 1);
    assert_eq!(
        outcome.profile.stored_records, 1,
        "the whole-file payload is stored verbatim"
    );
    drop(store);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read succeeds");
    assert!(
        values[0] == canonical,
        "the payload round trips: {} bytes read against {} canonical",
        values[0].len(),
        canonical.len()
    );

    let pack = pack_of_object(&path, root);
    assert_eq!(
        parse_header(&pack).expect("header").lane,
        PackLane::WholeFile
    );
    assert_eq!(
        version(&pack),
        VERSION_WHOLE_FILE_GROUPED,
        "a pack written now declares the version whose group grammar carries the record boundaries"
    );
    // The compact lane drops each record's two length fields at assembly and the
    // group's own end offsets carry the boundaries, so a record that is stored
    // verbatim is its tag and its payload and nothing else.
    let record = whole_file_record(&pack, 0, 0);
    assert_eq!(record[0], STORED_TAG);
    assert_eq!(
        record.len(),
        raw.len() + 1,
        "a stored record is the tag and the payload"
    );
    assert_eq!(&record[1..], raw.as_slice());
}

#[test]
fn a_payload_the_codec_can_shrink_keeps_its_frame_and_its_own_tag() {
    let dir = TempDir::new("framed-repeat");
    let path = dir.store_path("framed-repeat");
    let raw = repeat(96_000, 0x77);
    let (collected, root, _) = construct_file(&raw);
    let canonical = collected.objects()[0].2.clone();
    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(
        outcome.profile.stored_records, 0,
        "a compressible payload is not stored verbatim"
    );
    drop(store);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read succeeds");
    assert_eq!(values[0], canonical);

    let pack = pack_of_object(&path, root);
    let record = whole_file_record(&pack, 0, 0);
    assert_eq!(record[0], FULL_TAG);
    assert!(
        record.len() < raw.len() + 1,
        "the frame is narrower than the payload it describes: {} against {}",
        record.len(),
        raw.len() + 1
    );
}

#[test]
fn every_chunk_of_a_chunked_file_is_stored_when_it_cannot_be_shrunk() {
    let dir = TempDir::new("stored-chunks");
    let path = dir.store_path("stored-chunks");
    // Above the construction cutoff, so the content is chunked: the native lane
    // is the other lane a payload record can be stored in.
    let raw = noise(400_000);
    let (collected, root, _) = construct_file(&raw);
    let canonical = canonical_of(&collected, root);
    let chunks = collected
        .objects()
        .iter()
        .filter(|(_, role, _, _)| *role == ObjectRole::Chunk)
        .count();
    assert!(chunks > 1, "the file is chunked");
    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(
        outcome.profile.stored_records as usize, chunks,
        "every chunk payload of this file is stored verbatim"
    );
    drop(store);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read succeeds");
    assert_eq!(values[0], canonical);

    // A chunk's own pack, not the file-state root's: the root is an ordinary-lane
    // record and the payloads are what this case is about.
    let chunk = collected
        .objects()
        .iter()
        .find(|(_, role, _, _)| *role == ObjectRole::Chunk)
        .map(|(id, _, _, _)| *id)
        .expect("a chunk object");
    let pack = pack_of_object(&path, chunk);
    assert_eq!(parse_header(&pack).expect("header").lane, PackLane::Native);
    assert_eq!(version(&pack), VERSION_NATIVE_STORED);
    // Every record of the pack this locator names carries the stored tag.
    let header = parse_header(&pack).expect("header");
    for group in 0..header.group_count {
        let view = group_view(&pack, header, group).expect("group view");
        let body = &pack[view.start..view.end];
        let count = u32::from_le_bytes(body[..4].try_into().expect("record count")) as usize;
        let area = 4 + 4 * count;
        let mut previous = 0;
        for ordinal in 0..count {
            let end = u32::from_le_bytes(
                body[4 + 4 * ordinal..8 + 4 * ordinal]
                    .try_into()
                    .expect("record end"),
            ) as usize;
            let record = &body[area + previous..area + end];
            previous = end;
            assert_eq!(record[0], STORED_TAG, "record {ordinal} of group {group}");
        }
    }
}

/// The canonical bytes of the object `root` names.
///
/// A chunked file's root is a file-state object, not a payload, so the object the
/// root names has to be looked up rather than assumed to be the first emitted.
fn canonical_of(collected: &Collected, root: layerfs_content::ObjectId) -> Vec<u8> {
    collected
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, canonical, _)| canonical.clone())
        .expect("the root is among the emitted objects")
}

#[test]
fn a_payload_larger_than_a_compact_pack_is_stored_in_the_singleton_lane() {
    let dir = TempDir::new("stored-singleton");
    let path = dir.store_path("stored-singleton");
    // A cutoff a megabyte wide, so a payload can be larger than a compact pack
    // holds and the singleton lane is the one that carries it. The construction
    // policy has to match, because the cutoff decides the representation.
    let policy = StoragePolicy::new(1, 1_048_576, 8, 4);
    let construction = ConstructionPolicy::new(1_048_576, 8, 4);
    let raw = noise(400_000);
    let mut collected = Collected::new();
    let constructed = disabled(|scope| {
        layerfs_content::construct_bytes(
            construction,
            &construction.capacities(),
            &raw,
            &mut collected,
            scope.child("content"),
        )
    })
    .expect("constructed file");
    let root = constructed.root;
    assert_eq!(collected.objects().len(), 1);
    let canonical = collected.objects()[0].2.clone();

    let store =
        disabled(|scope| Store::create(&path, policy, scope.child("store"))).expect("store");
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.profile.stored_records, 1);
    drop(store);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read succeeds");
    assert_eq!(values[0], canonical);

    let pack = pack_of_object(&path, root);
    assert_eq!(
        parse_header(&pack).expect("header").lane,
        PackLane::Singleton
    );
    assert_eq!(version(&pack), VERSION_SINGLETON_STORED);
}

#[test]
fn the_stored_tag_carries_no_base_and_the_other_tags_are_untouched() {
    // The grammar, not the store: a payload stored verbatim is FULL by
    // construction, so a stored tag never names a base - and the two frame tags
    // keep the bases they always had.
    let mut record = vec![STORED_TAG];
    record.extend_from_slice(b"payload");
    let parsed = parse(PackLane::WholeFile, &record, 23 + 7).expect("a stored tag parses");
    assert_eq!(parsed.tag, STORED_TAG);
    assert_eq!(parsed.base, None, "a stored tag carries no base");
    assert_eq!(parsed.frame, b"payload");

    let mut framed = vec![PREFIX_TAG];
    framed.extend_from_slice(&[3_u8; 32]);
    framed.extend_from_slice(b"frame");
    let parsed = parse(PackLane::WholeFile, &framed, 23 + 5).expect("a prefix tag parses");
    assert_eq!(parsed.tag, PREFIX_TAG);
    assert!(parsed.base.is_some());
    assert_eq!(parsed.frame, b"frame");

    assert_ne!(STORED_TAG, FULL_TAG);
    assert_ne!(STORED_TAG, PREFIX_TAG);
    // An unknown tag is refused rather than guessed at, and the refusal names the
    // lane's own grammar.
    assert!(matches!(
        parse(PackLane::WholeFile, &[9_u8, 1, 2, 3], 26),
        Err(layerfs_storage::StorageError::Integrity(
            "compact record tag"
        ))
    ));
}

#[test]
fn a_payload_below_the_probe_width_is_decided_by_its_own_frame() {
    // Rule 2 of the treatment, on the store: below the probe width there is no
    // separate probe, and the payload is stored because the frame the codec
    // produced is not smaller than the payload it describes.
    let dir = TempDir::new("stored-post-hoc");
    let path = dir.store_path("stored-post-hoc");
    let raw = noise(700);
    let (collected, root, _) = construct_file(&raw);
    let canonical = collected.objects()[0].2.clone();
    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.profile.stored_records, 1);
    drop(store);

    let pack = pack_of_object(&path, root);
    let record = whole_file_record(&pack, 0, 0);
    assert_eq!(record[0], STORED_TAG);
    assert_eq!(record.len(), raw.len() + 1);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read succeeds");
    assert_eq!(values[0], canonical);
}

#[test]
fn a_store_of_compressible_content_still_writes_only_the_versions_it_writes() {
    // The version a lane writes does not depend on what the payload turned out to
    // be: the grammar is chosen by the build, not by the bytes. A compressible
    // payload saves through the same header a stored one does, which is what makes
    // the version a reader checks meaningful.
    let dir = TempDir::new("stored-version");
    let path = dir.store_path("stored-version");
    let (collected, _, _) = construct_file(&repeat(64_000, 0x21));
    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.profile.stored_records, 0);
    drop(store);

    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let pack: Vec<u8> = connection
        .query_row("SELECT data FROM object_packs LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("a pack row");
    let pack = support::truncate_pack(pack);
    assert_eq!(
        parse_header(&pack).expect("header").lane,
        PackLane::WholeFile
    );
    assert_eq!(version(&pack), VERSION_WHOLE_FILE_GROUPED);
    // That every version this crate has ever written - including the ones no lane
    // writes any more - is still recognized and mapped to its lane is asserted
    // against synthetic headers in `physical_formats.rs`, where the framings are
    // enumerated; the two constants are named here so a reader of this file can
    // see which version this lane writes now.
    let _ = (VERSION_WHOLE_FILE, VERSION_NATIVE, VERSION_SINGLETON);
    assert!(FinalizedObject::new(ObjectRole::WholeFile, Vec::new()).is_err());
}

#[test]
fn a_damaged_stored_payload_is_refused_and_never_served() {
    // A stored record carries no checksum, so the guarantee moves rather than
    // disappears: the payload is re-hashed into the canonical object and the
    // identity comparison is what refuses a damaged byte. This is the stored
    // form's half of `delta_chains.rs`'s corrupt-intermediate case, whose frame
    // checksum refuses a damaged *frame*.
    let dir = TempDir::new("stored-damage");
    let path = dir.store_path("stored-damage");
    let raw = noise(32_000);
    let (collected, root, _) = construct_file(&raw);
    let canonical = collected.objects()[0].2.clone();
    let store = create_store(&path);
    save_all(&store, &collected).expect("save succeeds");
    drop(store);

    support::corrupt_first_pack(&path);

    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[root]).expect_err("a damaged payload is refused");
    assert!(
        matches!(error, layerfs_storage::StorageError::Integrity(_)),
        "a damaged stored payload produced {error}"
    );
    let _ = canonical;
}
