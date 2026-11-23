//! Delta-chain hints (#758): a chain locate lets a read walk every level with
//! no further locate, a hint that disagrees with its record or its bytes is
//! dropped and counted while the read stays correct, strict handles fail on
//! it, a malformed answer fails hard and keeps nothing, a Save binds no
//! member, a hinted locator serves one demand, and a Store without chains
//! costs exactly the default calls.
#[path = "support/chain_provider.rs"]
mod chains;
#[path = "support/memory_metadata.rs"]
mod metadata;

use chains::{copy, depth, history, history_with, read, ChainProvider, Shape};
use layerfs_content::filesystem::directory::{apply_bindings, build_directory};
use layerfs_content::filesystem::{DirectoryRoot, FilesystemObjects, PathName};
use layerfs_content::{
    AdvisoryPredecessors, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{
    encoding::delta::edge::RecordDirectory,
    location::{ChainMember, LocatedChains},
    pack::layout,
    policy::{CANONICAL_LIMIT, GROUP_COUNT_LIMIT, RECORD_COUNT_LIMIT},
    port::ObjectKey,
    Storage, StorageCapacities, StorageError, StoragePolicy,
};
use std::sync::{Arc, Mutex};

const EDITS: Shape = Shape {
    files: 4,
    versions: 8,
    size: 24_000,
    rewrite: false,
    reopen: None,
};

fn refused(got: &layerfs_storage::StorageResult<Vec<Vec<u8>>>, reason: &str) {
    assert!(
        matches!(got, Err(StorageError::Integrity(found)) if *found == reason),
        "{got:?}"
    );
}

fn chain_port(memory: &metadata::MemoryMetadata) -> ChainProvider {
    ChainProvider::new(copy(memory))
}

fn editing(
    mut provider: ChainProvider,
    edit: impl FnMut(&mut LocatedChains) + Send + 'static,
) -> Arc<ChainProvider> {
    provider.edit = std::sync::Mutex::new(Some(Box::new(edit)));
    Arc::new(provider)
}

#[test]
fn a_hinted_chain_walk_sends_one_locate_and_one_group_read() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    let levels = depth(&memory, root);
    assert!(levels >= 5, "fixture chain depth {levels}");
    let plain = copy(&memory);
    let (expected, before) = read(plain.clone(), &[root], true);
    let expected = expected.unwrap();
    assert_eq!(
        before.locate,
        levels as u64 + 1,
        "today: one locate per level"
    );
    for ids in [false, true] {
        let mut provider = chain_port(&memory);
        provider.ids = ids;
        let provider = Arc::new(provider);
        let (got, after) = read(provider.clone(), &[root], true);
        assert_eq!(got.unwrap(), expected);
        assert_eq!(provider.calls("locate"), 1);
        assert_eq!(after.locate, 1);
        assert_eq!(provider.calls("read_groups"), 1);
        assert_eq!(provider.calls("read_scoped_pack"), 0);
        assert_eq!(provider.calls("read_packs"), 0);
        assert_eq!(after.group_reads, 1);
        assert_eq!(after.chain_rows, 1);
        assert_eq!(after.chain_members, levels as u64);
        assert_eq!(after.chain_hinted_bases, levels as u64);
        assert_eq!(after.chain_hint_stale, 0);
        assert_eq!(after.chain_unhinted_roots, 0);
    }
}

#[test]
fn saves_through_a_chain_port_store_what_the_default_port_stores() {
    // The last Save runs on a fresh handle, so its bases are located again.
    let shape = Shape {
        reopen: Some(EDITS.versions - 1),
        ..EDITS
    };
    let (plain, plain_roots, _) = history(shape, |m| m);
    let (hinted, hinted_roots, work) = history(shape, |m| Arc::new(ChainProvider::new(m)));
    assert_eq!(plain_roots, hinted_roots);
    let (plain, hinted) = (plain.state.lock().unwrap(), hinted.state.lock().unwrap());
    assert_eq!(plain.objects, hinted.objects);
    assert_eq!(plain.packs, hinted.packs);
    assert!(work.chain_rows > 0, "Saves located chains");
    assert_eq!(work.chain_hint_stale, 0);
}

