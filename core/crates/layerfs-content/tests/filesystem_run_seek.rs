//! Bounded count diagnostics over real FileBacking, separate from speed admission.
//! Expected rows/effects come from original snapshots/events, never lookup output.

#[path = "support/ordering_observer.rs"]
mod ordering_observer;
mod support;

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

use layerfs_content::filesystem::references::{
    merge_runs, MergeWork, OrderingBacking, PendingState, ReferenceReducer, Row, Run, RunReader,
    RunStore, ROW_BYTES,
};
use layerfs_content::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ContentError;
use ordering_observer::ObservedBacking;
use support::filesystem::{synthetic, value, TempDir};

const CAPACITY: u64 = 8 * 1024 * 1024;
const BUFFER: usize = 7 * ROW_BYTES;

fn typed_value(serial: u64, generation: u64) -> InodeValue {
    let mut value = value(
        InodeKind::RegularFile,
        synthetic(&format!("seek/content/{generation}/{serial}")),
        synthetic(&format!("seek/meta/{generation}/{serial}")),
    );
    value.namespace_ref_count = 1000 + generation;
    value
}

fn snapshot(serial: u64, generation: u64) -> Row {
    if (serial + generation) % 3 == 0 {
        Row::Count {
            serial,
            value: Some(typed_value(serial, generation)),
            count: generation * 100 + serial,
        }
    } else {
        Row::Effect {
            serial,
            value: Some(typed_value(serial, generation)),
            delta: generation as i64 * 17 - serial as i64,
        }
    }
}

fn points_per_query(rows: u64) -> u64 {
    let ceiling_log = u64::BITS - rows.leading_zeros();
    u64::from(ceiling_log) + 1
}

