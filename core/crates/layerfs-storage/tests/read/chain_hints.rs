//! Delta-chain hints (#758): a chain locate lets a read walk every level with
//! no further locate, a hint that disagrees with its record or its bytes is
//! dropped and counted while the read stays correct, strict handles fail on
//! it, a malformed answer fails hard and keeps nothing, a Save binds members
//! only to rebuild and hash its bases (keeping what passed), a hinted locator
//! serves one demand, and a Store without chains costs exactly the default
//! calls.
use crate::chains;
use crate::metadata;

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
    port::{ObjectKey, PersistenceError},
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

#[test]
fn a_fresh_handle_save_rebuilds_its_bases_without_a_locate_per_level() {
    // 24 files whose bases are chains of depth >= 7; the last Save opens a
    // fresh handle, so none of its bases' chain elements is located yet.
    let shape = Shape {
        files: 24,
        versions: 9,
        reopen: Some(8),
        ..EDITS
    };
    let last = shape.versions - 1;
    let count = |calls: &[(&str, usize)], name| calls.iter().filter(|c| c.0 == name).count();
    let (plain, roots, _) = history(shape, |m| m);
    let levels = roots.iter().map(|root| depth(&plain, *root)).min().unwrap();
    assert!(levels >= 7, "fixture depth {levels}");
    let plain_save = last_save(&plain, last);
    // Today: a locate per base per level.
    assert!(count(&plain_save, "locate") >= shape.files * (levels - 1));
    for batched in [false, true] {
        let mut work = None;
        let (memory, hinted_roots, _) = history_with(
            shape,
            |m| {
                let mut provider = ChainProvider::new(m);
                provider.batched = batched;
                Arc::new(provider)
            },
            |storage, objects| {
                let before = storage.diagnostics();
                let save = storage.begin_save().unwrap();
                for object in objects {
                    save.accept(object).unwrap();
                }
                save.finish().unwrap();
                work = Some((before, storage.diagnostics()));
            },
        );
        assert_eq!(roots, hinted_roots);
        {
            let (a, b) = (plain.state.lock().unwrap(), memory.state.lock().unwrap());
            assert_eq!(a.objects, b.objects, "the same records");
            assert_eq!(a.packs, b.packs, "byte-identical packs");
        }
        let (before, after) = work.unwrap();
        let save = last_save(&memory, last);
        // The membership locate of each wave and one group read per 2 MiB
        // page; every base's chain hashed and kept, none located by level.
        let waves = after.locate - before.locate;
        let bytes = after.group_read_bytes - before.group_read_bytes;
        assert_eq!(count(&save, "locate") as u64, waves, "{save:?}");
        assert!(waves <= 2, "{save:?}");
        assert!(
            count(&save, "read_groups") as u64 <= waves + bytes.div_ceil(2 << 20),
            "{save:?}"
        );
        assert_eq!(
            count(&save, "read_scoped_pack") + count(&save, "read_packs"),
            0
        );
        let verified = after.chain_verified_bases - before.chain_verified_bases;
        assert!(
            verified >= (shape.files * (levels - 1)) as u64,
            "{verified}"
        );
        assert_eq!(after.chain_hint_stale, 0);
    }
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

/// A directory of 4,000 names changed in `rounds` Saves on one handle; the
/// objects of one more change, constructed on that handle, are returned
/// unsaved with the Store, so a fresh handle can Save them.
fn directory_change(rounds: u64) -> (Arc<metadata::MemoryMetadata>, Vec<FinalizedObject>) {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap();
    let mut root: Option<DirectoryRoot> = None;
    for round in 0..=rounds {
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
        if round == rounds {
            return (memory, emitted.0);
        }
        let save = storage.begin_save().unwrap();
        for object in emitted.0 {
            save.accept(object).unwrap();
        }
        save.finish().unwrap();
    }
    unreachable!()
}

#[test]
fn a_fresh_handle_save_rebuilds_its_program_page_bases_without_a_locate_per_level() {
    // Directory pages stored as programs against the pages they replace,
    // chains of depth >= 4; the change is Saved by a fresh handle, so no
    // element of its bases' chains is located yet.
    let (memory, objects) = directory_change(6);
    let program_bases: Vec<usize> = objects
        .iter()
        .filter(|object| object.role() == ObjectRole::DirectoryLeaf)
        .filter_map(|object| object.predecessors().ids().next())
        .map(|base| depth(&memory, base))
        .filter(|levels| *levels > 0)
        .collect();
    assert!(!program_bases.is_empty(), "a page stored against a program");
    let levels = program_bases.iter().copied().max().unwrap();
    assert!(levels >= 4, "fixture depth {levels}");
    let save = |port: Arc<dyn layerfs_storage::port::PackPersistence>| {
        let storage = Storage::new(port).unwrap();
        storage.set_strict_hints(true);
        let save = storage.begin_save().unwrap();
        for object in objects.clone() {
            save.accept(object).unwrap();
        }
        save.finish().unwrap();
        storage.diagnostics()
    };
    let plain = copy(&memory);
    let plain_work = save(plain.clone());
    let count = |memory: &metadata::MemoryMetadata, name| {
        let calls = memory.calls.lock().unwrap();
        calls.iter().filter(|c| c.0 == name).count()
    };
    // Today: every level of a program page's chain is located alone.
    let today = count(&plain, "locate");
    assert!(today > program_bases.len() * levels, "{plain_work:?}");
    for batched in [false, true] {
        let mut provider = chain_port(&memory);
        provider.batched = batched;
        let provider = Arc::new(provider);
        let work = save(provider.clone());
        {
            let (a, b) = (
                plain.state.lock().unwrap(),
                provider.inner.state.lock().unwrap(),
            );
            assert_eq!(a.objects, b.objects, "the same records");
            assert_eq!(a.packs, b.packs, "byte-identical packs");
        }
        // The membership locate of each wave and the closure check of its
        // publication; every program-page base hashed and kept, none located
        // by level.
        let locates = count(&provider.inner, "locate");
        assert_eq!(locates as u64, work.locate);
        assert!(locates <= 2, "{work:?}");
        assert!(
            work.chain_verified_bases >= (program_bases.len() * levels) as u64,
            "{work:?}"
        );
        assert_eq!(work.chain_hint_stale, 0);
    }
}

/// Saves `objects` on a fresh handle over `port`, with or without strict
/// hints: the outcome, the handle's counts and the handle.
fn fresh_save(
    port: Arc<dyn layerfs_storage::port::PackPersistence>,
    objects: &[FinalizedObject],
    strict: bool,
) -> (
    layerfs_storage::StorageResult<()>,
    layerfs_storage::Diagnostics,
    Storage,
) {
    let storage = Storage::new(port).unwrap();
    storage.set_strict_hints(strict);
    let done = (|| {
        let save = storage.begin_save()?;
        for object in objects {
            save.accept(object.clone())?;
        }
        save.finish().map(drop)
    })();
    let work = storage.diagnostics();
    (done, work, storage)
}

fn is_program(role: ObjectRole) -> bool {
    matches!(
        role,
        ObjectRole::DirectoryLeaf
            | ObjectRole::DirectoryBranch
            | ObjectRole::InodeBranch
            | ObjectRole::ExtentLeaf
            | ObjectRole::ExtentBranch
    )
}

/// The first program page of `objects` whose first predecessor is stored
/// with its role as a program itself: the page's index and that base.
fn program_base(
    memory: &metadata::MemoryMetadata,
    objects: &[FinalizedObject],
) -> (usize, ObjectId) {
    let state = memory.state.lock().unwrap();
    objects
        .iter()
        .enumerate()
        .find_map(|(at, object)| {
            let base = object.predecessors().ids().next()?;
            let row = state.objects.get(&base)?;
            (is_program(object.role()) && row.role == object.role() && row.base.is_some())
                .then_some((at, base))
        })
        .expect("a page whose predecessor is a program")
}

fn same_store(a: &metadata::MemoryMetadata, b: &metadata::MemoryMetadata) {
    let (a, b) = (a.state.lock().unwrap(), b.state.lock().unwrap());
    assert_eq!(a.objects, b.objects, "the same records");
    assert_eq!(a.packs, b.packs, "byte-identical packs");
}

#[test]
fn a_misplaced_member_of_a_program_base_fails_its_hash_and_the_save_walks_it_from_locates() {
    // The member under a program base, moved onto another page of its role:
    // the Save's check of that base fails at the moved element, drops every
    // hint and selection walks the chain from authoritative locates.
    let (memory, objects) = directory_change(6);
    let (_, base) = program_base(&memory, &objects);
    let (below, member) = {
        let state = memory.state.lock().unwrap();
        let row = state.objects[&base];
        let below = state.objects[&row.base.unwrap()];
        let other = state
            .objects
            .values()
            .find(|other| {
                other.role == row.role
                    && other.object_id != below.object_id
                    && other.object_id != base
                    && (other.pack_id, other.group_number, other.record_number)
                        != (below.pack_id, below.group_number, below.record_number)
            })
            .expect("another page of the role");
        let member = ChainMember {
            object_id: None,
            pack: state.packs[&other.pack_id].info,
            group_number: other.group_number,
            record_number: other.record_number,
            canonical_length: other.canonical_length,
        };
        (below.object_id, member)
    };
    let plain = copy(&memory);
    fresh_save(plain.clone(), &objects, true).0.unwrap();
    let (expected, _) = read(copy(&memory), &[below], true);
    for strict in [false, true] {
        let provider = editing(chain_port(&memory), move |chains| {
            for chain in chains.chains.iter_mut().filter(|c| c.root == base) {
                chain.members[0] = member;
            }
        });
        let (done, work, storage) = fresh_save(provider.clone(), &objects, strict);
        assert!(work.chain_hint_stale >= 1, "{work:?}");
        if strict {
            assert!(
                matches!(done, Err(StorageError::Integrity("delta chain hint"))),
                "{done:?}"
            );
            continue;
        }
        done.unwrap();
        same_store(&plain, &provider.inner);
        // No locator at the moved position was kept: the element below the
        // base reads back through this handle.
        let got = storage.reader().unwrap().read_objects(&[below]);
        assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
    }
}

/// An object of `role` whose bytes are 2,000 fixed noise bytes with one
/// byte changed by `change`, so versions differ by one byte, and that
/// declare no page role; its predecessor is `base`.
fn noise_page(role: ObjectRole, change: u8, base: Option<ObjectId>) -> FinalizedObject {
    let mut state = 0x9e37_79b9_u32;
    let mut bytes: Vec<u8> = (0..2_000)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    bytes[1_000] ^= change;
    let canonical = layerfs_content::object::encode_bytes_object(&bytes).unwrap();
    let object = FinalizedObject::new(role, canonical).unwrap();
    match base {
        Some(base) => object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap()),
        None => object,
    }
}

