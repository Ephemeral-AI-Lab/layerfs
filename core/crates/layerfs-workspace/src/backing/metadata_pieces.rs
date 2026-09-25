//! Length-indexed, path-copied extent sequence for one file.
//!
//! An extent's logical start is derived from its position in the sequence: a
//! leaf carries packed extent records, and a branch carries child references
//! with the logical byte length of each child subtree. A structural insertion
//! therefore moves a suffix by rewriting the subtree lengths on its ancestor
//! path, never the suffix pages. One accepted edit folds only the leaves the
//! replaced interval touches, copies their ancestors and shares every other
//! page with the previous root. Reads iterate the same pages through a bounded
//! cursor instead of materializing the sequence.
use super::{
    metadata::{Arena, RootOwner},
    metadata_index::vector,
    metadata_pages::{self, ChildRef, PageRef, PieceRecord, MAX_EXTENT, PAGE, RECORD},
    segments::Window,
};
use crate::{
    overlay::pieces::{Piece, PieceKind},
    WorkspaceError,
};
use layerfs_bridge::contract::{MAX_FILE, MAX_REPLAY};
use std::time::{Duration, Instant};

/// Leaf capacity in extent records: the whole payload area of one page.
const LEAF_RECORDS: usize = (PAGE - metadata_pages::HEADER) / RECORD;
/// Branch capacity in child references.
const BRANCH_CHILDREN: usize = (PAGE - metadata_pages::HEADER) / ChildRef::BYTES;
/// Declared structural ceiling. The tree is canonical — every non-root branch
/// keeps at least two children — so its height only grows with the extent count.
const MAX_HEIGHT: u8 = metadata_pages::LEVEL_LIMIT;

/// One page store behind the extent-sequence traversal. In production this is
/// the Workspace's own COW arena, reached through the root that owns the pages
/// it copies; a bounded in-memory store serves external tests of the same
/// traversal, so its arithmetic is checkable without a mount or a service.
pub trait PieceStore {
    /// Reads one page's encoded body.
    fn read(&self, page: PageRef, window: &mut Window) -> Result<[u8; PAGE], WorkspaceError>;
    /// Writes one page whose body `encode` fills for the allocated identity.
    fn write<T>(
        &self,
        level: u8,
        window: &mut Window,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError>;
    /// The incarnation every page of this store is stamped with.
    fn incarnation(&self) -> [u8; 32];
}

/// The result of one path-copied replacement: the new root and what the splice
/// itself established about the sequence it published.
pub struct Pieces {
    pub root: PageRef,
    /// Stored extents the splice itself placed or saw. Partial when a subtree
    /// was shared; never a recorded figure.
    pub extents: u64,
    /// Maximal replacement runs the splice counted, or `u16::MAX` when a
    /// shared subtree made the exact figure unavailable to this splice.
    pub edits: u16,
    /// The sequence's exact replacement bytes: the replaced sequence's
    /// recorded total, minus what the interval dropped, plus what this splice
    /// inserted.
    pub replacement: u64,
    /// Logical byte length of the published sequence.
    pub length: u64,
}

/// Splits one extent into stored parts, each no larger than the extent ceiling.
/// A part boundary is a page boundary, never a new logical byte.
pub fn parts(piece: Piece) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut piece = piece;
    while piece.length > 0 {
        let length = piece.length.min(MAX_EXTENT);
        out.push(Piece { length, ..piece });
        if piece.kind != PieceKind::Zero {
            piece.offset += length;
        }
        piece.length -= length;
    }
    out
}

