//! fs read calls over a replay snapshot (#354, #352 F-035): the measurement,
//! equivalence and complexity probes of fix record 015.
//!
//! Each probe opens a scratch copy of a replay Store, never the original, and
//! is skipped unless `LFS354_STORE` (the copy) and `LFS354_RECORD` (the run's
//! `project.record`) are set. One probe per command, each under 120 s:
//!
//!   split   old read path against old byte scan, per file, plus the grep call
//!   export  the snapshot's tree as native files under `LFS354_OUT`
//!   dump    a battery of read calls as text to `LFS354_OUT`; old and new
//!           binaries' dumps must be equal (page tokens are left out)
//!   paging  statements of one grep, glob and list page at page 1 and 100
//!   time    one whole-tree grep
//!   calls   one call kind per process: statements, object ids, pack work, time
use layerfs_bridge::control::{Answer, Reply, SdkCall, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_store_service::{
    bootstrap::open_store_observed,
    read::{snapshot, window},
    Owner, OwnerConfig, StoreService,
};
use layerfs_telemetry::timer::Timing;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Live heap bytes and their peak, counted by a wrapper over the system
/// allocator: the `calls` probe reports the heap a call adds, apart from
/// allocator retention and SQLite's page cache that process RSS includes.
struct HeapCount;
static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
unsafe impl std::alloc::GlobalAlloc for HeapCount {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let ptr = unsafe { std::alloc::System.alloc(layout) };
        if !ptr.is_null() {
            let live =
                LIVE.fetch_add(layout.size(), std::sync::atomic::Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, std::sync::atomic::Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
    }
}
#[global_allocator]
static ALLOCATOR: HeapCount = HeapCount;

const PROJECT: &str = "dsh";
const NEVER: &str = "LFS352_NEVER_PRESENT";

struct Probe {
    service: StoreService,
    _owner: Owner,
    _dir: PathBuf,
}

fn probe() -> Option<Probe> {
    let store = PathBuf::from(std::env::var_os("LFS354_STORE")?);
    let record = std::fs::read(std::env::var_os("LFS354_RECORD")?).unwrap();
    let Reply::Sdk(value) = Answer::decode(&record).unwrap().reply else {
        panic!("project.record reply kind");
    };
    let Some(SdkValue::Bytes(manifest)) = value.get("manifest") else {
        panic!("project.record manifest");
    };
    let manifest = StoreManifest::decode(manifest).unwrap();
    let dir = std::env::temp_dir().join(format!("lfs354-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let opened = open_store_observed(
        PersistenceConfig::sqlite(&store).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &manifest.binding,
        manifest.cursor_key,
        64,
        64 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &dir.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let service = StoreService::new(opened, &owner);
    Some(Probe {
        service,
        _owner: owner,
        _dir: dir,
    })
}

impl Probe {
    fn call(&self, method: &str, fields: Vec<(&str, SdkValue)>) -> Result<SdkValue, String> {
        let mut map: Vec<(String, SdkValue)> = fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect();
        map.push(("project".into(), SdkValue::text(PROJECT)));
        map.push(("branch".into(), SdkValue::text("main")));
        let call = SdkCall {
            method: method.into(),
            args: SdkValue::Map(map.into_iter().collect()),
        };
        layerfs_api::call(&self.service, &call)
            .map_err(|refusal| format!("{}: {}", refusal.code, refusal.message))
    }

    fn statements(&self) -> u64 {
        let sql = self.service.opened.diagnostics().unwrap();
        sql.writer.statements + sql.readers.iter().map(|r| r.statements).sum::<u64>()
    }

    fn args(&self) -> SdkValue {
        SdkValue::map([
            ("project", SdkValue::text(PROJECT)),
            ("branch", SdkValue::text("main")),
        ])
    }
}

fn out() -> PathBuf {
    PathBuf::from(std::env::var_os("LFS354_OUT").expect("LFS354_OUT"))
}

/// The pre-#354 byte loop over windows already in memory: KMP, line count
/// and the shown prefix, one byte at a time. Binary checks are left out: they
/// are timed with the scan.
fn old_scan(windows: &[Vec<u8>], prefix: usize, pattern: &[u8]) -> (u64, bool) {
    let mut failure = vec![0; pattern.len()];
    for i in 1..pattern.len() {
        let mut j = failure[i - 1];
        while j > 0 && pattern[i] != pattern[j] {
            j = failure[j - 1];
        }
        if pattern[i] == pattern[j] {
            j += 1;
        }
        failure[i] = j;
    }
    let (mut number, mut length, mut found) = (1u64, 0usize, 0u64);
    let mut shown = Vec::new();
    let mut utf8 = Vec::new();
    let (mut matched, mut state) = (false, 0);
    for (w, bytes) in windows.iter().enumerate() {
        utf8.extend_from_slice(bytes);
        if utf8.contains(&0) {
            return (found, true);
        }
        let valid = match std::str::from_utf8(&utf8) {
            Ok(_) => utf8.len(),
            Err(e) if e.error_len().is_none() && w + 1 < windows.len() => e.valid_up_to(),
            Err(_) => return (found, true),
        };
        utf8.drain(..valid);
        for &byte in bytes {
            length += 1;
            if shown.len() < prefix {
                shown.push(byte);
            }
            if !matched {
                while state > 0 && byte != pattern[state] {
                    state = failure[state - 1];
                }
                if byte == pattern[state] {
                    state += 1;
                }
                if state == pattern.len() {
                    matched = true;
                }
            }
            if byte == b'\n' {
                found += matched as u64;
                std::hint::black_box(&shown);
                shown.clear();
                number += 1;
                length = 0;
                state = 0;
                matched = false;
            }
        }
    }
    std::hint::black_box((number, length));
    (found, false)
}

/// Canonical objects a reader returned: demands, ids and bytes.
struct Counting<'a> {
    inner: &'a dyn layerfs_content::AuthenticatedObjects,
    batches: std::cell::Cell<u64>,
    ids: std::cell::Cell<u64>,
    bytes: std::cell::Cell<u64>,
}
impl Counting<'_> {
    fn count(&self, out: &[Vec<u8>]) {
        self.batches.set(self.batches.get() + 1);
        self.ids.set(self.ids.get() + out.len() as u64);
        self.bytes
            .set(self.bytes.get() + out.iter().map(|o| o.len() as u64).sum::<u64>());
    }
}
impl layerfs_content::AuthenticatedObjects for Counting<'_> {
    fn read_canonical_batch(
        &self,
        ids: &[layerfs_content::ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        let out = self.inner.read_canonical_batch(ids)?;
        self.count(&out);
        Ok(out)
    }
    fn read_canonical_batch_scoped(
        &self,
        ids: &[layerfs_content::ObjectId],
        scope: layerfs_telemetry::timer::TimingScope<'_>,
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        let out = self.inner.read_canonical_batch_scoped(ids, scope)?;
        self.count(&out);
        Ok(out)
    }
}

#[test]
fn split() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("split") {
        return;
    }
    let Some(p) = probe() else { return };
    let store = p.service.store.clone();
    let (_, reader, snap) =
        layerfs_store_service::operator::args::snapshot(&store, &p.args()).unwrap();
    let (mut files, mut bytes, mut binary) = (0u64, 0u64, 0u64);
    let (mut walk, mut read, mut scan) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    let (mut opened, mut chunked) = (Duration::ZERO, 0u64);
    let mut ids = std::collections::HashMap::new();
    let counting = Counting {
        inner: reader.as_ref(),
        batches: Default::default(),
        ids: Default::default(),
        bytes: Default::default(),
    };
    let (mut read_statements, mut read_objects) = (0u64, 0u64);
    let mut started = Instant::now();
    window::walk(&reader, &snap, "/", |_, entry| {
        walk += started.elapsed();
        if entry.value.kind == InodeKind::RegularFile {
            let (statements, objects) = (p.statements(), p.service.store.work().object_ids);
            let t = Instant::now();
            let view = Timing::disabled("probe", |scope| {
                layerfs_content::file::FileView::open(
                    &counting,
                    entry.value.content_root,
                    scope.child("open"),
                )
            })
            .0
            .unwrap();
            opened += t.elapsed();
            chunked += view.whole_file_bytes().unwrap().is_none() as u64;
            let size = view.logical_len();
            let mut windows = Vec::new();
            let mut offset = 0;
            while offset < size {
                let end = size.min(offset + 16384);
                let mut window = Vec::new();
                Timing::disabled("probe", |scope| {
                    view.read_range(&counting, offset..end, &mut window, scope)
                })
                .0
                .unwrap();
                windows.push(window);
                offset = end;
            }
            read += t.elapsed();
            read_statements += p.statements() - statements;
            read_objects += p.service.store.work().object_ids - objects;
            let t = Instant::now();
            let (_, bad) = old_scan(&windows, 1024, NEVER.as_bytes());
            scan += t.elapsed();
            files += 1;
            bytes += size;
            binary += bad as u64;
            *ids.entry(entry.value.content_root).or_insert(0u64) += size;
        }
        started = Instant::now();
        Ok(true)
    })
    .unwrap();
    let distinct: u64 = {
        let mut seen = std::collections::HashMap::new();
        let mut sum = 0;
        // Size per distinct id: the first file's size (all copies are equal).
        window::walk(&reader, &snap, "/", |_, entry| {
            if entry.value.kind == InodeKind::RegularFile
                && seen.insert(entry.value.content_root, ()).is_none()
            {
                sum += snapshot::size(&reader, &entry).unwrap();
            }
            Ok(true)
        })
        .unwrap();
        sum
    };
    let t = Instant::now();
    let reply = p
        .call(
            "fs.grep",
            vec![
                ("pattern", SdkValue::text(NEVER)),
                ("path", SdkValue::text("/")),
                ("pageSize", SdkValue::Int(50)),
                ("pagePos", SdkValue::Int(0)),
            ],
        )
        .unwrap();
    let call = t.elapsed();
    println!(
        "split files={files} bytes={bytes} distinct_ids={} distinct_bytes={distinct} binary={binary}\n\
         walk={walk:?} read={read:?} (of which open={opened:?}, chunked files={chunked}) scan={scan:?} (probe passes)\n\
         reads: statements={read_statements} store object ids={read_objects} canonical demands={} ids={} bytes={} for file bytes={bytes}\n\
         grep call={call:?} skipped={:?} items={}",
        ids.len(),
        counting.batches.get(),
        counting.ids.get(),
        counting.bytes.get(),
        reply.get("skipped"),
        match reply.get("items") {
            Some(SdkValue::List(items)) => items.len(),
            _ => 0,
        },
    );
}

#[test]
fn export() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("export") {
        return;
    }
    let Some(p) = probe() else { return };
    let root = out();
    let store = p.service.store.clone();
    let (_, reader, snap) =
        layerfs_store_service::operator::args::snapshot(&store, &p.args()).unwrap();
    let (mut files, mut bytes) = (0u64, 0u64);
    window::walk(&reader, &snap, "/", |path, entry| {
        let target = root.join(path.trim_start_matches('/'));
        match entry.value.kind {
            InodeKind::Directory => std::fs::create_dir_all(&target).unwrap(),
            InodeKind::RegularFile => {
                let mut data = Vec::new();
                Timing::disabled("probe", |scope| {
                    layerfs_content::file::read_all_bounded(
                        reader.as_ref(),
                        entry.value.content_root,
                        u64::MAX,
                        &mut data,
                        scope.child("read"),
                    )
                })
                .0
                .unwrap();
                files += 1;
                bytes += data.len() as u64;
                std::fs::write(&target, data).unwrap();
            }
            InodeKind::Symlink => {}
        }
        Ok(true)
    })
    .unwrap();
    println!("export files={files} bytes={bytes} into {}", root.display());
}