/// Two noise pages of `role`, one Save each, the second stored as a program
/// against the first: the Store and the second page's id.
fn noise_history(role: ObjectRole) -> (Arc<metadata::MemoryMetadata>, ObjectId) {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap();
    let mut base = None;
    for change in 1..=2 {
        let object = noise_page(role, change, base);
        base = Some(object.id());
        let save = storage.begin_save().unwrap();
        save.accept(object).unwrap();
        save.finish().unwrap();
    }
    let base = base.unwrap();
    assert_eq!(depth(&memory, base), 1, "the second page is a program");
    (memory, base)
}

#[test]
fn captured_inode_reads_keep_authoritative_failures_in_a_mixed_group() {
    let (memory, base) = noise_history(ObjectRole::InodeBranch);
    let tail = noise_page(ObjectRole::InodeBranch, 3, None);
    let tail_id = tail.id();
    let saved = Storage::new(memory.clone()).unwrap();
    let save = saved.begin_save().unwrap();
    save.accept(tail).unwrap();
    save.finish().unwrap();
    let state = memory.state.lock().unwrap();
    let member = state.objects[&state.objects[&base].base.unwrap()].pack_id;
    let head = state.objects[&base].pack_id;
    let other = state.objects[&tail_id].pack_id;
    drop(state);
    assert_ne!(member, head);
    assert_ne!(member, other);
    // The chain belongs to `base`. With both roots in one demand the
    // first pack is authoritative, then its ancestor, then `tail_id`.
    for fault in [
        Ok(vec![
            (member, PersistenceError::Missing),
            (other, PersistenceError::Malformed),
        ]),
        Err(PersistenceError::Malformed),
        Err(PersistenceError::Uncertain),
    ] {
        let expected = fault.clone().err().unwrap_or(PersistenceError::Malformed);
        let mut provider = chain_port(&memory);
        provider.batched = true;
        *provider.classified_fault.lock().unwrap() = Some(fault);
        let provider = Arc::new(provider);
        let reader = Storage::new(provider.clone()).unwrap();
        let save = reader.begin_save().unwrap().joining_metadata_reads();
        let _captured = save.captured_sink();
        let error = save.read_objects(&[base, tail_id]).unwrap_err();
        let cause = match &error {
            StorageError::UnknownOutcome { original } => original.as_ref(),
            other => other,
        };
        let StorageError::Io(cause) = cause else {
            panic!("{error:?}");
        };
        assert_eq!(
            cause.get_ref().unwrap().downcast_ref::<PersistenceError>(),
            Some(&expected)
        );
        assert_eq!(
            error.is_unknown_outcome(),
            expected == PersistenceError::Uncertain
        );
        assert!(matches!(
            save.read_objects(&[base, tail_id]),
            Err(StorageError::Aborted)
        ));
        assert_eq!(provider.requests.lock().unwrap().len(), 1);
        assert_eq!(reader.diagnostics().chain_hint_stale, 0);
    }
}