/// One bounded producer of replacement extents. A replacement is consumed in
/// logical order, so a streaming Commit never needs it resident.
pub struct Replacement {
    items: Vec<Piece>,
}
impl Replacement {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }
    pub fn extend(&mut self, piece: Piece) {
        if piece.length > 0 {
            self.items.extend(parts(piece));
        }
    }
    pub fn into_parts(self) -> Parts {
        Parts {
            items: self.items,
            at: 0,
        }
    }
}
impl Default for Replacement {
    fn default() -> Self {
        Self::new()
    }
}
pub struct Parts {
    items: Vec<Piece>,
    at: usize,
}
impl Parts {
    /// The next declared part, or `None` once the replacement is exhausted.
    /// Every declared part, in order. The replacement is consumed here so the
    /// fold can merge it at the exact boundary it replaces.
    pub fn declare(&mut self) -> Result<Vec<Piece>, WorkspaceError> {
        let mut out = Vec::new();
        while let Some(piece) = self.take() {
            out.push(piece);
        }
        Ok(out)
    }
    pub fn take(&mut self) -> Option<Piece> {
        let piece = *self.items.get(self.at)?;
        self.at += 1;
        Some(piece)
    }
    /// The declared extents still unread, for the ownership fix-up a splice
    /// needs before the pages that name them are written.
    pub fn parts_mut(&mut self) -> &mut [Piece] {
        &mut self.items[self.at..]
    }
    /// The exact count of bytes this producer will hand out.
    pub fn declared(&self) -> Result<u64, WorkspaceError> {
        self.items
            .iter()
            .try_fold(0u64, |sum, piece| sum.checked_add(piece.length))
            .ok_or(WorkspaceError::Capacity)
    }
}

/// The logical interval one replacement covers, and the byte-count arithmetic
/// the resulting sequence must satisfy.
#[derive(Clone, Copy)]
pub struct Splice {
    /// Logical offset of the first replaced byte.
    pub start: u64,
    /// Logical offset one past the last replaced byte.
    pub end: u64,
    /// Base length that retained `Base` offsets must stay inside.
    pub old_base: u64,
    /// Replacement bytes the replaced sequence already recorded. A splice that
    /// shares an untouched subtree cannot see those bytes again, so the result's
    /// exact total is derived from this figure: `old - removed + inserted`.
    pub old_replacement: u64,
    /// Required logical length of the result.
    pub length: u64,
}