/// The calls of the last version's Save, after its construction reads.
fn last_save(memory: &metadata::MemoryMetadata, version: usize) -> Vec<(&'static str, usize)> {
    let calls = memory.calls.lock().unwrap();
    let at = calls
        .iter()
        .rposition(|call| *call == ("save", version))
        .unwrap();
    calls[at + 1..].to_vec()
}

#[test]
fn a_writer_reads_its_own_bases_chains_in_one_group_read() {
    // One writer handle: its own published rows are cached, so no locate
    // reaches the provider and only the local chain prefetch can batch.
    let shape = Shape {
        versions: 9,
        ..EDITS
    };
    let last = shape.versions - 1;
    let (plain, roots, _) = history(shape, |m| m);
    let plain_save = last_save(&plain, last);
    let count = |calls: &[(&str, usize)], name| calls.iter().filter(|c| c.0 == name).count();
    assert!(depth(&plain, roots[0]) >= 7);
    assert!(
        count(&plain_save, "read_scoped_pack") >= 7,
        "today: a read per level"
    );
    for batched in [true, false] {
        let (memory, batched_roots, work) = history(shape, |m| {
            let mut provider = ChainProvider::new(m);
            provider.chains = false;
            provider.batched = batched;
            Arc::new(provider)
        });
        assert_eq!(roots, batched_roots);
        {
            let (a, b) = (plain.state.lock().unwrap(), memory.state.lock().unwrap());
            assert_eq!(a.objects, b.objects);
            assert_eq!(a.packs, b.packs);
        }
        let save = last_save(&memory, last);
        if batched {
            assert_eq!(count(&save, "read_groups"), 1, "{save:?}");
            assert_eq!(count(&save, "read_scoped_pack"), 0, "{save:?}");
            assert_eq!(count(&save, "read_packs"), 0, "{save:?}");
            assert_eq!(work.chain_hint_stale, 0);
        } else {
            assert_eq!(save, plain_save);
            assert_eq!(*memory.calls.lock().unwrap(), *plain.calls.lock().unwrap());
            assert_eq!(work.group_reads, 0);
        }
    }
}

#[test]
fn a_member_naming_another_base_is_stale_and_the_walk_goes_on_level_by_level() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    let levels = depth(&memory, root) as u64;
    let (expected, _) = read(copy(&memory), &[root], true);
    // Only the root's own chain is wrong; the bases' chains are right.
    let wrong = move |chains: &mut LocatedChains| {
        for chain in chains.chains.iter_mut().filter(|c| c.root == root) {
            chain.members[1].object_id = Some(ObjectId::for_bytes(b"not this base"));
        }
    };
    for strict in [false, true] {
        let mut provider = chain_port(&memory);
        provider.ids = true;
        let provider = editing(provider, wrong);
        let (got, after) = read(provider.clone(), &[root], strict);
        assert_eq!(after.chain_hint_stale, 1);
        if strict {
            refused(&got, "delta chain hint");
            continue;
        }
        assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
        // The root's locate, then the authoritative locate of the base the
        // stale member named, whose own (right) chain carries the rest.
        assert_eq!(after.locate, 2);
        assert_eq!(after.chain_rows, 2);
        assert_eq!(after.chain_hinted_bases, levels - 1);
    }
}

/// The member of a depth-one chain moved onto another FULL record of the same
/// role: the walk cannot tell, the base's hash at reconstruction can.
fn misplaced_member() -> (Arc<metadata::MemoryMetadata>, ObjectId, ChainMember) {
    let shape = Shape {
        versions: 2,
        ..EDITS
    };
    let (memory, roots, _) = history(shape, |m| m);
    assert_eq!(depth(&memory, roots[0]), 1);
    let state = memory.state.lock().unwrap();
    let other = state.objects[&state.objects[&roots[1]].base.unwrap()];
    let member = ChainMember {
        object_id: None,
        pack: state.packs[&other.pack_id].info,
        group_number: other.group_number,
        record_number: other.record_number,
        canonical_length: other.canonical_length,
    };
    drop(state);
    (memory, roots[0], member)
}