/// One reply as text, without its page token.
fn render(text: &mut String, label: &str, reply: &Result<SdkValue, String>) {
    match reply {
        Ok(SdkValue::Map(map)) => {
            let _ = write!(text, "{label} ok");
            for (key, value) in map {
                if key != "nextPagePos" && key != "nextPage" {
                    let _ = write!(text, " {key}={value:?}");
                }
            }
            let _ = writeln!(text);
        }
        other => {
            let _ = writeln!(text, "{label} {other:?}");
        }
    }
}

fn items(reply: &SdkValue) -> Vec<SdkValue> {
    match reply.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// Pages through one call with its own tokens until `hasMore` is false.
fn pages(p: &Probe, method: &str, base: &[(&str, SdkValue)], size: i64) -> Vec<SdkValue> {
    let mut all = Vec::new();
    let mut position = SdkValue::Int(0);
    for _ in 0..100_000 {
        let mut fields = base.to_vec();
        fields.push(("pageSize", SdkValue::Int(size)));
        fields.push(("pagePos", position.clone()));
        let reply = p.call(method, fields).unwrap();
        all.extend(items(&reply));
        match reply.get("nextPagePos") {
            Some(SdkValue::Null) | None => return all,
            Some(next) => position = next.clone(),
        }
    }
    panic!("{method}: no last page");
}

/// True when `LFS354_PART` selects `name` (all parts when unset).
fn part(name: &str) -> bool {
    std::env::var("LFS354_PART").map_or(true, |part| part == name)
}

#[test]
fn dump() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("dump") {
        return;
    }
    let Some(p) = probe() else { return };
    let mut text = String::new();
    // `LFS354_PATHS`, `LFS354_PATTERNS` (comma lists) and `LFS354_SIZES`
    // narrow the battery so one command fits its limit on a large tree.
    let list = |name: &str, default: &[&str]| -> Vec<String> {
        std::env::var(name).map_or_else(
            |_| default.iter().map(|s| s.to_string()).collect(),
            |v| v.split(',').map(|s| s.replace("\\n", "\n")).collect(),
        )
    };
    let paths = list(
        "LFS354_PATHS",
        &[
            "/",
            "/packages",
            "/docs",
            "/packages/view",
            "/nope",
            "/README.md",
        ],
    );
    let patterns = list(
        "LFS354_PATTERNS",
        &[
            NEVER,
            "import",
            "TODO",
            "e",
            "\n",
            "}\n",
            "a\nb",
            "function (",
            "é",
            "😀",
            "\u{0}",
            "export default",
            "  ",
            "the",
        ],
    );
    let sizes: Vec<i64> = list("LFS354_SIZES", &["50", "1000"])
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
    for path in paths.iter().map(String::as_str) {
        for pattern in patterns.iter().map(String::as_str) {
            if !part("grep") {
                break;
            }
            for &size in &sizes {
                let reply = p.call(
                    "fs.grep",
                    vec![
                        ("pattern", SdkValue::text(pattern)),
                        ("path", SdkValue::text(path)),
                        ("pageSize", SdkValue::Int(size)),
                        ("pagePos", SdkValue::Int(0)),
                    ],
                );
                render(
                    &mut text,
                    &format!("grep {path} {pattern:?} {size}"),
                    &reply,
                );
            }
        }
        for glob in list(
            "LFS354_GLOBS",
            &[
                "**",
                "*",
                "**/*.js",
                "*.md",
                "packages/**",
                "packages/*/src/**/*.js",
                "docs/*",
                "/README.md",
                "!docs/**",
                "packages/view/**/index.*",
                "**/test/**",
                "?ackages/**",
                "packages",
                "packages/",
                "x/**",
            ],
        )
        .iter()
        .map(String::as_str)
        {
            if !part("glob") {
                break;
            }
            for &size in &sizes {
                let reply = p.call(
                    "fs.glob",
                    vec![
                        ("pattern", SdkValue::text(glob)),
                        ("path", SdkValue::text(path)),
                        ("pageSize", SdkValue::Int(size)),
                        ("pagePos", SdkValue::Int(0)),
                    ],
                );
                render(&mut text, &format!("glob {path} {glob:?} {size}"), &reply);
            }
        }
        for &size in sizes.iter().filter(|_| part("list")) {
            let reply = p.call(
                "fs.list",
                vec![
                    ("path", SdkValue::text(path)),
                    ("pageSize", SdkValue::Int(size)),
                    ("pagePos", SdkValue::Int(0)),
                ],
            );
            render(&mut text, &format!("list {path} {size}"), &reply);
        }
    }
    // Every regular file under a few folders: whole reads and line windows.
    let store = p.service.store.clone();
    let (_, reader, snap) =
        layerfs_store_service::operator::args::snapshot(&store, &p.args()).unwrap();
    let mut files = Vec::new();
    window::walk(&reader, &snap, "/", |path, entry| {
        if entry.value.kind == InodeKind::RegularFile
            && files.len() < std::env::var("LFS354_READS").map_or(4000, |n| n.parse().unwrap())
        {
            files.push(path.to_owned());
        }
        Ok(true)
    })
    .unwrap();
    for (i, file) in files.iter().enumerate().filter(|_| part("read")) {
        let windows: &[(i64, Option<i64>)] = if i % 10 == 0 {
            &[(1, None), (2, Some(3)), (100, Some(5000)), (2001, None)]
        } else {
            &[(1, None)]
        };
        for &(start, end) in windows {
            let mut fields = vec![
                ("path", SdkValue::text(file.as_str())),
                ("startLine", SdkValue::Int(start)),
            ];
            if let Some(end) = end {
                fields.push(("endLine", SdkValue::Int(end)));
            }
            let reply = p.call("fs.read", fields);
            render(&mut text, &format!("read {file} {start} {end:?}"), &reply);
        }
    }
    // Paging through equals the largest pages, on each binary's own tokens.
    let mut paged = 0;
    for (method, base) in [
        (
            "fs.grep",
            vec![
                ("pattern", SdkValue::text("import")),
                ("path", SdkValue::text("/packages")),
            ],
        ),
        (
            "fs.grep",
            vec![
                ("pattern", SdkValue::text("e")),
                ("path", SdkValue::text("/docs")),
            ],
        ),
        (
            "fs.glob",
            vec![
                ("pattern", SdkValue::text("**")),
                ("path", SdkValue::text("/")),
            ],
        ),
        ("fs.list", vec![("path", SdkValue::text("/"))]),
    ]
    .into_iter()
    .enumerate()
    // `LFS354_PAGES` (comma list of indexes) selects some of these calls, so
    // one command fits its limit on a large tree.
    .filter(|(index, _)| {
        part("pages")
            && std::env::var("LFS354_PAGES")
                .map_or(true, |v| v.split(',').any(|i| i == index.to_string()))
    })
    .map(|(_, call)| call)
    {
        let size = std::env::var("LFS354_PAGE").map_or(7, |s| s.parse().unwrap());
        let small = pages(&p, method, &base, size);
        let large = pages(&p, method, &base, 1000);
        assert_eq!(small, large, "{method} {base:?}: paging differs");
        let _ = writeln!(text, "pages {method} {base:?} {} items", small.len());
        for item in &small {
            let _ = writeln!(text, "  {item:?}");
        }
        paged += small.len();
    }
    std::fs::write(out(), &text).unwrap();
    println!(
        "dump {} bytes, {} reads, {paged} paged items -> {}",
        text.len(),
        files.len(),
        out().display()
    );
}