#[test]
fn a_retained_authoritative_group_keeps_authority_for_a_later_whole_read() {
    let (memory, base) = noise_history(ObjectRole::InodeBranch);
    let saved = Storage::new(memory.clone()).unwrap();
    let save = saved.begin_save().unwrap();
    let mut ids = Vec::new();
    for seed in 1..=24 {
        let mut state = seed as u32;
        let bytes: Vec<_> = (0..4_000)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        let canonical = layerfs_content::object::encode_bytes_object(&bytes).unwrap();
        let object = FinalizedObject::new(ObjectRole::InodeBranch, canonical).unwrap();
        ids.push(object.id());
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
    let (prime, held, guessed_group, info) = {
        let state = memory.state.lock().unwrap();
        let rows: Vec<_> = ids.iter().map(|id| state.objects[id]).collect();
        let pair = rows.iter().find_map(|row| {
            let pack = &state.packs[&row.pack_id];
            let header = layout::parse_header(&pack.body).unwrap();
            let view = layout::group_view(&pack.body, header, row.group_number).unwrap();
            let prime = rows.iter().find(|other| {
                other.object_id != row.object_id
                    && other.pack_id == row.pack_id
                    && other.group_number == row.group_number
            })?;
            rows.iter()
                .find(|other| {
                    row.pack_id == other.pack_id
                        && row.group_number != other.group_number
                        && 2 * (view.end - view.start) < pack.info.length
                })
                .map(|other| {
                    (
                        prime.object_id,
                        row.object_id,
                        other.group_number,
                        pack.info,
                    )
                })
        });
        pair.expect("one compact pack with distinct sparse groups")
    };
    assert!(info.length <= layerfs_storage::policy::PACK_LIMIT / 2);
    let mut port = chain_port(&memory);
    port.batched = true;
    let provider = editing(port, move |chains| {
        if let Some(hinted) = chains.chains.iter_mut().find(|c| c.root == base) {
            hinted.members[0].pack = info;
            hinted.members[0].group_number = guessed_group;
        }
    });
    let reader = Storage::new(provider.clone()).unwrap();
    let save = reader.begin_save().unwrap().joining_metadata_reads();
    let _captured = save.captured_sink();
    save.read_objects(&[prime]).unwrap();
    assert_eq!(reader.diagnostics().range_selected, 1);
    *provider.classified_fault.lock().unwrap() =
        Some(Ok(vec![(info.pack_id, PersistenceError::Missing)]));

    let error = save.read_objects(&[held, base]).unwrap_err();
    let StorageError::Io(cause) = error else {
        panic!("{error:?}");
    };
    assert_eq!(
        cause.get_ref().unwrap().downcast_ref::<PersistenceError>(),
        Some(&PersistenceError::Missing)
    );
    assert_eq!(reader.diagnostics().chain_hint_stale, 0);
    assert_eq!(provider.requests.lock().unwrap().len(), 1);
    assert_eq!(provider.calls("read_scoped_pack"), 1, "no replay");
}

#[test]
fn a_hinted_element_whose_page_header_names_another_role_is_not_kept() {
    // Pages stored under the directory-leaf role whose bytes declare no
    // page role: the catalog says directory leaf, a member carries no role,
    // so only the hinted element's authenticated bytes can say otherwise.
    let (memory, base) = noise_history(ObjectRole::DirectoryLeaf);
    let objects = vec![noise_page(ObjectRole::DirectoryLeaf, 3, Some(base))];
    let plain = copy(&memory);
    fresh_save(plain.clone(), &objects, true).0.unwrap();
    assert_eq!(depth(&plain, objects[0].id()), 2);
    for strict in [false, true] {
        let provider = Arc::new(chain_port(&memory));
        let (done, work, _) = fresh_save(provider.clone(), &objects, strict);
        assert_eq!(work.chain_hint_stale, 1, "{work:?}");
        assert_eq!(work.chain_verified_bases, 0, "{work:?}");
        if strict {
            assert!(
                matches!(done, Err(StorageError::Integrity("delta chain hint"))),
                "{done:?}"
            );
            continue;
        }
        done.unwrap();
        same_store(&plain, &provider.inner);
    }
}

#[test]
fn a_program_base_whose_own_row_is_wrong_fails_with_its_own_error_not_as_a_stale_hint() {
    // The base's catalog length disagrees with its record: a failure at the
    // authoritative root, which no hint below it caused.
    let (memory, objects) = directory_change(6);
    let (_, base) = program_base(&memory, &objects);
    let corrupt = || {
        let port = copy(&memory);
        port.state
            .lock()
            .unwrap()
            .objects
            .get_mut(&base)
            .unwrap()
            .canonical_length += 1;
        port
    };
    let (done, _, _) = fresh_save(corrupt(), &objects, true);
    let Err(StorageError::Integrity(reason)) = done else {
        panic!("{done:?}");
    };
    assert_eq!(reason, "branch delta length");
    for strict in [false, true] {
        let provider = Arc::new(ChainProvider::new(corrupt()));
        let (done, work, _) = fresh_save(provider, &objects, strict);
        assert!(
            matches!(done, Err(StorageError::Integrity(found)) if found == reason),
            "{done:?}"
        );
        assert_eq!(work.chain_hint_stale, 0, "{work:?}");
    }
}

#[test]
fn a_save_checks_only_the_chain_selection_walks() {
    // A directory leaf whose only predecessor is a stored directory branch:
    // selection takes no base of another role, so the Save walks none of
    // that branch's chain, even one whose root row is wrong.
    let (memory, branch) = noise_history(ObjectRole::DirectoryBranch);
    let objects = vec![noise_page(ObjectRole::DirectoryLeaf, 3, Some(branch))];
    let corrupt = || {
        let port = copy(&memory);
        port.state
            .lock()
            .unwrap()
            .objects
            .get_mut(&branch)
            .unwrap()
            .canonical_length += 1;
        port
    };
    let plain = corrupt();
    fresh_save(plain.clone(), &objects, true).0.unwrap();
    assert_eq!(depth(&plain, objects[0].id()), 0, "no base of another role");
    for strict in [false, true] {
        let provider = Arc::new(ChainProvider::new(corrupt()));
        let (done, work, _) = fresh_save(provider.clone(), &objects, strict);
        done.unwrap();
        assert_eq!(work.chain_hint_stale, 0, "{work:?}");
        same_store(&plain, &provider.inner);
    }
}

#[test]
fn a_pack_descriptor_only_a_stale_member_gave_is_forgotten_with_the_hints() {
    // The bottom member names its pack under another digest and a group the
    // pack lacks, with a matching directory, so no group read reaches it: the
    // walk binds it and fails there. The rebuild then locates that base and
    // meets the pack's real descriptor, which must not contradict the
    // member's: that descriptor went with the hints.
    let (memory, roots, _) = history(EDITS, |m| m);
    let root = roots[0];
    let bottom = depth(&memory, root) - 1;
    let pack = {
        let state = memory.state.lock().unwrap();
        let mut packs = vec![state.objects[&root].pack_id];
        let mut id = root;
        while let Some(base) = state.objects[&id].base {
            packs.push(state.objects[&base].pack_id);
            id = base;
        }
        let pack = packs.pop().unwrap();
        assert!(
            !packs.contains(&pack),
            "the bottom member's pack is its own"
        );
        state.packs[&pack].info
    };
    let named = layerfs_storage::location::PackInfo {
        key: ObjectKey::for_bytes(b"another digest"),
        ..pack
    };
    let (expected, _) = read(copy(&memory), &[root], true);
    for strict in [false, true] {
        let provider = editing(chain_port(&memory), move |chains| {
            for chain in chains.chains.iter_mut().filter(|c| c.root == root) {
                chain.members[bottom].pack = named;
                chain.members[bottom].group_number = GROUP_COUNT_LIMIT - 1;
            }
            for directory in &mut chains.directories {
                if directory.pack == pack {
                    directory.pack = named;
                }
            }
        });
        let (got, after) = read(provider.clone(), &[root], strict);
        assert_eq!(after.chain_hint_stale, 1);
        if strict {
            refused(&got, "delta chain hint");
            continue;
        }
        assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
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

#[test]
fn a_program_page_read_locates_every_deeper_level_of_its_chain_in_one_locate() {
    // A read binds no member of a program page's chain; the levels below
    // the first are still located together, from the base ids the cached
    // records name, with no member trusted (#758 fix 3).
    let (memory, _) = directory_change(6);
    let (page, levels) = {
        let state = memory.state.lock().unwrap();
        let pages: Vec<ObjectId> = state
            .objects
            .values()
            .filter(|row| is_program(row.role) && row.base.is_some())
            .map(|row| row.object_id)
            .collect();
        drop(state);
        pages
            .into_iter()
            .map(|page| (page, depth(&memory, page)))
            .max_by_key(|(_, levels)| *levels)
            .expect("a page stored as a program")
    };
    assert!(levels >= 4, "fixture depth {levels}");
    let (expected, today) = read(copy(&memory), &[page], true);
    let expected = expected.unwrap();
    assert_eq!(today.locate, levels as u64 + 1, "today: one per level");
    for batched in [false, true] {
        let mut provider = chain_port(&memory);
        provider.batched = batched;
        let provider = Arc::new(provider);
        let (got, after) = read(provider.clone(), &[page], true);
        assert_eq!(got.unwrap(), expected);
        // The page's own locate, then one for every level of its chain.
        assert_eq!(after.locate, 2, "{after:?}");
        let asked = provider.asked.lock().unwrap().clone();
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[1].len(), levels, "every level's base, located");
        assert_eq!(after.chain_hinted_bases, 0, "no member trusted");
        assert_eq!(after.chain_hint_stale, 0);
    }
    // A member that names a record off the chain makes the guesses below it
    // wrong: the read stays correct, locates those levels as before, and
    // counts nothing stale, strict or not.
    let other = {
        let state = memory.state.lock().unwrap();
        let chain = chain_of(&state.objects, page);
        let member = state
            .objects
            .values()
            .find(|other| is_program(other.role) && !chain.contains(&other.object_id))
            .expect("a page outside the chain");
        ChainMember {
            object_id: None,
            pack: state.packs[&member.pack_id].info,
            group_number: member.group_number,
            record_number: member.record_number,
            canonical_length: member.canonical_length,
        }
    };
    for strict in [false, true] {
        let provider = editing(chain_port(&memory), move |chains| {
            for chain in chains.chains.iter_mut().filter(|c| c.root == page) {
                chain.members[1] = other;
            }
        });
        let (got, after) = read(provider.clone(), &[page], strict);
        assert_eq!(got.unwrap(), expected);
        assert!(after.locate > 2, "{after:?}");
        assert!(after.locate <= levels as u64 + 1, "{after:?}");
        assert_eq!(after.chain_hinted_bases, 0);
        assert_eq!(after.chain_hint_stale, 0);
    }
}

/// `id`'s stored chain, `id` first.
fn chain_of(
    objects: &std::collections::BTreeMap<ObjectId, layerfs_storage::location::ObjectLocation>,
    id: ObjectId,
) -> Vec<ObjectId> {
    let mut chain = vec![id];
    while let Some(base) = objects[chain.last().unwrap()].base {
        chain.push(base);
    }
    chain
}

/// [`directory_change`] with one more change of the same pages Saved on
/// that handle after the returned objects were constructed, so the pages
/// the returned objects replace have stored descendants.
fn directory_change_with_sibling(
    rounds: u64,
) -> (Arc<metadata::MemoryMetadata>, Vec<FinalizedObject>) {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let storage = Storage::new(memory.clone()).unwrap();
    let change = |root: Option<DirectoryRoot>, removed: u64, added: u64| {
        let reader = storage.reader().unwrap();
        let mut emitted = Emitted::default();
        let root = {
            let mut objects = FilesystemObjects::new(&reader, &mut emitted);
            match root {
                None => {
                    build_directory(&mut objects, (1..=4_000).map(|s| Ok((name(s), s))))
                        .unwrap()
                        .0
                }
                Some(base) => {
                    let mut changes = vec![(name(removed), None), (name(added), Some(added))];
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
            }
        };
        (root, emitted.0)
    };
    let save = |objects: Vec<FinalizedObject>| {
        let save = storage.begin_save().unwrap();
        for object in objects {
            save.accept(object).unwrap();
        }
        save.finish().unwrap();
    };
    let mut root = None;
    for round in 0..rounds {
        let (next, objects) = change(root, 10 + round, 4_001 + round);
        save(objects);
        root = Some(next);
    }
    let (_, returned) = change(root, 10 + rounds, 4_001 + rounds);
    let (_, sibling) = change(root, 11 + rounds, 5_001 + rounds);
    save(sibling);
    (memory, returned)
}

#[test]
fn a_hinted_element_whose_record_leads_back_to_an_authoritative_one_is_a_stale_hint() {
    // A program base R with a stored descendant S (stored against R): the
    // member under R moved onto S's record leads the Save's check from that
    // hinted element back to R, located authoritatively, so the walk fails
    // as a cycle at an element only an unauthenticated record named. That
    // is the hint's failure: counted once (strict refuses) and selection
    // walks the chain from authoritative locates; the Save stores what the
    // default port stores.
    let (memory, objects) = directory_change_with_sibling(6);
    let (base, member) = {
        let state = memory.state.lock().unwrap();
        let found = objects.iter().find_map(|object| {
            let first = object.predecessors().ids().next()?;
            let row = state.objects.get(&first)?;
            if !is_program(object.role()) || row.role != object.role() || row.base.is_none() {
                return None;
            }
            let descendant = state.objects.values().find(|other| {
                other.base == Some(first) && objects.iter().all(|o| o.id() != other.object_id)
            })?;
            Some((first, *descendant))
        });
        let (base, descendant) = found.expect("a program base with a stored descendant");
        let member = ChainMember {
            object_id: None,
            pack: state.packs[&descendant.pack_id].info,
            group_number: descendant.group_number,
            record_number: descendant.record_number,
            canonical_length: descendant.canonical_length,
        };
        (base, member)
    };
    let plain = copy(&memory);
    fresh_save(plain.clone(), &objects, true).0.unwrap();
    for strict in [false, true] {
        let provider = editing(chain_port(&memory), move |chains| {
            for chain in chains.chains.iter_mut().filter(|c| c.root == base) {
                chain.members[0] = member;
            }
        });
        let (done, work, _) = fresh_save(provider.clone(), &objects, strict);
        assert_eq!(work.chain_hint_stale, 1, "{done:?} {work:?}");
        if strict {
            assert!(
                matches!(done, Err(StorageError::Integrity("delta chain hint"))),
                "{done:?}"
            );
            continue;
        }
        done.unwrap();
        same_store(&plain, &provider.inner);
    }
}

#[test]
fn an_authoritative_descriptor_that_contradicts_a_member_s_makes_the_hint_stale() {
    // Two roots read together whose bases share one pack P. The member under
    // the first names P under another digest and a group P lacks (with a
    // matching directory), so no group read reaches it and its descriptor
    // stays a member's; the second comes with no chain, so its base is
    // located at the next level and brings P's real descriptor. That
    // contradiction is a stale hint (counted once; strict refuses), every
    // hint is dropped, and the read stays correct.
    let (memory, roots, _) = history(EDITS, |m| m);
    let (first, second, pack) = {
        let state = memory.state.lock().unwrap();
        let base = |id: &ObjectId| state.objects[&state.objects[id].base.unwrap()];
        let chained: Vec<ObjectId> = roots
            .iter()
            .copied()
            .filter(|id| state.objects[id].base.is_some())
            .collect();
        let (first, second) = chained
            .iter()
            .flat_map(|a| chained.iter().map(move |b| (*a, *b)))
            .find(|(a, b)| a != b && base(a).pack_id == base(b).pack_id)
            .expect("two roots whose bases share a pack");
        (first, second, state.packs[&base(&first).pack_id].info)
    };
    let named = layerfs_storage::location::PackInfo {
        key: ObjectKey::for_bytes(b"another digest"),
        ..pack
    };
    let (expected, _) = read(copy(&memory), &[first, second], true);
    for strict in [false, true] {
        let provider = editing(chain_port(&memory), move |chains| {
            chains.chains.retain(|c| c.root != second);
            for chain in chains.chains.iter_mut().filter(|c| c.root == first) {
                chain.members[0].pack = named;
                chain.members[0].group_number = GROUP_COUNT_LIMIT - 1;
            }
            for directory in &mut chains.directories {
                if directory.pack == pack {
                    directory.pack = named;
                }
            }
        });
        let (got, after) = read(provider.clone(), &[first, second], strict);
        assert_eq!(after.chain_hint_stale, 1, "{got:?} {after:?}");
        if strict {
            refused(&got, "delta chain hint");
            continue;
        }
        assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
        assert_eq!(after.chain_hinted_bases, 1, "the member was bound first");
    }
}

#[test]
fn a_chain_a_read_binds_adds_no_guesses_to_another_chain_s_locate() {
    // Two chained roots with disjoint chains, the second answered without
    // its chain: the first root's members bind every level of its chain with
    // no locate, so the locates the second root's levels make carry none of
    // the first chain's ids (fix 3b; before it, the second locate carried the
    // first chain's deeper bases as guesses).
    let (memory, roots, _) = history(EDITS, |m| m);
    let (first, second, first_chain) = {
        let state = memory.state.lock().unwrap();
        let chained: Vec<ObjectId> = roots
            .iter()
            .copied()
            .filter(|id| state.objects[id].base.is_some())
            .collect();
        let (first, second) = chained
            .iter()
            .flat_map(|a| chained.iter().map(move |b| (*a, *b)))
            .find(|(a, b)| {
                let ca = chain_of(&state.objects, *a);
                let cb = chain_of(&state.objects, *b);
                a != b && ca.iter().all(|x| !cb.contains(x))
            })
            .expect("two chained roots with disjoint chains");
        (first, second, chain_of(&state.objects, first))
    };
    assert!(depth(&memory, first) >= 2, "a chain with deeper levels");
    let (expected, _) = read(copy(&memory), &[first, second], true);
    let (_, alone) = read(Arc::new(chain_port(&memory)), &[first], true);
    assert!(alone.chain_hinted_bases > 0, "{alone:?}");
    let provider = editing(chain_port(&memory), move |chains| {
        chains.chains.retain(|c| c.root != second);
    });
    let (got, after) = read(provider.clone(), &[first, second], true);
    assert_eq!(got.unwrap(), expected.unwrap());
    let asked = provider.asked.lock().unwrap().clone();
    assert!(asked.len() >= 2, "{asked:?}");
    for (call, ids) in asked.iter().enumerate().skip(1) {
        let carried = ids.iter().filter(|id| first_chain.contains(id)).count();
        assert_eq!(carried, 0, "locate {call} carries the bound chain's ids");
    }
    // Every level of the first chain is bound (here by the level walk and
    // again by the resolution), never located.
    assert!(
        after.chain_hinted_bases >= alone.chain_hinted_bases,
        "{after:?}"
    );
    assert_eq!(after.chain_hint_stale, 0);
}

#[test]
fn guesses_decode_each_group_once_per_level_and_count_it() {
    // Every program page of a changed directory read at once: their chains'
    // records lie in the groups of the Saves that wrote them, which several
    // pages share. A level's guesses decode each such group at most once and
    // count it in `prefetch_group_decodes` (fix 3b; before it, they decoded
    // a shared group once per page and counted nothing).
    let (memory, _) = directory_change(6);
    let (pages, chain_groups) = {
        let state = memory.state.lock().unwrap();
        let pages: Vec<ObjectId> = state
            .objects
            .values()
            .filter(|row| is_program(row.role) && row.base.is_some())
            .map(|row| row.object_id)
            .collect();
        let groups: std::collections::BTreeSet<(i64, usize)> = pages
            .iter()
            .flat_map(|page| chain_of(&state.objects, *page).into_iter().skip(1))
            .map(|id| (state.objects[&id].pack_id, state.objects[&id].group_number))
            .collect();
        (pages, groups.len() as u64)
    };
    assert!(pages.len() >= 2, "several program pages");
    let (expected, plain) = read(copy(&memory), &pages, true);
    let provider = Arc::new(chain_port(&memory));
    let (got, after) = read(provider, &pages, true);
    assert_eq!(got.unwrap(), expected.unwrap());
    assert_eq!(after.chain_hinted_bases, 0, "no member trusted");
    let guessed = after
        .prefetch_group_decodes
        .saturating_sub(plain.prefetch_group_decodes);
    assert!(guessed >= 1, "the guesses' decodes are counted: {after:?}");
    assert!(
        guessed <= chain_groups,
        "at most once per group: {guessed} > {chain_groups}"
    );
}