#[test]
fn a_misplaced_member_fails_its_hash_and_the_read_rebuilds_once() {
    let (memory, root, member) = misplaced_member();
    let (expected, _) = read(copy(&memory), &[root], true);
    for strict in [false, true] {
        let provider = editing(chain_port(&memory), move |chains| {
            for chain in chains.chains.iter_mut().filter(|c| c.root == root) {
                chain.members[0] = member;
            }
        });
        let (got, after) = read(provider.clone(), &[root], strict);
        assert_eq!(after.chain_hint_stale, 1);
        if strict {
            refused(&got, "delta chain hint");
        } else {
            assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
            // The rebuild located the base itself.
            assert_eq!(provider.calls("locate"), 2);
        }
    }
}

#[test]
fn bytes_that_fail_their_hash_at_the_authoritative_location_stay_a_hard_failure() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    // The last byte of the FULL record at the bottom of the chain.
    let (full, at) = {
        let state = memory.state.lock().unwrap();
        let mut id = root;
        while let Some(base) = state.objects[&id].base {
            id = base;
        }
        let location = state.objects[&id];
        let body = &state.packs[&location.pack_id].body;
        let header = layout::parse_header(body).unwrap();
        let view = layout::group_view(body, header, location.group_number).unwrap();
        assert_eq!(view.codec, layout::GroupCodec::Raw, "a stored FULL group");
        let records = RecordDirectory::of(&body[view.start..view.end]).unwrap();
        let (_, end) = records.range(location.record_number).unwrap();
        (location.pack_id, view.start + end - 1)
    };
    let provider = chain_port(&memory);
    *provider.corrupt_range.lock().unwrap() = Some((full, at));
    let provider = Arc::new(provider);
    let (got, after) = read(provider.clone(), &[root], false);
    let ranged = provider.requests.lock().unwrap()[0]
        .iter()
        .any(|read| read.pack.pack_id == full && !read.ranges.is_empty());
    assert!(ranged, "the FULL record's group was read as a range");
    refused(&got, "dependency identity");
    assert_eq!(
        after.chain_hint_stale, 1,
        "one rebuild, then the failure stands"
    );
}

#[test]
fn malformed_chain_answers_are_refused() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    let cap = StorageCapacities::from_policy(StoragePolicy::frozen_default())
        .unwrap()
        .delta_depth_for_role(ObjectRole::WholeFile);
    type Edit = fn(&mut LocatedChains, u8);
    let edits: [(&str, Edit); 9] = [
        ("unasked root", |c, _| {
            c.chains[0].root = ObjectId::for_bytes(b"unasked")
        }),
        ("two chains for one root", |c, _| {
            c.chains.push(c.chains[0].clone())
        }),
        ("more members than the role allows", |c, cap| {
            let last = *c.chains[0].members.last().unwrap();
            c.chains[0].members.resize(usize::from(cap) + 1, last);
        }),
        ("no members", |c, _| c.chains[0].members.clear()),
        ("group ordinal", |c, _| {
            c.chains[0].members[0].group_number = GROUP_COUNT_LIMIT
        }),
        ("record ordinal", |c, _| {
            c.chains[0].members[0].record_number = RECORD_COUNT_LIMIT
        }),
        ("empty length", |c, _| {
            c.chains[0].members[0].canonical_length = 0
        }),
        ("length over the limit", |c, _| {
            c.chains[0].members[0].canonical_length = CANONICAL_LIMIT + 1
        }),
        ("directory short of its body offset", |c, _| {
            c.directories[0].prefix.pop();
        }),
    ];
    for (name, edit) in edits {
        let provider = editing(chain_port(&memory), move |chains| edit(chains, cap));
        let (got, _) = read(provider, &[root], true);
        assert!(
            matches!(got, Err(StorageError::Integrity("port chain locator"))),
            "{name}: {got:?}"
        );
    }
}