#[test]
fn paging() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("paging") {
        return;
    }
    let Some(p) = probe() else { return };
    for (method, base) in [
        (
            "fs.grep",
            vec![
                ("pattern", SdkValue::text("e")),
                ("path", SdkValue::text("/")),
            ],
        ),
        (
            "fs.glob",
            vec![
                ("pattern", SdkValue::text("**")),
                ("path", SdkValue::text("/")),
            ],
        ),
        ("fs.list", vec![("path", SdkValue::text("/"))]),
    ] {
        let mut position = SdkValue::Int(0);
        let mut cost = Vec::new();
        for page in 1..=100 {
            let mut fields = base.clone();
            fields.push(("pageSize", SdkValue::Int(5)));
            fields.push(("pagePos", position.clone()));
            let before = p.statements();
            let reply = p.call(method, fields).unwrap();
            cost.push((page, p.statements() - before));
            match reply.get("nextPagePos") {
                Some(SdkValue::Null) | None => break,
                Some(next) => position = next.clone(),
            }
        }
        println!(
            "paging {method}: page 1 {:?}, last page {:?} of {}",
            cost.first(),
            cost.last(),
            cost.len()
        );
    }
}

#[test]
fn time() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("time") {
        return;
    }
    let Some(p) = probe() else { return };
    let before = p.statements();
    let t = Instant::now();
    let reply = p
        .call(
            "fs.grep",
            vec![
                ("pattern", SdkValue::text(NEVER)),
                ("path", SdkValue::text("/")),
                ("pageSize", SdkValue::Int(50)),
                ("pagePos", SdkValue::Int(0)),
            ],
        )
        .unwrap();
    println!(
        "time grep {NEVER} / : {:?}, statements {}, skipped {:?}",
        t.elapsed(),
        p.statements() - before,
        reply.get("skipped")
    );
}

