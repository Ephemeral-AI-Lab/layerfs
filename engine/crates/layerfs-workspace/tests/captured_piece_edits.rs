//! Reference pieces written through the ordinary Workspace write, read back
//! through the live and the captured view, and committed by the edit script.
//! Each case names the exact edits the script must seal; the committed file
//! is compared with the final bytes and with the byte-source canonical route.
mod captured_support;
mod common;
use captured_support::*;
use layerfs_content::{Edit, ObjectId};

use layerfs_overlay::{CapturedReader, ClaimedRange, IndexedOperationRecordScope, WriteClaims};
use layerfs_workspace::{
    CapturedFileWork, Operation, OverlayOperationRecords, Position, WriteData,
};

const C: u64 = 4096;
const N: u64 = 256 * C;
const WINDOW: u64 = 64 * 1024;
const FILE: u64 = 8;

fn noise(len: u64, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}
/// One file over a committed base, with the bytes it must hold.
struct Run {
    case: Case,
    root: [u8; 32],
    expected: Vec<u8>,
}
impl Run {
    fn new(size: u64) -> Self {
        let case = Case::over(common::fixture_with(noise(size, 7)));
        let base = case.workspace.base().unwrap();
        let root = base.inode(FILE).unwrap().value.content_root.to_bytes();
        let expected = case.bytes.clone();
        Self {
            case,
            root,
            expected,
        }
    }
    fn put(&mut self, at: u64, bytes: &[u8]) {
        let end = at as usize + bytes.len();
        if self.expected.len() < end {
            self.expected.resize(end, 0);
        }
        self.expected[at as usize..end].copy_from_slice(bytes);
    }
    fn write(&mut self, at: u64, bytes: &[u8]) {
        self.case.write(FILE, at, bytes);
        self.put(at, bytes);
    }
    fn size(&mut self, size: u64) {
        self.case.size(FILE, size);
        self.expected.resize(size as usize, 0);
    }
    /// Writes `content[source..source + length]` at `at`, each window with
    /// the claim that it is committed content of `root` from there.
    fn claimed(&mut self, at: u64, root: [u8; 32], content: &[u8], source: u64, length: u64) {
        let mut done = 0;
        while done < length {
            let count = WINDOW.min(length - done);
            let from = (source + done) as usize;
            let bytes = &content[from..from + count as usize];
            self.case.mutate(Operation::Write {
                serial: FILE,
                position: Position::At(at + done),
                data: WriteData::from(bytes).claimed(WriteClaims {
                    source: root,
                    after: None,
                    ranges: vec![ClaimedRange {
                        offset: at + done,
                        length: count,
                        source_offset: source + done,
                    }],
                }),
            });
            self.put(at + done, bytes);
            done += count;
        }
    }
    /// A piece of the file's own committed base.
    fn piece(&mut self, at: u64, source: u64, length: u64) {
        let base = self.case.bytes.clone();
        self.claimed(at, self.root, &base, source, length);
    }
    fn view<T>(&self, read: impl FnOnce(&layerfs_workspace::SourceView) -> T) -> T {
        let acquired = self
            .case
            .job(|job_client: &layerfs_daemon::OwnerClient, job_route| {
                job_client.acquire_base_source(job_route, self.case.request())
            });
        let source = acquired;
        let view = self.case.workspace.view_for_source(source).unwrap();
        let value = read(&view);
        self.case
            .job(|job_client: &layerfs_daemon::OwnerClient, job_route| {
                job_client.release_base_source(job_route, source)
            });
        value
    }
    /// The whole file through the live view, in read windows.
    fn live(&self) -> Vec<u8> {
        self.view(|view| {
            let mut bytes = Vec::new();
            loop {
                let at = bytes.len() as u64;
                let got = view
                    .read(&self.case.client, FILE, at, 128 * 1024, &mut bytes)
                    .unwrap();
                if got == 0 {
                    return bytes;
                }
            }
        })
    }
    /// The whole file through the captured reader, at odd window bounds.
    fn sealed(&self, reader: CapturedReader) -> Vec<u8> {
        self.view(|view| {
            let mut bytes = Vec::new();
            loop {
                let at = bytes.len() as u64;
                let got = view
                    .read_captured(&self.case.client, reader, FILE, at, 100_001, &mut bytes)
                    .unwrap();
                if got == 0 {
                    return bytes;
                }
            }
        })
    }
    fn edits(&self, scope: IndexedOperationRecordScope) -> Vec<Edit> {
        let (mut edits, mut after) = (Vec::new(), None);
        loop {
            let keys = self
                .case
                .client
                .operation_record_keys_after(scope, EDITS, after)
                .unwrap()
                .value;
            if keys.is_empty() {
                return edits;
            }
            for key in &keys {
                let row = self
                    .case
                    .client
                    .operation_record_get(
                        scope,
                        layerfs_overlay::IndexedOperationRecordKey {
                            kind: EDITS,
                            key: *key,
                        },
                    )
                    .unwrap()
                    .value
                    .unwrap();
                assert_eq!((row.len(), row[0]), (25, 1));
                let number = |at: usize| u64::from_be_bytes(row[at..at + 8].try_into().unwrap());
                edits.push(Edit::new(number(1), number(9), number(17)));
            }
            after = keys.last().copied();
        }
    }
    /// Reads the live and the sealed view, commits the file, and checks the
    /// sealed edits, the final bytes and the canonical root.
    fn commit(self, label: &str, edits: &[Edit]) -> (CapturedFileWork, u64) {
        let (work, reads, _) = self.commit_counted(label, edits);
        (work, reads)
    }
    /// The same, with the record calls of the attempt.
    fn commit_counted(self, label: &str, edits: &[Edit]) -> (CapturedFileWork, u64, u64) {
        assert!(self.live() == self.expected, "{label}: live view");
        let (reader, scope) = self.case.captured(70);
        assert!(self.sealed(reader) == self.expected, "{label}: sealed view");
        self.case.objects.calls.lock().unwrap().clear();
        let attempt = self.case.construct(reader, scope, FILE);
        let result = attempt.result.as_ref().unwrap();
        let reads: u64 = self.case.objects.calls.lock().unwrap().values().sum();
        assert_eq!(self.edits(scope), edits, "{label}: sealed edits");
        // Wall times are observations, not counted work.
        let work = CapturedFileWork {
            bind_ns: 0,
            source_ns: 0,
            ..attempt.custody.work
        };
        assert_eq!(work.normalized_edits, edits.len() as u64, "{label}");
        let original = attempt.custody.file.as_ref().unwrap().root();
        assert_eq!(result.logical_len, self.expected.len() as u64, "{label}");
        if edits.is_empty() {
            assert_eq!(result.root, original, "{label}: unchanged root");
        }
        let model = Model {
            root: original,
            base: self.case.bytes.len() as u64,
            bytes: self.expected.clone(),
            edits: edits.to_vec(),
        };
        self.case.oracle(result.root, &self.expected, Some(&model));
        let records = attempt.custody.records.work.calls;
        println!(
            "PIECE_COMMIT {label} edits={} reads={reads} records={records} {work:?}",
            edits.len()
        );
        self.case.release(attempt.custody, false);
        (work, reads, records)
    }
}

