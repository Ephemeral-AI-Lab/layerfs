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
    use layerfs_telemetry::timer::{stages, Timing};
    use std::{
        fs::{self, File, OpenOptions},
        io::{Read, Write},
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
        format!(
            "{{\"opcodes\":{{{}}},\"served\":{},\"inline\":{},\"refused\":{},\"largest_read\":{},\"largest_write\":{},\"served_ns\":{}}}",
            rows.join(","),
            field(receipt, "served"),
            field(receipt, "inline"),
            field(receipt, "refused"),
            field(receipt, "largest_read"),
            field(receipt, "largest_write"),
            field(receipt, "served_ns"),
        )
    }
    fn stage_delta(before: &([u64; 12], [u64; 12])) -> String {
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
            "{{\"max_write\":{},\"max_readahead\":{},\"max_background\":{},\"congestion\":{},\"loops\":{}}}",
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
        let dec0 = layerfs_mount_ports::decoded::snapshot();
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
        let (work1, sql1) = (opened.store.work(), readers(&opened));
        let split = stage_delta(&stage0);
        let dec = decoded_delta(&dec0);
        assert_eq!((total, sum), (BIG, expected), "cold read bytes");
        let receipt = unmount(&harness, &ready);
        let (files, logical, allocated, _) = cache_usage(&decoded);
        println!(
            "PROBE {{\"phase\":\"cold_read\",\"decoded\":{dec},\"cache_files\":{files},\"cache_bytes\":{logical},\"cache_allocated\":{allocated},\"profile\":{profile},\"read_ahead_kb\":[\"{ra_before}\",\"{ra_after}\"],\"store_resident_before\":{store_resident},\"bytes\":{total},\"seconds\":{seconds:.4},\"mb_s\":{:.1},\"rss_before_kb\":{rss},\"vm_hwm_kb\":{hwm},\"object_batches\":{},\"object_ids\":{},\"statements\":{},\"returned_rows\":{},\"stages\":{split},\"kernel\":{}}}",
            total as f64 / seconds / 1e6,
            work1.object_batches - work0.object_batches,
            work1.object_ids - work0.object_ids,
            sql1.statements - sql0.statements,
            sql1.returned_rows - sql0.returned_rows,
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
        let ready = harness.mount(5);
        let started = Instant::now();
        let status = Command::new("ls")
            .arg("-l")
            .arg(Path::new(&ready.directory).join("small"))
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        let seconds = started.elapsed().as_secs_f64();
        assert!(status.success());
        let receipt = unmount(&harness, &ready);
        println!(
            "PROBE {{\"phase\":\"ls_l\",\"entries\":{SMALL_FILES},\"seconds\":{seconds:.4},\"kernel\":{}}}",
            opcodes(&receipt)
        );
        harness.stop();
        drop(opened);
        fs::remove_dir_all(work).unwrap();
    }
}