/// The opened readers' Storage counters, summed.
fn storage(p: &Probe) -> layerfs_storage::Diagnostics {
    let mut sum = layerfs_storage::Diagnostics::default();
    for index in 0..256 {
        if let Some(d) = p.service.store.reader_storage_diagnostics(index) {
            sum.locate += d.locate;
            sum.read_packs += d.read_packs;
            sum.read_pack_selections += d.read_pack_selections;
            sum.whole_selected += d.whole_selected;
            sum.range_selected += d.range_selected;
            sum.range_acquired_bytes += d.range_acquired_bytes;
            sum.payload_reads += d.payload_reads;
            sum.payload_read_bytes += d.payload_read_bytes;
            sum.pack_read_bytes += d.pack_read_bytes;
            sum.locator_hits += d.locator_hits;
            sum.locator_misses += d.locator_misses;
            sum.pack_hits += d.pack_hits;
            sum.pack_misses += d.pack_misses;
            sum.prefetch_group_decodes += d.prefetch_group_decodes;
            sum.value_groups += d.value_groups;
        }
    }
    sum
}

/// Per file, the Store work of opening its root and of reading it whole,
/// grouped by the pack acquisitions its open took (one per chain level and
/// pack): statements, locator calls, pack bytes and time.
#[test]
fn chain() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("chain") {
        return;
    }
    let Some(p) = probe() else { return };
    let store = p.service.store.clone();
    let (_, reader, snap) =
        layerfs_store_service::operator::args::snapshot(&store, &p.args()).unwrap();
    let mut files = Vec::new();
    window::walk(&reader, &snap, "/", |_, entry| {
        if entry.value.kind == InodeKind::RegularFile {
            files.push(entry);
        }
        Ok(true)
    })
    .unwrap();
    // Row: files, statements, locates, pack selections, pack bytes, file bytes, ns.
    let mut open = std::collections::BTreeMap::<u64, [u64; 7]>::new();
    let mut whole = [0u64; 7];
    let (mut whole_files, mut chunked) = (0u64, 0u64);
    for entry in &files {
        let (s0, d0, t0) = (p.statements(), storage(&p), Instant::now());
        let view = Timing::disabled("probe", |scope| {
            layerfs_content::file::FileView::open(
                reader.as_ref(),
                entry.value.content_root,
                scope.child("open"),
            )
        })
        .0
        .unwrap();
        let (s1, d1, t1) = (p.statements(), storage(&p), Instant::now());
        let size = view.logical_len();
        let mut sink = Vec::new();
        if view.whole_file_bytes().unwrap().is_none() {
            chunked += 1;
            Timing::disabled("probe", |scope| {
                view.read_range(reader.as_ref(), 0..size, &mut sink, scope)
            })
            .0
            .unwrap();
        } else {
            whole_files += 1;
        }
        let (s2, d2, t2) = (p.statements(), storage(&p), Instant::now());
        let row = open
            .entry(d1.read_pack_selections - d0.read_pack_selections)
            .or_default();
        for (slot, value) in [
            1,
            s1 - s0,
            d1.locate - d0.locate,
            d1.read_pack_selections - d0.read_pack_selections,
            (d1.pack_read_bytes + d1.payload_read_bytes + d1.range_acquired_bytes)
                - (d0.pack_read_bytes + d0.payload_read_bytes + d0.range_acquired_bytes),
            size,
            (t1 - t0).as_nanos() as u64,
        ]
        .into_iter()
        .enumerate()
        {
            row[slot] += value;
        }
        for (slot, value) in [
            1,
            s2 - s1,
            d2.locate - d1.locate,
            d2.read_pack_selections - d1.read_pack_selections,
            (d2.pack_read_bytes + d2.payload_read_bytes + d2.range_acquired_bytes)
                - (d1.pack_read_bytes + d1.payload_read_bytes + d1.range_acquired_bytes),
            size,
            (t2 - t1).as_nanos() as u64,
        ]
        .into_iter()
        .enumerate()
        {
            whole[slot] += value;
        }
    }
    println!(
        "chain files={} (whole-file {whole_files}, chunked {chunked})",
        files.len()
    );
    println!("open by pack acquisitions: acq files stmts/file locates/file packbytes/file filebytes/file us/file");
    for (acquisitions, r) in &open {
        let n = r[0].max(1);
        println!(
            "  {acquisitions:>3} {:>6} {:>6.1} {:>6.1} {:>9} {:>9} {:>8.1}",
            r[0],
            r[1] as f64 / n as f64,
            r[2] as f64 / n as f64,
            r[4] / n,
            r[5] / n,
            r[6] as f64 / n as f64 / 1000.0
        );
    }
    println!(
        "chunked reads after open: stmts={} locates={} selections={} packbytes={} ms={:.1}",
        whole[1],
        whole[2],
        whole[3],
        whole[4],
        whole[6] as f64 / 1e6
    );
    let d = storage(&p);
    println!(
        "totals: locate={} selections={} whole={} range={} pack_hits={} pack_misses={} locator_hits={} misses={} prefetch_decodes={}",
        d.locate,
        d.read_pack_selections,
        d.whole_selected,
        d.range_selected,
        d.pack_hits,
        d.pack_misses,
        d.locator_hits,
        d.locator_misses,
        d.prefetch_group_decodes
    );
}