#[test]
fn fresh_ascending_hits_and_gaps_pay_one_sequential_file_pass() {
    let temp = TempDir::new("run-seek-ascending");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let observer = backing.observer();
    let mut store = RunStore::new(Some(&mut backing), BUFFER, CAPACITY);
    const ROWS: u64 = 103;
    let expected = (1..=ROWS)
        .map(|index| {
            let serial = index * 2;
            (serial, snapshot(serial, 1))
        })
        .collect::<BTreeMap<_, _>>();
    store.spill(&expected).unwrap();
    assert_eq!(store.single_run().unwrap().handle.len(), ROWS * 96);
    let before = observer.counts();
    let decoded_before = store.work().rows_read;
    for serial in 1..=ROWS * 2 + 1 {
        assert_eq!(store.find(serial).unwrap(), expected.get(&serial).copied());
    }
    let after = observer.counts();
    assert_eq!(after.reads - before.reads, ROWS.div_ceil(7));
    assert_eq!(after.requested_bytes - before.requested_bytes, ROWS * 96);
    assert_eq!(after.successful_bytes - before.successful_bytes, ROWS * 96);
    assert!(after.maximum_request <= BUFFER);
    assert_eq!(store.work().rows_read - decoded_before, ROWS);
    assert_eq!(after.creates, before.creates);
    assert_eq!(after.appends, before.appends);
    eprintln!("DIAGNOSTIC fresh-ascending rows={ROWS} read_at={} requested_bytes={} decoded_rows={ROWS} buffer={BUFFER}",
        after.reads - before.reads, after.requested_bytes - before.requested_bytes);
    store.release().unwrap();
    drop(store);
    assert_eq!(observer.counts().releases, 1);
    assert_eq!(backing.held_bytes(), 0);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn primed_prime_population_alternating_and_descending_points_have_logarithmic_reads() {
    let temp = TempDir::new("run-seek-prime");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let observer = backing.observer();
    let mut store = RunStore::new(Some(&mut backing), BUFFER, CAPACITY);
    const ROWS: u64 = 1021;
    let expected = (1..=ROWS)
        .map(|serial| (serial, snapshot(serial, 2)))
        .collect::<BTreeMap<_, _>>();
    store.spill(&expected).unwrap();
    assert_eq!(store.find(ROWS).unwrap(), expected.get(&ROWS).copied());
    let before = observer.counts();
    let decoded_before = store.work().rows_read;
    for low in 1..=ROWS {
        for serial in [low, ROWS] {
            assert_eq!(store.find(serial).unwrap(), expected.get(&serial).copied());
        }
    }
    for serial in (1..=ROWS).rev() {
        assert_eq!(store.find(serial).unwrap(), expected.get(&serial).copied());
    }
    let after = observer.counts();
    let queries = 3 * ROWS;
    let reads = after.reads - before.reads;
    assert!(reads <= queries * points_per_query(ROWS));
    assert_eq!(after.single_row_reads - before.single_row_reads, reads);
    assert_eq!(after.requested_bytes - before.requested_bytes, reads * 96);
    assert_eq!(after.successful_bytes - before.successful_bytes, reads * 96);
    assert_eq!(store.work().rows_read - decoded_before, reads);
    assert_eq!(
        after.creates, before.creates,
        "point errors/ordering cannot consolidate"
    );
    assert_eq!(after.appends, before.appends);
    assert_eq!(after.flushes, before.flushes);
    eprintln!("DIAGNOSTIC primed-alternating-descending rows={ROWS} queries={queries} read_at={reads} requested_bytes={} max_point_request=96 bound_reads={}",
        reads * 96, queries * points_per_query(ROWS));
    store.release().unwrap();
    drop(store);
    assert_eq!(backing.held_bytes(), 0);
}

#[test]
fn backward_and_gap_queries_leave_forward_lookahead_and_buffer_intact() {
    let temp = TempDir::new("run-seek-lookahead");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let observer = backing.observer();
    let mut store = RunStore::new(Some(&mut backing), 2 * ROW_BYTES, CAPACITY);
    let expected = [10, 20, 30, 40, 50]
        .map(|serial| (serial, snapshot(serial, 1)))
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    store.spill(&expected).unwrap();
    assert_eq!(store.find(35).unwrap(), None);
    assert_eq!(store.find(10).unwrap(), expected.get(&10).copied());
    let before = observer.counts();
    assert_eq!(store.find(36).unwrap(), None);
    assert_eq!(store.find(40).unwrap(), expected.get(&40).copied());
    assert_eq!(
        observer.counts().reads,
        before.reads,
        "point seek must preserve inline lookahead"
    );
    assert_eq!(store.find(45).unwrap(), None);
    assert_eq!(store.find(30).unwrap(), expected.get(&30).copied());
    let before = observer.counts();
    assert_eq!(store.find(49).unwrap(), None);
    assert_eq!(store.find(50).unwrap(), expected.get(&50).copied());
    assert_eq!(observer.counts().reads, before.reads);
    store.release().unwrap();
}

#[test]
fn genuinely_overlapping_tiers_use_complete_newest_rows_and_exact_gap_owners() {
    let temp = TempDir::new("run-seek-overlap");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let mut store = RunStore::new(Some(&mut backing), BUFFER, CAPACITY);
    let mut expected = BTreeMap::new();
    for generation in 1..=3 {
        let batch = (1..=129)
            .filter(|serial| generation != 3 || serial % 2 == 1)
            .map(|serial| (serial, snapshot(serial, generation)))
            .collect::<BTreeMap<_, _>>();
        expected.extend(batch.iter().map(|(serial, row)| (*serial, *row)));
        store.spill(&batch).unwrap();
    }
    assert_eq!(store.live_runs(), 2);
    assert_eq!(
        store.find(64).unwrap(),
        Some(snapshot(64, 2)),
        "newest gap falls through only to older owner"
    );
    for serial in (1..=129).rev().chain(1..=129) {
        assert_eq!(store.find(serial).unwrap(), expected.get(&serial).copied());
    }
    let changed = [(64, snapshot(64, 4)), (200, snapshot(200, 4))]
        .into_iter()
        .collect();
    expected.extend([(64, snapshot(64, 4)), (200, snapshot(200, 4))]);
    store.spill(&changed).unwrap();
    assert_eq!(
        store.find(64).unwrap(),
        Some(snapshot(64, 4)),
        "replaced tier cannot reuse its old gap"
    );
    let mut observed = Vec::new();
    store
        .visit_newest_first(|row| {
            observed.push(row);
            Ok(true)
        })
        .unwrap();
    assert_eq!(observed, expected.values().copied().collect::<Vec<_>>());
    store.release().unwrap();
}

#[test]
fn consolidation_admits_both_live_inputs_before_creating_its_output() {
    let temp = TempDir::new("run-seek-consolidation-limit");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let observer = backing.observer();
    const LIMIT: u64 = 5 * 96;
    let mut store = RunStore::new(Some(&mut backing), BUFFER, LIMIT);
    store.check_capacity().unwrap();
    for serial in 1..=3 {
        store
            .spill(&BTreeMap::from([(serial, snapshot(serial, 1))]))
            .unwrap();
    }
    assert_eq!(store.live_runs(), 2);
    assert_eq!(store.owned_bytes(), 3 * 96);
    let before = observer.counts();
    assert_eq!(
        store.consolidate().unwrap_err(),
        ContentError::ObjectLimitExceeded {
            limit: LIMIT as usize,
            actual: 6 * 96,
        }
    );
    let after = observer.counts();
    assert_eq!(
        after.creates, before.creates,
        "adopted newest input remains charged before output creation"
    );
    assert_eq!(after.appends, before.appends);
    assert_eq!(after.reads, before.reads);
    store.release().unwrap();
    assert_eq!(store.owned_bytes(), 0);
    drop(store);
    assert_eq!(backing.held_bytes(), 0);
    assert_eq!(observer.counts().releases, 1);
    eprintln!("DIAGNOSTIC consolidation-admission live_input_bytes=288 proposed_output_bytes=288 limit=480 backing_capacity={CAPACITY} refusal_before_create=true");
}

#[test]
fn spill_carry_admits_the_actual_growing_intermediate_before_the_next_output() {
    let temp = TempDir::new("run-seek-carry-limit");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let observer = backing.observer();
    const LIMIT: u64 = 16 * 96;
    let mut store = RunStore::new(Some(&mut backing), BUFFER, LIMIT);
    store.check_capacity().unwrap();
    for serial in 1..=7 {
        store
            .spill(&BTreeMap::from([(serial, snapshot(serial, 1))]))
            .unwrap();
    }
    assert_eq!(store.live_runs(), 3);
    assert_eq!(store.owned_bytes(), 7 * 96);
    let before = observer.counts();
    assert_eq!(
        store
            .spill(&BTreeMap::from([(8, snapshot(8, 1))]))
            .unwrap_err(),
        ContentError::ObjectLimitExceeded {
            limit: LIMIT as usize,
            actual: 17 * 96
        }
    );
    let after = observer.counts();
    assert_eq!(
        after.creates - before.creates,
        3,
        "incoming1/intermediate2/intermediate4 exist; output8 is refused before creation"
    );
    assert_eq!(after.appends - before.appends, 7);
    assert_eq!(after.appended_bytes - before.appended_bytes, 7 * 96);
    assert_eq!(after.flushes - before.flushes, 3);
    store.release().unwrap();
    assert_eq!(store.owned_bytes(), 0);
    drop(store);
    assert_eq!(backing.held_bytes(), 0);
    assert_eq!(observer.counts().releases, 1);
    eprintln!("DIAGNOSTIC carry-admission current_inputs_bytes=768 pending_bytes=96 proposed_output_bytes=768 limit=1536 backing_capacity={CAPACITY} last_output_creates=0");
}

#[test]
fn spill_refuses_map_key_mismatch_and_zero_before_creating_or_appending() {
    for bad in [
        Row::Effect {
            serial: 8,
            value: None,
            delta: 1,
        },
        Row::Effect {
            serial: 0,
            value: None,
            delta: 1,
        },
    ] {
        let temp = TempDir::new("run-seek-pre-effect");
        let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
        let observer = backing.observer();
        let mut store = RunStore::new(Some(&mut backing), BUFFER, CAPACITY);
        let mut pending = BTreeMap::from([(1, snapshot(1, 1))]);
        pending.insert(if bad.serial() == 0 { 0 } else { 7 }, bad);
        assert_eq!(
            store.spill(&pending).unwrap_err(),
            ContentError::InvalidOrderingRecord("spill row order/key")
        );
        assert_eq!(observer.counts().creates, 0);
        assert_eq!(observer.counts().appends, 0);
        assert_eq!(store.pending_bytes(), 0);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        store.release().unwrap();
    }
}

fn supplied_run(backing: &mut dyn OrderingBacking, serials: &[u64]) -> Run {
    let mut handle = backing.create_run().unwrap();
    for serial in serials {
        handle
            .append(&snapshot(*serial, 1).encode().unwrap())
            .unwrap();
    }
    handle.flush().unwrap();
    Run {
        handle,
        count: serials.len() as u64,
        first: serials[0],
        last: *serials.last().unwrap(),
    }
}

#[test]
fn supplied_run_metadata_and_sequential_order_are_checked_against_real_file_rows() {
    let temp = TempDir::new("run-seek-metadata");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let observer = backing.observer();
    let mut malformed = supplied_run(&mut backing, &[1, 2]);
    let good = supplied_run(&mut backing, &[4, 5]);
    malformed.count = 3;
    malformed.last = 3;
    let before = observer.counts();
    let mut work = MergeWork::default();
    assert!(matches!(
        merge_runs(&mut backing, &malformed, &good, BUFFER, &mut work),
        Err(ContentError::InvalidOrderingRecord("run length"))
    ));
    assert_eq!(
        observer.counts().creates,
        before.creates,
        "bad reported length refuses before output creation"
    );
    assert_eq!(observer.counts().reads, before.reads);
    malformed.count = u64::MAX;
    malformed.last = u64::MAX;
    assert!(matches!(
        merge_runs(&mut backing, &malformed, &good, BUFFER, &mut work),
        Err(ContentError::LengthOverflow)
    ));
    malformed.count = 2;
    malformed.first = 0;
    malformed.last = 2;
    assert!(matches!(
        merge_runs(&mut backing, &malformed, &good, BUFFER, &mut work),
        Err(ContentError::InvalidOrderingRecord("run bounds"))
    ));
    let mut unordered = supplied_run(&mut backing, &[1, 3, 2, 4]);
    unordered.last = 4;
    let mut reader = RunReader::new(&unordered, BUFFER);
    assert_eq!(reader.next().unwrap(), Some(snapshot(1, 1)));
    assert_eq!(reader.next().unwrap(), Some(snapshot(3, 1)));
    assert_eq!(
        reader.next().unwrap_err(),
        ContentError::InvalidOrderingRecord("run row order")
    );
    drop(reader);
    drop(unordered);
    drop(malformed);
    drop(good);
    backing.release().unwrap();
    assert_eq!(backing.held_bytes(), 0);
}

#[test]
fn repeated_largest_serial_is_an_independent_point_without_saturating_resume_loss() {
    let temp = TempDir::new("run-seek-largest-serial");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let mut store = RunStore::new(Some(&mut backing), BUFFER, CAPACITY);
    let expected = [u64::MAX - 1, u64::MAX]
        .map(|serial| {
            (
                serial,
                Row::Effect {
                    serial,
                    value: None,
                    delta: -9,
                },
            )
        })
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    store.spill(&expected).unwrap();
    for serial in [u64::MAX, u64::MAX - 1, u64::MAX, u64::MAX] {
        assert_eq!(store.find(serial).unwrap(), expected.get(&serial).copied());
    }
    store.release().unwrap();
}

#[test]
fn real_short_and_corrupt_newest_point_reads_propagate_without_older_fallback() {
    for truncate in [true, false] {
        let temp = TempDir::new("run-seek-read-error");
        let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
        let observer = backing.observer();
        let mut store = RunStore::new(Some(&mut backing), BUFFER, CAPACITY);
        for generation in 1..=3 {
            let batch = (1..=97)
                .map(|serial| (serial, snapshot(serial, generation)))
                .collect();
            store.spill(&batch).unwrap();
        }
        assert_eq!(store.live_runs(), 2);
        assert_eq!(store.find(97).unwrap(), Some(snapshot(97, 3)));
        let newest = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .max()
            .unwrap();
        let mut file = OpenOptions::new().write(true).open(newest).unwrap();
        if truncate {
            file.set_len(0).unwrap();
        } else {
            file.seek(SeekFrom::Start(48 * 96 + 8)).unwrap();
            file.write_all(&[9]).unwrap();
        }
        drop(file);
        let before = observer.counts();
        let expected_error = if truncate {
            ContentError::Io
        } else {
            ContentError::InvalidOrderingRecord("version")
        };
        assert_eq!(store.find(1).unwrap_err(), expected_error);
        let after = observer.counts();
        assert_eq!(
            after.reads - before.reads,
            1,
            "no older-tier or sequential rescue read"
        );
        assert_eq!(after.requested_bytes - before.requested_bytes, 96);
        assert_eq!(after.creates, before.creates);
        assert_eq!(after.appends, before.appends);
        assert_eq!(after.flushes, before.flushes);
        store.release().unwrap();
        drop(store);
        assert_eq!(observer.counts().releases, 1);
        assert_eq!(backing.held_bytes(), 0);
    }
}

#[derive(Default)]
struct Effects {
    new: bool,
    retained: u64,
    removed: u64,
    value: Option<InodeValue>,
}

impl Effects {
    fn expected(&self) -> PendingState {
        if self.new {
            PendingState::New {
                value: self.value,
                count: self.retained,
            }
        } else {
            PendingState::Existing {
                value: self.value,
                delta: self.retained as i64 - self.removed as i64,
            }
        }
    }
}

#[test]
fn real_reducer_spills_preserve_independent_retained_removed_and_value_events() {
    let temp = TempDir::new("run-seek-reducer");
    let mut backing = ObservedBacking::new(temp.path(), CAPACITY);
    let observer = backing.observer();
    let mut reducer = ReferenceReducer::new(3, Some(&mut backing), BUFFER, CAPACITY);
    reducer.check_backing_capacity().unwrap();
    let mut oracle = BTreeMap::<u64, Effects>::new();
    for serial in 1..=33 {
        reducer.declare_new(serial).unwrap();
        oracle.entry(serial).or_default().new = true;
    }
    for generation in 0..4 {
        for index in 1..=67 {
            let serial = (index * 29 + generation * 17) % 67 + 1;
            reducer.note_retained_binding(serial).unwrap();
            oracle.entry(serial).or_default().retained += 1;
            if serial > 33 && (index + generation) % 2 == 0 {
                reducer.note_removed_binding(serial).unwrap();
                oracle.entry(serial).or_default().removed += 1;
                if index % 4 == 0 {
                    reducer.note_removed_binding(serial).unwrap();
                    oracle.entry(serial).or_default().removed += 1;
                }
            }
            if (index + generation) % 3 == 0 {
                let input_value = typed_value(serial, generation);
                reducer.note_value(serial, input_value).unwrap();
                oracle.entry(serial).or_default().value = Some(input_value);
            }
        }
    }
    let pending_winner = typed_value(34, 100);
    reducer.note_value(34, pending_winner).unwrap();
    oracle.get_mut(&34).unwrap().value = Some(pending_winner);
    assert!(reducer.work().rows_spilled > 0);
    assert!(reducer.work().runs.merges > 0);
    assert!(reducer.pending_rows() <= 3);
    let before = observer.counts();
    for serial in (1..=67).rev().chain(1..=67) {
        assert_eq!(
            reducer.state(serial).unwrap(),
            Some(oracle[&serial].expected())
        );
    }
    let after = observer.counts();
    assert_eq!(after.creates, before.creates);
    assert_eq!(after.appends, before.appends);
    assert_eq!(after.flushes, before.flushes);
    assert_eq!(
        reducer.touched_serials(7).unwrap(),
        oracle
            .iter()
            .map(|(serial, effects)| (*serial, effects.expected()))
            .collect::<Vec<_>>()
    );
    eprintln!("DIAGNOSTIC reducer pending_limit=3 serials=67 spilled_rows={} merges={} read_at={} requested_bytes={}",
        reducer.work().rows_spilled, reducer.work().runs.merges, observer.counts().reads, observer.counts().requested_bytes);
    reducer.release().unwrap();
    drop(reducer);
    assert_eq!(observer.counts().releases, 1);
    assert_eq!(backing.held_bytes(), 0);
}
