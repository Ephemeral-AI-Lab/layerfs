//! Prototype (experiment `smallread`, FUSE profile probe): a tree of many
//! small committed files and cold per-tool-call walks over it.
//!
//! `prepare` writes `tree/`: [`TREE_FILES`] text files of 0.5–64 KiB (skewed
//! small: `512 · 2^(7u²)` bytes, median about 1.7 KiB) in nested
//! directories, all from fixed seeds, and records the expected totals in
//! `tree.expect`. Each phase runs in its own child process (`tree <fixture>
//! <work> <phase>`), so the daemon's object cache, lengths and metadata
//! caches start empty; it mounts a fresh Workspace, evicts the Store copy,
//! checks residency with `fincore` and walks the tree:
//!
//! - `read`: one thread opens, reads whole and closes every file, in
//!   directory order (like `grep -r`).
//! - `read8`: the same with 8 threads taking directories from one queue.
//! - `stat`: one thread `lstat`s every file (like `find -printf %s`).
//!
//! Every walk folds what it saw into order-free totals and checks them
//! against the source's.
use super::mounted::Harness;
use super::probe::{
    cache_state, evict, fold, opcodes, readers, resident, resident_reset, stage_delta, status_kb,
};
use layerfs_history::BranchId;
use layerfs_persistence::{PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_telemetry::timer::stages;
use layerfs_workspace::small_probe;
use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

/// Files in `tree/`.
pub const TREE_FILES: usize = 20_000;
/// Top-level directories ("packages").
const TOP: u64 = 24;
/// Deepest directory level below the top level.
const DEPTH: usize = 4;
const WORDS: [&str; 32] = [
    "const",
    "let",
    "function",
    "return",
    "import",
    "export",
    "from",
    "default",
    "module",
    "require",
    "value",
    "index",
    "length",
    "string",
    "number",
    "object",
    "array",
    "this",
    "new",
    "if",
    "else",
    "for",
    "while",
    "true",
    "false",
    "null",
    "undefined",
    "async",
    "await",
    "class",
    "extends",
    "=>",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }
}
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
fn name_hash(rel: &str) -> u64 {
    rel.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}
fn file_digest(rel: &str, bytes: &[u8]) -> u64 {
    mix(name_hash(rel) ^ fold(bytes.len() as u64, bytes))
}
fn stat_digest(rel: &str, size: u64, mode: u32) -> u64 {
    mix(name_hash(rel) ^ size ^ (u64::from(mode) << 40))
}

/// Order-free totals of one walk.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Totals {
    pub files: u64,
    pub bytes: u64,
    pub digest: u64,
    pub dirs: u64,
}
impl Totals {
    fn add(&mut self, other: Totals) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.digest = self.digest.wrapping_add(other.digest);
        self.dirs += other.dirs;
    }
    fn parse(text: &str) -> Self {
        let n: Vec<u64> = text
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        Self {
            files: n[0],
            bytes: n[1],
            digest: n[2],
            dirs: n[3],
        }
    }
    fn render(&self) -> String {
        format!(
            "{} {} {} {}",
            self.files, self.bytes, self.digest, self.dirs
        )
    }
}

/// One text file of `size` bytes: its path, then words.
fn text(rng: &mut Rng, rel: &str, size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(size + 32);
    out.extend_from_slice(b"// ");
    out.extend_from_slice(rel.as_bytes());
    out.push(b'\n');
    let mut column = 0;
    while out.len() < size {
        let r = rng.next();
        out.extend_from_slice(WORDS[(r % 32) as usize].as_bytes());
        if (r >> 32) & 3 == 0 {
            out.extend_from_slice(format!("{}", (r >> 40) % 10_000).as_bytes());
        }
        column += 1;
        out.push(if column % 9 == 0 { b'\n' } else { b' ' });
    }
    out.truncate(size);
    out
}