/// Every regular file's root read through a Storage handle opened on the
/// copy, with a fresh reader per file (as each Store demand has now) or one
/// reader for the whole walk (`LFS354_ARM=shared`): statements, pack bytes
/// and time, to size a per-call reader before building one.
#[test]
fn reuse() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("reuse") {
        return;
    }
    let Some(p) = probe() else { return };
    let store = p.service.store.clone();
    let (_, reader, snap) =
        layerfs_store_service::operator::args::snapshot(&store, &p.args()).unwrap();
    let mut roots = Vec::new();
    window::walk(&reader, &snap, "/", |_, entry| {
        if entry.value.kind == InodeKind::RegularFile {
            roots.push(entry.value.content_root);
        }
        Ok(true)
    })
    .unwrap();
    let record = std::fs::read(std::env::var_os("LFS354_RECORD").unwrap()).unwrap();
    let Reply::Sdk(value) = Answer::decode(&record).unwrap().reply else {
        panic!("record");
    };
    let Some(SdkValue::Bytes(manifest)) = value.get("manifest") else {
        panic!("manifest");
    };
    let manifest = StoreManifest::decode(manifest).unwrap();
    let handles = layerfs_persistence::Handles::open_read_only(
        PersistenceConfig::sqlite(std::env::var_os("LFS354_STORE").unwrap())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &manifest.binding,
        manifest.cursor_key,
    )
    .unwrap();
    let storage = layerfs_storage::Storage::new(handles.storage.clone()).unwrap();
    let shared = std::env::var("LFS354_ARM").as_deref() == Ok("shared");
    let one = storage.reader().unwrap();
    let before = (
        handles.diagnostics().unwrap().statements,
        storage.diagnostics(),
    );
    let t = Instant::now();
    let (mut bytes, mut chunked) = (0u64, 0u64);
    for root in &roots {
        let fresh;
        let objects: &dyn layerfs_content::AuthenticatedObjects = if shared {
            &one
        } else {
            fresh = storage.reader().unwrap();
            &fresh
        };
        let view = Timing::disabled("probe", |scope| {
            layerfs_content::file::FileView::open(objects, *root, scope.child("open"))
        })
        .0
        .unwrap();
        bytes += view.logical_len();
        if view.whole_file_bytes().unwrap().is_none() {
            chunked += 1;
            let mut sink = Vec::new();
            Timing::disabled("probe", |scope| {
                view.read_range(objects, 0..view.logical_len(), &mut sink, scope)
            })
            .0
            .unwrap();
        }
    }
    let elapsed = t.elapsed();
    let after = (
        handles.diagnostics().unwrap().statements,
        storage.diagnostics(),
    );
    println!(
        "reuse arm={} files={} chunked={chunked} bytes={bytes} time={elapsed:?} statements={} \
         selections={} locate={} pack_bytes={} pack_hits={}",
        if shared { "shared" } else { "fresh" },
        roots.len(),
        after.0 - before.0,
        after.1.read_pack_selections - before.1.read_pack_selections,
        after.1.locate - before.1.locate,
        (after.1.pack_read_bytes + after.1.payload_read_bytes + after.1.range_acquired_bytes)
            - (before.1.pack_read_bytes
                + before.1.payload_read_bytes
                + before.1.range_acquired_bytes),
        after.1.pack_hits - before.1.pack_hits,
    );
}

