//! #818 Phase 0 guard (verification Principles 4–6): one seeded call
//! sequence over all 15 `fs.*` SDK methods and Namespace Init, refusals
//! included, runs on SQLite and on PostgreSQL, and the two transcripts must
//! be equal. Later round-trip lanes run it to show that answers, refusal
//! codes and order, continuation tokens, blame owners and coverage, revision
//! and diff pages, `removed_count`, Init roots and each Commit's logical and
//! canonical Save statistics did not change.
//!
//! Calls go through the SDK's own route, as the probe
//! `fs_call_round_trips.rs` does: Init through `layerfs_project::init`, then
//! `open_store_observed` with the SDK's reader capacity and cache, a
//! `StoreService` and `methods::call`.
//!
//! Both backends Init the same source directory, so Init facts (root, head
//! Layer, entries, Namespace counts) compare raw. A file an SDK write
//! creates takes the wall clock as its mtime, so what derives from it
//! differs between any two runs, one backend twice included:
//!
//! * a Commit's root and id: the transcript names each by the Commit's
//!   position (`C<n>`, `R<n>`), any other id by its first appearance
//!   (`X<n>`) and a prefix by the id it abbreviates;
//! * the stored and metadata bytes of a Commit (`newStored`, `newMetadata`):
//!   its metadata compresses differently, by a few bytes; they are dropped,
//!   and `newCanonical` and `logical` are compared;
//! * the order of blame `origins`, kept in id order: each run checks that
//!   order, and the transcript compares the set;
//! * times taken by a call (`time`, `*_ms`, `*Ms`, `*At`, a `modifiedMs` at
//!   or after setup): dropped.
//!
//! Everything else compares as returned, inode serials and marker frontiers
//! included. Continuation tokens are decoded and their query and state
//! compared; their tags are keyed per payload and dropped.
//!
//! Each step also names its expected outcome (an answer or a refusal code),
//! so the SQLite half alone is a check. The PostgreSQL half needs
//! `LAYERFS_TEST_POSTGRES_URL` and reports NOT_RUN without it. Set
//! `EQUIV_PRINT=1` to print each backend's transcript.
use layerfs_protocol::provision::install::{Answer, Reply, SdkCall, SdkRefusal, SdkValue};
use layerfs_protocol::sdk::{hex, unhex};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId,
};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqliteAcquisitionSchema, SqliteHandles,
    SqlitePersistenceProfile, StoreSelection,
};
use layerfs_project::{init, InitRequest, Initialized};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BINDING: &[u8] = b"sdk-round-trip-equivalence";
const CURSOR: [u8; 32] = [83; 32];
const PROJECT: &str = "eq";
/// The SDK Store service's reader capacity and object cache
/// (`store_service.rs`: `CALLS`, `CACHE_BYTES`), as the probe opens it.
const CALLS: usize = 64;
const CACHE_BYTES: usize = 64 * 1024;
/// The one seed of every generated name, content and choice.
const SEED: u64 = 0x0818_5eed_0b5e_ed00;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// SplitMix64: the fixed-seed generator for the tree and the call choices.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
    /// A line of words with no hex-only token, so no content looks like an id.
    fn line(&mut self) -> String {
        const WORDS: [&str; 12] = [
            "alpha", "bravo", "charlie", "delta", "echo", "golf", "hotel", "india", "juliet",
            "kilo", "lima", "oscar",
        ];
        let words = 2 + self.below(5);
        (0..words)
            .map(|_| WORDS[self.below(WORDS.len() as u64) as usize])
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn hard_link(source: &Path, from: &str, to: &str) {
    std::fs::create_dir_all(source.join(to).parent().unwrap()).unwrap();
    std::fs::hard_link(source.join(from), source.join(to)).unwrap();
}

fn symlink(source: &Path, target: &str, at: &str) {
    std::os::unix::fs::symlink(target, source.join(at)).unwrap();
}

/// The main tree: small seeded files with grep hits, a 600-entry directory
/// with hardlinks across its 512-row windows, a depth-10 file, symlinks
/// (to a file, to a directory, to a symlink, dangling) for an intermediate
/// symlink, repeated (device, inode) hardlinks in one directory, an
/// all-hardlink directory, 30 directories repeating the same 10 names
/// (repeated names in one placement window), the history files, the paths
/// `project.remove_file` hides, a text file, an empty file, binary and
/// chunked files, and an empty directory.
fn build_main_tree(source: &Path, rng: &mut Rng) {
    for d in 0..6 {
        for f in 0..25 {
            let mut text = String::new();
            for _ in 0..(3 + rng.below(9)) {
                text.push_str(&rng.line());
                if rng.below(7) == 0 {
                    text.push_str(&format!(" needle {d} {f}"));
                }
                text.push('\n');
            }
            write(&source.join(format!("src/d{d:02}/f{f:03}.txt")), text);
        }
    }
    for n in 0..600 {
        let name = format!("wide/w{n:04}");
        if n > 0 && n % 97 == 0 {
            hard_link(source, "wide/w0000", &name);
        } else {
            write(&source.join(name), format!("wide {n} {}\n", rng.line()));
        }
    }
    write(&source.join("deep/a/b/c/d/e/f/leaf.txt"), "leaf\n");
    write(&source.join("deep/a/b/c/d/e/f/g/h/bottom.txt"), "bottom\n");
    std::fs::create_dir_all(source.join("links")).unwrap();
    symlink(source, "../src/d00/f000.txt", "links/to_file");
    symlink(source, "../src/d01", "links/to_dir");
    symlink(source, "to_dir", "links/chain");
    symlink(source, "../absent", "links/dangling");
    write(&source.join("hard/orig.txt"), "orig\n");
    for n in 1..=4 {
        hard_link(source, "hard/orig.txt", &format!("hard/h{n}"));
    }
    hard_link(source, "src/d00/f001.txt", "hard/x.txt");
    for n in 0..10 {
        let from = if n < 5 {
            format!("src/d02/f{n:03}.txt")
        } else {
            "hard/orig.txt".to_owned()
        };
        hard_link(source, &from, &format!("allhard/k{n}"));
    }
    for r in 0..30 {
        for n in 0..10 {
            write(
                &source.join(format!("same/r{r:02}/n{n}")),
                format!("same {r} {n}\n"),
            );
        }
    }
    let a: Vec<String> = (1..=40)
        .map(|i| format!("line {i} {}", rng.line()))
        .collect();
    let a = a.join("\n") + "\n";
    write(&source.join("hist/a.txt"), a);
    write(&source.join("hist/b.txt"), "b0\n");
    write(&source.join("hide/gone.txt"), "gone\n");
    write(&source.join("hide/keep.txt"), "keep\n");
    write(&source.join("hide/dir/c1.txt"), "c1\n");
    write(&source.join("hide/dir/c2.txt"), "c2 needle\n");
    write(&source.join("hide/dir/sub/c3.txt"), "c3\n");
    let big: String = (1..=700)
        .map(|i| {
            if i == 650 {
                "the needle line\n".to_owned()
            } else {
                format!("row {i} {}\n", rng.line())
            }
        })
        .collect();
    write(&source.join("text/big.txt"), big);
    write(&source.join("text/empty.txt"), "");
    let mut binary = rng.bytes(512);
    binary[0] = 0xff;
    write(&source.join("text/bin.dat"), binary);
    write(&source.join("text/chunky.bin"), rng.bytes(300 * 1024));
    write(&source.join("tool/run.sh"), "#!/bin/sh\necho run\n");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        source.join("tool/run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    std::fs::create_dir_all(source.join("emptydir")).unwrap();
}

/// A flat directory past the 4,096-row write window, with hardlinks to an
/// earlier file on both sides of the boundary and a run of repeated
/// (device, inode) entries straddling it, beside an all-hardlink directory.
fn build_window_tree(source: &Path, rng: &mut Rng) {
    for n in 0..4_200 {
        let name = format!("flat/e{n:05}");
        if (4_090..4_102).contains(&n) {
            hard_link(source, "flat/e04089", &name);
        } else if n > 0 && n % 509 == 0 {
            hard_link(source, "flat/e00000", &name);
        } else {
            write(&source.join(name), format!("{n} {}\n", rng.line()));
        }
    }
    for n in 0..20 {
        hard_link(
            source,
            &format!("flat/e{:05}", n * 7),
            &format!("links/l{n:02}"),
        );
    }
}

/// Raw Init facts; both backends Init the same source, so these are equal.
fn init_facts(done: &Initialized) -> String {
    let w = &done.namespace_work;
    format!(
        "init root={} head={} root_serial={} entries={} unique_files={} regular_aliases={} \
         namespace_entries={} jobs={} directory_children={} wide_directories={}",
        hex(done.root.as_bytes()),
        hex(done.stack.head_layer.as_slice()),
        done.root_serial,
        done.entries,
        w.unique_files,
        w.regular_aliases,
        w.entries,
        w.jobs,
        w.directory_children,
        w.wide_directories,
    )
}

/// FNV-1a: a short stable name for returned bytes.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The canonical text of an answer: ids renamed, setup-later times dropped.
struct Canon {
    /// Full ids and their names, in the order they were named.
    known: Vec<(String, String)>,
    names: HashMap<String, String>,
    unnamed: usize,
    /// Times at or after this (ms) were taken by a call, not by setup.
    after: i64,
}

impl Canon {
    fn new(after: i64) -> Self {
        Self {
            known: Vec::new(),
            names: HashMap::new(),
            unnamed: 0,
            after,
        }
    }

    fn name(&mut self, id: &str, name: String) {
        if !self.names.contains_key(id) {
            self.names.insert(id.to_owned(), name.clone());
            self.known.push((id.to_owned(), name));
        }
    }

    fn id(&mut self, id: &str) -> String {
        if let Some(name) = self.names.get(id) {
            return name.clone();
        }
        let prefixed: Vec<&(String, String)> = self
            .known
            .iter()
            .filter(|(full, _)| full.starts_with(id))
            .collect();
        match prefixed.as_slice() {
            [(_, name)] => format!("{name}[..{}]", id.len()),
            [] => {
                self.unnamed += 1;
                let name = format!("X{}", self.unnamed);
                self.name(id, name.clone());
                name
            }
            _ => format!("<ambiguous prefix of {}>", id.len()),
        }
    }

    /// Renames every lowercase hex run of 12 or more characters that stands
    /// alone (an id or an id prefix); everything else is kept.
    fn text(&mut self, text: &str) -> String {
        if let Some(token) = text.strip_prefix("lfq1:") {
            return self.token(token);
        }
        let bytes = text.as_bytes();
        let hexish = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        let (mut out, mut copied, mut i) = (String::new(), 0, 0);
        while i < bytes.len() {
            if hexish(bytes[i]) && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
                let mut j = i;
                while j < bytes.len() && hexish(bytes[j]) {
                    j += 1;
                }
                if j - i >= 12 && (j == bytes.len() || !bytes[j].is_ascii_alphanumeric()) {
                    out.push_str(&text[copied..i]);
                    let name = self.id(&text[i..j]);
                    out.push_str(&name);
                    copied = j;
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }
        out.push_str(&text[copied..]);
        format!("{out:?}")
    }

    /// A continuation token: its decoded query and state; the tag is dropped.
    fn token(&mut self, token: &str) -> String {
        let decoded = token
            .split_once(':')
            .and_then(|(body, _tag)| unhex(body))
            .and_then(|payload| Answer::decode(&payload).ok());
        match decoded {
            Some(Answer {
                id: 1,
                reply: Reply::Sdk(value),
            }) => format!("lfq1{}", self.value(&value, None)),
            _ => panic!("an undecodable continuation token: {token}"),
        }
    }

    fn value(&mut self, value: &SdkValue, key: Option<&str>) -> String {
        match value {
            SdkValue::Null => "null".into(),
            SdkValue::Bool(b) => b.to_string(),
            SdkValue::Int(n) => match key {
                Some("time") => "<time>".into(),
                Some("modifiedMs") if *n >= self.after => "<written>".into(),
                Some("modifiedMs") => n.to_string(),
                Some(k) if k.ends_with("_ms") || k.ends_with("Ms") || k.ends_with("At") => {
                    "<time>".into()
                }
                Some("newStored" | "newMetadata" | "newStoredBytes" | "newMetadataBytes") => {
                    "<compressed>".into()
                }
                _ => n.to_string(),
            },
            SdkValue::Text(text) => self.text(text),
            SdkValue::Bytes(bytes) => format!("bytes({},{:016x})", bytes.len(), fnv(bytes)),
            SdkValue::List(items) if key == Some("origins") => {
                let ids: Vec<&str> = items
                    .iter()
                    .filter_map(|origin| origin.get("ref").and_then(|r| r.get_text("id")))
                    .collect();
                assert!(
                    ids.len() == items.len() && ids.windows(2).all(|w| w[0] < w[1]),
                    "blame origins are kept in id order: {ids:?}"
                );
                let mut items: Vec<String> = items.iter().map(|v| self.value(v, None)).collect();
                items.sort();
                format!("set[{}]", items.join(","))
            }
            SdkValue::List(items) => {
                let items: Vec<String> = items.iter().map(|v| self.value(v, None)).collect();
                format!("[{}]", items.join(","))
            }
            SdkValue::Map(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}:{}", self.value(v, Some(k))))
                    .collect();
                format!("{{{}}}", fields.join(","))
            }
        }
    }
}

/// What a step must answer: any answer, or a refusal with this code.
enum Expect {
    Ok,
    Refused(&'static str),
}

/// One backend's run of the sequence.
struct Run<'a> {
    service: &'a StoreService,
    canon: Canon,
    lines: Vec<String>,
    surprises: Vec<String>,
    /// Raw ids of the Commits the run made, in order.
    commits: Vec<String>,
}

type Fields<'f> = Vec<(&'f str, SdkValue)>;

impl Run<'_> {
    fn call_in(
        &self,
        project: &str,
        method: &str,
        fields: Fields<'_>,
    ) -> Result<SdkValue, SdkRefusal> {
        let mut fields = fields;
        fields.push(("project", t(project)));
        let call = SdkCall {
            method: method.into(),
            args: SdkValue::Map(
                fields
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ),
        };
        layerfs_store_service::methods::call(self.service, &call)
    }

    /// Runs one step in `project`, records its canonical answer and checks
    /// the expected outcome; returns the raw answer.
    fn step_in(
        &mut self,
        project: &str,
        label: &str,
        expect: Expect,
        method: &str,
        fields: Fields<'_>,
    ) -> Option<SdkValue> {
        let reply = self.call_in(project, method, fields);
        if let Ok(value) = &reply {
            if let Some(commit) = value.get("commit") {
                let (id, root) = (commit.get_text("id"), commit.get_text("root"));
                if let (Some(id), Some(root)) = (id, root) {
                    let n = self.commits.len() + 1;
                    self.canon.name(id, format!("C{n}"));
                    self.canon.name(root, format!("R{n}"));
                    self.commits.push(id.to_owned());
                }
            }
        }
        let shown = match &reply {
            Ok(value) => format!("ok {}", self.canon.value(value, None)),
            Err(refusal) => format!(
                "refused {} {} {}",
                refusal.code,
                self.canon.text(&refusal.message),
                self.canon.value(&refusal.detail, None)
            ),
        };
        let line = format!("{:03} {label} {method} => {shown}", self.lines.len() + 1);
        match (&expect, &reply) {
            (Expect::Ok, Ok(_)) => {}
            (Expect::Refused(code), Err(refusal)) if refusal.code == *code => {}
            (Expect::Ok, Err(_)) => self.surprises.push(format!("expected an answer: {line}")),
            (Expect::Refused(code), _) => self
                .surprises
                .push(format!("expected refusal {code}: {line}")),
        }
        self.lines.push(line);
        reply.ok()
    }

    /// A step in project `eq`.
    fn step(
        &mut self,
        label: &str,
        expect: Expect,
        method: &str,
        fields: Fields<'_>,
    ) -> Option<SdkValue> {
        self.step_in(PROJECT, label, expect, method, fields)
    }

    /// A step on Branch `work` at `path`, labelled by the path.
    fn on(&mut self, expect: Expect, method: &str, path: &str, extra: Fields<'_>) -> SdkValue {
        let mut fields = vec![b("work"), p(path)];
        fields.extend(extra);
        self.step(path, expect, method, fields).unwrap_or_default()
    }

    /// The raw id of the `n`th Commit (1-based) of this run.
    fn commit(&self, n: usize) -> String {
        self.commits[n - 1].clone()
    }
}

