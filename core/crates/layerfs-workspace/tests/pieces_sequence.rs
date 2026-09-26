//! Length-indexed extent sequence: page format, path-copied splice and cursor.
//!
//! Every case drives the same traversal the mounted Workspace uses, with the
//! pages written into a bounded in-memory store that speaks the identical page
//! format, so boundaries, holes, append, truncate, overlap, old generations and
//! refusal behaviour are checkable without a mount, a Store or a service. The
//! implicit-base cases pin the mounted shape of a first local edit of an
//! existing file; the shared-subtree cases pin multi-leaf trees, which a
//! single-leaf fixture can never reach.
#![cfg(target_os = "linux")]
use layerfs_bridge::contract::MAX_FILE;
use layerfs_workspace::{
    sequence::{
        metadata_pages::{self, PageRef, MAX_EXTENT},
        metadata_pieces::{self, PieceStore, Replacement, Splice},
        Window,
    },
    Piece, PieceKind, WorkspaceError,
};
use std::{cell::RefCell, collections::HashMap, time::Instant};

/// A generous bound for one in-memory page operation.
fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(30)
}

const INCARNATION: [u8; 32] = [7; 32];
const PAGE: usize = metadata_pages::PAGE;

#[derive(Clone)]
struct Page {
    bytes: [u8; PAGE],
}

