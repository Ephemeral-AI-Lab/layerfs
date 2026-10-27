#![allow(dead_code)]
//! Public S5 engine proofs: byte-exact layered reads against a reference
//! model, fragmentation/shrink/staircase adversaries with runtime counters,
//! holes and measured page amplification. No private source or fault hooks.
use layerfs_overlay::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
};

pub struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-payload-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn db(&self) -> Overlay {
        Overlay::create(&self.0.join("overlay.sqlite"), ProfileConfig::default()).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
/// One regular file driven through the public compound job, with its expected
/// complete view and the immutable base content beneath it.
pub struct File<'a> {
    pub db: &'a Overlay,
    pub source: BaseSource,
    pub serial: u64,
    pub base: Vec<u8>,
    pub expect: Vec<u8>,
    pub touched: bool,
    /// Committed content a reference piece may name, by its root.
    pub sources: Vec<([u8; 32], Vec<u8>)>,
    /// Revision of this file's last publication.
    pub revision: i64,
}
impl<'a> File<'a> {
    pub fn new(db: &'a Overlay, source: BaseSource, serial: u64, base: Vec<u8>) -> Self {
        Self {
            db,
            source,
            serial,
            expect: base.clone(),
            base,
            touched: false,
            sources: Vec::new(),
            revision: 0,
        }
    }
    pub fn inode(&self, size: u64) -> Inode {
        Inode {
            serial: self.serial,
            kind: InodeKind::File,
            mode: 0o644,
            mtime_seconds: 1,
            mtime_nanoseconds: 2,
            nlink: 1,
            size,
            // Used only for the first local row: the inherited base length.
            inherited_cutoff: self.base.len() as u64,
            born: 0,
            entries: 0,
            subdirs: 0,
        }
    }
    pub fn apply(&mut self, size: u64, write: Option<PayloadWrite>) {
        self.apply_claimed(size, write, None)
    }
    pub fn apply_claimed(
        &mut self,
        size: u64,
        write: Option<PayloadWrite>,
        claims: Option<WriteClaims>,
    ) {
        let publication = self
            .db
            .apply(
                self.source,
                &Changes {
                    inodes: vec![self.inode(size)],
                    write,
                    claims: claims.map(Box::new),
                    ..Changes::default()
                },
            )
            .unwrap();
        self.revision = publication.revision();
        self.db.reply_attempted(publication).unwrap();
        self.touched = true;
    }
    /// A write whose claimed ranges the caller made equal to committed
    /// content. The expected view is that of the same plain write.
    pub fn write_claimed(&mut self, offset: u64, data: &[u8], claims: WriteClaims) {
        let end = offset as usize + data.len();
        if self.expect.len() < end {
            self.expect.resize(end, 0);
        }
        self.expect[offset as usize..end].copy_from_slice(data);
        self.apply_claimed(
            self.expect.len() as u64,
            Some(PayloadWrite {
                serial: self.serial,
                offset,
                data: Arc::from(data),
            }),
            Some(claims),
        );
    }
    pub fn write(&mut self, offset: u64, data: &[u8]) {
        let end = offset as usize + data.len();
        if self.expect.len() < end {
            self.expect.resize(end, 0);
        }
        self.expect[offset as usize..end].copy_from_slice(data);
        self.apply(
            self.expect.len() as u64,
            Some(PayloadWrite {
                serial: self.serial,
                offset,
                data: Arc::from(data),
            }),
        );
    }
    pub fn resize(&mut self, size: u64) {
        self.expect.resize(size as usize, 0);
        self.apply(size, None);
    }
    /// The complete effective view, composed like Workspace does: local bytes,
    /// then the one inherited span from the base, zero beyond the base's EOF.
    pub fn read(&self, offset: u64, length: u32) -> Vec<u8> {
        let Some(local) = self
            .db
            .source_read(self.source, self.serial, offset, length)
            .unwrap()
        else {
            assert!(!self.touched);
            let end = (offset as usize + length as usize).min(self.base.len());
            return self.base[(offset as usize).min(end)..end].to_vec();
        };
        assert_eq!(local.size, self.expect.len() as u64);
        self.compose(&local, &self.base)
    }
    /// The bytes of one local read plan over `base`: local bytes, the one
    /// inherited span from the base, and each reference range from its source.
    pub fn compose(&self, local: &LocalRead, base: &[u8]) -> Vec<u8> {
        let mut data = local.data.clone();
        assert_eq!(local.span.is_some(), !local.inherited.is_empty());
        if let Some((from, to)) = local.span {
            assert!(from >= local.offset && to <= local.offset + data.len() as u64 && from < to);
        }
        for (slot, byte) in data.iter_mut().enumerate() {
            if local
                .inherited
                .get(slot / 8)
                .is_some_and(|bits| bits & (1 << (slot % 8)) != 0)
            {
                let at = local.offset as usize + slot;
                let (from, to) = local.span.unwrap();
                assert!((from as usize..to as usize).contains(&at));
                *byte = base.get(at).copied().unwrap_or(0);
            }
        }
        // A byte a reference range supplies is read from that range's source.
        assert_eq!(local.referenced.is_empty(), local.references.is_empty());
        assert!(local.references.len() <= READ_REFERENCES);
        if !local.references.is_empty() {
            assert_eq!(local.referenced.len(), data.len().div_ceil(8));
        }
        let mut supplied = vec![false; data.len()];
        for range in &local.references {
            // A range lies in the window and starts on a cell of it. It ends
            // on a cell, with the window, or where a shrink cut its piece.
            let (from, to) = (range.offset, range.offset + range.length);
            assert!(from >= local.offset && to <= local.offset + data.len() as u64 && from < to);
            assert!(from % 4096 == 0 || from == local.offset);
            let content = &self
                .sources
                .iter()
                .find(|(root, _)| *root == range.source)
                .expect("source of a reference range")
                .1;
            let mut any = false;
            for at in from..to {
                let slot = (at - local.offset) as usize;
                assert!(!supplied[slot], "ranges overlap");
                supplied[slot] = true;
                if local.referenced[slot / 8] & (1 << (slot % 8)) != 0 {
                    let inherited = local.inherited.get(slot / 8);
                    assert!(inherited.is_none_or(|bits| bits & (1 << (slot % 8)) == 0));
                    data[slot] = content[(range.source_offset + (at - range.offset)) as usize];
                    any = true;
                }
            }
            assert!(any, "a range supplies a byte");
        }
        for (slot, held) in supplied.iter().enumerate() {
            let bit = local
                .referenced
                .get(slot / 8)
                .is_some_and(|bits| bits & (1 << (slot % 8)) != 0);
            assert!(*held || !bit, "a referenced byte outside every range");
        }
        data
    }
    pub fn check(&self) {
        let mut offset = 0_usize;
        loop {
            let got = self.read(offset as u64, READ_WINDOW as u32);
            let end = (offset + READ_WINDOW).min(self.expect.len());
            assert_eq!(got.len(), end - offset.min(end));
            if got != self.expect[offset.min(end)..end] {
                let at = got
                    .iter()
                    .zip(&self.expect[offset..end])
                    .position(|(a, b)| a != b)
                    .unwrap();
                panic!(
                    "byte {} differs: got {} expected {}",
                    offset + at,
                    got[at],
                    self.expect[offset + at]
                );
            }
            if end == self.expect.len() {
                break;
            }
            offset = end;
        }
    }
}
pub fn pattern(length: usize, seed: u8) -> Vec<u8> {
    (0..length)
        .map(|i| ((i * 31 + i / 251) as u8).wrapping_add(seed) | 1)
        .collect()
}
/// Bytes without a period, so a wrong source offset reads different bytes.
pub fn noise(length: usize, seed: u64) -> Vec<u8> {
    let mut random = Random(seed);
    (0..length).map(|_| random.next(256) as u8).collect()
}
pub struct Random(pub u64);
impl Random {
    pub fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound
    }
}
pub fn work(db: &Overlay, job: impl FnOnce()) -> (u64, u64, u64) {
    let before = db.diagnostics();
    job();
    let after = db.diagnostics();
    let mut total = (0, 0, 0);
    for (index, (a, z)) in before.statements.iter().zip(&after.statements).enumerate() {
        assert_eq!(
            z.fullscan_steps, a.fullscan_steps,
            "fullscan family {index}"
        );
        assert_eq!(z.sorts, a.sorts, "sort family {index}");
        assert_eq!(
            z.autoindex_rows, a.autoindex_rows,
            "autoindex family {index}"
        );
        assert_eq!(z.reprepares, a.reprepares, "reprepare family {index}");
        total.0 += z.executions - a.executions;
        total.1 += z.vm_steps - a.vm_steps;
        total.2 += z.rows_changed - a.rows_changed;
    }
    total
}