/// Writes `source/tree` and `<fixture>/tree.expect` (read totals, then stat
/// totals, one line each).
pub fn generate(source: &Path, fixture: &Path) {
    let root = source.join("tree");
    fs::create_dir(&root).unwrap();
    let mut rng = Rng(0x7EE5_0F_5EED_u64);
    let mut dirs = vec![PathBuf::new()];
    let mut level = vec![PathBuf::new()];
    for depth in 0..=DEPTH {
        let mut next = Vec::new();
        for parent in &level {
            let children = if depth == 0 { TOP } else { rng.next() % 4 };
            for child in 0..children {
                let name = if depth == 0 {
                    format!("p{child:02}")
                } else {
                    format!("d{child}")
                };
                let rel = parent.join(name);
                fs::create_dir(root.join(&rel)).unwrap();
                dirs.push(rel.clone());
                next.push(rel);
            }
        }
        level = next;
    }
    let (mut read, mut stat) = (Totals::default(), Totals::default());
    for n in 0..TREE_FILES {
        let dir = &dirs[(rng.next() % dirs.len() as u64) as usize];
        let u = rng.unit();
        let size = (512.0 * 2_f64.powf(7.0 * u * u)) as usize;
        let rel = dir.join(format!("f{n:05}.js"));
        let rel = rel.to_str().unwrap();
        let mut file_rng = Rng(mix(n as u64 + 1) | 1);
        let bytes = text(&mut file_rng, rel, size);
        let path = root.join(rel);
        fs::write(&path, &bytes).unwrap();
        let mode = fs::metadata(&path).unwrap().mode() & 0o7777;
        read.add(Totals {
            files: 1,
            bytes: size as u64,
            digest: file_digest(rel, &bytes),
            dirs: 0,
        });
        stat.add(Totals {
            files: 1,
            bytes: size as u64,
            digest: stat_digest(rel, size as u64, mode),
            dirs: 0,
        });
    }
    read.dirs = dirs.len() as u64;
    stat.dirs = dirs.len() as u64;
    fs::write(
        fixture.join("tree.expect"),
        format!("{}\n{}\n", read.render(), stat.render()),
    )
    .unwrap();
    println!(
        "PROBE {{\"phase\":\"prepare_tree\",\"files\":{},\"bytes\":{},\"dirs\":{}}}",
        read.files, read.bytes, read.dirs
    );
}

#[derive(Clone, Copy)]
enum Visit {
    Read,
    Stat,
}

/// Walks `root` with `threads` threads taking directories from one queue.
/// A directory's files are visited in its listing order, then its
/// subdirectories are queued so that one thread walks depth-first in
/// listing order.
fn walk(root: &Path, threads: usize, visit: Visit) -> Totals {
    struct Queue {
        dirs: Vec<PathBuf>,
        active: usize,
    }
    let queue = Mutex::new(Queue {
        dirs: vec![root.to_path_buf()],
        active: 0,
    });
    let changed = Condvar::new();
    let total = Mutex::new(Totals::default());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let mut mine = Totals::default();
                let mut data = Vec::with_capacity(128 * 1024);
                loop {
                    let dir = {
                        let mut state = queue.lock().unwrap();
                        loop {
                            if let Some(dir) = state.dirs.pop() {
                                state.active += 1;
                                break Some(dir);
                            }
                            if state.active == 0 {
                                break None;
                            }
                            // Bounded: woken by every queue change.
                            state = changed
                                .wait_timeout(state, Duration::from_secs(1))
                                .unwrap()
                                .0;
                        }
                    };
                    let Some(dir) = dir else {
                        changed.notify_all();
                        break;
                    };
                    mine.dirs += 1;
                    let mut subdirs = Vec::new();
                    for entry in fs::read_dir(&dir).unwrap() {
                        let entry = entry.unwrap();
                        let path = entry.path();
                        if entry.file_type().unwrap().is_dir() {
                            subdirs.push(path);
                            continue;
                        }
                        let rel = path.strip_prefix(root).unwrap().to_str().unwrap();
                        let (bytes, digest) = match visit {
                            Visit::Read => {
                                data.clear();
                                File::open(&path).unwrap().read_to_end(&mut data).unwrap();
                                (data.len() as u64, file_digest(rel, &data))
                            }
                            Visit::Stat => {
                                let meta = fs::symlink_metadata(&path).unwrap();
                                let size = meta.len();
                                (size, stat_digest(rel, size, meta.mode() & 0o7777))
                            }
                        };
                        mine.add(Totals {
                            files: 1,
                            bytes,
                            digest,
                            dirs: 0,
                        });
                    }
                    let mut state = queue.lock().unwrap();
                    state.dirs.extend(subdirs.into_iter().rev());
                    state.active -= 1;
                    drop(state);
                    changed.notify_all();
                }
                total.lock().unwrap().add(mine);
            });
        }
    });
    total.into_inner().unwrap()
}

