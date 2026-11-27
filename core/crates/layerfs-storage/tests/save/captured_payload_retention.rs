//! A captured network Save retains only acknowledged winner bodies, sharing
//! the existing published-pack byte budget with metadata.
use crate::chains;
use crate::metadata;
use crate::support;

use chains::ChainProvider;
use layerfs_content::{
    object::codec::encode_bytes_object, AdvisoryPredecessors, FinalizedConsumer, FinalizedObject,
    ObjectRole,
};
use layerfs_storage::{
    location::PackDomain,
    policy::{DEPENDENCY_PACK_CACHE_BYTES, WAVE_CANONICAL_BYTES_LIMIT},
    Storage,
};
use metadata::MemoryMetadata;
use std::{collections::BTreeMap, sync::Arc};

const FILE_BYTES: usize = 128 * 1024 - 1;

fn network(memory: &Arc<MemoryMetadata>) -> Arc<ChainProvider> {
    let mut port = ChainProvider::new(memory.clone());
    port.batched = true;
    Arc::new(port)
}

fn payloads(count: usize) -> Vec<FinalizedObject> {
    support::noise(FILE_BYTES * count)
        .chunks_exact(FILE_BYTES)
        .map(|bytes| {
            FinalizedObject::new(
                ObjectRole::WholeFile,
                support::assembled_small_object(bytes),
            )
            .unwrap()
        })
        .collect()
}

fn save(storage: &Storage, objects: &[FinalizedObject], captured: bool) {
    let save = storage.begin_save().unwrap();
    let mut sink = if captured {
        save.captured_sink()
    } else {
        save.sink()
    };
    for object in objects {
        if let Err(label) = sink.accept(object.clone()) {
            panic!("accept {label:?}: {:?}", save.take_failure());
        }
    }
    save.finish().unwrap();
}

fn retained(memory: &MemoryMetadata) -> BTreeMap<i64, (PackDomain, usize)> {
    // The fixture owns two Arcs for each persistently registered body: one in
    // its pack table, the other in the acknowledged publication receipt.
    memory
        .state
        .lock()
        .unwrap()
        .packs
        .values()
        .filter(|pack| Arc::strong_count(&pack.body) > 2)
        .map(|pack| (pack.info.pack_id, (pack.info.domain, pack.info.length)))
        .collect()
}

fn pack_requests(provider: &ChainProvider) -> Vec<i64> {
    let mut scoped = std::mem::take(&mut *provider.inner.acquired_pack_ids.lock().unwrap());
    scoped.extend(
        std::mem::take(&mut *provider.requests.lock().unwrap())
            .into_iter()
            .flatten()
            .map(|read| read.pack.pack_id),
    );
    scoped
}

fn assert_read(storage: &Storage, expected: &FinalizedObject) {
    assert_eq!(
        storage
            .reader_after_save()
            .unwrap()
            .read_objects(&[expected.id()])
            .unwrap(),
        [expected.canonical()]
    );
}

#[test]
fn captured_network_save_keeps_recent_payload_and_preserves_metadata_under_pressure() {
    let payload = payloads((2 * WAVE_CANONICAL_BYTES_LIMIT as usize) / FILE_BYTES + 1);
    let metadata: Vec<_> = support::noise(16 * 32 * 1024)
        .chunks_exact(32 * 1024)
        .map(|bytes| {
            FinalizedObject::new(ObjectRole::FileState, encode_bytes_object(bytes).unwrap())
                .unwrap()
        })
        .collect();
    let objects: Vec<_> = metadata.iter().chain(&payload).cloned().collect();

    let baseline = Arc::new(MemoryMetadata::default());
    let baseline_port = network(&baseline);
    let ordinary = Storage::new(baseline_port.clone()).unwrap();
    save(&ordinary, &objects, false);
    let baseline_retained = retained(&baseline);
    assert!(!baseline_retained.is_empty());
    assert!(baseline_retained
        .values()
        .all(|(domain, _)| *domain == PackDomain::Metadata));

    let memory = Arc::new(MemoryMetadata::default());
    let captured_port = network(&memory);
    let captured = Storage::new(captured_port.clone()).unwrap();
    save(&captured, &objects, true);
    let retained = retained(&memory);
    assert_eq!(
        retained
            .iter()
            .filter(|(_, (domain, _))| *domain == PackDomain::Metadata)
            .map(|(id, shape)| (*id, *shape))
            .collect::<BTreeMap<_, _>>(),
        baseline_retained,
    );
    let kept_bytes: usize = retained.values().map(|(_, len)| *len).sum();
    assert!(kept_bytes <= DEPENDENCY_PACK_CACHE_BYTES);
    let state = memory.state.lock().unwrap();
    let oldest = &payload[0];
    let oldest_pack = state.objects[&oldest.id()].pack_id;
    assert!(!retained.contains_key(&oldest_pack));
    let recent = payload
        .iter()
        .rev()
        .find(|item| retained.contains_key(&state.objects[&item.id()].pack_id))
        .expect("a captured payload is retained");
    let recent_pack = state.objects[&recent.id()].pack_id;
    drop(state);

    pack_requests(&baseline_port);
    assert_read(&ordinary, recent);
    assert_eq!(pack_requests(&baseline_port), [recent_pack]);

    pack_requests(&captured_port);
    assert_read(&captured, recent);
    assert_read(&captured, oldest);
    assert_eq!(pack_requests(&captured_port), [oldest_pack]);
}