/// The content root the pieces of these proofs name.
pub const ROOT: [u8; 32] = [77; 32];
const CELL: u64 = CELL_BYTES as u64;
const WINDOW: u64 = WRITE_WINDOW as u64;
pub fn claims(after: Option<i64>, ranges: &[(u64, u64, u64)]) -> WriteClaims {
    WriteClaims {
        source: ROOT,
        after,
        ranges: ranges
            .iter()
            .map(|&(offset, length, source_offset)| ClaimedRange {
                offset,
                length,
                source_offset,
            })
            .collect(),
    }
}
/// Reference rows and payload rows of one Workspace.
pub fn rows(db: &Overlay, _route: ()) -> (u64, u64) {
    let counts = db.resources().unwrap().counts;
    (counts.reference_rows, counts.payload_cells)
}
/// A file whose committed content is `content`, which is also what a piece
/// of it may name.
pub fn committed<'a>(db: &'a Overlay, source: BaseSource, content: &[u8]) -> File<'a> {
    let mut file = File::new(db, source, 7, content.to_vec());
    file.sources.push((ROOT, content.to_vec()));
    file
}
pub fn part(content: &[u8], from: u64, length: u64) -> &[u8] {
    &content[from as usize..(from + length) as usize]
}

/// One window of committed content at any alignment with up to three claimed
/// ranges; every byte outside them differs from the content.
pub fn scattered(file: &mut File, content: &[u8], random: &mut Random) {
    let size = file.expect.len() as u64;
    let length = 1 + random.next(WINDOW) as usize;
    let from = random.next((content.len() - length) as u64 + 1) as usize;
    let mut offset = random.next(size + 20_000);
    if random.next(2) == 0 {
        offset -= offset % CELL;
    }
    let mut cuts: Vec<usize> = (0..6)
        .map(|_| random.next(length as u64 + 1) as usize)
        .collect();
    cuts.sort_unstable();
    if random.next(3) == 0 {
        cuts = vec![0, length];
    }
    let mut data = content[from..from + length].to_vec();
    let mut kept = vec![false; length];
    let mut ranges = Vec::new();
    for cut in cuts.chunks(2).filter(|cut| cut[0] < cut[1]) {
        kept[cut[0]..cut[1]].fill(true);
        let length = (cut[1] - cut[0]) as u64;
        ranges.push((offset + cut[0] as u64, length, (from + cut[0]) as u64));
    }
    for (byte, kept) in data.iter_mut().zip(kept) {
        if !kept {
            *byte ^= 0x5a;
        }
    }
    if ranges.is_empty() {
        file.write(offset, &data);
    } else {
        file.write_claimed(offset, &data, claims(None, &ranges));
    }
}
/// Three sequential requests of one run of committed content. Each claim
/// ends inside its window and the next one starts in those bytes, as a
/// recognizer that carries a partial chunk claims them. Sometimes another
/// publication lands between two requests.
pub fn run(file: &mut File, content: &[u8], random: &mut Random) {
    let mut offset = random.next(file.expect.len() as u64 + 1);
    if random.next(3) != 0 {
        offset -= offset % CELL;
    }
    let mut from = random.next(content.len() as u64 / 2);
    let (mut tail, mut after) = (0, None);
    for _ in 0..3 {
        let length = 20_000 + random.next(60_000);
        if from + length > content.len() as u64 {
            break;
        }
        let left = random.next(9_000);
        let claimed = (offset - tail, length + tail - left, from - tail);
        file.write_claimed(
            offset,
            part(content, from, length),
            claims(after, &[claimed]),
        );
        after = Some(file.revision);
        (offset, from, tail) = (offset + length, from + length, left);
        match random.next(4) {
            0 if tail != 0 => file.write(offset - 1 - random.next(tail), &[0xa5]),
            1 => file.write(random.next(offset), &[0x5a]),
            _ => {}
        }
    }
}