#[test]
fn full_rewrites_cost_exactly_the_default_calls() {
    let shape = Shape {
        versions: 3,
        rewrite: true,
        ..EDITS
    };
    let (plain, roots, plain_work) = history(shape, |m| m);
    let (hinted, hinted_roots, hinted_work) = history(shape, |m| Arc::new(ChainProvider::new(m)));
    assert_eq!(roots, hinted_roots);
    assert!(roots.iter().all(|root| depth(&plain, *root) == 0));
    assert_eq!(*plain.calls.lock().unwrap(), *hinted.calls.lock().unwrap());
    assert_eq!(plain_work, hinted_work);
    assert_eq!(hinted_work.chain_rows + hinted_work.group_reads, 0);
    let plain_port = copy(&plain);
    let hinted_port = Arc::new(chain_port(&plain));
    let (expected, plain_read) = read(plain_port.clone(), &roots, true);
    let (got, hinted_read) = read(hinted_port.clone(), &roots, true);
    assert_eq!(got.unwrap(), expected.unwrap());
    assert_eq!(plain_read, hinted_read);
    assert_eq!(
        *plain_port.calls.lock().unwrap(),
        *hinted_port.inner.calls.lock().unwrap()
    );
}

#[test]
fn a_save_that_loses_its_registrations_binds_no_member_reading_the_winners() {
    // Another writer publishes the last version's objects, stored against
    // their predecessors, just before this Save publishes them without any:
    // every registration is lost and the Save reads the winners back through
    // a chain locate. The Save's handle is fresh and its Save walks no
    // chain, so the winners' deeper bases are not among its locators.
    let shape = Shape {
        versions: 6,
        reopen: Some(5),
        ..EDITS
    };
    let port: Arc<Mutex<Option<Arc<ChainProvider>>>> = Arc::default();
    let wrapped = port.clone();
    let mut raced = None;
    history_with(
        shape,
        move |memory| {
            let provider = Arc::new(ChainProvider::new(memory));
            *wrapped.lock().unwrap() = Some(provider.clone());
            provider
        },
        |storage, objects| {
            let provider = port.lock().unwrap().clone().unwrap();
            let (memory, theirs) = (provider.inner.clone(), objects.clone());
            *provider.before_publish.lock().unwrap() = Some(Box::new(move || {
                let other = Storage::new(memory).unwrap();
                let save = other.begin_save().unwrap();
                for object in theirs {
                    save.accept(object).unwrap();
                }
                save.finish().unwrap();
            }));
            let count = objects.len() as u64;
            let before = storage.diagnostics();
            let save = storage.begin_save().unwrap();
            for object in objects {
                save.accept(object.with_predecessors(AdvisoryPredecessors::new()))
                    .unwrap();
            }
            let outcome = save.finish().unwrap();
            raced = Some((count, outcome, before, storage.diagnostics()));
        },
    );
    let (count, outcome, before, after) = raced.unwrap();
    assert_eq!(outcome.reused, count, "every registration was lost");
    assert!(after.chain_rows > before.chain_rows, "the winners' chains");
    assert_eq!(after.chain_hinted_bases, before.chain_hinted_bases);
    assert_eq!(after.chain_hint_stale, 0);
}

#[test]
fn a_hinted_locator_serves_only_the_demand_that_bound_it() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    let base = memory.state.lock().unwrap().objects[&root].base.unwrap();
    let plain = Storage::new(copy(&memory)).unwrap();
    let length = plain.reader().unwrap().file_lengths(&[base]).unwrap();
    let bytes = plain.reader().unwrap().read_objects(&[base]).unwrap();
    for lengths in [true, false] {
        let provider = Arc::new(chain_port(&memory));
        let storage = Storage::new(provider.clone()).unwrap();
        storage.set_strict_hints(true);
        let reader = storage.reader().unwrap();
        reader.read_objects(&[root]).unwrap();
        let bound = storage.diagnostics();
        assert!(
            bound.chain_hinted_bases > 0,
            "the first demand bound the base"
        );
        // The next demand's root is that base: located again, not served
        // from the earlier demand's hint (its length or position).
        if lengths {
            assert_eq!(reader.file_lengths(&[base]).unwrap(), length);
        } else {
            assert_eq!(reader.read_objects(&[base]).unwrap(), bytes);
        }
        let after = storage.diagnostics();
        assert_eq!(after.locate - bound.locate, 1);
        assert_eq!(provider.asked.lock().unwrap().last(), Some(&vec![base]));
        assert_eq!(after.chain_hint_stale, 0);
    }
}

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