#[test]
fn captured_save_uses_the_first_winner_and_releases_the_losing_pack() {
    let memory = Arc::new(MemoryMetadata::default());
    let provider = network(&memory);
    let storage = Storage::new(provider.clone()).unwrap();
    let objects = payloads(WAVE_CANONICAL_BYTES_LIMIT as usize / FILE_BYTES + 2);
    let raced = objects[0].clone();
    let their_memory = memory.clone();
    let their_object = raced.clone();
    *provider.before_publish.lock().unwrap() = Some(Box::new(move || {
        let theirs = Storage::new(their_memory).unwrap();
        save(&theirs, &[their_object], false);
    }));
    save(&storage, &objects, true);

    let state = memory.state.lock().unwrap();
    let winner = state.objects[&raced.id()].pack_id;
    let losing_batch = state
        .registrations
        .iter()
        .find(|batch| {
            batch
                .objects
                .iter()
                .any(|row| row.object_id == raced.id() && row.pack_id != winner)
        })
        .expect("captured publication loses to the interleaved first writer");
    let loser = losing_batch
        .objects
        .iter()
        .find(|row| row.object_id == raced.id())
        .unwrap()
        .pack_id;
    assert_eq!(
        losing_batch
            .objects
            .iter()
            .filter(|row| row.pack_id == loser)
            .count(),
        1
    );
    assert_eq!(Arc::strong_count(&state.packs[&loser].body), 2);
    drop(state);

    pack_requests(&provider);
    assert_read(&storage, &raced);
    assert_eq!(pack_requests(&provider), [winner]);
}

/// Small whole files that share one six-group pack, then a captured Save of
/// an edited version of each, declaring the old one as its base. The Save
/// reads each base as one group of the shared pack. Once the groups it keeps
/// cover half the pack, the next read keeps the whole pack, which serves the
/// rest: five reads of the pack, not one per group.
#[test]
fn captured_base_reads_keep_a_dense_shared_pack_whole() {
    const FILES: usize = 12;
    const BYTES: usize = 20_000;
    let memory = Arc::new(MemoryMetadata::default());
    // Whole small packs answer group reads, as PostgreSQL's do.
    let mut port = ChainProvider::new(memory.clone());
    (port.batched, port.ranged) = (true, false);
    let provider = Arc::new(port);
    let raw: Vec<Vec<u8>> = support::noise(FILES * BYTES)
        .chunks_exact(BYTES)
        .map(<[u8]>::to_vec)
        .collect();
    let bases: Vec<_> = raw
        .iter()
        .map(|bytes| {
            FinalizedObject::new(
                ObjectRole::WholeFile,
                support::assembled_small_object(bytes),
            )
            .unwrap()
        })
        .collect();
    save(&Storage::new(provider.clone()).unwrap(), &bases, false);
    let state = memory.state.lock().unwrap();
    let shared = state.objects[&bases[0].id()].pack_id;
    assert!(bases
        .iter()
        .all(|base| state.objects[&base.id()].pack_id == shared));
    let header = layerfs_storage::pack::layout::parse_header(&state.packs[&shared].body).unwrap();
    assert_eq!(header.group_count, 6);
    drop(state);

    let edited: Vec<_> = raw
        .iter()
        .zip(&bases)
        .map(|(bytes, base)| {
            let mut bytes = bytes.clone();
            bytes[7] ^= 0x5a;
            FinalizedObject::new(
                ObjectRole::WholeFile,
                support::assembled_small_object(&bytes),
            )
            .unwrap()
            .with_predecessors(AdvisoryPredecessors::explicit(base.id()).unwrap())
        })
        .collect();
    // A wave's worth of unrelated files after each edited one puts every
    // base read in its own wave: one group of the shared pack at a time.
    let fillers = payloads(FILES * (WAVE_CANONICAL_BYTES_LIMIT as usize / FILE_BYTES + 1));
    let objects: Vec<_> = edited
        .iter()
        .zip(fillers.chunks(fillers.len() / FILES))
        .flat_map(|(object, filler)| std::iter::once(object).chain(filler))
        .cloned()
        .collect();
    let storage = Storage::new(provider.clone()).unwrap();
    pack_requests(&provider);
    save(&storage, &objects, true);
    let requests = pack_requests(&provider);
    assert_eq!(requests.iter().filter(|id| **id == shared).count(), 5);
    for object in &edited {
        assert_read(&storage, object);
    }
}