/// User and system CPU microseconds of this process so far.
fn cpu_us() -> (u64, u64) {
    let usage = nix::sys::resource::getrusage(nix::sys::resource::UsageWho::RUSAGE_SELF).unwrap();
    let us = |t: nix::sys::time::TimeVal| t.tv_sec() as u64 * 1_000_000 + t.tv_usec() as u64;
    (us(usage.user_time()), us(usage.system_time()))
}

fn small_delta(before: &[u64; small_probe::COUNTERS]) -> String {
    let after = small_probe::snapshot();
    let rows: Vec<String> = (0..small_probe::COUNTERS)
        .map(|i| {
            format!(
                "\"{}\":{}",
                small_probe::COUNTER_NAMES[i],
                after[i] - before[i]
            )
        })
        .collect();
    format!("{{{}}}", rows.join(","))
}
/// Every file below `root`.
fn files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files(&entry.path(), out);
        } else {
            out.push(entry.path());
        }
    }
}
/// Writes the native copy of the tree once (setup, outside any timing).
pub fn native_setup(fixture: &Path) {
    let native = fixture.join("native");
    if native.join("tree").exists() {
        return;
    }
    fs::create_dir_all(&native).unwrap();
    generate(&native, fixture);
}
/// The native reference: the same tree from the volume's ext4, guest-cold.
/// Every file's pages are dropped (`POSIX_FADV_DONTNEED` after one `sync`)
/// and then the guest's dentries and inodes (`drop_caches` 2); residency
/// is summed over all files by `fincore` before the walk.
fn native_phase(root: &Path, phase: &str, threads: usize, expected: Totals) {
    nix::unistd::sync();
    let mut all = Vec::new();
    files(root, &mut all);
    for file in &all {
        let file = File::open(file).unwrap();
        nix::fcntl::posix_fadvise(
            &file,
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_DONTNEED,
        )
        .unwrap();
    }
    let out = std::process::Command::new("fincore")
        .args(["--bytes", "--noheadings", "--output", "RES"])
        .args(&all)
        .output()
        .unwrap();
    let resident: u64 = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| line.trim().parse::<u64>().unwrap_or(u64::MAX / 4))
        .sum();
    drop(all);
    let drop = || fs::write("/proc/sys/vm/drop_caches", "2").is_ok();
    let dropped = drop()
        || (std::process::Command::new("mount")
            .args(["-o", "remount,rw", "/proc/sys"])
            .status()
            .is_ok_and(|status| status.success())
            && drop());
    let cpu0 = cpu_us();
    let started = Instant::now();
    let seen = walk(root, threads, Visit::Read);
    let seconds = started.elapsed().as_secs_f64();
    let cpu1 = cpu_us();
    let correct = seen == expected;
    println!(
        "PROBE {{\"phase\":\"tree_{phase}\",\"threads\":{threads},\"resident_before\":{resident},\"dentries_dropped\":{dropped},\"files\":{},\"bytes\":{},\"dirs\":{},\"correct\":{correct},\"seconds\":{seconds:.4},\"files_s\":{:.0},\"mb_s\":{:.1},\"cpu_user_us\":{},\"cpu_sys_us\":{}}}",
        seen.files,
        seen.bytes,
        seen.dirs,
        seen.files as f64 / seconds,
        seen.bytes as f64 / seconds / 1e6,
        cpu1.0 - cpu0.0,
        cpu1.1 - cpu0.1,
    );
    assert!(correct, "tree_{phase}: {seen:?} against {expected:?}");
}

