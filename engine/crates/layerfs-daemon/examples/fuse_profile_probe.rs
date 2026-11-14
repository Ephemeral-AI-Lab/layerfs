//! FUSE profile probe (diagnostic, Linux only): per-request time split of a
//! cold large-file read and request counts under a selected kernel profile.
//!
//! `prepare <dir>` builds one sealed Disposable Store with `rand` (500 MiB of
//! non-repeating bytes) and `small/` (2,000 files of 1 KiB), and keeps a
//! native copy of `rand` as the host reference. `run <dir> <work>` clones that
//! Store into `<work>` (`--setup clone`), evicts its pages, and measures one
//! sample of each phase on its own fresh mount, printing `PROBE <json>` lines.
//! The profile comes from `LAYERFS_PROBE_FUSE_WINDOW` and
//! `LAYERFS_PROBE_FUSE_BACKGROUND`, read by the in-process daemon.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../tests/support/installed_store.rs"]
mod installed;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../tests/support/mounted.rs"]
mod mounted;

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("NOT_RUN: Linux only");
}

#[cfg(target_os = "linux")]
fn main() {
    probe::main()
}

#[cfg(target_os = "linux")]
mod probe {
    use super::{installed, mounted::Harness};
    use layerfs_bridge::control::ReadyMount;
    use layerfs_fuse::request::Opcode;
    use layerfs_history::{
        BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
        LayerStackId,
    };
    use layerfs_persistence::{
        PersistenceConfig, SqlWork, SqliteAcquisitionSchema, SqliteHandles,
        SqlitePersistenceProfile,
    };
    use layerfs_project::{init, InitRequest};
    use layerfs_storage::{Storage, StoragePolicy};
    use layerfs_store_service::bootstrap::{open_store_observed, OpenedStore};
    use layerfs_telemetry::timer::{copies, stages, Timing};
    use std::{
        ffi::OsStr,
        fs::{self, File, OpenOptions},
        io::{Read, Write},
        os::fd::AsRawFd,
        os::unix::fs::{FileExt, MetadataExt, PermissionsExt},
        path::Path,
        process::Command,
        time::{Duration, Instant},
    };

    const BIG: u64 = 500 * 1024 * 1024;
    const SMALL_FILES: usize = 2_000;
    const SMALL_BYTES: usize = 1024;
    /// The user buffer of every sequential read and write.
    const BUFFER: usize = 1024 * 1024;
    const SEQ_WRITE: usize = 256 * 1024 * 1024;
    const RANDOM_WRITES: usize = 16_384;
    const CREATES: usize = 2_000;
    /// The daemon's Store object cache (`install_types.rs`).
    const CACHE: usize = 64 * 1024 * 1024;

    /// The fixed non-repeating xorshift64* stream of `rand`.
    struct Stream(u64);
    impl Stream {
        fn new(seed: u64) -> Self {
            Self(seed)
        }
        fn fill(&mut self, out: &mut [u8]) {
            for word in out.chunks_mut(8) {
                let mut state = self.0;
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                self.0 = state;
                let value = state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes();
                word.copy_from_slice(&value[..word.len()]);
            }
        }
    }
    /// Order-sensitive fold of 8-byte words; a cheap equality check.
    fn fold(mut sum: u64, bytes: &[u8]) -> u64 {
        for word in bytes.chunks(8) {
            let mut value = [0; 8];
            value[..word.len()].copy_from_slice(word);
            sum = sum.rotate_left(5) ^ u64::from_le_bytes(value);
        }
        sum
    }

    /// Prototype (`verify-once`): in-cache hash rates on this CPU, before any
    /// measured phase, when `LAYERFS_PROBE_HASHBENCH` is set. Objects are the
    /// cold read's mean size (500 MiB / 27,214), packs its mean pack size;
    /// 64 objects (1.2 MiB) and 8 packs (1.9 MiB) are reused, so this is the
    /// hash's own speed over cache-resident bytes, not a cold claim.
    fn hash_bench() {
        use layerfs_content::object::OBJECT_DOMAIN;
        const OBJECT: usize = 19_265;
        const PACK: usize = 235_000;
        const CALLS: usize = 4_096;
        const PACK_CALLS: usize = 256;
        let mut stream = Stream::new(0x5eed);
        let objects: Vec<Vec<u8>> = (0..64)
            .map(|_| {
                let mut buffer = vec![0; OBJECT];
                stream.fill(&mut buffer);
                buffer
            })
            .collect();
        let packs: Vec<Vec<u8>> = (0..8)
            .map(|_| {
                let mut buffer = vec![0; PACK];
                stream.fill(&mut buffer);
                buffer
            })
            .collect();
        let rate = |seconds: f64, bytes: usize, calls: usize| {
            format!(
                "{{\"us_per_call\":{:.2},\"gb_s\":{:.3}}}",
                seconds * 1e6 / calls as f64,
                (bytes * calls) as f64 / seconds / 1e9
            )
        };
        let mut check = 0_u8;
        let started = Instant::now();
        for index in 0..CALLS {
            let mut hasher = blake3::Hasher::new();
            hasher.update(OBJECT_DOMAIN);
            hasher.update(&objects[index % 64]);
            check ^= hasher.finalize().as_bytes()[0];
        }
        let incremental = rate(started.elapsed().as_secs_f64(), OBJECT, CALLS);
        let started = Instant::now();
        for index in 0..CALLS {
            let mut framed = Vec::with_capacity(OBJECT_DOMAIN.len() + OBJECT);
            framed.extend_from_slice(OBJECT_DOMAIN);
            framed.extend_from_slice(&objects[index % 64]);
            check ^= blake3::hash(&framed).as_bytes()[0];
        }
        let one_shot = rate(started.elapsed().as_secs_f64(), OBJECT, CALLS);
        let started = Instant::now();
        for index in 0..CALLS {
            check ^= blake3::hash(&objects[index % 64]).as_bytes()[0];
        }
        let unprefixed = rate(started.elapsed().as_secs_f64(), OBJECT, CALLS);
        let started = Instant::now();
        for index in 0..CALLS {
            let copy = objects[index % 64].clone();
            check ^= copy[0];
        }
        let copy = rate(started.elapsed().as_secs_f64(), OBJECT, CALLS);
        let started = Instant::now();
        for index in 0..PACK_CALLS {
            check ^= layerfs_storage::port::ObjectKey::for_bytes(&packs[index % 8]).as_bytes()[0];
        }
        let sha = rate(started.elapsed().as_secs_f64(), PACK, PACK_CALLS);
        // The one-shot form must give the incremental identity.
        let mut framed = OBJECT_DOMAIN.to_vec();
        framed.extend_from_slice(&objects[0]);
        let mut hasher = blake3::Hasher::new();
        hasher.update(OBJECT_DOMAIN);
        hasher.update(&objects[0]);
        assert_eq!(blake3::hash(&framed), hasher.finalize());
        println!(
            "PROBE {{\"phase\":\"hash_bench\",\"object_bytes\":{OBJECT},\"pack_bytes\":{PACK},\"blake3_incremental\":{incremental},\"blake3_one_shot_with_copy\":{one_shot},\"blake3_unprefixed\":{unprefixed},\"copy_only\":{copy},\"pack_sha256\":{sha},\"check\":{check}}}"
        );
    }

