//! Length-indexed extent sequence: page format, path-copied splice and cursor.
//!
//! Every case drives the same traversal the mounted Workspace uses, with the
//! pages written into a bounded in-memory store that speaks the identical page
//! format, so boundaries, holes, append, truncate, overlap, old generations and
//! refusal behaviour are checkable without a mount, a Store or a service.
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
        .splice(root, 1020, 1028, 2064, 2064, &[local(0, 8, 9, 10)])
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
            200,
            &[zero(100), local(0, 100, 11, 12)],
        )
        .unwrap();
    assert_eq!(grown.length, 200);
    assert_eq!(f.render(grown.root, 200), "0:Z:0:100 100:L:0:100");
    // Replacing the tail of that payload keeps the hole and the retained head.
    let replaced = f
        .splice(grown.root, 150, 200, 0, 200, &[local(0, 50, 13, 14)])
        .unwrap();
    assert_eq!(replaced.length, 200);
    assert_eq!(
        f.render(replaced.root, 200),
        "0:Z:0:100 100:L:0:50 150:L:0:50"
    );
    // Truncating to the hole's own end keeps exactly those bytes.
    let truncated = f
        .splice(replaced.root, 100, 200, 0, 100, &[] as &[Piece])
        .unwrap();
    assert_eq!(truncated.length, 100);
    assert_eq!(f.render(truncated.root, 100), "0:Z:0:100");
}

#[test]
fn appends_at_the_end_and_extends_with_zeros() {
    let f = Fixture::new();
    let root = f.build(&[base(0, 512)], 512).unwrap();
    let appended = f
        .splice(root, 512, 512, 512, 768, &[local(0, 256, 13, 14)])
        .unwrap();
    assert_eq!(f.render(appended.root, 768), "0:B:0:512 512:L:0:256");
    let extended = f
        .splice(appended.root, 768, 768, 512, 1024, &[zero(256)])
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
        .splice(root, 4096, 4096, 4096, 8192, &[base(0, 4096)])
        .is_err());
    // A length past the logical ceiling is refused before any page is written.
    f.store.reset();
    let writes = f.store.written();
    assert!(f
        .splice(root, 4096, 4096, 4096, MAX_FILE + 1, &[base(4096, 4096)])
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
        .splice(first, 8224, 8224, 16448, 16464, &[local(0, 16, 23, 24)])
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
        .splice(root, 0, 64, length, length, &[local(0, 64, 31, 32)])
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
    let refused = f.splice(root, 1088, 1088, 1088, 1600, &[base(0, 512)]);
    assert!(refused.is_err());
    // The old root still reads its own sequence, and the refusal published no
    // new tree: the store holds exactly the pages the first build wrote.
    assert_eq!(f.render(root, 1088), before);
    assert_eq!(f.store.page_ids(), pages);
}