/// One call kind per process (`LFS354_CALL`): its statements, object ids
/// demanded at the Store, storage counters and time. `read` is one fs.read
/// call per regular file, for the first `LFS354_READS` files in walk order,
/// and `stat` the same with fs.stat (the fixed part: snapshot, lookup, size);
/// the others are one call over `/` (grep for a pattern that is never
/// present unless `LFS354_PATTERN`, glob `**`, list `/`; `LFS354_ROOT`
/// instead of `/`), each at `LFS354_PAGE` items. `heap_peak` is the most live
/// heap the call held above its start, `heap_after` what it left.
#[test]
fn calls() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("calls") {
        return;
    }
    let Some(p) = probe() else { return };
    let kind = std::env::var("LFS354_CALL").expect("LFS354_CALL");
    let page = std::env::var("LFS354_PAGE").map_or(1000, |v| v.parse().unwrap());
    let mut files = Vec::new();
    if kind == "read" || kind == "stat" {
        let limit = std::env::var("LFS354_READS").map_or(usize::MAX, |v| v.parse().unwrap());
        let (_, reader, snap) =
            layerfs_store_service::operator::args::snapshot(&p.service.store, &p.args()).unwrap();
        window::walk(&reader, &snap, "/", |path, entry| {
            if entry.value.kind == InodeKind::RegularFile {
                files.push(path.to_owned());
            }
            Ok(files.len() < limit)
        })
        .unwrap();
    }
    let (s0, d0, o0) = (
        p.statements(),
        storage(&p),
        p.service.store.work().object_ids,
    );
    let heap0 = LIVE.load(std::sync::atomic::Ordering::Relaxed);
    PEAK.store(heap0, std::sync::atomic::Ordering::Relaxed);
    let t = Instant::now();
    let mut calls = 0;
    let pattern = std::env::var("LFS354_PATTERN").unwrap_or_else(|_| NEVER.into());
    let paged = |method: &str, extra: Vec<(&str, SdkValue)>| {
        let root = std::env::var("LFS354_ROOT").unwrap_or_else(|_| "/".into());
        let mut fields = vec![
            ("path", SdkValue::text(&root)),
            ("pageSize", SdkValue::Int(page)),
            ("pagePos", SdkValue::Int(0)),
        ];
        fields.extend(extra);
        p.call(method, fields).unwrap();
    };
    match kind.as_str() {
        "grep" => paged("fs.grep", vec![("pattern", SdkValue::text(&pattern))]),
        "glob" => paged("fs.glob", vec![("pattern", SdkValue::text("**"))]),
        "list" => paged("fs.list", vec![]),
        "read" | "stat" => {
            for path in &files {
                // A refusal (Binary) is an answer too.
                let _ = p.call(&format!("fs.{kind}"), vec![("path", SdkValue::text(path))]);
                calls += 1;
            }
        }
        other => panic!("unknown LFS354_CALL {other}"),
    }
    let elapsed = t.elapsed();
    let heap_peak = PEAK.load(std::sync::atomic::Ordering::Relaxed) - heap0;
    let heap_after = LIVE.load(std::sync::atomic::Ordering::Relaxed) as i64 - heap0 as i64;
    let (s1, d1, o1) = (
        p.statements(),
        storage(&p),
        p.service.store.work().object_ids,
    );
    let objects = o1 - o0;
    println!(
        "calls {kind} calls={calls} time={elapsed:?} statements={} object_ids={objects} \
         statements_per_object={:.2} locates={} pack_selections={} pack_bytes={} pack_hits={} pack_misses={} \
         heap_peak={heap_peak} heap_after={heap_after}",
        s1 - s0,
        (s1 - s0) as f64 / objects.max(1) as f64,
        d1.locate - d0.locate,
        d1.read_pack_selections - d0.read_pack_selections,
        d1.pack_read_bytes - d0.pack_read_bytes,
        d1.pack_hits - d0.pack_hits,
        d1.pack_misses - d0.pack_misses,
    );
}

