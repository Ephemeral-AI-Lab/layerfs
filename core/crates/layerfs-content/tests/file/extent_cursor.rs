//! The extent cursor over a committed file, chunk identity from raw bytes,
//! and the cut-candidate scan, against the tree a real construction emitted.

#[path = "../support/mod.rs"]
mod support;

use layerfs_content::file::cdc::{cut_candidates, MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES};
use layerfs_content::file::mapping::{chunk_id, encode_chunk_object, ChunkIdentity, Extent};
use layerfs_content::{apply_edits, Edit, EditRequest, FileView, ObjectId};
use support::{
    build_file, disabled_scope, edits::Edits, edits::Parts, noise, read_back, MemoryStore,
};

const MIB: usize = 1 << 20;

fn view(store: &MemoryStore, root: ObjectId) -> FileView {
    disabled_scope(|scope| FileView::open(store, root, scope.child("open"))).unwrap()
}
/// Every extent from `offset`, and the mapping pages the pass read.
fn extents(store: &MemoryStore, root: ObjectId, offset: u64) -> (Vec<Extent>, u64) {
    let mut cursor = view(store, root).extents().expect("a chunked file");
    cursor.seek(offset);
    let mut found = Vec::new();
    disabled_scope(|scope| {
        while let Some(extent) = cursor.next(store, scope)? {
            found.push(extent);
        }
        Ok::<_, layerfs_content::ContentError>(())
    })
    .unwrap();
    (found, cursor.pages())
}

#[test]
fn the_cursor_visits_every_extent_in_order_and_identity_equals_the_tree() {
    let bytes = noise(3 * MIB + 1234);
    let (store, built) = build_file(&bytes);
    let (found, pages) = extents(&store, built.root, 0);
    let mut at = 0_u64;
    for extent in &found {
        assert_eq!(extent.offset, at, "extents are contiguous");
        assert_eq!(
            extent.chunk_offset, 0,
            "a fresh construction slices nothing"
        );
        let payload = &bytes[at as usize..at as usize + extent.length as usize];
        // The identity of raw bytes is the identity of their chunk object,
        // and that is the id the tree recorded.
        let id = chunk_id(payload).unwrap();
        assert_eq!(
            id,
            ObjectId::for_bytes(&encode_chunk_object(payload).unwrap())
        );
        assert_eq!(id, extent.chunk);
        // The same identity from the bytes in two parts, cut anywhere.
        let cut = (at as usize * 7 + 3) % (payload.len() + 1);
        let mut parts = ChunkIdentity::new(payload.len()).unwrap();
        parts.update(&payload[..cut]).unwrap();
        assert_eq!(parts.remaining(), payload.len() - cut);
        assert_eq!(parts.finish().is_err(), cut < payload.len());
        let mut later = parts.clone();
        later.update(&payload[cut..]).unwrap();
        assert_eq!(later.finish().unwrap(), id);
        // More bytes than the declared length are refused and add nothing.
        assert!(later.update(&[0]).is_err());
        assert_eq!(later.finish().unwrap(), id);
        at += u64::from(extent.length);
    }
    assert_eq!(at, bytes.len() as u64);
    assert!(found.len() > 100, "{}", found.len());
    assert!(chunk_id(&bytes[..MAXIMUM_CHUNK_BYTES + 1]).is_err());
    assert!(ChunkIdentity::new(MAXIMUM_CHUNK_BYTES + 1).is_err());
    println!("EXTENTS full extents={} pages={pages}", found.len());

    // A seek returns the extent that holds the offset and reads one page a
    // level on the way, however far into the file it lands.
    let (state, mut worst) = (view(&store, built.root).file_state().unwrap().unwrap(), 0);
    for offset in [1, 8191, MIB as u64, 2 * MIB as u64 + 77, at - 1] {
        let mut cursor = view(&store, built.root).extents().unwrap();
        cursor.seek(offset);
        let first = disabled_scope(|scope| cursor.next(&store, scope))
            .unwrap()
            .unwrap();
        assert!(first.offset <= offset && offset < first.offset + u64::from(first.length));
        assert_eq!(
            *found
                .iter()
                .find(|extent| extent.offset == first.offset)
                .unwrap(),
            first
        );
        assert_eq!(cursor.pages(), u64::from(state.tree_level) + 1);
        worst = worst.max(cursor.pages());
        // And the pass continues in order from there.
        let (rest, _) = extents(&store, built.root, offset);
        let from = found.iter().position(|extent| *extent == first).unwrap();
        assert_eq!(rest, found[from..]);
    }
    println!("EXTENTS seek pages={worst} level={}", state.tree_level);
    let mut cursor = view(&store, built.root).extents().unwrap();
    cursor.seek(at);
    assert!(disabled_scope(|scope| cursor.next(&store, scope))
        .unwrap()
        .is_none());
}