fn name(serial: u64) -> PathName {
    PathName::new(&format!("source-file-{serial:06}.rs")).unwrap()
}

/// A directory of 4,000 names and three small changes, one Save each, so
/// its pages are stored as programs against the pages they replace.
fn directory_history() -> Arc<metadata::MemoryMetadata> {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap();
    let mut root: Option<DirectoryRoot> = None;
    for round in 0..4_u64 {
        let reader = storage.reader().unwrap();
        let mut emitted = Emitted::default();
        {
            let mut objects = FilesystemObjects::new(&reader, &mut emitted);
            root = Some(match root {
                None => {
                    build_directory(&mut objects, (1..=4_000).map(|s| Ok((name(s), s))))
                        .unwrap()
                        .0
                }
                Some(base) => {
                    let added = 4_001 + round;
                    let mut changes = vec![(name(10 + round), None), (name(added), Some(added))];
                    changes.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                    apply_bindings(
                        &mut objects,
                        Some(base),
                        changes.into_iter().map(Ok),
                        layerfs_content::filesystem::MAXIMUM_SCRATCH_BYTES,
                        &mut |_, _| Ok(()),
                    )
                    .unwrap()
                    .0
                }
            });
        }
        drop(reader);
        let save = storage.begin_save().unwrap();
        for object in emitted.0 {
            save.accept(object).unwrap();
        }
        save.finish().unwrap();
    }
    memory
}

#[test]
fn a_program_page_binds_no_member_so_a_misplaced_one_cannot_break_its_read() {
    let memory = directory_history();
    let program = |role| {
        matches!(
            role,
            ObjectRole::DirectoryLeaf
                | ObjectRole::DirectoryBranch
                | ObjectRole::InodeBranch
                | ObjectRole::ExtentLeaf
                | ObjectRole::ExtentBranch
        )
    };
    // A page stored against another, and the member of its chain moved onto
    // another page of its role: the pool reader that rebuilds it has no
    // rebuild of its own.
    let (page, member) = {
        let state = memory.state.lock().unwrap();
        let page = *state
            .objects
            .values()
            .find(|row| program(row.role) && row.base.is_some())
            .expect("a page stored as a program");
        let base = state.objects[&page.base.unwrap()];
        let other = state
            .objects
            .values()
            .find(|row| {
                row.role == page.role
                    && row.object_id != base.object_id
                    && row.object_id != page.object_id
                    && (row.pack_id, row.group_number, row.record_number)
                        != (base.pack_id, base.group_number, base.record_number)
            })
            .expect("another page of the role");
        let member = ChainMember {
            object_id: None,
            pack: state.packs[&other.pack_id].info,
            group_number: other.group_number,
            record_number: other.record_number,
            canonical_length: other.canonical_length,
        };
        (page.object_id, member)
    };
    let (expected, _) = read(copy(&memory), &[page], true);
    for strict in [false, true] {
        let provider = editing(chain_port(&memory), move |chains| {
            for chain in chains.chains.iter_mut().filter(|c| c.root == page) {
                chain.members[0] = member;
            }
        });
        let (got, after) = read(provider.clone(), &[page], strict);
        assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
        assert!(after.chain_rows > 0, "the page's chain was located");
        assert_eq!(after.chain_hinted_bases, 0);
        assert_eq!(after.chain_hint_stale, 0);
    }
}

#[test]
fn a_member_pack_that_does_not_answer_as_named_is_stale_and_the_read_goes_on() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    let bottom = depth(&memory, root) - 1;
    let (expected, _) = read(copy(&memory), &[root], true);
    type Wrong = fn(&mut ChainMember);
    let cases: [(&str, Wrong); 2] = [
        ("a pack that is not stored", |m| m.pack.pack_id += 1_000_000),
        ("another digest", |m| {
            m.pack.key = ObjectKey::for_bytes(b"another digest")
        }),
    ];
    for (name, wrong) in cases {
        for strict in [false, true] {
            let provider = editing(chain_port(&memory), move |chains| {
                for chain in chains.chains.iter_mut().filter(|c| c.root == root) {
                    wrong(&mut chain.members[bottom]);
                }
            });
            let (got, after) = read(provider.clone(), &[root], strict);
            assert_eq!(after.chain_hint_stale, 1, "{name}");
            assert_eq!(provider.calls("read_groups"), 1, "{name}");
            if strict {
                refused(&got, "delta chain hint");
                continue;
            }
            assert_eq!(got.unwrap(), *expected.as_ref().unwrap(), "{name}");
        }
    }
}