const OK: Expect = Expect::Ok;

fn no(code: &'static str) -> Expect {
    Expect::Refused(code)
}

fn b(name: &str) -> (&'static str, SdkValue) {
    ("branch", t(name))
}

fn p(path: &str) -> (&'static str, SdkValue) {
    ("path", t(path))
}

fn int(name: &str, n: i64) -> (&str, SdkValue) {
    (name, SdkValue::Int(n))
}

fn text<'n>(name: &'n str, value: &str) -> (&'n str, SdkValue) {
    (name, t(value))
}

fn flag(name: &str, value: bool) -> (&str, SdkValue) {
    (name, SdkValue::Bool(value))
}

/// A file selector of `fs.diff_files`, at a Commit or on the call's Branch.
fn at(commit: Option<&str>, path: &str) -> SdkValue {
    let mut fields = vec![("path".to_owned(), t(path))];
    if let Some(commit) = commit {
        fields.push(("commit".to_owned(), t(commit)));
    }
    SdkValue::Map(fields)
}

/// One change of `fs.apply`.
fn change(op: &str, fields: Fields<'_>) -> SdkValue {
    let mut all = vec![("op".to_owned(), t(op))];
    all.extend(fields.into_iter().map(|(k, v)| (k.to_owned(), v)));
    SdkValue::Map(all)
}