/// One tree phase in this (child) process: `fixture` holds `tree.expect`,
/// `work` the Store copy the parent made.
pub fn phase(fixture: &Path, work: &Path, phase: &str) {
    let expect = fs::read_to_string(fixture.join("tree.expect")).unwrap();
    let mut lines = expect.lines();
    let (read_expect, stat_expect) = (
        Totals::parse(lines.next().unwrap()),
        Totals::parse(lines.next().unwrap()),
    );
    let (threads, visit, expected) = match phase {
        "read" => (1, Visit::Read, read_expect),
        "read8" => (8, Visit::Read, read_expect),
        "stat" => (1, Visit::Stat, stat_expect),
        "native" | "native8" => {
            let threads = if phase == "native" { 1 } else { 8 };
            let root = fixture.join("native").join("tree");
            return native_phase(&root, phase, threads, read_expect);
        }
        other => panic!("tree phase {other}"),
    };
    let copy = work.join("store").join("store.sqlite");
    let opened = open_store_observed(
        PersistenceConfig::sqlite(&copy)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        super::installed::BINDING,
        super::installed::CURSOR,
        64,
        super::probe::CACHE,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let directory = work.join(format!("tree-{phase}"));
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let harness = Harness::new(
        opened.store.clone(),
        &directory,
        BranchId::from_authority([24; 16]),
    );
    let ready = harness.mount(20);
    evict(&copy);
    let store_resident = resident(&copy);
    let root = Path::new(&ready.directory).join("tree");
    let (stage0, work0, sql0) = (stages::snapshot(), opened.store.work(), readers(&opened));
    let busy0 = stages::busy::snapshot();
    let cache0 = cache_state(&opened);
    let small0 = small_probe::snapshot();
    let cpu0 = cpu_us();
    let rss = resident_reset();
    let started = Instant::now();
    let seen = walk(&root, threads, visit);
    let seconds = started.elapsed().as_secs_f64();
    let cpu1 = cpu_us();
    let hwm = status_kb("VmHWM:");
    let (work1, sql1) = (opened.store.work(), readers(&opened));
    let busy1 = stages::busy::snapshot();
    let split = stage_delta(&stage0);
    let cache1 = cache_state(&opened);
    let small = small_delta(&small0);
    let loops = ready.receipt.loops;
    let receipt = format!("{:?}", harness.unmount(&ready).kernel);
    let correct = seen == expected;
    println!(
        "PROBE {{\"phase\":\"tree_{phase}\",\"threads\":{threads},\"loops\":{loops},\"small\":{small},\"store_resident_before\":{store_resident},\"files\":{},\"bytes\":{},\"dirs\":{},\"correct\":{correct},\"seconds\":{seconds:.4},\"files_s\":{:.0},\"mb_s\":{:.1},\"cpu_user_us\":{},\"cpu_sys_us\":{},\"rss_before_kb\":{rss},\"vm_hwm_kb\":{hwm},\"object_batches\":{},\"object_ids\":{},\"statements\":{},\"returned_rows\":{},\"store_busy_ns\":{},\"store_peak\":{},\"blob_reads\":{},\"blob_read_bytes\":{},\"cache\":[{cache0},{cache1}],\"stages\":{split},\"kernel\":{}}}",
        seen.files,
        seen.bytes,
        seen.dirs,
        seen.files as f64 / seconds,
        seen.bytes as f64 / seconds / 1e6,
        cpu1.0 - cpu0.0,
        cpu1.1 - cpu0.1,
        work1.object_batches - work0.object_batches,
        work1.object_ids - work0.object_ids,
        sql1.statements - sql0.statements,
        sql1.returned_rows - sql0.returned_rows,
        busy1.0 - busy0.0,
        busy1.1,
        sql1.blob_read_calls - sql0.blob_read_calls,
        sql1.blob_read_bytes - sql0.blob_read_bytes,
        opcodes(&receipt)
    );
    assert!(correct, "tree_{phase}: {seen:?} against {expected:?}");
    harness.stop();
    drop(opened);
}

/// Prototype (experiment `smallread`): coherence of a small committed file
/// that a read-only OPEN may store in the kernel page cache (`push`), on one
/// mount, then a second Workspace's view. Not timed.
pub fn coherence_small(harness: &Harness) {
    use std::os::unix::fs::FileExt;
    let push0 = small_probe::snapshot();
    let ready = harness.mount(11);
    let dir = Path::new(&ready.directory).join("tree").join("p00");
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    names.sort();
    let path = names[0].clone();
    let base = fs::read(&path).unwrap();
    let pattern = vec![0x5a_u8; 64];
    // A reader opened before the change; its pages may have been stored.
    let reader = File::open(&path).unwrap();
    let mut first = vec![0; base.len()];
    reader.read_exact_at(&mut first, 0).unwrap();
    let reader_ok = first == base;
    let writer = fs::OpenOptions::new().write(true).open(&path).unwrap();
    writer.write_all_at(&pattern, 0).unwrap();
    drop(writer);
    let mut page = vec![0; base.len()];
    reader.read_exact_at(&mut page, 0).unwrap();
    let mut expected = base.clone();
    expected[..pattern.len()].copy_from_slice(&pattern);
    let reader_after_write = page == expected;
    drop(reader);
    // A new reader after the change: the file is no longer the base content.
    let fresh = fs::read(&path).unwrap();
    let fresh_new = fresh == expected;
    // A truncate by path while a reader is open.
    let second = names[1].clone();
    let reader = File::open(&second).unwrap();
    let mut probe = vec![0; 16];
    reader.read_exact_at(&mut probe, 0).unwrap();
    nix::unistd::truncate(&second, 8).unwrap();
    let past_end = reader.read_at(&mut probe, 8).map_or(-1, |n| n as i64);
    drop(reader);
    let fresh_past_end = File::open(&second)
        .unwrap()
        .read_at(&mut probe, 8)
        .map_or(-1, |n| n as i64);
    let receipt = format!("{:?}", harness.unmount(&ready).kernel);
    // A second Workspace still sees the committed bytes.
    let other = harness.mount(12);
    let rel = path.strip_prefix(&ready.directory).unwrap();
    let other_bytes = fs::read(Path::new(&other.directory).join(rel)).unwrap();
    let other_base = other_bytes == base;
    let other_receipt = format!("{:?}", harness.unmount(&other).kernel);
    let coherent = reader_ok
        && reader_after_write
        && fresh_new
        && past_end == 0
        && fresh_past_end == 0
        && other_base;
    println!(
        "PROBE {{\"phase\":\"coherence_small\",\"file_bytes\":{},\"reader_ok\":{reader_ok},\"reader_after_write\":{reader_after_write},\"fresh_new\":{fresh_new},\"truncate_reader_past_end\":{past_end},\"truncate_fresh_past_end\":{fresh_past_end},\"other_workspace_base\":{other_base},\"coherent\":{coherent},\"small\":{},\"kernel\":{},\"other_kernel\":{}}}",
        base.len(),
        small_delta(&push0),
        opcodes(&receipt),
        opcodes(&other_receipt)
    );
    assert!(coherent, "small-file coherence");
}
