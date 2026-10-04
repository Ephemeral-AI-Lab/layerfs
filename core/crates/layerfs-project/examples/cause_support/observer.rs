//! Fixed aggregate observer; worker sums overlap the owner-thread regions.
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
#[derive(Clone, Copy)]
pub enum Step {
    Scan,
    Files,
    Construct,
    Read,
    Send,
    Receive,
    Prerequisites,
    Tree,
    ReserveInodes,
    Genesis,
    SaveBegin,
    SaveFinish,
}
const N: usize = 12;
static CALLS: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static WALL: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static BYTES: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
pub fn note(step: Step, start: Instant, bytes: u64) {
    let i = step as usize;
    CALLS[i].fetch_add(1, Ordering::Relaxed);
    WALL[i].fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    BYTES[i].fetch_add(bytes, Ordering::Relaxed);
}
pub struct Span {
    step: Step,
    start: Instant,
}
pub fn span(step: Step) -> Span {
    Span {
        step,
        start: Instant::now(),
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        note(self.step, self.start, 0)
    }
}
pub fn json() -> String {
    let names = [
        "scan",
        "files_owner",
        "construct_worker_sum",
        "file_read_worker_sum",
        "producer_send_worker_sum",
        "owner_receive",
        "prerequisites",
        "filesystem_tree",
        "reserve_inodes",
        "genesis",
        "save_begin",
        "save_finish",
    ];
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!(
                "\"{name}\":{{\"calls\":{},\"wall_ns\":{},\"bytes\":{}}}",
                CALLS[i].load(Ordering::Relaxed),
                WALL[i].load(Ordering::Relaxed),
                BYTES[i].load(Ordering::Relaxed)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}
pub struct MeasuredFile(File);
impl MeasuredFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        File::open(path).map(Self)
    }
    pub fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.0.metadata()
    }
}
impl Read for MeasuredFile {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let start = Instant::now();
        let result = self.0.read(out);
        note(
            Step::Read,
            start,
            result.as_ref().copied().unwrap_or(0) as u64,
        );
        result
    }
}