    pub fn main() {
        let args: Vec<String> = std::env::args().collect();
        match args.get(1).map(String::as_str) {
            Some("prepare") => prepare(Path::new(&args[2])),
            Some("run") => run(Path::new(&args[2]), Path::new(&args[3])),
            _ => panic!("usage: prepare <dir> | run <dir> <work>"),
        }
    }

    fn prepare(dir: &Path) {
        let started = Instant::now();
        fs::create_dir_all(dir).unwrap();
        let source = dir.join("source");
        fs::create_dir(&source).unwrap();
        let mut stream = Stream::new(0x9E37_79B9_7F4A_7C15);
        let mut buffer = vec![0; BUFFER];
        let mut file = File::create(source.join("rand")).unwrap();
        let mut sum = 0;
        for _ in 0..BIG / BUFFER as u64 {
            stream.fill(&mut buffer);
            sum = fold(sum, &buffer);
            file.write_all(&buffer).unwrap();
        }
        drop(file);
        fs::create_dir(source.join("small")).unwrap();
        for n in 0..SMALL_FILES {
            let mut bytes = vec![0; SMALL_BYTES];
            Stream::new(n as u64 + 1).fill(&mut bytes);
            fs::write(source.join("small").join(format!("f{n:05}")), bytes).unwrap();
        }
        let store = dir.join("store");
        fs::create_dir(&store).unwrap();
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
        let sealed = store.join("sealed.sqlite");
        let config = PersistenceConfig::sqlite(&sealed)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
        let handles = SqliteHandles::create(
            config,
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: installed::BINDING.to_vec(),
                cursor_key: installed::CURSOR,
                incarnation: 1,
            },
        )
        .unwrap();
        let flags: Vec<String> = handles
            .profile()
            .compile_options
            .iter()
            .filter(|o| o.contains("MMAP") || o.contains("OVERFLOW") || o.contains("CACHE"))
            .map(|o| format!("\"{o}\""))
            .collect();
        println!(
            "PROBE {{\"phase\":\"prepare_profile\",\"page_size\":{},\"sqlite\":\"{}\",\"compile_options\":[{}]}}",
            handles.profile().page_size,
            handles.profile().sqlite_version,
            flags.join(",")
        );
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let initialized = Timing::disabled("probe.init", |timing| {
            init(
                &storage,
                &handles.history,
                InitRequest {
                    source: &source,
                    acquisition: &handles.acquisition,
                    stack: LayerStackId::from_authority([23; 16]),
                    name: HistoryName::new("installed").unwrap(),
                    scope_seed: [31; 32],
                    deadline: Instant::now() + Duration::from_secs(100),
                },
                timing,
            )
        })
        .0
        .unwrap();
        let branch = BranchId::from_authority([24; 16]);
        handles
            .history
            .fork(&ForkRequest {
                stack: initialized.stack.id,
                branch,
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(initialized.stack.head_layer),
            })
            .unwrap();
        drop(storage);
        let sealed = handles.seal().unwrap();
        fs::rename(&sealed.path, store.join("store.sqlite")).unwrap();
        // The native reference keeps `rand`; the rest of the source goes.
        fs::rename(source.join("rand"), dir.join("native-rand")).unwrap();
        fs::remove_dir_all(&source).unwrap();
        fs::write(dir.join("rand.fold"), format!("{sum}")).unwrap();
        println!(
            "PROBE {{\"phase\":\"prepare\",\"entries\":{},\"store_bytes\":{},\"seconds\":{:.3}}}",
            initialized.entries,
            fs::metadata(store.join("store.sqlite")).unwrap().len(),
            started.elapsed().as_secs_f64()
        );
    }

    /// Writes dirty pages back (setup only) and drops a file's page cache.
    fn evict(path: &Path) {
        let file = File::open(path).unwrap();
        file.sync_all().unwrap();
        nix::fcntl::posix_fadvise(
            &file,
            0,
            0,
            nix::fcntl::PosixFadviseAdvice::POSIX_FADV_DONTNEED,
        )
        .unwrap();
    }
    /// Resident bytes of a file, by `fincore`.
    fn resident(path: &Path) -> u64 {
        let out = Command::new("fincore")
            .args(["--bytes", "--noheadings", "--output", "RES"])
            .arg(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(u64::MAX)
    }
    fn status_kb(field: &str) -> u64 {
        fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix(field))
            .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
            .unwrap()
    }
    /// `/proc/self/io` counters: read syscalls, bytes read by them, storage bytes.
    fn io() -> [u64; 3] {
        let text = fs::read_to_string("/proc/self/io").unwrap_or_default();
        let get = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|rest| rest.trim().parse().ok())
                .unwrap_or(0)
        };
        [get("syscr:"), get("rchar:"), get("read_bytes:")]
    }
    /// The Store file's page size from its header (1 means 65,536).
    fn page_size(path: &Path) -> u64 {
        let mut header = [0; 18];
        File::open(path).unwrap().read_exact(&mut header).unwrap();
        match u16::from_be_bytes([header[16], header[17]]) {
            1 => 65_536,
            n => u64::from(n),
        }
    }
    fn resident_reset() -> u64 {
        fs::write("/proc/self/clear_refs", "5").unwrap();
        status_kb("VmRSS:")
    }
    fn readers(opened: &OpenedStore) -> SqlWork {
        let mut total = SqlWork::default();
        for reader in opened.diagnostics().unwrap().readers {
            total.accumulate(&reader);
        }
        total
    }
    /// One bracketed array or number of the Debug-rendered `OpcodeWork`.
    fn array(receipt: &str, key: &str) -> Vec<u64> {
        let key = format!("{key}: [");
        let at = receipt.find(&key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
        let end = at + receipt[at..].find(']').unwrap();
        receipt[at..end]
            .split(", ")
            .map(|n| n.parse().unwrap())
            .collect()
    }
    fn field(receipt: &str, name: &str) -> u64 {
        let key = format!(" {name}: ");
        let at = receipt.find(&key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
        receipt[at..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap()
    }
    /// The nonzero opcodes of a drain receipt as `{name:[count,ns]}`.
    fn opcodes(receipt: &str) -> String {
        let (counts, nanos) = (array(receipt, "opcodes"), array(receipt, "nanos"));
        let rows: Vec<String> = counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(i, count)| format!("\"{}\":[{count},{}]", Opcode::NAMES[i], nanos[i]))
            .collect();
        let (counts, nanos) = (
            array(receipt, "phase_counts"),
            array(receipt, "phase_nanos"),
        );
        let phases: Vec<String> = counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(i, count)| {
                format!(
                    "\"{}\":[{count},{}]",
                    layerfs_fuse::request::Phase::NAMES[i],
                    nanos[i]
                )
            })
            .collect();
        format!(
            "{{\"opcodes\":{{{}}},\"phases\":{{{}}},\"forget_units\":{},\"served\":{},\"inline\":{},\"refused\":{},\"largest_read\":{},\"largest_write\":{},\"served_ns\":{}}}",
            rows.join(","),
            phases.join(","),
            field(receipt, "forget_units"),
            field(receipt, "served"),
            field(receipt, "inline"),
            field(receipt, "refused"),
            field(receipt, "largest_read"),
            field(receipt, "largest_write"),
            field(receipt, "served_ns"),
        )
    }
    fn stage_delta(before: &([u64; stages::STAGES], [u64; stages::STAGES])) -> String {
        let after = stages::snapshot();
        let rows: Vec<String> = (0..stages::STAGES)
            .map(|i| {
                format!(
                    "\"{}\":[{},{}]",
                    stages::STAGE_NAMES[i],
                    after.0[i] - before.0[i],
                    after.1[i] - before.1[i]
                )
            })
            .collect();
        format!("{{{}}}", rows.join(","))
    }
    /// The object cache's charge, entries, hits, misses, evictions and
    /// authenticated bytes.
    fn cache_state(opened: &OpenedStore) -> String {
        let w = opened.store.cache_work().unwrap();
        format!(
            "{{\"charged\":{},\"objects\":{},\"hits\":{},\"misses\":{},\"evictions\":{},\"authenticated\":{}}}",
            w.charged_cache_bytes,
            w.cached_objects,
            w.cache_hits,
            w.cache_misses,
            w.evictions,
            w.authenticated_bytes
        )
    }
    fn copy_delta(before: &([u64; copies::SITES], [u64; copies::SITES])) -> String {
        let after = copies::snapshot();
        let rows: Vec<String> = (0..copies::SITES)
            .map(|i| {
                format!(
                    "\"{}\":[{},{}]",
                    copies::SITE_NAMES[i],
                    after.0[i] - before.0[i],
                    after.1[i] - before.1[i]
                )
            })
            .collect();
        format!("{{{}}}", rows.join(","))
    }
    /// Raises the mount's bdi readahead to the window; returns (before, after) KiB.
    fn readahead(directory: &str, window: u32) -> (String, String) {
        let dev = fs::metadata(directory).unwrap().dev();
        let path = format!(
            "/sys/class/bdi/{}:{}/read_ahead_kb",
            nix::sys::stat::major(dev),
            nix::sys::stat::minor(dev)
        );
        let read = || {
            fs::read_to_string(&path)
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let before = read();
        if window > 128 * 1024 {
            fs::write(&path, format!("{}", window / 1024)).unwrap();
        }
        (before, read())
    }
    fn unmount(harness: &Harness, ready: &ReadyMount) -> String {
        format!("{:?}", harness.unmount(ready).kernel)
    }
    /// Prototype (experiment `passthrough`): the decoded-file cache counters
    /// since `before`, as JSON.
    fn decoded_delta(before: &[u64; 10]) -> String {
        let after = layerfs_mount_ports::decoded::snapshot();
        let rows: Vec<String> = (0..10)
            .map(|i| {
                format!(
                    "\"{}\":{}",
                    layerfs_mount_ports::decoded::NAMES[i],
                    after[i] - before[i]
                )
            })
            .collect();
        format!("{{{}}}", rows.join(","))
    }
    /// The plain files of the decoded-file cache.
    fn cached_files(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut files: Vec<_> = fs::read_dir(dir)
            .map(|entries| entries.filter_map(|e| Some(e.ok()?.path())).collect())
            .unwrap_or_default();
        files.sort();
        files
    }
    /// (files, logical bytes, allocated bytes, resident bytes) of the cache.
    fn cache_usage(dir: &Path) -> (usize, u64, u64, u64) {
        let files = cached_files(dir);
        let (mut logical, mut allocated, mut resident_bytes) = (0, 0, 0);
        for file in &files {
            let meta = fs::metadata(file).unwrap();
            logical += meta.len();
            allocated += meta.blocks() * 512;
            resident_bytes += resident(file);
        }
        (files.len(), logical, allocated, resident_bytes)
    }
    /// Reads a whole file in `BUFFER` reads: (bytes, fold, seconds).
    fn read_all(path: &Path) -> (u64, u64, f64) {
        let mut buffer = vec![0; BUFFER];
        let started = Instant::now();
        let mut file = File::open(path).unwrap();
        let (mut total, mut sum) = (0_u64, 0);
        loop {
            let n = file.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            sum = fold(sum, &buffer[..n]);
            total += n as u64;
        }
        drop(file);
        (total, sum, started.elapsed().as_secs_f64())
    }
    /// One re-read of `rand` on a fresh mount; `cold` evicts the Store and
    /// every cached file first and records their residency.
    #[allow(clippy::too_many_arguments)]
    fn reread(
        harness: &Harness,
        opened: &OpenedStore,
        copy: &Path,
        decoded: &Path,
        tag: u8,
        phase: &str,
        cold: bool,
        expected: u64,
    ) {
        let ready = harness.mount(tag);
        if cold {
            evict(copy);
            for file in cached_files(decoded) {
                evict(&file);
            }
        }
        let store_resident = resident(copy);
        let (files, logical, allocated, cache_resident) = cache_usage(decoded);
        let path = Path::new(&ready.directory).join("rand");
        let (stage0, work0, dec0) = (
            stages::snapshot(),
            opened.store.work(),
            layerfs_mount_ports::decoded::snapshot(),
        );
        let rss = resident_reset();
        let (total, sum, seconds) = read_all(&path);
        let hwm = status_kb("VmHWM:");
        let work1 = opened.store.work();
        let split = stage_delta(&stage0);
        let dec = decoded_delta(&dec0);
        let correct = (total, sum) == (BIG, expected);
        let receipt = unmount(harness, &ready);
        println!(
            "PROBE {{\"phase\":\"{phase}\",\"cold\":{cold},\"store_resident_before\":{store_resident},\"cache_files\":{files},\"cache_bytes\":{logical},\"cache_allocated\":{allocated},\"cache_resident_before\":{cache_resident},\"bytes\":{total},\"correct\":{correct},\"seconds\":{seconds:.4},\"mb_s\":{:.1},\"rss_before_kb\":{rss},\"vm_hwm_kb\":{hwm},\"object_ids\":{},\"decoded\":{dec},\"stages\":{split},\"kernel\":{}}}",
            total as f64 / seconds / 1e6,
            work1.object_ids - work0.object_ids,
            opcodes(&receipt)
        );
        assert!(correct, "{phase} bytes");
    }
    /// Coherence of the cache and passthrough with a Workspace that writes
    /// the cached file, on one mount, then a second Workspace's view.
    fn coherence(harness: &Harness) {
        let mut expected = vec![0; BUFFER];
        Stream::new(0x9E37_79B9_7F4A_7C15).fill(&mut expected);
        let mut pattern = vec![0; 4096];
        Stream::new(17).fill(&mut pattern);
        let classify = |bytes: &[u8]| {
            if bytes == pattern.as_slice() {
                "new"
            } else if bytes == &expected[..4096] {
                "base"
            } else {
                "other"
            }
        };
        let dec0 = layerfs_mount_ports::decoded::snapshot();
        let ready = harness.mount(8);
        let path = Path::new(&ready.directory).join("rand");
        let reader = File::open(&path).unwrap();
        let mut first = vec![0; BUFFER];
        reader.read_exact_at(&mut first, 0).unwrap();
        let reader_ok = first == expected;
        let writer = OpenOptions::new().write(true).open(&path);
        let writer_open = match &writer {
            Ok(_) => "ok".to_owned(),
            Err(error) => format!("{error}"),
        };
        let mut reader_after_write = "not_written";
        if let Ok(writer) = &writer {
            writer.write_all_at(&pattern, 0).unwrap();
            let mut page = vec![0; 4096];
            reader.read_exact_at(&mut page, 0).unwrap();
            reader_after_write = classify(&page);
        }
        let wrote_first = writer.is_ok();
        drop(writer);
        drop(reader);
        let mut writer_retry = "not_needed".to_owned();
        if !wrote_first {
            match OpenOptions::new().write(true).open(&path) {
                Ok(writer) => {
                    writer.write_all_at(&pattern, 0).unwrap();
                    writer_retry = "ok".into();
                }
                Err(error) => writer_retry = format!("{error}"),
            }
        }
        let fresh = File::open(&path).unwrap();
        let mut page = vec![0; BUFFER];
        fresh.read_exact_at(&mut page, 0).unwrap();
        let fresh_head = classify(&page[..4096]);
        let fresh_tail_base = page[4096..] == expected[4096..];
        drop(fresh);
        let receipt = unmount(harness, &ready);
        // A second Workspace still sees the committed bytes.
        let other = harness.mount(9);
        let other_file = File::open(Path::new(&other.directory).join("rand")).unwrap();
        let mut page = vec![0; 4096];
        other_file.read_exact_at(&mut page, 0).unwrap();
        let other_head = classify(&page);
        drop(other_file);
        let other_receipt = unmount(harness, &other);
        let coherent = reader_ok
            && fresh_head == "new"
            && fresh_tail_base
            && other_head == "base"
            && reader_after_write != "other";
        println!(
            "PROBE {{\"phase\":\"coherence\",\"reader_ok\":{reader_ok},\"writer_open\":\"{writer_open}\",\"reader_after_write\":\"{reader_after_write}\",\"writer_retry\":\"{writer_retry}\",\"fresh_head\":\"{fresh_head}\",\"fresh_tail_base\":{fresh_tail_base},\"other_workspace_head\":\"{other_head}\",\"coherent\":{coherent},\"decoded\":{},\"kernel\":{},\"other_kernel\":{}}}",
            decoded_delta(&dec0),
            opcodes(&receipt),
            opcodes(&other_receipt)
        );

        // A change that needs no open: truncate by path while a reader is
        // open. POSIX: the reader then reads nothing past the new end.
        let dec0 = layerfs_mount_ports::decoded::snapshot();
        let ready = harness.mount(10);
        let path = Path::new(&ready.directory).join("rand");
        let reader = File::open(&path).unwrap();
        let mut page = vec![0; 4096];
        reader.read_exact_at(&mut page, 8192).unwrap();
        let truncated = match nix::unistd::truncate(&path, 4096) {
            Ok(()) => "ok".to_owned(),
            Err(error) => format!("{error}"),
        };
        let length_after = fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX);
        let reader_past_end = reader.read_at(&mut page, 8192).map_or(-1, |n| n as i64);
        let reader_stale = reader_past_end > 0 && page == expected[8192..12288];
        drop(reader);
        let fresh = File::open(&path).unwrap();
        let fresh_past_end = fresh.read_at(&mut page, 8192).map_or(-1, |n| n as i64);
        drop(fresh);
        let receipt = unmount(harness, &ready);
        println!(
            "PROBE {{\"phase\":\"coherence_truncate\",\"truncate\":\"{truncated}\",\"length_after\":{length_after},\"reader_past_end\":{reader_past_end},\"reader_stale_base\":{reader_stale},\"fresh_past_end\":{fresh_past_end},\"posix\":{},\"decoded\":{},\"kernel\":{}}}",
            reader_past_end == 0 && fresh_past_end == 0,
            decoded_delta(&dec0),
            opcodes(&receipt)
        );
    }

    /// FNV-1a over a listing's output: equal listings compare equal across arms.
    fn fnv(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
    }
    /// Runs one listing command, its binary warmed outside the timing.
    fn listed(program: &str, args: &[&OsStr]) -> (f64, Vec<u8>) {
        Command::new(program).arg("--version").output().unwrap();
        let started = Instant::now();
        let out = Command::new(program).args(args).output().unwrap();
        let seconds = started.elapsed().as_secs_f64();
        assert!(
            out.status.success(),
            "{program}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        (seconds, out.stdout)
    }
    fn summary(out: &[u8]) -> String {
        let text = String::from_utf8_lossy(out);
        let lines: Vec<&str> = text.lines().collect();
        let head: Vec<&&str> = lines.iter().take(3).collect();
        let tail: Vec<&&str> = lines.iter().rev().take(2).collect();
        format!(
            "\"lines\":{},\"hash\":\"{:016x}\",\"head\":{head:?},\"tail\":{tail:?}",
            lines.len(),
            fnv(out)
        )
    }
    /// Prototype custody check (`LAYERFS_PROBE_FORGET_CHECK=1`, after the
    /// timing): the guest drops its unused dentries and inodes, so the kernel
    /// forgets every lookup reference it holds on this mount. The engine's
    /// kernel lookup rows must then fall to the root's implicit one: a row
    /// left over is a reference the kernel never held, and one the kernel
    /// held that the engine did not fails FORGET as an underflow.
    fn forget_check(harness: &Harness, ready: &ReadyMount) -> String {
        if std::env::var("LAYERFS_PROBE_FORGET_CHECK").as_deref() != Ok("1") {
            return "null".into();
        }
        let count = || harness.engine_resources(ready.token).open.kernel_entries;
        let before = count();
        let started = Instant::now();
        let drop = || fs::write("/proc/sys/vm/drop_caches", "2").is_ok();
        // The container's /proc/sys is read-only until remounted.
        let dropped = drop()
            || (Command::new("mount")
                .args(["-o", "remount,rw", "/proc/sys"])
                .status()
                .is_ok_and(|status| status.success())
                && drop());
        let mut after = count();
        while after > 1 && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
            after = count();
        }
        format!(
            "{{\"dropped\":{dropped},\"kernel_entries_before\":{before},\"kernel_entries_after\":{after},\"waited_ms\":{}}}",
            started.elapsed().as_millis()
        )
    }
    /// One `getdents64` call into a buffer of `size` bytes: (ino, off, name).
    fn getdents(fd: i32, size: usize) -> Vec<(u64, i64, String)> {
        let mut buffer = vec![0_u8; size];
        // SAFETY: the buffer is valid for `size` bytes for the call.
        let n = unsafe {
            nix::libc::syscall(
                nix::libc::SYS_getdents64,
                fd,
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        };
        assert!(n >= 0, "getdents64: {}", std::io::Error::last_os_error());
        let (mut at, mut out) = (0, Vec::new());
        while at < n as usize {
            let ino = u64::from_ne_bytes(buffer[at..at + 8].try_into().unwrap());
            let off = i64::from_ne_bytes(buffer[at + 8..at + 16].try_into().unwrap());
            let reclen = u16::from_ne_bytes(buffer[at + 16..at + 18].try_into().unwrap());
            let name = &buffer[at + 19..at + reclen as usize];
            let name = &name[..name.iter().position(|b| *b == 0).unwrap()];
            out.push((ino, off, String::from_utf8_lossy(name).into_owned()));
            at += reclen as usize;
        }
        out
    }
    /// One listing command over `small/` on its own fresh mount.
    fn directory_phase(harness: &Harness, tag: u8, phase: &str, program: &str, args: &[&str]) {
        let ready = harness.mount(tag);
        let small = Path::new(&ready.directory).join("small");
        // `@` stands for the directory; without one it goes last.
        let mut argv: Vec<&OsStr> = args
            .iter()
            .map(|arg| match *arg {
                "@" => small.as_os_str(),
                arg => OsStr::new(arg),
            })
            .collect();
        if !args.contains(&"@") {
            argv.push(small.as_os_str());
        }
        let (seconds, out) = listed(program, &argv);
        let forget = forget_check(harness, &ready);
        let receipt = unmount(harness, &ready);
        println!(
            "PROBE {{\"phase\":\"{phase}\",\"entries\":{SMALL_FILES},\"seconds\":{seconds:.4},{},\"forget\":{forget},\"kernel\":{}}}",
            summary(&out),
            opcodes(&receipt)
        );
    }

    fn run(dir: &Path, work: &Path) {
        let window: u32 = std::env::var("LAYERFS_PROBE_FUSE_WINDOW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(131_072);
        fs::create_dir(work).unwrap();
        let store = work.join("store");
        fs::create_dir(&store).unwrap();
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
        let copy = store.join("store.sqlite");
        fs::copy(dir.join("store").join("store.sqlite"), &copy).unwrap();
        let expected: u64 = fs::read_to_string(dir.join("rand.fold"))
            .unwrap()
            .parse()
            .unwrap();
        let load = fs::read_to_string("/proc/loadavg").unwrap();
        // Prototype (experiment `passthrough`): the decoded-file cache lives
        // under this daemon's work directory; set before the daemon starts.
        let decoded = work.join("decoded");
        std::env::set_var("LAYERFS_PROBE_DECODED_DIR", &decoded);
        println!(
            "PROBE {{\"phase\":\"switches\",\"read\":\"{}\",\"decoded\":\"{}\",\"passthrough\":\"{}\"}}",
            std::env::var("LAYERFS_PROBE_READ").unwrap_or_default(),
            std::env::var("LAYERFS_PROBE_DECODED").unwrap_or_default(),
            std::env::var("LAYERFS_PROBE_PASSTHROUGH").unwrap_or_default(),
        );
        println!(
            "PROBE {{\"phase\":\"setup\",\"window\":{window},\"background\":\"{}\",\"loadavg\":\"{}\"}}",
            std::env::var("LAYERFS_PROBE_FUSE_BACKGROUND").unwrap_or_else(|_| "1".into()),
            load.trim()
        );
        if std::env::var_os("LAYERFS_PROBE_HASHBENCH").is_some() {
            hash_bench();
        }

        // Native reference: the same bytes from the volume's ext4, cold.
        let native = dir.join("native-rand");
        evict(&native);
        let cold = resident(&native);
        let started = Instant::now();
        let mut file = File::open(&native).unwrap();
        let mut buffer = vec![0; BUFFER];
        let (mut total, mut sum) = (0_u64, 0);
        loop {
            let n = file.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            sum = fold(sum, &buffer[..n]);
            total += n as u64;
        }
        let seconds = started.elapsed().as_secs_f64();
        assert_eq!((total, sum), (BIG, expected));
        println!(
            "PROBE {{\"phase\":\"native_cold_read\",\"resident_before\":{cold},\"bytes\":{total},\"seconds\":{seconds:.4},\"mb_s\":{:.1}}}",
            total as f64 / seconds / 1e6
        );
        drop(file);

        let opened = open_store_observed(
            PersistenceConfig::sqlite(&copy)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
            installed::BINDING,
            installed::CURSOR,
            64,
            CACHE,
            layerfs_storage::ReservationBlocks::default(),
        )
        .unwrap();
        let branch = BranchId::from_authority([24; 16]);
        let harness = Harness::new(opened.store.clone(), work, branch);

        // 1. Cold sequential read of `rand` on a fresh mount.
        let ready = harness.mount(1);
        let profile = format!(
            "{{\"selected\":{},\"max_write\":{},\"max_readahead\":{},\"max_background\":{},\"congestion\":{},\"loops\":{}}}",
            ready.receipt.selected,
            ready.receipt.max_write,
            ready.receipt.max_readahead,
            ready.receipt.max_background,
            ready.receipt.congestion_threshold,
            ready.receipt.loops
        );
        let (ra_before, ra_after) = readahead(&ready.directory, window);
        evict(&copy);
        let store_resident = resident(&copy);
        let path = Path::new(&ready.directory).join("rand");
        let (stage0, work0, sql0) = (stages::snapshot(), opened.store.work(), readers(&opened));
        let (busy0, session0) = (stages::busy::snapshot(), opened.store.read_work());
        let dec0 = layerfs_mount_ports::decoded::snapshot();
        let io0 = io();
        let copies0 = copies::snapshot();
        let cache0 = cache_state(&opened);
        let rss = resident_reset();
        let started = Instant::now();
        let mut file = File::open(&path).unwrap();
        let (mut total, mut sum) = (0_u64, 0);
        loop {
            let n = file.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            sum = fold(sum, &buffer[..n]);
            total += n as u64;
        }
        drop(file);
        let seconds = started.elapsed().as_secs_f64();
        let hwm = status_kb("VmHWM:");
        let (rss_file, rss_anon) = (status_kb("RssFile:"), status_kb("RssAnon:"));
        let io1 = io();
        let (work1, sql1) = (opened.store.work(), readers(&opened));
        let (busy1, session1) = (stages::busy::snapshot(), opened.store.read_work());
        let lanes = format!(
            "\"sessions\":\"{}\",\"store_busy_ns\":{},\"store_peak\":{},\"session_requests\":{},\"session_wait_ns\":{},\"readers\":{}",
            std::env::var("LAYERFS_PROBE_READ_SESSIONS").unwrap_or_else(|_| "1".into()),
            busy1.0 - busy0.0,
            busy1.1,
            session1.requests - session0.requests,
            session1.lock_wait_ns - session0.lock_wait_ns,
            opened.store.reuse_work().readers_opened,
        );
        let split = stage_delta(&stage0);
        let dec = decoded_delta(&dec0);
        let copied = copy_delta(&copies0);
        let cache1 = cache_state(&opened);
        assert_eq!((total, sum), (BIG, expected), "cold read bytes");
        let receipt = unmount(&harness, &ready);
        let (files, logical, allocated, _) = cache_usage(&decoded);
        let sql = format!(
            "{{\"vm_steps\":{},\"statement_ns\":{},\"read_snapshots\":{},\"read_snapshot_ns\":{},\"blob_opens\":{},\"blob_reads\":{},\"blob_read_bytes\":{},\"blob_read_ns\":{},\"phases\":[{}]}}",
            sql1.vm_steps - sql0.vm_steps,
            sql1.statement_ns - sql0.statement_ns,
            sql1.read_snapshots - sql0.read_snapshots,
            sql1.read_snapshot_ns - sql0.read_snapshot_ns,
            sql1.blob_open_calls - sql0.blob_open_calls,
            sql1.blob_read_calls - sql0.blob_read_calls,
            sql1.blob_read_bytes - sql0.blob_read_bytes,
            sql1.blob_read_ns - sql0.blob_read_ns,
            // Statement phases: prepare, query, next, map, cursor drop,
            // status, statement drop; each [calls, ns].
            sql1.statement_phases
                .iter()
                .zip(sql0.statement_phases)
                .map(|(a, b)| format!("[{},{}]", a.calls - b.calls, a.wall_ns - b.wall_ns))
                .collect::<Vec<_>>()
                .join(","),
        );
        println!(
            "PROBE {{\"phase\":\"cold_read\",\"decoded\":{dec},\"cache_files\":{files},\"cache_bytes\":{logical},\"cache_allocated\":{allocated},\"profile\":{profile},\"read_ahead_kb\":[\"{ra_before}\",\"{ra_after}\"],\"store_page_size\":{},\"store_bytes\":{},\"store_resident_before\":{store_resident},\"bytes\":{total},\"seconds\":{seconds:.4},\"mb_s\":{:.1},\"rss_before_kb\":{rss},\"vm_hwm_kb\":{hwm},\"rss_file_kb\":{rss_file},\"rss_anon_kb\":{rss_anon},\"read_syscalls\":{},\"read_syscall_bytes\":{},\"storage_read_bytes\":{},\"object_batches\":{},\"object_ids\":{},\"statements\":{},\"returned_rows\":{},{lanes},\"sql\":{sql},\"stages\":{split},\"copies\":{copied},\"copy_switches\":\"{:?}\",\"cache\":[{cache0},{cache1}],\"kernel\":{}}}",
            page_size(&copy),
            fs::metadata(&copy).unwrap().len(),
            total as f64 / seconds / 1e6,
            io1[0] - io0[0],
            io1[1] - io0[1],
            io1[2] - io0[2],
            work1.object_batches - work0.object_batches,
            work1.object_ids - work0.object_ids,
            sql1.statements - sql0.statements,
            sql1.returned_rows - sql0.returned_rows,
            copies::switches(),
            opcodes(&receipt)
        );

        // 1b. Prototype (experiment `passthrough`): the per-tool-call re-read
        // on a fresh mount, cold (Store and cached files evicted) then warm.
        reread(
            &harness,
            &opened,
            &copy,
            &decoded,
            6,
            "reread_cold",
            true,
            expected,
        );
        reread(
            &harness,
            &opened,
            &copy,
            &decoded,
            7,
            "reread_warm",
            false,
            expected,
        );
        coherence(&harness);

        // 2. Sequential write of a new 256 MiB file, 1 MiB writes.
        let mut pool = vec![0; 32 * 1024 * 1024];
        Stream::new(7).fill(&mut pool);
        let ready = harness.mount(2);
        readahead(&ready.directory, window);
        let rss = resident_reset();
        let started = Instant::now();
        let mut file = File::create(Path::new(&ready.directory).join("seq.bin")).unwrap();
        for n in 0..SEQ_WRITE / BUFFER {
            // A different 1 MiB slice each time within the pool's 32 slices,
            // shifted per pass so no two writes carry the same bytes.
            let at = (n % 32) * BUFFER;
            let mut chunk = pool[at..at + BUFFER].to_vec();
            chunk[..8].copy_from_slice(&(n as u64).to_le_bytes());
            file.write_all(&chunk).unwrap();
        }
        drop(file);
        let seconds = started.elapsed().as_secs_f64();
        let hwm = status_kb("VmHWM:");
        let receipt = unmount(&harness, &ready);
        println!(
            "PROBE {{\"phase\":\"seq_write\",\"bytes\":{SEQ_WRITE},\"seconds\":{seconds:.4},\"mb_s\":{:.1},\"rss_before_kb\":{rss},\"vm_hwm_kb\":{hwm},\"kernel\":{}}}",
            SEQ_WRITE as f64 / seconds / 1e6,
            opcodes(&receipt)
        );
        drop(pool);

        // 3. 4 KiB random writes into the committed 500 MiB file.
        let ready = harness.mount(3);
        let rss = resident_reset();
        let mut block = vec![0; 4096];
        Stream::new(11).fill(&mut block);
        let mut lcg = 0x1234_5678_u64;
        let started = Instant::now();
        let file = OpenOptions::new()
            .write(true)
            .open(Path::new(&ready.directory).join("rand"))
            .unwrap();
        for _ in 0..RANDOM_WRITES {
            lcg = lcg.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let at = ((lcg >> 33) % (BIG / 4096)) * 4096;
            file.write_all_at(&block, at).unwrap();
        }
        drop(file);
        let seconds = started.elapsed().as_secs_f64();
        let hwm = status_kb("VmHWM:");
        let receipt = unmount(&harness, &ready);
        let bytes = RANDOM_WRITES * 4096;
        println!(
            "PROBE {{\"phase\":\"random_write_4k\",\"writes\":{RANDOM_WRITES},\"bytes\":{bytes},\"seconds\":{seconds:.4},\"mb_s\":{:.1},\"rss_before_kb\":{rss},\"vm_hwm_kb\":{hwm},\"kernel\":{}}}",
            bytes as f64 / seconds / 1e6,
            opcodes(&receipt)
        );

        // 4. Small-file creates: 2,000 files of 1 KiB in a new directory.
        let ready = harness.mount(4);
        let new = Path::new(&ready.directory).join("new");
        let mut small = vec![0; SMALL_BYTES];
        Stream::new(13).fill(&mut small);
        let started = Instant::now();
        fs::create_dir(&new).unwrap();
        for n in 0..CREATES {
            let mut file = File::create(new.join(format!("c{n:05}"))).unwrap();
            file.write_all(&small).unwrap();
        }
        let seconds = started.elapsed().as_secs_f64();
        let receipt = unmount(&harness, &ready);
        println!(
            "PROBE {{\"phase\":\"small_create\",\"files\":{CREATES},\"seconds\":{seconds:.4},\"per_file_us\":{:.1},\"kernel\":{}}}",
            seconds * 1e6 / CREATES as f64,
            opcodes(&receipt)
        );

        // 5. `ls -l` of the committed 2,000-file directory on a fresh mount.
        directory_phase(&harness, 5, "ls_l", "ls", &["-l", "--time-style=full-iso"]);
        // 6. Plain `ls`: names only, no attribute is needed.
        directory_phase(&harness, 6, "ls", "ls", &[]);
        // 7. A stat walk: `find` must stat every file for its size and mode.
        directory_phase(
            &harness,
            7,
            "find_stat",
            "find",
            &["@", "-type", "f", "-printf", "%s %m %n %f\n"],
        );

        // 8. A repeated offset: a seek back to a handed-out offset relists
        //    the same names, which reuses the published reply.
        let ready = harness.mount(8);
        let small = Path::new(&ready.directory).join("small");
        let fallback0 = layerfs_fuse::request::fallback_entries();
        let started = Instant::now();
        let directory = File::open(&small).unwrap();
        let raw = directory.as_raw_fd();
        let first = getdents(raw, 4096);
        let resume = first.last().unwrap().1;
        let second = getdents(raw, 4096);
        // SAFETY: plain lseek on a descriptor this function owns.
        let at = unsafe { nix::libc::lseek(raw, resume, nix::libc::SEEK_SET) };
        assert_eq!(at, resume);
        let third = getdents(raw, 4096);
        drop(directory);
        let seconds = started.elapsed().as_secs_f64();
        let same = second == third;
        let nonzero = [&first, &second, &third]
            .iter()
            .all(|batch| batch.iter().all(|entry| entry.0 != 0));
        let fallback = layerfs_fuse::request::fallback_entries() - fallback0;
        let forget = forget_check(&harness, &ready);
        let receipt = unmount(&harness, &ready);
        println!(
            "PROBE {{\"phase\":\"seek_relist\",\"seconds\":{seconds:.4},\"counts\":[{},{},{}],\"relist_equal\":{same},\"inos_nonzero\":{nonzero},\"fallback_entries\":{fallback},\"forget\":{forget},\"kernel\":{}}}",
            first.len(),
            second.len(),
            third.len(),
            opcodes(&receipt)
        );
        assert!(same && nonzero, "relisted names differ");

        // 9. `ls -l` of a directory changed in the Workspace first: creates,
        //    renames, a rename over an existing name, deletes, a directory
        //    and a symlink. The listing must show exactly the expected names.
        let ready = harness.mount(9);
        let small = Path::new(&ready.directory).join("small");
        let mut expected: Vec<String> = (0..SMALL_FILES).map(|n| format!("f{n:05}")).collect();
        for n in 0..5_usize {
            fs::write(small.join(format!("new{n}")), vec![n as u8; 100 * (n + 1)]).unwrap();
            expected.push(format!("new{n}"));
            let (old, new) = (format!("f{:05}", 10 + n), format!("r{:05}", 10 + n));
            fs::rename(small.join(&old), small.join(&new)).unwrap();
            expected.retain(|name| *name != old);
            expected.push(new);
            let gone = format!("f{:05}", 20 + n);
            fs::remove_file(small.join(&gone)).unwrap();
            expected.retain(|name| *name != gone);
        }
        fs::rename(small.join("f00030"), small.join("f00031")).unwrap();
        expected.retain(|name| name != "f00030");
        fs::create_dir(small.join("dnew")).unwrap();
        std::os::unix::fs::symlink("f00000", small.join("lnk")).unwrap();
        expected.push("dnew".into());
        expected.push("lnk".into());
        expected.sort();
        let (seconds, out) = listed(
            "ls",
            &[
                OsStr::new("-l"),
                OsStr::new("--time-style=+"),
                small.as_os_str(),
            ],
        );
        // `mode links owner group size <empty time> name [-> target]`.
        let mut names: Vec<String> = String::from_utf8_lossy(&out)
            .lines()
            .skip(1)
            .map(|line| {
                line.split_whitespace()
                    .nth(5)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        names.sort();
        let names_ok = names == expected;
        let forget = forget_check(&harness, &ready);
        let receipt = unmount(&harness, &ready);
        println!(
            "PROBE {{\"phase\":\"modified_ls_l\",\"seconds\":{seconds:.4},\"names_ok\":{names_ok},\"expected\":{},{},\"forget\":{forget},\"kernel\":{}}}",
            expected.len(),
            summary(&out),
            opcodes(&receipt)
        );
        assert!(names_ok, "modified listing names differ");
        harness.stop();
        drop(opened);
        fs::remove_dir_all(work).unwrap();
    }
}
