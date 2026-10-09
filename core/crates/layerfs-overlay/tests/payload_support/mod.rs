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
        let publication = self
            .db
            .apply(
                self.source,
                &Changes {
                    inodes: vec![self.inode(size)],
                    write,
                    ..Changes::default()
                },
            )
            .unwrap();
        self.db.reply_attempted(publication).unwrap();
        self.touched = true;
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
                *byte = self.base.get(at).copied().unwrap_or(0);
            }
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