#[test]
fn a_piece_equal_to_the_base_is_no_edit() {
    let mut run = Run::new(N);
    run.piece(8 * C, 8 * C, 40 * C);
    let (work, _) = run.commit("unchanged", &[]);
    assert_eq!(work.committed_bytes, 0);
}

#[test]
fn insert_delete_and_replace_are_one_edit_each() {
    let p = 96 * C;
    // Insert one cell at `p`: the tail is the base, one cell later.
    let mut run = Run::new(N);
    run.piece(p + C, p, N - p);
    run.write(p, &noise(C, 21));
    let (work, _) = run.commit("insert", &[Edit::insert(p, C)]);
    assert_eq!(work.committed_bytes, 0);

    // Delete one cell at `p`: the tail is the base, one cell earlier.
    let mut run = Run::new(N);
    run.piece(p, p + C, N - p - C);
    run.size(N - C);
    let (work, _) = run.commit("delete", &[Edit::delete(p, p + C)]);
    assert_eq!(work.committed_bytes, 0);

    // Replace one cell at `p` by three.
    let mut run = Run::new(N);
    run.piece(p + 3 * C, p + C, N - p - C);
    run.write(p, &noise(3 * C, 22));
    let (work, _) = run.commit("replace", &[Edit::new(p, p + C, 3 * C)]);
    assert_eq!(work.committed_bytes, 0);

    // Insert at the head: the whole base, one cell later.
    let mut run = Run::new(N);
    run.piece(C, 0, N);
    run.write(0, &noise(C, 23));
    let (work, _) = run.commit("prepend", &[Edit::insert(0, C)]);
    assert_eq!(work.committed_bytes, 0);
}