/// The fixed part of one fs.read, step by step, summed over the first
/// `LFS354_READS` regular files: snapshot open (project, root, tree table),
/// path lookup and size, then the content read, each as fs.read does it.
/// Statements, object ids, pack bytes and time per step.
#[test]
fn fixed() {
    if std::env::var("LFS354_MODE").as_deref() != Ok("fixed") {
        return;
    }
    let Some(p) = probe() else { return };
    let limit = std::env::var("LFS354_READS").map_or(usize::MAX, |v| v.parse().unwrap());
    let mut files = Vec::new();
    {
        let (_, reader, snap) =
            layerfs_store_service::operator::args::snapshot(&p.service.store, &p.args()).unwrap();
        window::walk(&reader, &snap, "/", |path, entry| {
            if entry.value.kind == InodeKind::RegularFile {
                files.push(path.to_owned());
            }
            Ok(files.len() < limit)
        })
        .unwrap();
    }
    // Per step, the counters in NAMES, then ns.
    const NAMES: [&str; 12] = [
        "statements",
        "transactions",
        "object_ids",
        "demands",
        "locates",
        "locator_hits",
        "locator_misses",
        "pack_selections",
        "whole_packs",
        "inode_value_reads",
        "pack_bytes",
        "group_decodes",
    ];
    let mut steps = [[0u64; 13]; 4];
    let mark = |p: &Probe| {
        let d = storage(p);
        let sql = p.service.opened.diagnostics().unwrap();
        let transactions =
            sql.writer.transactions + sql.readers.iter().map(|r| r.transactions).sum::<u64>();
        let work = p.service.store.work();
        (
            [
                p.statements(),
                transactions,
                work.object_ids,
                work.object_batches,
                d.locate,
                d.locator_hits,
                d.locator_misses,
                d.read_pack_selections,
                d.read_packs,
                d.value_groups,
                d.pack_read_bytes,
                d.prefetch_group_decodes,
            ],
            Instant::now(),
        )
    };
    let add = |row: &mut [u64; 13], a: &([u64; 12], Instant), b: &([u64; 12], Instant)| {
        for (i, cell) in row.iter_mut().take(12).enumerate() {
            *cell += b.0[i] - a.0[i];
        }
        row[12] += (b.1 - a.1).as_nanos() as u64;
    };
    for path in &files {
        let m0 = mark(&p);
        let (_, reader, snap) =
            layerfs_store_service::operator::args::snapshot(&p.service.store, &p.args()).unwrap();
        let m1 = mark(&p);
        // As fs.read: the lookup through a memo over the call's caches, the
        // content through the caches.
        let cached = layerfs_store_service::read::cached::Cached::new(&reader).unwrap();
        let entry = snapshot::existing(
            &layerfs_content::compare::memo::Memo::new(&cached),
            &snap,
            path,
        )
        .unwrap();
        let m2 = mark(&p);
        snapshot::size(&reader, &entry).unwrap();
        let m3 = mark(&p);
        // A refusal (Binary) is an answer too.
        let _ = window::lines(&cached, &entry, 48 * 1024, 1, |n, _| Ok(n < 2000));
        let m4 = mark(&p);
        add(&mut steps[0], &m0, &m1);
        add(&mut steps[1], &m1, &m2);
        add(&mut steps[2], &m2, &m3);
        add(&mut steps[3], &m3, &m4);
    }
    for (name, row) in ["open", "lookup", "size", "content"].iter().zip(steps) {
        let mut line = format!("fixed {name} files={}", files.len());
        for (counter, value) in NAMES.iter().zip(row) {
            write!(line, " {counter}={value}").unwrap();
        }
        println!("{line} time={:?}", Duration::from_nanos(row[12]));
    }
}