/// Counts and validates the sequence one replacement publishes, charging every
/// extent before the page that names it is written.
#[derive(Default)]
pub struct Fold {
    length: u64,
    extents: u64,
    edits: u16,
    replacement: u64,
    base: u64,
    pending: u64,
}
impl Fold {
    pub fn new() -> Self {
        Self::default()
    }
    /// Charges one extent to its pages before the page that names it is written.
    /// `base_length` bounds a retained canonical read; a sequence built from no
    /// base at all has no canonical byte to stay inside.
    pub fn push(&mut self, piece: Piece, base_length: u64) -> Result<(), WorkspaceError> {
        if piece.length == 0
            || piece.length > MAX_EXTENT
            || piece
                .offset
                .checked_add(piece.length)
                .is_none_or(|end| end > MAX_FILE)
        {
            return Err(WorkspaceError::Io);
        }
        match piece.kind {
            PieceKind::Base => {
                if piece.payload != 0
                    || piece.custody != PageRef::NULL
                    || piece.offset < self.base
                    || piece.offset + piece.length > base_length
                {
                    return Err(WorkspaceError::Io);
                }
                if piece.offset != self.base || self.pending != 0 {
                    self.edits = self.edits.checked_add(1).ok_or(WorkspaceError::Io)?;
                }
                self.base = piece.offset + piece.length;
                self.pending = 0;
            }
            PieceKind::Local => {
                if piece.payload == 0 || piece.custody == PageRef::NULL {
                    return Err(WorkspaceError::Io);
                }
                self.charge(piece.length)?;
            }
            PieceKind::Zero => {
                if piece.offset != 0 || piece.payload != 0 || piece.custody != PageRef::NULL {
                    return Err(WorkspaceError::Io);
                }
                self.charge(piece.length)?;
            }
        }
        self.length = self
            .length
            .checked_add(piece.length)
            .filter(|length| *length <= MAX_FILE)
            .ok_or_else(|| WorkspaceError::Io)?;
        self.extents = self.extents.checked_add(1).ok_or(WorkspaceError::Io)?;
        Ok(())
    }
    fn charge(&mut self, length: u64) -> Result<(), WorkspaceError> {
        self.pending = self.pending.checked_add(length).ok_or(WorkspaceError::Io)?;
        self.replacement = self
            .replacement
            .checked_add(length)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
    /// Charges one shared subtree's logical length. Its extents stay unread,
    /// so the fold cannot count them; only the length the result must cover
    /// moves.
    pub fn carry(&mut self, length: u64) -> Result<(), WorkspaceError> {
        self.length = self
            .length
            .checked_add(length)
            .filter(|length| *length <= MAX_FILE)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
    /// Counts the sequence's trailing deviation from its base: a final run of
    /// replacement bytes, or a tail that ends short of the recorded base — a
    /// truncation lowering later derives as one deletion edit.
    pub fn close(&mut self, base_bound: u64) -> Result<(), WorkspaceError> {
        if self.pending != 0 || self.base < base_bound {
            self.edits = self.edits.checked_add(1).ok_or(WorkspaceError::Io)?;
        }
        Ok(())
    }
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn extents(&self) -> u64 {
        self.extents
    }
    pub fn edits(&self) -> u16 {
        self.edits
    }
    pub fn replacement(&self) -> u64 {
        self.replacement
    }
}

/// Builds one length-indexed sequence holding exactly `pieces`, in order.
pub fn build<S: PieceStore + ?Sized>(
    store: &S,
    pieces: &[Piece],
    length: u64,
    window: &mut Window,
) -> Result<PageRef, WorkspaceError> {
    let mut replacement = Replacement::new();
    for piece in pieces {
        replacement.extend(*piece);
    }
    Ok(replace(
        store,
        PageRef::NULL,
        Splice {
            start: 0,
            end: 0,
            old_base: 0,
            old_replacement: 0,
            length,
        },
        &mut replacement.into_parts(),
        window,
    )?
    .root)
}

/// Replaces `old[start..end]` with every part of `replacement`, sharing every
/// subtree the interval does not touch and copying only the ancestors of the
/// leaves it folds. A NULL `old` folds into the sequence the version already
/// is: one implicit base read of its selected content, or an empty sequence.
pub fn replace<S: PieceStore + ?Sized>(
    store: &S,
    old: PageRef,
    splice: Splice,
    replacement: &mut Parts,
    window: &mut Window,
) -> Result<Pieces, WorkspaceError> {
    if splice.end < splice.start || splice.length > MAX_FILE || splice.old_base > MAX_FILE {
        return Err(WorkspaceError::Io);
    }
    let complete = splice.start == 0 && splice.end == 0 && splice.old_base == 0;
    let declared = replacement.declared()?;
    // One replacement cannot declare more bytes than a file may hold; the
    // result's own length is bounded separately above. A net-shrinking splice
    // (a deletion) is as legitimate as an insertion, so the interval never
    // bounds the declared parts.
    if declared > MAX_FILE {
        return Err(WorkspaceError::Capacity);
    }
    // A sequence with no canonical base at all - a fresh file's construction or
    // a first publication - retains no canonical read to bound.
    let base_length = if splice.old_base == 0 {
        MAX_FILE
    } else {
        splice.old_base
    };
    let declared_parts = replacement.declare()?;
    let inserted = declared_parts.iter().try_fold(0u64, |sum, piece| {
        if piece.kind == PieceKind::Base {
            Ok(sum)
        } else {
            sum.checked_add(piece.length)
                .ok_or(WorkspaceError::Capacity)
        }
    })?;
    let mut walk = Walk::new(store, window, base_length, declared_parts);
    let mut level = if old == PageRef::NULL {
        // A NULL root is either a version that is exactly one base read of its
        // selected content, or a sequence that is empty; both fold without a
        // tree. `current` is the length of the sequence being replaced.
        let current = splice
            .length
            .checked_add(splice.end - splice.start)
            .and_then(|span| span.checked_sub(declared))
            .ok_or(WorkspaceError::Io)?;
        if current == 0 {
            // An empty sequence, or a construction with no base at all: the
            // result is exactly the replacement.
            if splice.start != 0 || splice.end != 0 {
                return Err(WorkspaceError::Io);
            }
            walk.insert()?;
        } else if current == splice.old_base {
            // One base read of the selected content: the splice folds into
            // that implicit base, so the retained prefix and tail read the
            // base exactly as a stored leaf would. A stored leaf holds a long
            // base read as adjacent parts no larger than the extent ceiling,
            // so the implicit fold splits them the same way.
            if splice.end > splice.old_base {
                return Err(WorkspaceError::Io);
            }
            if splice.start > 0 {
                for part in parts(Piece::base(splice.start)) {
                    walk.emit(part)?;
                }
            }
            walk.insert()?;
            if splice.end < splice.old_base {
                for part in parts(Piece {
                    kind: PieceKind::Base,
                    length: splice.old_base - splice.end,
                    offset: splice.end,
                    payload: 0,
                    custody: PageRef::NULL,
                }) {
                    walk.emit(part)?;
                }
            }
        } else {
            // A NULL root with a partially edited sequence cannot exist.
            return Err(WorkspaceError::Io);
        }
        Vec::new()
    } else {
        let (level, covered) = walk.descend(old, splice, 0, 0)?;
        if covered != splice.start {
            return Err(WorkspaceError::Io);
        }
        // A replacement that begins exactly at the end of the retained
        // sequence, or one whose interval no leaf of the old tree holds, is
        // merged here. A merge already made inside a leaf leaves this empty.
        walk.insert()?;
        level
    };
    walk.close_leaf()?;
    let dropped = walk.dropped;
    let shared = walk.shared;
    walk.fold.close(splice.old_base)?;
    let fold = walk.fold;
    if fold.length() != splice.length || walk.line != splice.length {
        eprintln!(
            "LFS_PIECES_MISMATCH fold_length={} splice_length={} line={}",
            fold.length(),
            splice.length,
            walk.line
        );
        return Err(WorkspaceError::Io);
    }
    // The exact replacement total carries over from the figure the replaced
    // sequence recorded: a shared subtree's bytes are untouched by the
    // interval, so only the bytes the folded leaves dropped and the bytes this
    // splice inserted move it.
    let total = splice
        .old_replacement
        .checked_sub(dropped)
        .ok_or(WorkspaceError::Io)?
        .checked_add(inserted)
        .ok_or(WorkspaceError::Capacity)?;
    if !shared && fold.replacement() != total {
        // With every extent folded, the emitted bytes and the carried total
        // must agree exactly.
        eprintln!(
            "LFS_PIECES_TOTAL emitted={} carried={}",
            fold.replacement(),
            total
        );
        return Err(WorkspaceError::Io);
    }
    if fold.edits() > 256 || (!complete && total > MAX_REPLAY) {
        return Err(WorkspaceError::Capacity);
    }
    level.extend(std::mem::take(&mut walk.leaves));
    let window = walk.window;
    let root = pack(store, level, window)?;
    Ok(Pieces {
        root,
        extents: fold.extents(),
        edits: if shared { u16::MAX } else { fold.edits() },
        replacement: total,
        length: fold.length(),
    })
}

/// The production page store: one file's pages live in an arena that the
/// sequence's owning root writes. Ownership, references and cleanup are the
/// root's, exactly as for a keyed-cell page.
impl PieceStore for Arena {
    fn read(&self, page: PageRef, window: &mut Window) -> Result<[u8; PAGE], WorkspaceError> {
        self.load_raw(page, window, lease())
    }
    fn write<T>(
        &self,
        _level: u8,
        _window: &mut Window,
        _encode: impl FnOnce(PageRef, &mut [u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        // A new page always belongs to a candidate root, never to an arena on
        // its own: the traversal must be given that root as its store.
        Err(WorkspaceError::Unsupported)
    }
    fn incarnation(&self) -> [u8; 32] {
        self.directory.incarnation
    }
}

/// Walks one old tree in order, folding the leaves the replaced interval
/// touches, sharing every other subtree by reference, and writing the result.
struct Walk<'a, 'w, S: PieceStore + ?Sized> {
    store: &'a S,
    window: &'w mut Window,
    /// The replacement extents, merged into the sequence in order.
    replacement: Vec<Piece>,
    /// Non-Base bytes the replaced interval dropped from the old sequence.
    dropped: u64,
    /// Whether any subtree the interval does not touch was shared by
    /// reference: the fold then saw only part of the result, so the exact edit
    /// count is unavailable to this splice.
    shared: bool,
    fold: Fold,
    /// Records of the leaf currently open for writing.
    records: Vec<PieceRecord>,
    /// Leaf pages written or shared, in sequence order.
    leaves: Vec<ChildRef>,
    /// The last extent placed, for merging with the next one.
    last: Option<Piece>,
    /// Logical length of every byte placed or carried.
    line: u64,
    base_length: u64,
}
impl<'a, 'w, S: PieceStore + ?Sized> Walk<'a, 'w, S> {
    fn new(
        store: &'a S,
        window: &'w mut Window,
        base_length: u64,
        replacement: Vec<Piece>,
    ) -> Self {
        Self {
            store,
            window,
            replacement,
            dropped: 0,
            shared: false,
            fold: Fold::new(),
            records: Vec::new(),
            leaves: Vec::new(),
            last: None,
            line: 0,
            base_length,
        }
    }
    /// Folds one subtree in order, answering the pages that replace it at its
    /// own level. `lower` is the subtree's logical start and `source` the byte
    /// offset its first `Base` extent reads from; both are carried so a leaf
    /// can tell where the replaced interval begins and ends.
    fn descend(
        &mut self,
        page: PageRef,
        splice: Splice,
        lower: u64,
        source: u64,
    ) -> Result<(Vec<ChildRef>, u64), WorkspaceError> {
        let bytes = self.store.read(page, self.window)?;
        let length = metadata_pages::piece_body_length(&bytes)?;
        let upper = lower.checked_add(length).ok_or(WorkspaceError::Io)?;
        if upper < splice.start || lower >= splice.end {
            // The replaced interval does not touch this subtree. It is shared
            // exactly as it stands and placed by the lengths on its own path.
            // A subtree ending exactly where the interval begins is carried the
            // same way: the replacement is merged at the boundary above it.
            // The fold charges only the length; the subtree's own extents and
            // replacement bytes stay unread, so the exact edit count is no
            // longer available to this splice. The open leaf closes first: a
            // folded sibling's records must become a page before this shared
            // page, or the parent would place them out of order.
            self.close_leaf()?;
            self.shared = true;
            self.fold.carry(length)?;
            self.line = self
                .line
                .checked_add(length)
                .filter(|line| *line <= MAX_FILE)
                .ok_or(WorkspaceError::Capacity)?;
            let mut pages = std::mem::take(&mut self.leaves);
            pages.push(ChildRef { page, length });
            return Ok((pages, upper.min(splice.start)));
        }
        if metadata_pages::PageKind::of(&bytes)? != metadata_pages::PageKind::Pieces {
            return Err(WorkspaceError::Io);
        }
        if bytes[48] == 0 {
            let covered = self.leaf(&bytes, splice, source)?;
            Ok((std::mem::take(&mut self.leaves), covered))
        } else {
            let (_, children) = metadata_pages::decode_pieces_branch(
                self.store.incarnation(),
                page,
                &bytes,
                MAX_FILE,
            )?;
            let mut level = Vec::new();
            let mut covered = lower;
            let mut reads = source;
            for child in children {
                let stop = covered
                    .checked_add(child.length)
                    .ok_or(WorkspaceError::Io)?;
                let (nested, _) = self.descend(child.page, splice, covered, reads)?;
                level.extend(nested);
                covered = stop;
                reads = reads.checked_add(child.length).ok_or(WorkspaceError::Io)?;
            }
            // The merge point the parent folds at: the subtree's end, or the
            // interval's start when the subtree reaches past it.
            Ok((level, covered.min(splice.start)))
        }
    }
    /// Folds one leaf: the retained prefix, the replacement, then the retained
    /// remainder. A piece that straddles a boundary is split, so the sequence
    /// keeps exactly the bytes the accepted edit selected.
    fn leaf(&mut self, bytes: &[u8], splice: Splice, source: u64) -> Result<u64, WorkspaceError> {
        let r = PageRef::parse(&bytes[40..48])?;
        let records =
            metadata_pages::decode_pieces_leaf(self.store.incarnation(), r, bytes, MAX_FILE)?;
        let mut at = source;
        for record in records {
            let piece = Piece::from_record(record)?;
            if piece.kind == PieceKind::Base && piece.offset != at {
                return Err(WorkspaceError::Io);
            }
            let stop = at.checked_add(piece.length).ok_or(WorkspaceError::Io)?;
            if stop <= splice.start || at >= splice.end {
                // Outside the replaced interval: retained exactly, and keeping
                // its own origin offset.
                self.emit(piece)?;
            } else {
                // The extent overlaps the interval. The side before it is
                // retained, the overlap is dropped, and the side after it keeps
                // reading its own origin from one byte past the overlap.
                let from = at.max(splice.start);
                let to = stop.min(splice.end);
                if piece.kind != PieceKind::Base {
                    self.dropped = self
                        .dropped
                        .checked_add(to - from)
                        .ok_or(WorkspaceError::Io)?;
                }
                if from > at {
                    self.emit(Piece {
                        length: from - at,
                        ..piece
                    })?;
                }
                self.insert()?;
                if to < stop {
                    let mut tail = piece;
                    if tail.kind != PieceKind::Zero {
                        tail.offset = piece
                            .offset
                            .checked_add(to - at)
                            .ok_or(WorkspaceError::Io)?;
                    }
                    tail.length = stop - to;
                    self.emit(tail)?;
                }
            }
            at = stop;
        }
        Ok(at.min(splice.start))
    }
    /// Merges the replacement extents into the sequence at this boundary.
    fn insert(&mut self) -> Result<(), WorkspaceError> {
        let replacement = std::mem::take(&mut self.replacement);
        for piece in replacement {
            self.emit(piece)?;
        }
        Ok(())
    }
    /// Writes the open leaf, if any, as one page of extent records.
    fn close_leaf(&mut self) -> Result<(), WorkspaceError> {
        if self.records.is_empty() {
            return Ok(());
        }
        let length = self.records.iter().try_fold(0u64, |sum, r| {
            sum.checked_add(r.length).ok_or(WorkspaceError::Io)
        })?;
        let incarnation = self.store.incarnation();
        let records = std::mem::take(&mut self.records);
        let page = self.store.write(0, self.window, |r, bytes| {
            metadata_pages::encode_pieces_leaf(incarnation, r, &records, bytes).map(|()| r)
        })?;
        self.leaves.push(ChildRef { page, length });
        self.records = vector(LEAF_RECORDS)?;
        self.last = None;
        Ok(())
    }
    fn emit(&mut self, piece: Piece) -> Result<(), WorkspaceError> {
        if piece.length == 0 || piece.length > MAX_EXTENT {
            return Err(WorkspaceError::Io);
        }
        self.fold.push(piece, self.base_length)?;
        if let Some(last) = self.last {
            if mergeable(last, piece) && self.records.len() < LEAF_RECORDS {
                let merged = last
                    .length
                    .checked_add(piece.length)
                    .ok_or(WorkspaceError::Io)?;
                let record = self.records.last_mut().ok_or(WorkspaceError::Io)?;
                record.length = merged;
                self.last = Some(Piece {
                    length: merged,
                    ..last
                });
                self.line = self
                    .line
                    .checked_add(piece.length)
                    .filter(|line| *line <= MAX_FILE)
                    .ok_or(WorkspaceError::Capacity)?;
                return Ok(());
            }
        }
        if self.records.len() == LEAF_RECORDS {
            self.close_leaf()?;
        }
        self.records.push(piece.record());
        self.last = Some(piece);
        self.line = self
            .line
            .checked_add(piece.length)
            .filter(|line| *line <= MAX_FILE)
            .ok_or(WorkspaceError::Capacity)?;
        Ok(())
    }
}

/// The Workspace's own COW arena is the production page store for one file's
/// extent sequence. Every page it writes carries the same ownership, reference
/// accounting and cleanup obligations as a keyed-cell page.
impl PieceStore for RootOwner {
    fn read(&self, page: PageRef, window: &mut Window) -> Result<[u8; PAGE], WorkspaceError> {
        self.arena.load_raw(page, window, lease())
    }
    fn write<T>(
        &self,
        _level: u8,
        window: &mut Window,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        Ok(self.write_raw_page(window, lease(), encode)?.1)
    }
    fn incarnation(&self) -> [u8; 32] {
        self.arena.directory.incarnation
    }
}

/// The arena's own reads and writes are charged against the operation deadline
/// by their caller; this is the bound one page operation may take on its own.
fn lease() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

impl RootOwner {
    /// Builds one file's extent sequence from the exact extents given, in order.
    pub fn build_pieces(
        &self,
        pieces: &[Piece],
        length: u64,
        window: &mut Window,
    ) -> Result<PageRef, WorkspaceError> {
        build(self, pieces, length, window)
    }
}

/// The bounded reader for one file's extents, re-exported so the splice engine
/// and its callers stay on one module path.
pub use super::metadata_cursor::{cursor, piece_at, Cursor};

/// Writes every branch level above the folded and shared leaves. Every
/// non-root branch keeps at least two children, so the tree stays canonical.
fn pack<S: PieceStore + ?Sized>(
    store: &S,
    mut lower: Vec<ChildRef>,
    window: &mut Window,
) -> Result<PageRef, WorkspaceError> {
    if lower.is_empty() {
        return Ok(PageRef::NULL);
    }
    let incarnation = store.incarnation();
    let mut level = 1u8;
    while lower.len() > 1 {
        if level > MAX_HEIGHT {
            return Err(WorkspaceError::Capacity);
        }
        let mut upper = vector(lower.len().div_ceil(BRANCH_CHILDREN))?;
        for chunk in lower.chunks(BRANCH_CHILDREN) {
            let length = chunk.iter().try_fold(0u64, |sum, c| {
                sum.checked_add(c.length).ok_or(WorkspaceError::Io)
            })?;
            let page = store.write(level, window, |r, bytes| {
                metadata_pages::encode_pieces_branch(incarnation, r, level, chunk, bytes)
                    .map(|()| r)
            })?;
            upper.push(ChildRef { page, length });
        }
        lower = upper;
        level += 1;
    }
    Ok(lower[0].page)
}

/// Whether two adjacent extents may be stored as one extent.
fn mergeable(a: Piece, b: Piece) -> bool {
    a.kind == b.kind
        && a.payload == b.payload
        && a.custody == b.custody
        && a.length + b.length <= MAX_EXTENT
        && (a.kind == PieceKind::Zero || a.offset + a.length == b.offset)
}