#[test]
fn a_provider_defect_on_a_hinted_walk_is_not_replayed() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let (hinted, unhinted) = (roots[0], roots[1]);
    let mut locates = 0;
    // The second root comes without a chain, so the walk locates its base
    // while the first root's base is bound; that answer is malformed.
    let provider = editing(chain_port(&memory), move |chains| {
        locates += 1;
        chains.chains.retain(|c| c.root != unhinted);
        if locates == 2 {
            let mut unasked = chains.chains[0].clone();
            unasked.root = ObjectId::for_bytes(b"unasked");
            chains.chains.push(unasked);
        }
    });
    let (got, after) = read(provider.clone(), &[hinted, unhinted], false);
    refused(&got, "port chain locator");
    assert!(after.chain_hinted_bases > 0, "a hinted locator was in use");
    assert_eq!(after.chain_hint_stale, 0);
    assert_eq!(
        provider.asked.lock().unwrap().len(),
        2,
        "no locate after it"
    );
}

#[test]
fn a_refused_chain_answer_keeps_none_of_its_rows() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    let (expected, _) = read(copy(&memory), &[root], true);
    let mut first = true;
    let provider = editing(chain_port(&memory), move |chains| {
        if std::mem::take(&mut first) {
            chains.chains.push(chains.chains[0].clone());
        }
    });
    let storage = Storage::new(provider.clone()).unwrap();
    storage.set_strict_hints(true);
    let reader = storage.reader().unwrap();
    refused(&reader.read_objects(&[root]), "port chain locator");
    assert_eq!(reader.read_objects(&[root]).unwrap(), expected.unwrap());
    let asked = provider.asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[1], vec![root], "the root is located again");
}

#[test]
fn a_group_read_admits_its_ranges_when_the_pack_is_dense_in_the_cache() {
    let (memory, roots, _) = history(EDITS, |m| m);
    // Roots 0, 2 and 3 end in groups 0, 1 and 2 of one pack; by the third
    // read the first two hold over half of it, so the cache would take the
    // pack whole if it were acquiring it.
    {
        let state = memory.state.lock().unwrap();
        let bottom = |mut id: ObjectId| {
            while let Some(base) = state.objects[&id].base {
                id = base;
            }
            state.objects[&id]
        };
        let ends: Vec<_> = [roots[0], roots[2], roots[3]].map(bottom).to_vec();
        assert!(ends.iter().all(|row| row.pack_id == ends[0].pack_id));
        let groups: Vec<_> = ends.iter().map(|row| row.group_number).collect();
        assert_eq!(groups, [0, 1, 2]);
        let pack = &state.packs[&ends[0].pack_id];
        let header = layout::parse_header(&pack.body).unwrap();
        let held: usize = (0..2)
            .map(|g| {
                let view = layout::group_view(&pack.body, header, g).unwrap();
                view.end - view.start
            })
            .sum();
        assert!(held >= pack.info.length.div_ceil(2), "fixture density");
    }
    let provider = Arc::new(chain_port(&memory));
    let storage = Storage::new(provider.clone()).unwrap();
    storage.set_strict_hints(true);
    let reader = storage.reader().unwrap();
    for root in [roots[0], roots[2], roots[3]] {
        let (expected, _) = read(copy(&memory), &[root], true);
        assert_eq!(reader.read_objects(&[root]).unwrap(), expected.unwrap());
    }
    assert_eq!(provider.calls("read_groups"), 3);
    assert_eq!(provider.calls("read_scoped_pack"), 0);
    assert_eq!(provider.calls("read_packs"), 0);
}
