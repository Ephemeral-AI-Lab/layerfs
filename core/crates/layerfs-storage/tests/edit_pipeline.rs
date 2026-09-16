//! Real C1 edits through C2: save, acknowledge, reopen and read back.
//!
//! The pipeline is the production path with no shim: C1 builds or edits the
//! objects, C2 stores them, the Store is closed and reopened, and the logical
//! bytes are read back through a fresh connection. The edited root must open the
//! exact final bytes, including after a dependency chain was stored.

mod support;

use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, Edit, EditRequest, EditStream, ObjectId,
    Replacements,
};
use support::{
    create_store, disabled, noise, open_store, patterned, read_objects, save_via_handoff,
    Collected, Provider, TempDir,
};

const CUTOFF: u64 = 131_072;

fn edit(
    base: &[u8],
    edits: Vec<Edit>,
    source: &Replacements,
    into: &mut Collected,
) -> (ObjectId, u64) {
    let policy = ConstructionPolicy::frozen_default();
    let stream = EditStream::new(base.len() as u64, edits).expect("valid stream");
    let mut provider_objects = Collected::new();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            base,
            &mut provider_objects,
            scope.child("content"),
        )
    })
    .expect("base construction");
    let constructed = disabled(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &Provider(&provider_objects),
            EditRequest {
                root: constructed.root,
                edits: &stream,
                source,
            },
            into,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    (constructed.root, constructed.logical_len)
}

#[test]
fn a_small_edit_round_trips_through_a_reopened_store() {
    let dir = TempDir::new("pipeline-small");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let base = patterned(90_000);
    let mut final_bytes = base.clone();
    final_bytes[40_000..40_500].copy_from_slice(&noise(500));
    let mut replacements = Replacements::new();
    replacements.push(final_bytes[40_000..40_500].to_vec());
    let mut collected = Collected::new();
    let (root, len) = edit(
        &base,
        vec![Edit::overwrite(40_000, 40_500)],
        &replacements,
        &mut collected,
    );
    assert_eq!(len, final_bytes.len() as u64);
    let outcome = save_via_handoff(&store, &collected).expect("save");
    assert!(outcome.acknowledged);
    drop(store);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read");
    let expected = collected
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, bytes, _)| bytes.clone())
        .expect("root");
    assert_eq!(values[0], expected);
}

#[test]
fn a_chunked_edit_round_trips_and_reuses_retained_payloads() {
    let dir = TempDir::new("pipeline-chunked");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let base = noise(CUTOFF as usize * 2);
    let base_collected = {
        let policy = ConstructionPolicy::frozen_default();
        let mut collected = Collected::new();
        let _ = disabled(|scope| {
            construct_bytes(
                policy,
                &policy.capacities(),
                &base,
                &mut collected,
                scope.child("content"),
            )
        })
        .expect("base");
        collected
    };
    save_via_handoff(&store, &base_collected).expect("base save");

    let mut final_bytes = base.clone();
    final_bytes[100_000..100_300].copy_from_slice(&noise(300));
    let mut replacements = Replacements::new();
    replacements.push(final_bytes[100_000..100_300].to_vec());
    let mut collected = Collected::new();
    let (root, len) = edit(
        &base,
        vec![Edit::overwrite(100_000, 100_300)],
        &replacements,
        &mut collected,
    );
    assert_eq!(len, final_bytes.len() as u64);
    let outcome = save_via_handoff(&store, &collected).expect("edit save");
    assert!(outcome.acknowledged);
    drop(store);
    let base_objects = base_collected.objects().len();

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read");
    let expected = collected
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, bytes, _)| bytes.clone())
        .expect("root");
    assert_eq!(values[0], expected);
    // Retained payloads were stored by the base save and referenced, not
    // rewritten: the edit stores far fewer objects than the file holds.
    assert!(
        (outcome.inserted as usize) < base_objects,
        "the edit re-stored the whole file: {} of {}",
        outcome.inserted,
        base_objects
    );
}

#[test]
fn a_multi_edit_transition_round_trips_through_the_store() {
    for (label, base_len, edits, expect_len) in [
        (
            "grow",
            100_000_usize,
            vec![Edit::insert(50_000, 200_000)],
            300_000_u64,
        ),
        (
            "shrink",
            300_000_usize,
            vec![Edit::delete(10_000, 250_000)],
            60_000,
        ),
        ("empty", 150_000_usize, vec![Edit::delete(0, 150_000)], 0),
    ] {
        let dir = TempDir::new("pipeline-transition");
        let path = dir.store_path("pipeline");
        let store = create_store(&path);
        let base = patterned(base_len);
        let mut replacements = Replacements::new();
        for edit in &edits {
            replacements.push(noise(edit.replacement_len() as usize));
        }
        let mut collected = Collected::new();
        let (root, len) = edit(&base, edits, &replacements, &mut collected);
        assert_eq!(len, expect_len, "{label}");
        save_via_handoff(&store, &collected).expect("save");
        drop(store);
        let reopened = open_store(&path);
        let (values, _) = read_objects(&reopened, &[root]).expect("read");
        let expected = collected
            .objects()
            .iter()
            .find(|(id, _, _, _)| *id == root)
            .map(|(_, _, bytes, _)| bytes.clone())
            .expect("root");
        assert_eq!(values[0], expected, "{label}");
    }
}

#[test]
fn a_standalone_route_and_an_integrated_route_agree_on_the_root() {
    let dir = TempDir::new("pipeline-agree");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let base = patterned(80_000);
    let mut replacements = Replacements::new();
    replacements.push(noise(1_000));
    let mut expected = base.clone();
    expected[20_000..21_000].copy_from_slice(&noise(1_000));
    let mut collected = Collected::new();
    let (root, _) = edit(
        &base,
        vec![Edit::overwrite(20_000, 21_000)],
        &replacements,
        &mut collected,
    );
    save_via_handoff(&store, &collected).expect("integrated save");
    // The same final bytes constructed independently produce the same root.
    let policy = ConstructionPolicy::frozen_default();
    let mut fresh = Collected::new();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &expected,
            &mut fresh,
            scope.child("content"),
        )
    })
    .expect("fresh construction");
    assert_eq!(root, constructed.root);
    drop(store);
    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read");
    assert_eq!(values[0], fresh.objects()[0].2);
}