/// A bounded in-memory page store with the real page codecs.
#[derive(Default)]
struct Store {
    pages: RefCell<HashMap<u32, Page>>,
    next: RefCell<u32>,
    reads: RefCell<Vec<PageRef>>,
    writes: RefCell<Vec<PageRef>>,
}
impl PieceStore for Store {
    fn read(&self, page: PageRef, _window: &mut Window) -> Result<[u8; PAGE], WorkspaceError> {
        self.reads.borrow_mut().push(page);
        Ok(self
            .pages
            .borrow()
            .get(&page.slot)
            .ok_or(WorkspaceError::Io)?
            .bytes)
    }
    fn write<T>(
        &self,
        _level: u8,
        _window: &mut Window,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let slot = {
            let mut next = self.next.borrow_mut();
            *next += 1;
            *next
        };
        let page = PageRef { slot, epoch: 1 };
        let mut bytes = [0; PAGE];
        let value = encode(page, &mut bytes)?;
        self.pages.borrow_mut().insert(slot, Page { bytes });
        self.writes.borrow_mut().push(page);
        Ok(value)
    }
    fn incarnation(&self) -> [u8; 32] {
        INCARNATION
    }
}
impl Store {
    fn reset(&self) {
        self.reads.borrow_mut().clear();
        self.writes.borrow_mut().clear();
    }
    fn written(&self) -> usize {
        self.writes.borrow().len()
    }
    /// One encoded page, for a reader that must judge its declared format.
    fn page(&self, r: PageRef, window: &mut Window) -> [u8; PAGE] {
        PieceStore::read(self, r, window).unwrap()
    }
    fn read_pages(&self) -> Vec<u32> {
        self.reads.borrow().iter().map(|p| p.slot).collect()
    }
    /// Every page the store holds, so a test can look for shared subtrees.
    fn page_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.pages.borrow().keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

struct Fixture {
    store: Store,
    window: RefCell<Window>,
}
impl Fixture {
    fn new() -> Self {
        Self {
            store: Store::default(),
            window: RefCell::new(Window([0; 131_072])),
        }
    }
    fn with_window<T>(&self, f: impl FnOnce(&mut Window) -> T) -> T {
        let mut window = self.window.borrow_mut();
        f(&mut window)
    }
    /// Materializes one sequence through the bounded cursor.
    fn walk(&self, root: PageRef, length: u64) -> Vec<(u64, Piece)> {
        let mut out = Vec::new();
        self.with_window(|window| {
            let mut cursor =
                metadata_pieces::cursor(&self.store, root, 0, length, window, deadline()).unwrap();
            while let Some(entry) = cursor.next(window).unwrap() {
                out.push(entry);
            }
        });
        out
    }
    fn render(&self, root: PageRef, length: u64) -> String {
        self.walk(root, length)
            .into_iter()
            .map(|(start, piece)| {
                format!(
                    "{start}:{}:{}:{}",
                    match piece.kind {
                        PieceKind::Base => "B",
                        PieceKind::Local => "L",
                        PieceKind::Zero => "Z",
                    },
                    piece.offset,
                    piece.length
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn build(&self, pieces: &[Piece], length: u64) -> Result<PageRef, WorkspaceError> {
        self.with_window(|window| metadata_pieces::build(&self.store, pieces, length, window))
    }
    fn splice(
        &self,
        old: PageRef,
        start: u64,
        end: u64,
        old_base: u64,
        old_replacement: u64,
        length: u64,
        pieces: &[Piece],
    ) -> Result<metadata_pieces::Pieces, WorkspaceError> {
        let mut replacement = Replacement::new();
        for piece in pieces {
            replacement.extend(*piece);
        }
        let mut parts = replacement.into_parts();
        self.with_window(|window| {
            metadata_pieces::replace(
                &self.store,
                old,
                Splice {
                    start,
                    end,
                    old_base,
                    old_replacement,
                    length,
                },
                &mut parts,
                window,
            )
        })
    }
    fn piece_at(
        &self,
        root: PageRef,
        offset: u64,
        length: u64,
    ) -> Result<(u64, Piece), WorkspaceError> {
        self.with_window(|window| {
            metadata_pieces::piece_at(&self.store, root, offset, length, window, deadline())
        })
    }
}

fn base(offset: u64, length: u64) -> Piece {
    Piece {
        kind: PieceKind::Base,
        length,
        offset,
        payload: 0,
        custody: PageRef::NULL,
    }
}
fn local(offset: u64, length: u64, payload: u64, custody: u32) -> Piece {
    Piece {
        kind: PieceKind::Local,
        length,
        offset,
        payload,
        custody: PageRef {
            slot: custody,
            epoch: 1,
        },
    }
}
fn zero(length: u64) -> Piece {
    Piece {
        kind: PieceKind::Zero,
        length,
        offset: 0,
        payload: 0,
        custody: PageRef::NULL,
    }
}

#[test]
fn reads_a_sequence_in_order_with_derived_starts() {
    let f = Fixture::new();
    // A canonical file with two edits: bytes 100..120 replaced, bytes 2000..2050
    // dropped into a hole. Every extents' logical start is derived, not stored.
    let root = f
        .build(
            &[
                base(0, 100),
                local(0, 20, 3, 4),
                base(120, 1880),
                zero(50),
                base(2050, 2046),
            ],
            4096,
        )
        .unwrap();
    assert_eq!(
        f.render(root, 4096),
        "0:B:0:100 100:L:0:20 120:B:120:1880 2000:Z:0:50 2050:B:2050:2046"
    );
}

#[test]
fn overwrites_inside_one_extent_without_touching_its_neighbours() {
    let f = Fixture::new();
    let root = f
        .build(&[base(0, 1024), local(0, 16, 5, 6), base(1040, 1024)], 2064)
        .unwrap();
    // 8 bytes at offset 1020: the tail of the first base, 8 payload bytes, and
    // the untouched local edit plus the rest of the canonical file all survive.
    let second = f
        .splice(root, 1020, 1028, 2064, 16, 2064, &[local(0, 8, 9, 10)])
        .unwrap();
    assert_eq!(second.length, 2064);
    assert_eq!(
        f.render(second.root, 2064),
        "0:B:0:1020 1020:L:0:8 1028:L:4:12 1040:B:1040:1024"
    );
}

#[test]
fn covers_a_hole_and_a_position_past_the_end() {
    let f = Fixture::new();
    // A fresh file whose second half is payload: bytes 0..100 are a hole and
    // bytes 100..200 are private content.
    let grown = f
        .splice(
            PageRef::NULL,
            0,
            0,
            0,
            0,
            200,
            &[zero(100), local(0, 100, 11, 12)],
        )
        .unwrap();
    assert_eq!(grown.length, 200);
    assert_eq!(f.render(grown.root, 200), "0:Z:0:100 100:L:0:100");
    // Replacing the tail of that payload keeps the hole and the retained head.
    let replaced = f
        .splice(
            grown.root,
            150,
            200,
            0,
            grown.replacement,
            200,
            &[local(0, 50, 13, 14)],
        )
        .unwrap();
    assert_eq!(replaced.length, 200);
    assert_eq!(
        f.render(replaced.root, 200),
        "0:Z:0:100 100:L:0:50 150:L:0:50"
    );
    // Truncating to the hole's own end keeps exactly those bytes.
    let truncated = f
        .splice(
            replaced.root,
            100,
            200,
            0,
            replaced.replacement,
            100,
            &[] as &[Piece],
        )
        .unwrap();
    assert_eq!(truncated.length, 100);
    assert_eq!(f.render(truncated.root, 100), "0:Z:0:100");
}

#[test]
fn appends_at_the_end_and_extends_with_zeros() {
    let f = Fixture::new();
    let root = f.build(&[base(0, 512)], 512).unwrap();
    let appended = f
        .splice(root, 512, 512, 512, 0, 768, &[local(0, 256, 13, 14)])
        .unwrap();
    assert_eq!(f.render(appended.root, 768), "0:B:0:512 512:L:0:256");
    let extended = f
        .splice(
            appended.root,
            768,
            768,
            512,
            appended.replacement,
            1024,
            &[zero(256)],
        )
        .unwrap();
    assert_eq!(
        f.render(extended.root, 1024),
        "0:B:0:512 512:L:0:256 768:Z:0:256"
    );
    assert_eq!(extended.edits, 1);
    assert_eq!(extended.replacement, 512);
}

#[test]
fn refuses_a_replacement_beyond_the_base_or_past_max_file() {
    let f = Fixture::new();
    let root = f.build(&[base(0, 4096)], 4096).unwrap();
    // A retained canonical read that reaches past the recorded base is refused.
    assert!(f
        .splice(root, 4096, 4096, 4096, 0, 8192, &[base(0, 4096)])
        .is_err());
    // A length past the logical ceiling is refused before any page is written.
    f.store.reset();
    let writes = f.store.written();
    assert!(f
        .splice(root, 4096, 4096, 4096, 0, MAX_FILE + 1, &[base(4096, 4096)])
        .is_err());
    assert_eq!(f.store.written(), writes);
}

#[test]
fn splits_an_extent_larger_than_a_leaf_into_page_sized_parts() {
    let f = Fixture::new();
    let length = MAX_EXTENT * 2 + 5;
    let root = f.build(&[zero(length)], length).unwrap();
    let pieces = f.walk(root, length);
    assert_eq!(pieces.len(), 3);
    assert_eq!(
        pieces.iter().map(|(_, p)| p.length).collect::<Vec<_>>(),
        vec![MAX_EXTENT, MAX_EXTENT, 5]
    );
}

#[test]
fn an_unchanged_old_generation_stays_readable_after_a_splice() {
    let f = Fixture::new();
    let first = f
        .build(
            &[base(0, 8192), local(0, 64, 21, 22), base(8256, 8192)],
            8192 + 64 + 8192,
        )
        .unwrap();
    let before = f.render(first, 8192 + 64 + 8192);
    let second = f
        .splice(first, 8224, 8224, 16448, 64, 16464, &[local(0, 16, 23, 24)])
        .unwrap();
    assert_ne!(second.root, first);
    // The older root still reads exactly the sequence it published.
    assert_eq!(f.render(first, 8192 + 64 + 8192), before);
    assert_eq!(
        f.render(second.root, 16464),
        "0:B:0:8192 8192:L:0:32 8224:L:0:16 8240:L:32:32 8272:B:8256:8192"
    );
}

#[test]
fn a_shared_subtree_is_not_rewritten() {
    let f = Fixture::new();
    // Enough extents for a rooted tree above a single leaf.
    let pieces: Vec<Piece> = (0..700).map(|index| base(index * 64, 64)).collect();
    let length = 700 * 64;
    let root = f.build(&pieces, length).unwrap();
    let before = f.store.page_ids();
    f.store.reset();
    let changed = f
        .splice(root, 0, 64, length, 0, length, &[local(0, 64, 31, 32)])
        .unwrap();
    assert_eq!(changed.length, length);
    // Only the first leaf and its ancestors are written; every other page of the
    // sequence is still the one the previous root owns.
    let rewritten = f.store.writes.borrow().clone();
    assert!(rewritten.len() <= 3, "rewrote {} pages", rewritten.len());
    let after = f.store.page_ids();
    assert_eq!(before.len(), after.len() - rewritten.len());
    let read = f.walk(changed.root, length);
    let total: u64 = read.iter().map(|(_, piece)| piece.length).sum();
    assert_eq!(total, length, "the whole sequence must still read in order");
    assert_eq!(read[0].0, 0);
}

#[test]
fn a_cursor_seeks_and_reads_only_the_path_it_needs() {
    let f = Fixture::new();
    let pieces: Vec<Piece> = (0..700).map(|index| base(index * 64, 64)).collect();
    let length = 700 * 64;
    let root = f.build(&pieces, length).unwrap();
    f.store.reset();
    // The build merges the contiguous canonical runs into one extent, so the
    // lookup answers with the extent that covers the offset, not with a 64-byte
    // step: the offset is inside it and the derived start is its own start.
    let (start, piece) = f.piece_at(root, 40_000, length).unwrap();
    assert!(start <= 40_000 && 40_000 < start + piece.length);
    assert_eq!(piece.kind, PieceKind::Base);
    // A bounded lookup reads the path plus one leaf, not the whole sequence.
    assert!(
        f.store.read_pages().len() <= 4,
        "read {:?}",
        f.store.read_pages()
    );
}

#[test]
fn a_cursor_seeks_into_the_middle_child_of_a_multi_leaf_branch() {
    let f = Fixture::new();
    let pieces: Vec<Piece> = (0..400u64)
        .map(|index| {
            if index % 2 == 0 {
                base(index / 2, 1)
            } else {
                local(0, 1, index + 1, index as u32 + 1)
            }
        })
        .collect();
    let root = f.build(&pieces, 400).unwrap();
    for offset in [0, 123, 124, 125, 247, 248, 399] {
        let (start, piece) = f.piece_at(root, offset, 400).unwrap();
        assert_eq!(start, offset);
        assert_eq!(piece.kind, pieces[offset as usize].kind);
    }
}

#[test]
fn one_cursor_walk_never_holds_the_whole_sequence() {
    let f = Fixture::new();
    let pieces: Vec<Piece> = (0..3_000).map(|index| base(index * 8, 8)).collect();
    let length = 3_000 * 8;
    let root = f.build(&pieces, length).unwrap();
    // Adjacent contiguous canonical runs merge into one extent, which is the
    // intended local merge; the cursor still walks the same bytes in order.
    let mut next = 0u64;
    let mut extents = 0;
    f.with_window(|window| {
        let mut cursor =
            metadata_pieces::cursor(&f.store, root, 0, length, window, deadline()).unwrap();
        while let Some((start, piece)) = cursor.next(window).unwrap() {
            assert_eq!(start, next);
            assert!(piece.length > 0);
            next += piece.length;
            extents += 1;
        }
    });
    assert_eq!(next, length);
    assert!(
        extents < 3_000,
        "the merge must reduce the stored extent count"
    );
}

#[test]
fn refuses_a_page_that_does_not_declare_the_length_indexed_format() {
    let f = Fixture::new();
    let root = f.build(&[base(0, 512)], 512).unwrap();
    // The same body, relabelled as the pre-length-indexed absolute-offset form:
    // its reader must refuse it rather than reinterpret it.
    f.with_window(|window| {
        let bytes = f.store.page(root, window);
        assert_eq!(
            metadata_pages::PageKind::of(&bytes).unwrap(),
            metadata_pages::PageKind::Pieces
        );
        let mut older = bytes;
        older[49] = 0;
        older[54] = 0;
        assert!(metadata_pages::PageKind::of(&older).is_err());
    });
}

#[test]
fn a_refused_splice_leaves_the_published_sequence_unchanged() {
    let f = Fixture::new();
    let root = f
        .build(&[base(0, 512), local(0, 64, 41, 42), base(576, 512)], 1088)
        .unwrap();
    let before = f.render(root, 1088);
    let pages = f.store.page_ids();
    f.store.reset();
    // A retained canonical read beyond the recorded base is refused.
    let refused = f.splice(root, 1088, 1088, 1088, 0, 1600, &[base(0, 512)]);
    assert!(refused.is_err());
    // The old root still reads its own sequence, and the refusal published no
    // new tree: the store holds exactly the pages the first build wrote.
    assert_eq!(f.render(root, 1088), before);
    assert_eq!(f.store.page_ids(), pages);
}

#[test]
fn a_first_edit_folds_into_one_implicit_base_read() {
    let f = Fixture::new();
    // A base-backed version with no stored sequence is exactly one base read of
    // its selected content: the first local edit folds into that implicit base.
    // This is the mounted shape of a one-byte overwrite of an existing file.
    let first = f
        .splice(PageRef::NULL, 0, 1, 6, 0, 6, &[local(0, 1, 3, 4)])
        .unwrap();
    assert_eq!(first.length, 6);
    assert_eq!(first.edits, 1);
    assert_eq!(first.replacement, 1);
    assert_eq!(f.render(first.root, 6), "0:L:0:1 1:B:1:5");
    // A second edit of the same version replaces bytes; it never deletes them.
    let second = f
        .splice(
            first.root,
            2,
            4,
            6,
            first.replacement,
            6,
            &[local(0, 2, 5, 6)],
        )
        .unwrap();
    assert_eq!(second.length, 6);
    assert_eq!(second.replacement, 3);
    assert_eq!(f.render(second.root, 6), "0:L:0:1 1:B:1:1 2:L:0:2 4:B:4:2");
}

#[test]
fn a_first_edit_inside_an_implicit_base_retains_both_sides() {
    let f = Fixture::new();
    let middle = f
        .splice(PageRef::NULL, 2, 4, 6, 0, 6, &[local(0, 2, 5, 6)])
        .unwrap();
    assert_eq!(middle.edits, 1);
    assert_eq!(f.render(middle.root, 6), "0:B:0:2 2:L:0:2 4:B:4:2");
    // A whole-file replacement of the implicit base is one replacement run.
    let whole = f
        .splice(PageRef::NULL, 0, 6, 6, 0, 6, &[local(0, 6, 7, 8)])
        .unwrap();
    assert_eq!(whole.edits, 1);
    assert_eq!(whole.replacement, 6);
    assert_eq!(f.render(whole.root, 6), "0:L:0:6");
}

#[test]
fn an_implicit_base_longer_than_one_extent_folds_into_split_parts() {
    let f = Fixture::new();
    // A never-edited base-backed file longer than the extent ceiling holds its
    // base read as adjacent parts, exactly as a stored leaf would; the implicit
    // fold must split the retained prefix and tail the same way instead of
    // emitting one over-ceiling extent. The mounted route never edited a file
    // this large, so the shape was unreachable before the route harness.
    let ceiling = u64::from(MAX_EXTENT);
    let base = 3 * ceiling + 10;
    let first = f
        .splice(PageRef::NULL, 10, 14, base, 0, base, &[local(0, 4, 5, 6)])
        .unwrap();
    assert_eq!(first.length, base);
    assert_eq!(first.edits, 1);
    assert_eq!(first.replacement, 4);
    let parts: Vec<(u64, Piece)> = f.walk(first.root, base);
    // The tail is exactly three parts: two at the ceiling and one remainder.
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0].1.kind, PieceKind::Base);
    assert_eq!(parts[0].1.length, 10);
    assert_eq!(parts[1].1.kind, PieceKind::Local);
    assert_eq!(parts[1].1.length, 4);
    assert_eq!(parts[2].1.kind, PieceKind::Base);
    assert_eq!(parts[2].1.offset, 14);
    assert_eq!(parts[2].1.length, ceiling);
    assert_eq!(parts[3].1.offset, 14 + ceiling);
    assert_eq!(parts[3].1.length, ceiling);
    assert_eq!(parts[4].1.offset, 14 + 2 * ceiling);
    assert_eq!(parts[4].1.length, ceiling - 4);
    assert_eq!(
        parts[4].0 + parts[4].1.length,
        base,
        "the split tail must cover the whole retained base"
    );
    // A second edit on that stored sequence splices the multi-part tail
    // without confusing a part boundary with a logical byte boundary.
    let second = f
        .splice(
            first.root,
            14 + ceiling,
            14 + ceiling + 2,
            base,
            4,
            base,
            &[zero(2)],
        )
        .unwrap();
    assert_eq!(second.length, base);
    assert_eq!(second.replacement, 6);
    let spliced = f.piece_at(second.root, 14 + ceiling, base).unwrap();
    assert_eq!(spliced.1.kind, PieceKind::Zero);
    assert_eq!(spliced.1.length, 2);
}

#[test]
fn an_insertion_shifts_the_base_tail_and_the_next_edit_still_splices() {
    let f = Fixture::new();
    // An insertion moves every later byte, so the retained base tail keeps
    // its own origin and a base origin is no longer its logical position.
    // The next splice must still fold that shifted sequence: the walk checks
    // the leaf's base reads never go backwards, not that they stayed aligned.
    let first = f
        .splice(PageRef::NULL, 0, 0, 6, 0, 7, &[(local(0, 1, 5, 6))])
        .unwrap();
    assert_eq!(f.render(first.root, 7), "0:L:0:1 1:B:0:6");
    let second = f.splice(first.root, 3, 5, 6, 1, 7, &[(zero(2))]).unwrap();
    assert_eq!(second.length, 7);
    assert_eq!(second.edits, 2);
    assert_eq!(second.replacement, 3);
    assert_eq!(f.render(second.root, 7), "0:L:0:1 1:B:0:2 3:Z:0:2 5:B:4:2");
}

#[test]
fn final_runs_at_1024_are_admitted() {
    let f = Fixture::new();
    // One splice folds a replacement with base-delimited final runs. Each
    // run is one local byte delimited by one base byte, so nothing merges and
    // the count is exact.
    let budget = 1_024usize;
    let old: Vec<Piece> = (0..budget as u64)
        .flat_map(|at| [base(at, 1), local(0, 1, at + 1, at as u32 + 1)])
        .collect();
    let root = f.build(&old, 2 * budget as u64).unwrap();
    // `count` replacement runs: all but the last are delimited by a following
    // base byte, and the last is the trailing deviation, so the fold counts
    // exactly `count` runs.
    let runs = |count: usize| -> Vec<Piece> {
        (0..count - 1)
            .flat_map(|at| {
                let value = at as u64;
                [
                    local(0, 1, value % 251 + 1, (value as u32) % 250 + 1),
                    base(value, 1),
                ]
            })
            .chain(std::iter::once(local(
                0,
                1,
                (count as u64) % 251 + 1,
                (count as u32) % 250 + 1,
            )))
            .collect()
    };
    let at_budget = budget as u64;
    let full = f
        .splice(
            root,
            0,
            2 * at_budget,
            2 * at_budget,
            at_budget,
            2 * at_budget - 1,
            &runs(budget),
        )
        .unwrap();
    assert_eq!(full.length, 2 * at_budget - 1);
    assert_eq!(full.edits, budget as u16);
    assert_eq!(full.replacement, at_budget);
    let walked = f.walk(full.root, 2 * at_budget - 1);
    assert_eq!(walked.len(), 2 * budget - 1);
    assert_eq!(walked[0].1.kind, PieceKind::Local);
    assert_eq!(walked[1].1.kind, PieceKind::Base);
}

#[test]
fn an_implicit_base_append_past_eof_fills_the_gap() {
    let f = Fixture::new();
    // A write that starts past the end of a never-edited file keeps the whole
    // base, fills the gap with zeros and appends its payload.
    let appended = f
        .splice(
            PageRef::NULL,
            6,
            6,
            6,
            0,
            11,
            &[zero(4), local(0, 1, 9, 10)],
        )
        .unwrap();
    assert_eq!(appended.edits, 1);
    assert_eq!(appended.replacement, 5);
    assert_eq!(f.render(appended.root, 11), "0:B:0:6 6:Z:0:4 10:L:0:1");
}

#[test]
fn an_implicit_base_truncate_and_extension_are_exact() {
    let f = Fixture::new();
    // A truncation is one trailing deletion edit: the retained prefix is one
    // base read and the tail is gone.
    let shrunk = f.splice(PageRef::NULL, 4, 6, 6, 0, 4, &[]).unwrap();
    assert_eq!(shrunk.length, 4);
    assert_eq!(shrunk.edits, 1);
    assert_eq!(shrunk.replacement, 0);
    assert_eq!(f.render(shrunk.root, 4), "0:B:0:4");
    // A logical extension owns zero bytes past the base's end.
    let grown = f.splice(PageRef::NULL, 6, 6, 6, 0, 8, &[zero(2)]).unwrap();
    assert_eq!(grown.edits, 1);
    assert_eq!(grown.replacement, 2);
    assert_eq!(f.render(grown.root, 8), "0:B:0:6 6:Z:0:2");
}

#[test]
fn an_empty_sequence_after_a_full_shrink_takes_a_whole_rewrite() {
    let f = Fixture::new();
    // Truncating an implicit base to nothing publishes an empty sequence: a
    // NULL root, exactly as a fresh file with no content.
    let empty = f.splice(PageRef::NULL, 0, 6, 6, 0, 0, &[]).unwrap();
    assert_eq!(empty.length, 0);
    assert_eq!(empty.root, PageRef::NULL);
    assert_eq!(empty.edits, 1);
    // Rewriting that empty version replaces no base bytes even though the
    // version still records one: the result is exactly the replacement.
    let rewritten = f
        .splice(PageRef::NULL, 0, 0, 6, 0, 30, &[local(0, 30, 15, 16)])
        .unwrap();
    assert_eq!(rewritten.length, 30);
    assert_eq!(rewritten.edits, 1);
    assert_eq!(rewritten.replacement, 30);
    assert_eq!(f.render(rewritten.root, 30), "0:L:0:30");
}

#[test]
fn an_implicit_base_replacement_beyond_the_base_is_refused() {
    let f = Fixture::new();
    let pages = f.store.page_ids();
    f.store.reset();
    // The replaced interval must stay inside the implicit base read.
    let refused = f.splice(PageRef::NULL, 0, 7, 6, 0, 6, &[local(0, 7, 11, 12)]);
    assert!(matches!(refused, Err(WorkspaceError::Io)));
    // A refusal publishes no page.
    assert_eq!(f.store.written(), 0);
    assert_eq!(f.store.page_ids(), pages);
}

#[test]
fn a_shared_subtree_records_the_unknown_count_and_the_exact_total() {
    let f = Fixture::new();
    // 300 extents with distinct payloads never merge, so the built tree holds
    // three leaves under one branch: a splice in the middle shares the two
    // leaves it does not touch.
    let pieces: Vec<Piece> = (0..300)
        .map(|index| local(index * 64, 64, index as u64 + 1, (index + 1) as u32))
        .collect();
    let length = 300 * 64;
    let root = f.build(&pieces, length).unwrap();
    let before = f.render(root, length);
    let pages = f.store.page_ids();
    f.store.reset();
    // Piece 128 covers [8192, 8256); replacing it shares the first and last
    // leaves and folds only the middle one.
    let changed = f
        .splice(
            root,
            8192,
            8256,
            0,
            19_200,
            length,
            &[local(0, 64, 999, 888)],
        )
        .unwrap();
    assert_eq!(changed.length, length);
    // The exact edit count is unavailable to a splice that shared a subtree.
    assert_eq!(changed.edits, u16::MAX);
    // The replacement total still carries exactly: 19_200 dropped 64, added 64.
    assert_eq!(changed.replacement, 19_200);
    // Only the folded leaf and its branch were written; the shared leaves are
    // still the pages the previous root owns.
    assert!(
        f.store.written() <= 2,
        "rewrote {} pages",
        f.store.written()
    );
    assert_eq!(f.store.page_ids().len(), pages.len() + f.store.written());
    // The older root still reads exactly the sequence it published, and the
    // new one replaced exactly piece 128.
    assert_eq!(f.render(root, length), before);
    let read = f.walk(changed.root, length);
    assert_eq!(read.len(), 300);
    assert_eq!(read[128].0, 8192);
    assert_eq!(read[128].1.payload, 999);
    assert_eq!(read[127].1.payload, 128);
    assert_eq!(read[129].1.payload, 130);
    // A second shared splice keeps the totals exact across generations.
    let again = f
        .splice(
            changed.root,
            0,
            64,
            0,
            changed.replacement,
            length,
            &[local(0, 64, 777, 887)],
        )
        .unwrap();
    assert_eq!(again.edits, u16::MAX);
    assert_eq!(again.replacement, 19_200);
}

/// Every page of one tree, with its declared level and its entry count: extent
/// records for a leaf, child references for a branch.
fn tree(f: &Fixture, root: PageRef) -> Vec<(PageRef, u8, usize)> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(page) = pending.pop() {
        let bytes = f.with_window(|window| f.store.page(page, window));
        let level = bytes[48];
        if level == 0 {
            let used = metadata_pages::body_used(&bytes).unwrap();
            out.push((page, level, used / metadata_pages::RECORD));
            continue;
        }
        let (_, children) =
            metadata_pages::decode_pieces_branch(INCARNATION, page, &bytes, MAX_FILE).unwrap();
        for child in &children {
            pending.push(child.page);
        }
        out.push((page, level, children.len()));
    }
    out
}

/// Every structural rule the readers rely on, checked over a whole tree: a
/// branch holds at least one child, every child of a branch declares exactly one
/// level below its parent, and a leaf holds whole extent records. Answers the
/// leaf count, the root's declared level and how many branches hold one child.
fn structure(f: &Fixture, root: PageRef) -> (usize, u8, usize) {
    let root_level = f.with_window(|window| f.store.page(root, window)[48]);
    let mut leaves = 0;
    let mut spines = 0;
    let mut pending = vec![(root, None)];
    while let Some((page, parent)) = pending.pop() {
        let bytes = f.with_window(|window| f.store.page(page, window));
        let level = bytes[48];
        if let Some(parent) = parent {
            assert_eq!(
                level + 1,
                parent,
                "page {page:?} declares level {level} below its parent's {parent}"
            );
        }
        if level == 0 {
            let used = metadata_pages::body_used(&bytes).unwrap();
            assert!(used > 0 && used % metadata_pages::RECORD == 0);
            leaves += 1;
            continue;
        }
        let (declared, children) =
            metadata_pages::decode_pieces_branch(INCARNATION, page, &bytes, MAX_FILE).unwrap();
        assert_eq!(declared, level);
        assert!(!children.is_empty(), "branch {page:?} holds no child");
        if children.len() == 1 {
            spines += 1;
        }
        for child in &children {
            pending.push((child.page, Some(level)));
        }
    }
    assert!(leaves > 0);
    (leaves, root_level, spines)
}

/// One level-indexed sequence whose root is two levels above its leaves: 249
/// leaf pages of 124 one-byte extents, packed into two level-1 branches.
fn two_level_fixture() -> (Fixture, PageRef, u64) {
    let f = Fixture::new();
    let records = 124 * 249;
    let length = records as u64;
    let root = f.build(&separated(length), length).unwrap();
    (f, root, length)
}

/// `count` one-byte private extents that never merge: every acquisition has its
/// own identity, exactly as a sequence of separated writes does.
fn separated(count: u64) -> Vec<Piece> {
    (0..count)
        .map(|at| local(0, 1, at + 1, (at % 4096 + 1) as u32))
        .collect()
}

#[test]
fn a_fold_that_collapses_a_whole_branch_keeps_every_path_one_depth() {
    let (f, root, length) = two_level_fixture();
    let (leaves, level, spines) = structure(&f, root);
    assert_eq!((leaves, level, spines), (249, 2, 0));
    // The root's first child is a level-1 branch over 125 leaf pages, so it
    // covers exactly 125 * 124 one-byte extents.
    let child = 125 * 124;
    let spliced = f
        .splice(
            root,
            0,
            child,
            0,
            length,
            length,
            &[local(0, child, 900_001, 4_001)],
        )
        .unwrap();
    assert_eq!(spliced.length, length);
    // The whole child collapsed to one leaf page. It keeps its own level: the
    // sibling branch beside it is untouched, so a shallower page there would
    // leave the root's children at two depths and the cursor would refuse the
    // tree.
    let (leaves, level, spines) = structure(&f, spliced.root);
    assert_eq!(spines, 1, "the collapsed child must be lifted, not dropped");
    assert_eq!(level, 2);
    // 124 untouched leaf pages plus the collapsed child's one.
    assert_eq!(leaves, 125);
    let mut expected = vec![format!("0:L:0:{child}")];
    for at in child..length {
        expected.push(format!("{at}:L:0:1"));
    }
    assert_eq!(f.render(spliced.root, length), expected.join(" "));
    // A second fold over the same collapsed child replaces its lifted page
    // instead of stacking another level above it.
    let again = f
        .splice(
            spliced.root,
            0,
            child,
            0,
            length,
            length,
            &[local(0, child, 900_002, 4_002)],
        )
        .unwrap();
    let (leaves, level, spines) = structure(&f, again.root);
    assert_eq!(spines, 1);
    assert_eq!(leaves, 125, "a repeated collapse must not stack a level");
    assert_eq!(level, 2);
    assert_eq!(
        f.walk(again.root, length).len(),
        1 + (length - child) as usize
    );
    // The earlier generation still reads exactly what it published.
    assert_eq!(f.render(spliced.root, length), expected.join(" "));
}

#[test]
fn a_fold_that_removes_a_whole_branch_drops_its_level() {
    let (f, root, length) = two_level_fixture();
    let child = 125 * 124;
    // Delete the whole first child: the folded branch answers with no page at
    // all, so the root keeps its other child and nothing is lifted for it.
    let spliced = f
        .splice(root, 0, child, 0, length, length - child, &[] as &[Piece])
        .unwrap();
    assert_eq!(spliced.length, length - child);
    let (leaves, level, spines) = structure(&f, spliced.root);
    assert_eq!(spines, 0);
    assert_eq!(level, 1, "the root loses the level it no longer needs");
    assert_eq!(leaves as u64, (length - child) / 124);
    let walked = f.walk(spliced.root, length - child);
    assert_eq!(walked.len(), (length - child) as usize);
    assert_eq!(walked[0].0, 0);
    assert_eq!(
        walked.iter().map(|(_, piece)| piece.length).sum::<u64>(),
        length - child
    );
}

#[test]
fn a_full_4_gib_sequence_round_trips_at_the_exact_file_boundary() {
    let f = Fixture::new();
    // The declared maximum file: one base read of exactly 4 GiB, which the
    // extent ceiling splits into 257 stored records. The last record ends
    // exactly at the file bound, and the whole sequence must stay readable.
    let root = f.build(&[base(0, MAX_FILE)], MAX_FILE).unwrap();
    let walked = f.walk(root, MAX_FILE);
    assert_eq!(walked.len(), 257);
    let mut expected = 0u64;
    for (start, piece) in &walked {
        assert_eq!(*start, expected);
        assert_eq!(piece.kind, PieceKind::Base);
        expected += piece.length;
    }
    assert_eq!(expected, MAX_FILE);
    let last = walked.last().unwrap().1;
    assert_eq!(last.offset + last.length, MAX_FILE);
    // The same bound in the other direction: one byte past it is refused.
    assert!(f.build(&[base(0, MAX_FILE + 1)], MAX_FILE + 1).is_err());
}

#[test]
fn the_249_leaf_transition_keeps_every_branch_non_singleton() {
    let f = Fixture::new();
    // 249 leaves is the first count whose last branch chunk would hold a
    // single child; every non-root branch must keep at least two.
    let records = 124 * 249;
    let root = f.build(&separated(records), records).unwrap();
    let pages = tree(&f, root);
    let leaves = pages.iter().filter(|(_, level, _)| *level == 0).count();
    assert_eq!(leaves, 249);
    for (page, level, entries) in &pages {
        if *level == 0 {
            assert!(*entries > 0 && *entries <= 124);
        } else {
            assert!(
                *entries >= 2,
                "branch {page:?} at level {level} holds {entries} children"
            );
            assert!(*entries <= 248);
        }
    }
    let walked = f.walk(root, records);
    assert_eq!(walked.len(), records as usize);
}

#[test]
fn four_thousand_and_ninety_seven_separated_runs_read_and_splice_by_path() {
    let f = Fixture::new();
    let runs = 4_097u64;
    let length = 2 * runs;
    // One local byte between two base bytes: 4,097 maximal replacement runs.
    let pieces: Vec<Piece> = (0..runs)
        .flat_map(|at| [local(0, 1, at + 1, (at % 4096 + 1) as u32), base(at + 1, 1)])
        .collect();
    let root = f.build(&pieces, length).unwrap();
    assert_eq!(f.walk(root, length).len(), 2 * runs as usize);
    // A narrow splice in the middle reads one path, not the whole sequence.
    let at = 2 * 2_048;
    f.store.reset();
    let spliced = f
        .splice(root, at, at + 1, length, runs, length, &[local(0, 1, 1, 1)])
        .unwrap();
    let reads = f.store.read_pages().len();
    let writes = f.store.written();
    println!("PIECES_SCALE runs={runs} splice_reads={reads} splice_writes={writes}");
    assert!(reads <= 8, "one narrow splice read {reads} pages");
    assert!(writes <= 8, "one narrow splice wrote {writes} pages");
    assert_eq!(spliced.length, length);
    assert_eq!(f.walk(spliced.root, length).len(), 2 * runs as usize);
}

#[test]
fn sixty_five_thousand_separated_runs_stay_bounded_and_path_local() {
    let f = Fixture::new();
    let runs = 65_536u64;
    let root = f.build(&separated(runs), runs).unwrap();
    let pages = tree(&f, root);
    let leaves = pages.iter().filter(|(_, level, _)| *level == 0).count();
    assert_eq!(leaves, (runs as usize).div_ceil(124));
    assert_eq!(f.walk(root, runs).len(), runs as usize);
    f.store.reset();
    let spliced = f
        .splice(
            root,
            runs / 2,
            runs / 2 + 1,
            runs,
            runs,
            runs,
            &[local(0, 1, 1, 1)],
        )
        .unwrap();
    let reads = f.store.read_pages().len();
    let writes = f.store.written();
    println!(
        "PIECES_SCALE runs={runs} leaves={leaves} splice_reads={reads} splice_writes={writes}"
    );
    assert!(reads <= 12, "one narrow splice read {reads} pages");
    assert!(writes <= 12, "one narrow splice wrote {writes} pages");
    let walked = f.walk(spliced.root, runs);
    assert_eq!(walked.len(), runs as usize);
    assert_eq!(walked[0].1.length, 1);
    assert_eq!(
        walked.iter().map(|(_, piece)| piece.length).sum::<u64>(),
        runs
    );
}