#[test]
fn several_edits_keep_their_own_final_offsets() {
    let (p, q) = (32 * C, 160 * C);
    // Insert a cell at `p` and delete one at `q`: between them the base is a
    // cell later, and after `q` it is inherited at its own offset again.
    let mut run = Run::new(N);
    run.piece(p + C, p, q - p);
    run.write(p, &noise(C, 31));
    let (work, _) = run.commit(
        "insert-delete",
        &[Edit::insert(p, C), Edit::delete(q + C, q + 2 * C)],
    );
    assert_eq!(work.committed_bytes, 0);

    // The same with a few bytes written inside the shifted range: a cell
    // over the piece, and a window whose other bytes the piece supplies.
    let mut run = Run::new(N);
    run.piece(p + C, p, q - p);
    run.write(p, &noise(C, 32));
    let at = p + 11 * C + 5;
    run.write(at, b"xyz");
    let (work, _) = run.commit(
        "masked",
        &[
            Edit::insert(p, C),
            Edit::overwrite(at, at + 3),
            Edit::delete(q + C, q + 2 * C),
        ],
    );
    assert_eq!(work.committed_bytes, 0);
}

#[test]
fn two_shrinking_edges_translate_the_second_from_its_negative_delta() {
    let (p, q) = (32 * C, 160 * C);
    let mut run = Run::new(N);
    let base = run.case.bytes.clone();
    run.write(p, &base[(p + C) as usize..(p + 2 * C) as usize]);
    run.piece(p + C, p + 2 * C, q - p - 2 * C);
    run.write(q - C, &base[(q + C) as usize..(q + 2 * C) as usize]);
    run.piece(q, q + 2 * C, N - q - 2 * C);
    run.size(N - 2 * C);
    let (work, _) = run.commit(
        "two-shrinking-edges",
        &[Edit::delete(p, p + C), Edit::delete(q - C, q)],
    );
    assert_eq!(work.edge_windows, 4);
    assert_eq!(work.edge_read_bytes, 8 * C);
    assert_eq!(work.edge_retained_bytes, 2 * C);
    assert_eq!(work.committed_bytes, 0);
}

#[test]
fn shrinking_edge_mismatches_at_both_ends_remain_replacement_bytes() {
    let p = 96 * C;
    let mut run = Run::new(N);
    let mut copied = run.case.bytes[(p + C) as usize..(p + 2 * C) as usize].to_vec();
    copied[0] ^= 1;
    copied[C as usize - 1] ^= 1;
    run.write(p, &copied);
    run.piece(p + C, p + 2 * C, N - p - 2 * C);
    run.size(N - C);
    let (work, _) = run.commit("shrinking-mismatches", &[Edit::new(p, p + 2 * C, C)]);
    assert_eq!(work.edge_windows, 2);
    assert_eq!(work.edge_retained_bytes, 0);
}

#[test]
fn an_insert_longer_than_the_tail_is_found_beyond_the_surviving_base() {
    let p = N - 8 * C;
    let mut run = Run::new(N);
    run.piece(p + 16 * C, p, 8 * C);
    run.write(p, &noise(16 * C, 41));
    let (work, _) = run.commit("long-insert", &[Edit::insert(p, 16 * C)]);
    assert_eq!(work.committed_bytes, 0);
}

#[test]
fn a_cut_piece_and_a_regrown_tail_are_one_trailing_edit() {
    let p = 64 * C;
    let mut run = Run::new(N);
    run.piece(p + C, p, N - p);
    run.write(p, &noise(C, 51));
    run.size(p + 5 * C);
    run.size(p + 9 * C);
    let (work, _) = run.commit(
        "cut-regrow",
        &[
            Edit::insert(p, C),
            Edit::new(p + 5 * C, p + 5 * C + (N - p - 4 * C), 4 * C),
        ],
    );
    assert_eq!(work.committed_bytes, 0);
}