/// A backend and where its Store lives.
enum Backend {
    Sqlite,
    Postgres(String),
}

impl Backend {
    fn name(&self) -> &'static str {
        match self {
            Backend::Sqlite => "sqlite",
            Backend::Postgres(_) => "postgresql",
        }
    }
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn private_dir(path: &Path) {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .unwrap();
}

/// Inits `source` as project `eq` with `main`, opens the Store as the SDK
/// does and runs `scenario`; returns the Init facts and the transcript.
fn run_backend(
    backend: &Backend,
    work: &Path,
    schema: &str,
    source: &Path,
    after: i64,
    tag: u8,
    scenario: fn(&mut Run),
) -> (Vec<String>, Vec<String>) {
    let dir = work.join(backend.name());
    private_dir(&dir);
    let request = |acquisition| InitRequest {
        source,
        acquisition,
        stack: LayerStackId::from_authority([tag; 16]),
        name: HistoryName::new(PROJECT).unwrap(),
        scope_seed: [tag; 32],
        deadline: Instant::now() + Duration::from_secs(100),
    };
    let fork = |history: &dyn HistoryCatalog, done: &Initialized| {
        history
            .fork(&ForkRequest {
                stack: done.stack.id,
                branch: BranchId::from_authority([tag ^ 0xff; 16]),
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(done.stack.head_layer),
            })
            .unwrap();
    };
    let started = Instant::now();
    let (done, selection) = match backend {
        Backend::Sqlite => {
            let config = PersistenceConfig::sqlite(dir.join("store.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Normal)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
            let handles =
                SqliteHandles::create(config, StoragePolicy::frozen_default(), &catalog()).unwrap();
            let storage = Storage::new(handles.storage.clone()).unwrap();
            let done = Timing::disabled("init", |timing| {
                init(
                    &storage,
                    &handles.history,
                    request(&handles.acquisition),
                    timing,
                )
            })
            .0
            .unwrap();
            fork(&handles.history, &done);
            drop(storage);
            let sealed = handles.seal().unwrap().path;
            let config = PersistenceConfig::sqlite(sealed)
                .with_sqlite_profile(SqlitePersistenceProfile::Normal);
            (done, StoreSelection::Sqlite(config))
        }
        Backend::Postgres(url) => {
            let selection = PostgresSelection::new(url, schema);
            let handles =
                PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                    .unwrap();
            let storage = Storage::new(handles.storage.clone()).unwrap();
            let done = Timing::disabled("init", |timing| {
                init(
                    &storage,
                    &handles.history,
                    request(&handles.acquisition),
                    timing,
                )
            })
            .0
            .unwrap();
            fork(&handles.history, &done);
            (done, StoreSelection::Postgres(selection))
        }
    };
    let init_ms = started.elapsed().as_millis();
    let facts = vec![init_facts(&done)];
    let opened = open_store_observed(
        selection,
        BINDING,
        CURSOR,
        CALLS,
        CACHE_BYTES,
        ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &dir.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let service = StoreService::new(opened, &owner);
    let mut run = Run {
        service: &service,
        canon: Canon::new(after),
        lines: Vec::new(),
        surprises: Vec::new(),
        commits: Vec::new(),
    };
    run.canon.name(&hex(done.root.as_bytes()), "R0".into());
    run.canon
        .name(&hex(done.stack.head_layer.as_slice()), "L0".into());
    let calls_started = Instant::now();
    scenario(&mut run);
    let calls_ms = calls_started.elapsed().as_millis();
    let (lines, surprises, commits) = (run.lines, run.surprises, run.commits.len());
    if std::env::var_os("EQUIV_PRINT").is_some() {
        for line in &lines {
            eprintln!("EQUIV {} {line}", backend.name());
        }
    }
    drop(service);
    owner.stop().unwrap();
    eprintln!(
        "EQUIV {}: init {init_ms} ms, {} steps ({commits} Commits) {calls_ms} ms",
        backend.name(),
        lines.len()
    );
    assert!(
        surprises.is_empty(),
        "{} unexpected outcomes on {}:\n{}",
        surprises.len(),
        backend.name(),
        surprises.join("\n")
    );
    (facts, lines)
}

fn drop_schema(url: &str, schema: &str) {
    if let Ok(mut client) = postgres::Client::connect(url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
}

/// Builds the source once, runs SQLite and (with a URL) PostgreSQL, and
/// requires the two transcripts to be equal. `tag` is the project's stack,
/// scope and `main` identity: PostgreSQL advisory locks are scoped to the
/// database, not the schema, so tests of one binary that share an identity
/// refuse each other `Busy` (docs/lesson/testing/one-database-one-test-binary.md).
fn equivalence(label: &str, tag: u8, build: fn(&Path, &mut Rng), scenario: fn(&mut Run)) {
    let work = std::env::temp_dir().join(format!("lfs818-eq-{label}-{}", std::process::id()));
    private_dir(&work);
    let source = work.join("source");
    std::fs::create_dir_all(&source).unwrap();
    build(&source, &mut Rng(SEED));
    // Setup's times end here; a write's mtime is at or after `after`.
    let after = now_ms() + 1;
    std::thread::sleep(Duration::from_millis(5));
    let schema = format!("lfs818_eq_{label}_{}", std::process::id());
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let sqlite = run_backend(
            &Backend::Sqlite,
            &work,
            &schema,
            &source,
            after,
            tag,
            scenario,
        );
        let Some(url) = url.clone() else {
            eprintln!(
                "EQUIV {label}: PostgreSQL half NOT_RUN (LAYERFS_TEST_POSTGRES_URL is unset)"
            );
            return;
        };
        let backend = Backend::Postgres(url);
        let postgres = run_backend(&backend, &work, &schema, &source, after, tag, scenario);
        assert_eq!(sqlite.0, postgres.0, "Init facts differ between backends");
        let differ: Vec<String> = sqlite
            .1
            .iter()
            .zip(&postgres.1)
            .filter(|(s, p)| s != p)
            .take(8)
            .map(|(s, p)| format!("sqlite:     {s}\npostgresql: {p}"))
            .collect();
        assert!(
            differ.is_empty() && sqlite.1.len() == postgres.1.len(),
            "{label}: answers differ between backends ({} vs {} steps):\n{}",
            sqlite.1.len(),
            postgres.1.len(),
            differ.join("\n")
        );
        eprintln!(
            "EQUIV {label}: {} steps equal on sqlite and postgresql",
            sqlite.1.len()
        );
    }));
    if let Some(url) = &url {
        drop_schema(url, &schema);
    }
    let _ = std::fs::remove_dir_all(&work);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// Reads before any write, with the hidden paths in place, and the write
/// refusals a hidden path decides (nothing is committed).
fn reads(r: &mut Run) {
    r.step("fork work", OK, "branch.fork", vec![text("name", "work")]);
    r.step(
        "hide a file",
        OK,
        "project.remove_file",
        vec![p("hide/gone.txt")],
    );
    r.step(
        "hide a dir",
        OK,
        "project.remove_file",
        vec![p("/hide/dir")],
    );

    for path in [
        "/",
        "/src/d03/f007.txt",
        "/deep/a/b/c/d/e/f/g/h/bottom.txt",
        "/links/to_file",
        "/links/to_dir",
        "/links/chain",
        "/links/dangling",
        "/hard/h3",
        "/allhard/k7",
        "/wide",
        "/wide/w0194",
        "/tool/run.sh",
        "/emptydir",
        "/hide/keep.txt",
    ] {
        r.on(OK, "fs.stat", path, vec![]);
    }
    r.on(
        no("InvalidInput"),
        "fs.stat",
        "/links/to_dir/f001.txt",
        vec![],
    );
    r.on(no("Removed"), "fs.stat", "/hide/gone.txt", vec![]);
    r.on(no("Removed"), "fs.stat", "/hide/dir/sub/c3.txt", vec![]);
    r.on(no("NoSuchPath"), "fs.stat", "/absent", vec![]);
    r.on(no("InvalidInput"), "fs.stat", "bad", vec![]);
    let ghost = vec![b("ghost"), p("/")];
    r.step("bad branch", no("NoSuchBranch"), "fs.stat", ghost);
    let nope = vec![b("work"), p("/")];
    r.step_in("nope", "bad project", no("NoSuchProject"), "fs.stat", nope);
    r.step("on main", OK, "fs.stat", vec![b("main"), p("/hist/a.txt")]);

    let binary = || flag("binary", true);
    r.on(OK, "fs.read", "/src/d04/f011.txt", vec![]);
    r.on(OK, "fs.read", "/text/big.txt", vec![]);
    let lines = vec![int("startLine", 640), int("endLine", 660)];
    r.on(OK, "fs.read", "/text/big.txt", lines);
    r.on(OK, "fs.read", "/text/empty.txt", vec![]);
    r.on(OK, "fs.read", "/text/bin.dat", vec![binary()]);
    r.on(no("Binary"), "fs.read", "/text/bin.dat", vec![]);
    r.on(OK, "fs.read", "/text/chunky.bin", vec![binary()]);
    r.on(OK, "fs.read", "/allhard/k2", vec![]);
    r.on(OK, "fs.read", "/deep/a/b/c/d/e/f/leaf.txt", vec![]);
    r.on(no("NotFile"), "fs.read", "/links/to_file", vec![]);
    r.on(
        no("InvalidInput"),
        "fs.read",
        "/links/to_dir/f002.txt",
        vec![],
    );
    r.on(no("IsDirectory"), "fs.read", "/src", vec![]);
    r.on(no("Removed"), "fs.read", "/hide/dir/c1.txt", vec![]);
    r.on(
        no("NoSuchPath"),
        "fs.read",
        "/absent",
        vec![int("startLine", 0)],
    );
    r.on(
        no("InvalidInput"),
        "fs.read",
        "/text/big.txt",
        vec![int("startLine", 0)],
    );

    for path in ["/", "/hide", "/links", "/allhard", "/emptydir"] {
        r.on(OK, "fs.list", path, vec![]);
    }
    for pos in [0, 500, 590] {
        let page = vec![int("pageSize", 100), int("pagePos", pos)];
        r.on(OK, "fs.list", "/wide", page);
    }
    r.on(
        OK,
        "fs.list",
        "/wide",
        vec![int("page", 3), int("pageSize", 250)],
    );
    r.on(no("NotDirectory"), "fs.list", "/hard/orig.txt", vec![]);
    r.on(no("Removed"), "fs.list", "/hide/dir", vec![]);
    r.on(
        no("NoSuchPath"),
        "fs.list",
        "/absent",
        vec![int("pageSize", 0)],
    );
    r.on(no("InvalidInput"), "fs.list", "/", vec![int("pageSize", 0)]);

    let pattern = |value| text("pattern", value);
    r.on(OK, "fs.glob", "/", vec![pattern("**/*.txt")]);
    r.on(OK, "fs.glob", "/hide", vec![pattern("**")]);
    r.on(OK, "fs.glob", "/src/d02", vec![pattern("f01*")]);
    let page = vec![pattern("w05*"), int("page", 2), int("pageSize", 40)];
    r.on(OK, "fs.glob", "/wide", page);
    r.on(OK, "fs.glob", "/same", vec![pattern("**/n3")]);
    r.on(no("Removed"), "fs.glob", "/hide/dir", vec![pattern("*")]);

    r.on(OK, "fs.grep", "/", vec![pattern("needle")]);
    r.on(OK, "fs.grep", "/src/d01", vec![pattern("needle")]);
    r.on(OK, "fs.grep", "/hide", vec![pattern("needle")]);
    let folded = vec![pattern("NEEDLE"), flag("ignoreCase", true)];
    r.on(OK, "fs.grep", "/text", folded);
    let page = vec![pattern("lima"), int("page", 2), int("pageSize", 10)];
    r.on(OK, "fs.grep", "/src", page);
    r.on(no("InvalidInput"), "fs.grep", "bad", vec![pattern("")]);
    r.on(no("Removed"), "fs.grep", "/hide/dir", vec![pattern("c")]);

    // History reads of a hidden path while the base is the marked Layer;
    // a file diff answers a hidden side as `removed` with its frontier.
    r.on(no("Removed"), "fs.blame", "/hide/dir/c2.txt", vec![]);
    r.on(no("Removed"), "fs.revision", "/hide/gone.txt", vec![]);
    let left = ("left", at(None, "/hide/gone.txt"));
    let right = ("right", at(None, "/hist/b.txt"));
    let sides = vec![b("work"), left, right];
    r.step("diff hidden", OK, "fs.diff_files", sides);

    // A write's private Workspace sees a hidden path as absent; edit and a
    // cp source tell `Removed` from absent through the base view (#818 W1).
    let replace = |old| vec![text("oldText", old), text("newText", "x")];
    r.on(no("Removed"), "fs.edit", "/hide/dir/c1.txt", replace("c1"));
    let from = |path| vec![b("work"), text("from", path), text("to", "/copied")];
    r.step(
        "cp hidden",
        no("Removed"),
        "fs.cp",
        from("/hide/dir/c2.txt"),
    );
    r.step(
        "mv hidden",
        no("NoSuchPath"),
        "fs.mv",
        from("/hide/gone.txt"),
    );
    r.on(no("NoSuchPath"), "fs.rm", "/hide/dir/c2.txt", vec![]);
    r.on(no("NoSuchPath"), "fs.edit", "/absent", replace("a"));
}

/// History on `work`: 24 seeded Commits through `fs.write` and `fs.edit`,
/// then blame, revision pages and file diffs over them.
fn history(r: &mut Run) {
    let mut rng = Rng(SEED ^ 0x4157);
    let original = r.on(OK, "fs.read", "/hist/a.txt", vec![]);
    let mut a: Vec<String> = original
        .get_text("text")
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    assert!(!a.is_empty(), "fs.read answers `text` for a text file");
    for k in 1..=24usize {
        match k % 4 {
            0 => {
                let content = format!("b{k} {}\n", rng.line());
                r.on(
                    OK,
                    "fs.write",
                    "/hist/b.txt",
                    vec![text("content", &content)],
                );
            }
            1 => {
                a.push(format!("line {} added at {k}", a.len() + 1));
                let content = a.join("\n") + "\n";
                r.on(
                    OK,
                    "fs.write",
                    "/hist/a.txt",
                    vec![text("content", &content)],
                );
            }
            _ => {
                let i = rng.below(a.len() as u64) as usize;
                let new = format!("line {} edited at {k} {}", i + 1, rng.line());
                let old = std::mem::replace(&mut a[i], new);
                let edit = vec![text("oldText", &old), text("newText", &a[i])];
                r.on(OK, "fs.edit", "/hist/a.txt", edit);
            }
        }
    }

    let (c3, c10, c20) = (r.commit(3), r.commit(10), r.commit(20));
    r.on(OK, "fs.blame", "/hist/a.txt", vec![]);
    r.on(OK, "fs.blame", "/hist/a.txt", vec![text("commit", &c10)]);
    r.on(
        OK,
        "fs.blame",
        "/hist/a.txt",
        vec![text("commit", &c10[..14])],
    );
    let window = vec![int("fromLine", 5), int("lineCount", 10)];
    r.on(OK, "fs.blame", "/hist/a.txt", window);
    r.on(OK, "fs.blame", "/hist/b.txt", vec![]);
    r.step("on main", OK, "fs.blame", vec![b("main"), p("/hist/a.txt")]);
    r.on(no("NotFile"), "fs.blame", "/hist", vec![int("fromLine", 0)]);
    r.on(no("NoSuchPath"), "fs.blame", "/hist/none.txt", vec![]);
    let bad = vec![text("commit", "zz")];
    r.on(no("InvalidInput"), "fs.blame", "/hist/a.txt", bad);
    // The first write whiteouts the hidden paths in `work`'s tree.
    r.on(no("NoSuchPath"), "fs.blame", "/hide/dir/c2.txt", vec![]);

    let mut extra = vec![int("pageSize", 5)];
    for _ in 0..10 {
        let page = r.on(OK, "fs.revision", "/hist/a.txt", extra);
        let Some(token) = page.get_text("continuation") else {
            break;
        };
        extra = vec![int("pageSize", 5), text("continuation", token)];
    }
    let second = vec![int("page", 2), int("pageSize", 5)];
    r.on(OK, "fs.revision", "/hist/a.txt", second);
    let branch = vec![text("traversal", "branch")];
    r.on(OK, "fs.revision", "/hist/b.txt", branch);
    r.on(OK, "fs.revision", "/hist/a.txt", vec![text("commit", &c10)]);
    let sideways = vec![text("traversal", "sideways")];
    r.on(no("InvalidInput"), "fs.revision", "/hist/a.txt", sideways);
    r.on(OK, "fs.revision", "/hide/gone.txt", vec![]);
    let ghost = vec![b("ghost"), p("/hist/a.txt")];
    r.step("bad branch", no("NoSuchBranch"), "fs.revision", ghost);

    let a_at = |commit: Option<&str>| at(commit, "/hist/a.txt");
    let mut diff = |label, expect, fields| {
        r.step(label, expect, "fs.diff_files", fields);
    };
    let sides = vec![("left", a_at(Some(&c3))), ("right", a_at(Some(&c20)))];
    diff("C3 C20", OK, sides);
    let sides = vec![
        b("work"),
        ("left", a_at(Some("main"))),
        ("right", a_at(None)),
    ];
    diff("main work", OK, sides);
    let paged = vec![
        b("work"),
        ("left", a_at(Some(&c3))),
        ("right", a_at(None)),
        int("page", 2),
        int("pageSize", 3),
        flag("includeAttribution", true),
    ];
    diff("page with attribution", OK, paged);
    let b_now = || at(None, "/hist/b.txt");
    let two = vec![b("work"), ("left", b_now()), ("right", a_at(None))];
    diff("two files", OK, two);
    let missing = vec![
        b("work"),
        ("left", at(None, "/hist/none.txt")),
        ("right", b_now()),
        flag("allowMissing", true),
    ];
    diff("allow missing", OK, missing);
    diff(
        "absent left",
        no("NoSuchPath"),
        vec![("left", at(None, "/absent"))],
    );
    let hidden = vec![
        b("work"),
        ("left", at(None, "/hide/gone.txt")),
        ("right", b_now()),
    ];
    diff("hidden after a write", no("NoSuchPath"), hidden);
}

/// Writes and their refusals on `work`, then reads of what they made.
fn writes(r: &mut Run) {
    let content = |value| vec![text("content", value)];
    let bytes = vec![("bytes", SdkValue::Bytes(b"new\n".to_vec()))];
    r.on(OK, "fs.write", "/src/d00/new.txt", bytes);
    r.on(OK, "fs.write", "/src/d03/f007.txt", content("changed\n"));
    r.on(OK, "fs.write", "/src/d03/f007.txt", content("changed\n"));
    let mode = vec![text("content", "#!/bin/sh\n"), int("mode", 0o750)];
    r.on(OK, "fs.write", "/tool/new.sh", mode);
    let deep = "/deep/a/b/c/d/e/f/g/h/new.txt";
    r.on(OK, "fs.write", deep, content("deeper\n"));
    let deeper = "/deep/a/b/c/d/e/f/g/h/i/j/new.txt";
    r.on(no("NoSuchPath"), "fs.write", deeper, content("x"));
    r.on(OK, "fs.write", "/hard/h2", content("h2 written\n"));
    r.on(OK, "fs.write", "/hide/gone.txt", content("back\n"));
    let main = vec![b("main"), p("bad"), int("content", 1)];
    r.step("main", no("ReadOnly"), "fs.write", main);
    r.step("no branch", no("InvalidInput"), "fs.write", vec![p("/x")]);
    let ghost = vec![b("ghost"), p("/x"), text("content", "x")];
    r.step("bad branch", no("NoSuchBranch"), "fs.write", ghost);
    let nope = vec![b("work"), p("/x"), text("content", "x")];
    r.step_in("nope", "bad project", no("NoSuchProject"), "fs.write", nope);
    r.on(
        no("InvalidInput"),
        "fs.write",
        "bad",
        vec![int("content", 1)],
    );
    r.on(no("InvalidInput"), "fs.write", "bad", content("x"));
    r.on(no("IsDirectory"), "fs.write", "/src", content("x"));
    r.on(
        no("NotDirectory"),
        "fs.write",
        "/hard/orig.txt/x",
        content("x"),
    );

    let replace = |old, new| vec![text("oldText", old), text("newText", new)];
    let needle = replace("the needle line", "the NEEDLE line");
    r.on(OK, "fs.edit", "/text/big.txt", needle);
    let mut all = replace("3", "three");
    all.push(flag("replaceAll", true));
    r.on(OK, "fs.edit", "/same/r03/n3", all);
    let absent = replace("absent text", "x");
    r.on(no("NoMatch"), "fs.edit", "/text/big.txt", absent);
    r.on(
        no("NoSuchPath"),
        "fs.edit",
        "/hide/dir/c1.txt",
        replace("c1", "x"),
    );
    r.on(no("NotFile"), "fs.edit", "/src", replace("a", "x"));

    r.on(OK, "fs.mkdir", "/made", vec![]);
    r.on(OK, "fs.mkdir", "/made/x/y", vec![]);
    r.on(OK, "fs.mkdir", "/made", vec![]);
    r.on(no("NotDirectory"), "fs.mkdir", "/hard/orig.txt/d", vec![]);

    let pair = |from, to| vec![b("work"), text("from", from), text("to", to)];
    r.step(
        "cp file",
        OK,
        "fs.cp",
        pair("/src/d01/f004.txt", "/made/copy.txt"),
    );
    r.step("cp dir", OK, "fs.cp", pair("/src/d02", "/made/d02"));
    r.step(
        "cp symlink",
        OK,
        "fs.cp",
        pair("/links/chain", "/made/chain"),
    );
    r.step("cp hardlink", OK, "fs.cp", pair("/allhard/k6", "/made/k6"));
    let hidden = pair("/hide/dir/c2.txt", "/made/c2");
    r.step("cp hidden", no("NoSuchPath"), "fs.cp", hidden);
    r.step(
        "cp absent",
        no("NoSuchPath"),
        "fs.cp",
        pair("/absent", "/made/a"),
    );
    let onto = pair("/src/d01/f004.txt", "/made/copy.txt");
    r.step("cp onto a file", no("Exists"), "fs.cp", onto);

    r.step(
        "mv file",
        OK,
        "fs.mv",
        pair("/made/copy.txt", "/made/moved.txt"),
    );
    r.step("mv dir", OK, "fs.mv", pair("/made/d02", "/made/x/d02"));
    r.step(
        "mv absent",
        no("NoSuchPath"),
        "fs.mv",
        pair("/absent", "/made/b"),
    );

    r.on(OK, "fs.rm", "/made/moved.txt", vec![]);
    r.on(OK, "fs.rm", "/src/d05", vec![flag("recursive", true)]);
    r.on(OK, "fs.rm", "/same/r07", vec![]);
    r.on(
        no("NotEmpty"),
        "fs.rm",
        "/src/d04",
        vec![flag("recursive", false)],
    );
    r.on(no("NoSuchPath"), "fs.rm", "/absent", vec![]);

    let from_to = |from, to| vec![text("from", from), text("to", to)];
    let uno = vec![
        p("/applied/one.txt"),
        text("oldText", "one"),
        text("newText", "uno"),
    ];
    let changes = vec![
        change("mkdir", vec![p("/applied")]),
        change(
            "write",
            vec![p("/applied/one.txt"), text("content", "one\n")],
        ),
        change("mkdir", vec![p("/applied/dir")]),
        change("cp", from_to("/applied/one.txt", "/applied/dir/two")),
        change("edit", uno),
        change("chmod", vec![p("/applied/one.txt"), int("mode", 0o600)]),
        change("mv", from_to("/src/d04/f000.txt", "/applied/moved")),
        change("rm", vec![p("/src/d04/f001.txt")]),
    ];
    let apply = |changes| vec![b("work"), ("changes", SdkValue::List(changes))];
    r.step("apply", OK, "fs.apply", apply(changes));
    let failing = vec![
        change(
            "write",
            vec![p("/applied/three"), text("content", "three\n")],
        ),
        change("rm", vec![p("/applied/absent")]),
        change("write", vec![p("/applied/four"), text("content", "four\n")]),
    ];
    r.step(
        "apply refused at 1",
        no("NoSuchPath"),
        "fs.apply",
        apply(failing),
    );
    let streamed = vec![change("write", vec![p("/s.txt"), int("streamed", 5)])];
    r.step(
        "apply streamed",
        no("InvalidInput"),
        "fs.apply",
        apply(streamed),
    );
    let unknown = vec![change("link", vec![p("/s.txt")])];
    r.step(
        "apply unknown op",
        no("InvalidInput"),
        "fs.apply",
        apply(unknown),
    );

    for path in [
        "/hide/gone.txt",
        "/made",
        "/applied/one.txt",
        "/hard/h2",
        "/hard/h3",
        "/tool/new.sh",
    ] {
        r.on(OK, "fs.stat", path, vec![]);
    }
    r.on(OK, "fs.read", "/hide/gone.txt", vec![]);
    r.on(OK, "fs.list", "/hide", vec![]);
    r.on(OK, "fs.list", "/made/x/d02", vec![int("pageSize", 7)]);
    r.on(OK, "fs.glob", "/", vec![text("pattern", "**/*.sh")]);
    r.on(OK, "fs.grep", "/text", vec![text("pattern", "NEEDLE")]);
    r.on(OK, "fs.revision", "/hide/gone.txt", vec![]);
    r.on(OK, "fs.blame", "/hide/gone.txt", vec![]);
    r.step("stats work", OK, "project.stats", vec![b("work")]);
    r.step("stats main", OK, "project.stats", vec![]);
    let listed = vec![b("work"), int("limit", 100)];
    r.step("commits", OK, "branch.list_commits", listed);
}

fn fs_scenario(r: &mut Run) {
    reads(r);
    history(r);
    writes(r);
}

/// Reads and one write over the tree past the 4,096-row write window.
fn window_scenario(r: &mut Run) {
    r.step("fork work", OK, "branch.fork", vec![text("name", "work")]);
    for path in [
        "/flat/e04089",
        "/flat/e04095",
        "/flat/e04101",
        "/flat/e04102",
        "/flat/e03054",
        "/links/l13",
    ] {
        r.on(OK, "fs.stat", path, vec![]);
    }
    for page in [1, 82, 83, 84] {
        let page = vec![int("page", page), int("pageSize", 50)];
        r.on(OK, "fs.list", "/flat", page);
    }
    r.on(OK, "fs.list", "/links", vec![]);
    let pattern = vec![text("pattern", "e040*"), int("pageSize", 200)];
    r.on(OK, "fs.glob", "/flat", pattern);
    r.on(OK, "fs.read", "/links/l05", vec![]);
    r.on(
        OK,
        "fs.write",
        "/flat/e04093",
        vec![text("content", "alone\n")],
    );
    r.on(OK, "fs.read", "/flat/e04094", vec![]);
    r.step("stats", OK, "project.stats", vec![b("work")]);
}

#[test]
fn fs_calls_answer_alike_on_both_backends() {
    equivalence("fs", 83, build_main_tree, fs_scenario);
}

#[test]
fn init_past_the_write_window_answers_alike_on_both_backends() {
    equivalence("window", 85, build_window_tree, window_scenario);
}