#[test]
fn a_whole_file_base_has_no_extents() {
    let (store, built) = build_file(&noise(10_000));
    assert!(view(&store, built.root).extents().is_none());
}

#[test]
fn cut_candidates_include_the_boundaries_the_scanner_took() {
    let bytes = noise(2 * MIB);
    let (store, built) = build_file(&bytes);
    let (found, _) = extents(&store, built.root, 0);
    let (mut checked, mut forced, mut candidates) = (0, 0, 0_usize);
    cut_candidates(&bytes, &mut |_| candidates += 1);
    for extent in &found[..found.len() - 1] {
        let length = extent.length as usize;
        let end = (extent.offset + u64::from(extent.length)) as usize;
        if length == MAXIMUM_CHUNK_BYTES {
            forced += 1;
            continue;
        }
        if length < MINIMUM_CHUNK_BYTES + 64 {
            continue;
        }
        checked += 1;
        // From the chunk's own start and from windows that begin inside the
        // chunk, each moved back one byte when it would not start on the
        // chunk's pair alignment.
        for back in [length, 4096, 64] {
            let back = back + (length - back) % 2;
            let mut hit = false;
            cut_candidates(&bytes[end - back..], &mut |at| hit |= at == back);
            assert!(hit, "boundary at {end} from {back} bytes before it");
        }
    }
    println!(
        "CUT_CANDIDATES chunks={} checked={checked} forced={forced} candidates={candidates} bytes={}",
        found.len(),
        bytes.len()
    );
    assert!(checked > 50);
    // About one position in 4096 is a candidate.
    assert!(candidates > bytes.len() / 8192 && candidates < bytes.len() / 2048);
}

#[test]
fn an_edited_file_reports_slices_whose_bytes_do_not_hash_to_their_chunk() {
    let bytes = noise(MIB);
    let (store, built) = build_file(&bytes);
    let at = (MIB / 2 + 100) as u64;
    let mut parts = Parts::new();
    parts.push(vec![0x5a; 10]);
    let stream = Edits::new(bytes.len() as u64, vec![Edit::insert(at, 10)]).unwrap();
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let mut result = store.merged_clone();
    let edited = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &store,
            EditRequest {
                root: built.root,
                edits: &stream,
                source: &parts,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .unwrap();
    let mut expected = bytes.clone();
    expected.splice(at as usize..at as usize, vec![0x5a; 10]);
    assert_eq!(read_back(&result, edited.root).unwrap(), expected);

    let (found, _) = extents(&result, edited.root, 0);
    let (mut next, mut whole, mut slices) = (0_u64, 0, 0);
    for extent in &found {
        assert_eq!(extent.offset, next);
        next += u64::from(extent.length);
        let payload = &expected[extent.offset as usize..next as usize];
        if chunk_id(payload).unwrap() == extent.chunk {
            assert_eq!(extent.chunk_offset, 0);
            whole += 1;
        } else {
            // A slice of a chunk: its own bytes are not the chunk's identity,
            // so an identity compare misses it and never claims it.
            slices += 1;
        }
    }
    assert_eq!(next, expected.len() as u64);
    println!("EXTENTS edited whole={whole} slices={slices}");
    assert!(whole > 20 && slices <= 4, "{whole} {slices}");
}