#[test]
fn a_reference_that_cannot_be_retained_is_read_as_new_bytes() {
    // Backward: bytes of the base from before the base cursor.
    let mut run = Run::new(N);
    run.piece(16 * C, 0, 8 * C);
    let (work, _) = run.commit("backward", &[Edit::overwrite(16 * C, 24 * C)]);
    assert!(work.committed_bytes >= 8 * C && work.committed_bytes % (8 * C) == 0);

    // Partly behind: a forward piece, then inherited bytes it already
    // consumed. Only those four cells are read.
    let mut run = Run::new(N);
    run.piece(0, 4 * C, 8 * C);
    let (work, _) = run.commit(
        "behind",
        &[Edit::delete(0, 4 * C), Edit::insert(8 * C, 4 * C)],
    );
    assert!(work.committed_bytes >= 4 * C && work.committed_bytes % (4 * C) == 0);

    // Another committed root.
    let mut run = Run::new(N);
    let other = noise(64 * C, 61);
    let root: ObjectId = common::file(&run.case.store, &other);
    run.claimed(40 * C, root.to_bytes(), &other, 3 * C + 17, 8 * C);
    let (work, _) = run.commit("other-root", &[Edit::overwrite(40 * C, 48 * C)]);
    assert!(work.committed_bytes >= 8 * C && work.committed_bytes % (8 * C) == 0);
}

#[test]
fn commit_work_of_an_insert_does_not_grow_with_the_file() {
    let insert = |size: u64| {
        let p = size / 2;
        let mut run = Run::new(size);
        run.piece(p + C, p, size - p);
        run.write(p, &noise(C, 71));
        run.commit("sized", &[Edit::insert(p, C)])
    };
    let (small, small_reads) = insert(1 << 20);
    let (large, large_reads) = insert(6 << 20);
    println!("PIECE_COMMIT_SIZES small={small:?} reads={small_reads} large={large:?} reads={large_reads}");
    // Inherited, the new cell, one reference: the same steps and rows for a
    // tail of 512 KiB and of 3 MiB, and no committed byte read back.
    assert_eq!(small, large);
    assert_eq!(small.committed_bytes, 0);
    assert!(
        small.normalization_steps <= 6 && small.source_steps <= 4,
        "{small:?}"
    );
    // Object reads follow the height of the tree, not the length of the file.
    assert!(
        large_reads <= small_reads + 8,
        "{small_reads} {large_reads}"
    );
}

#[test]
fn commit_work_of_a_delete_does_not_grow_with_the_file() {
    let delete = |size: u64| {
        let p = size / 2;
        let mut run = Run::new(size);
        run.piece(p, p + C, size - p - C);
        run.size(size - C);
        run.commit_counted("sized delete", &[Edit::delete(p, p + C)])
    };
    let (small, small_reads, small_records) = delete(1 << 20);
    let (large, large_reads, large_records) = delete(6 << 20);
    println!(
        "PIECE_COMMIT_DELETE small={small:?} reads={small_reads} records={small_records} \
         large={large:?} reads={large_reads} records={large_records}"
    );
    // The same steps and rows for a tail of 512 KiB and of 3 MiB. The record
    // calls of the edit follow the height of the mapping tree, as they do
    // for an insert, not the length of the tail.
    assert_eq!(small, large);
    assert_eq!(small.committed_bytes, 0);
    assert!(
        large_records <= small_records * 5,
        "{small_records} {large_records}"
    );
}

#[test]
fn commit_record_calls_grow_with_the_edits_and_no_faster() {
    // Pieces of 32 KiB in place, each followed by 16 KiB of stored bytes
    // that equal the base: one equal-length edit for each stretch.
    let mut calls = Vec::new();
    for count in [8_u64, 32, 128] {
        let mut run = Run::new(2048 * C);
        let base = run.case.bytes.clone();
        let mut edits = Vec::new();
        for index in 0..count {
            let at = 64 * C + index * 12 * C;
            run.piece(at, at, 8 * C);
            let from = (at + 8 * C) as usize;
            run.write(at + 8 * C, &base[from..from + 4 * C as usize]);
            edits.push(Edit::new(at + 8 * C, at + 12 * C, 4 * C));
        }
        let (work, reads, records) = run.commit_counted("edits", &edits);
        assert_eq!(work.committed_bytes, 0);
        calls.push((count, records, reads));
    }
    println!("PIECE_COMMIT_SCALING {calls:?}");
    // Four times the edits cost at most five times the record calls.
    for pair in calls.windows(2) {
        assert!(pair[1].1 <= pair[0].1 * 5, "{calls:?}");
    }
}
