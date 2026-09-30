//! Real direct Service legacy-profile binding input; no process hardcap claim.
//! Native executable guard eligibility remains the separate R1e capability.
//! The allocation observer covers direct name-only receive, excluding later C1
//! graph/SQL work, caller fixtures, worker threads and native allocation.

#[path = "support/prepared_binding.rs"]
mod fixture;

use fixture::{Fixture, ORIGINAL};
use layerfs_bridge::contract::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    collections::BTreeSet,
    fs::OpenOptions,
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::Mutex,
};

static SERIAL: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Default)]
struct AllocationSpan {
    calls: usize,
    largest: usize,
}
thread_local! { static ALLOCATION: Cell<Option<AllocationSpan>> = const { Cell::new(None) }; }
fn observe_allocation(bytes: usize) {
    let _ = ALLOCATION.try_with(|span| {
        if let Some(mut active) = span.get() {
            active.calls += 1;
            active.largest = active.largest.max(bytes);
            span.set(Some(active));
        }
    });
}
struct ObservedSystem;
// SAFETY: all allocation operations forward unchanged to System. Only scalar
// counters on this thread change; no allocation or pointer inspection is added.
unsafe impl GlobalAlloc for ObservedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        observe_allocation(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        observe_allocation(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, bytes: usize) -> *mut u8 {
        observe_allocation(bytes);
        unsafe { System.realloc(pointer, layout, bytes) }
    }
}
#[global_allocator]
static GLOBAL: ObservedSystem = ObservedSystem;

fn spools() -> BTreeSet<PathBuf> {
    let prefix = format!("layerfs-prepared-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_str()
                .filter(|name| name.starts_with(&prefix))
                .map(|_| entry.path())
        })
        .collect()
}

#[derive(Clone, Copy)]
enum NativeFault {
    TruncateSlot,
    HeaderOffset,
}

struct Body<'a> {
    input: Cursor<&'a [u8]>,
    baseline: BTreeSet<PathBuf>,
    owned_spools: Vec<PathBuf>,
    calls: u64,
    bytes: u64,
    maximum_request: usize,
    first: bool,
    eof: bool,
    capture: bool,
    allocations: Option<AllocationSpan>,
    fault: Option<NativeFault>,
}

impl<'a> Body<'a> {
    fn new(bytes: &'a [u8], capture: bool, fault: Option<NativeFault>) -> Self {
        Self {
            input: Cursor::new(bytes),
            baseline: spools(),
            owned_spools: Vec::new(),
            calls: 0,
            bytes: 0,
            maximum_request: 0,
            first: true,
            eof: false,
            capture,
            allocations: None,
            fault,
        }
    }
    fn stop(&mut self) {
        if self.capture && self.allocations.is_none() {
            self.allocations = ALLOCATION.with(|span| span.replace(None));
        }
    }
    fn removed(&mut self) {
        self.stop();
        assert!(
            self.owned_spools.iter().all(|path| !path.exists()),
            "known completion removes owned prepared spool"
        );
        assert_eq!(spools(), self.baseline);
    }
}
impl Drop for Body<'_> {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Read for Body<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.first {
            self.first = false;
            self.owned_spools = spools().difference(&self.baseline).cloned().collect();
            assert_eq!(
                self.owned_spools.len(),
                1,
                "the real receive spool exists before the first body read"
            );
            // Observation setup is outside its own allocation span.
            if self.capture {
                ALLOCATION.with(|span| span.set(Some(AllocationSpan::default())));
            }
        }
        self.calls += 1;
        self.maximum_request = self.maximum_request.max(output.len());
        let bytes = self.input.read(output)?;
        self.bytes += bytes as u64;
        if bytes == 0 && !self.eof {
            self.eof = true;
            self.stop();
            if let Some(fault) = self.fault {
                let mut file = OpenOptions::new().write(true).open(&self.owned_spools[0])?;
                match fault {
                    NativeFault::TruncateSlot => file.set_len(79)?,
                    NativeFault::HeaderOffset => {
                        file.seek(SeekFrom::Start(56))?;
                        file.write_all(&1u64.to_be_bytes())?;
                    }
                }
            }
        }
        Ok(bytes)
    }
}

fn alias_names(count: usize, full_name: bool) -> Vec<Vec<u8>> {
    let mut names = (0..count)
        .map(|index| format!("alias-{index:06}").into_bytes())
        .collect::<Vec<_>>();
    if full_name {
        *names.last_mut().unwrap() = vec![b'z'; 255];
    }
    names
}

#[test]
fn wide_alias_stages_keep_exact_names_refs_base_bytes_and_bounded_name_receive() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("wide");
    for (index, count) in [15, 16, 17, 127, 128, 129, 1024, 4088, 4096, 4097, 4120]
        .into_iter()
        .enumerate()
    {
        let names = alias_names(count, count == 129);
        let bindings = names
            .iter()
            .map(|name| (name.clone(), Some(f.file_serial)))
            .collect();
        let (header, raw) = f.prepared([index as u8 + 1; 32], 1, &[(f.root_serial, bindings)], &[]);
        let mut body = Body::new(&raw, true, None);
        let stage = f.stage(header.clone(), &mut body).unwrap();
        body.removed();
        assert!(body.eof);
        assert_eq!(body.bytes, raw.len() as u64);
        assert!(body.maximum_request <= 255);
        let allocation = body.allocations.unwrap();
        assert!(
            allocation.largest <= 16 * 1024,
            "name-only receive allocated one {}B owner for {count} names",
            allocation.largest
        );
        let expected = std::iter::once((b"a".to_vec(), f.file_serial))
            .chain(names.iter().cloned().map(|name| (name, f.file_serial)))
            .collect::<Vec<_>>();
        assert_eq!(f.list(stage.candidate_root, b""), expected);
        let Response::Stat {
            serial,
            kind,
            references,
            content,
            metadata,
            ..
        } = f.stat(stage.candidate_root, b"a")
        else {
            panic!("stat")
        };
        assert_eq!(
            (serial, kind, references, content, metadata),
            (
                f.file_serial,
                1,
                count as u64 + 1,
                f.file_content,
                f.file_metadata
            )
        );
        assert_eq!(f.current_stage(header.workspace), stage);
        assert_eq!(
            (
                stage.expected_root,
                stage.construction_base_root,
                stage.scope
            ),
            (f.root, f.root, f.scope)
        );
        assert_eq!(f.list(f.root, b""), vec![(b"a".to_vec(), f.file_serial)]);
        assert_eq!(f.original_bytes(), ORIGINAL);
        eprintln!("DIAGNOSTIC direct-legacy name-receive names={count} body_bytes={} read_calls={} maximum_read={} allocation_calls={} maximum_single_rust_allocation={} native_spools=1 removed=true",
            body.bytes, body.calls, body.maximum_request, allocation.calls, allocation.largest);
    }
}

#[test]
fn empty_maintained_row_and_real_empty_declaration_complete_without_name_collection() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("empty");
    let (header, raw) = f.prepared([61; 32], 1, &[(f.root_serial, Vec::new())], &[]);
    let mut body = Body::new(&raw, false, None);
    let maintained = f.stage(header, &mut body).unwrap();
    body.removed();
    assert_eq!(
        maintained.candidate_root, f.root,
        "zero maintained bindings preserve canonical root"
    );
    let directory = f.reserve();
    let (header, raw) = f.prepared(
        [62; 32],
        1,
        &[
            (f.root_serial, vec![(b"d".to_vec(), Some(directory))]),
            (directory, Vec::new()),
        ],
        &[(directory, 0o755)],
    );
    let mut body = Body::new(&raw, false, None);
    let declared = f.stage(header, &mut body).unwrap();
    body.removed();
    assert_eq!(f.list(declared.candidate_root, b"d"), Vec::new());
    let Response::Stat {
        serial,
        kind,
        references,
        mode,
        ..
    } = f.stat(declared.candidate_root, b"d")
    else {
        panic!("directory")
    };
    assert_eq!((serial, kind, references, mode), (directory, 2, 1, 0o755));
    let missing = f.reserve();
    let (header, raw) = f.prepared(
        [63; 32],
        1,
        &[(
            f.root_serial,
            vec![(b"missing-row".to_vec(), Some(missing))],
        )],
        &[(missing, 0o755)],
    );
    let mut body = Body::new(&raw, false, None);
    let error = f.stage(header, &mut body).unwrap_err();
    body.removed();
    assert_eq!(error.code, Code::InvalidInput);
    assert!(!error.unknown);
}

#[test]
fn tombstone_and_unmentioned_names_preserve_distinct_alias_effects() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("tombstone");
    let (header, raw) = f.prepared(
        [71; 32],
        1,
        &[(
            f.root_serial,
            vec![
                (b"a".to_vec(), None),
                (b"b".to_vec(), Some(f.file_serial)),
                (b"ghost".to_vec(), None),
            ],
        )],
        &[],
    );
    let mut body = Body::new(&raw, false, None);
    let stage = f.stage(header, &mut body).unwrap();
    body.removed();
    assert_eq!(
        f.list(stage.candidate_root, b""),
        vec![(b"b".to_vec(), f.file_serial)]
    );
    let Response::Stat {
        references,
        content,
        metadata,
        ..
    } = f.stat(stage.candidate_root, b"b")
    else {
        panic!("file")
    };
    assert_eq!(
        (references, content, metadata),
        (1, f.file_content, f.file_metadata)
    );
    assert_eq!(f.list(f.root, b""), vec![(b"a".to_vec(), f.file_serial)]);
    assert_eq!(f.original_bytes(), ORIGINAL);
}

#[test]
fn malformed_global_eof_count_order_and_name_width_keep_the_prior_exact_stage() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("malformed");
    let (header, raw) = f.prepared(
        [81; 32],
        1,
        &[(f.root_serial, vec![(b"b".to_vec(), Some(f.file_serial))])],
        &[],
    );
    let mut body = Body::new(&raw, false, None);
    let original = f.stage(header, &mut body).unwrap();
    body.removed();
    for case in 0..4 {
        let names = if case == 2 {
            vec![b"z".to_vec(), b"b".to_vec()]
        } else if case == 3 {
            vec![vec![b'b'; 256], b"z".to_vec()]
        } else {
            vec![b"b".to_vec(), b"z".to_vec()]
        };
        let (mut header, mut raw) = f.prepared(
            [81; 32],
            case + 2,
            &[(
                f.root_serial,
                names
                    .into_iter()
                    .map(|name| (name, Some(f.file_serial)))
                    .collect(),
            )],
            &[],
        );
        if case == 0 {
            raw.push(9);
        }
        if case == 1 {
            header.totals.names += 1;
            header.totals.name_bytes += 11;
        }
        let mut body = Body::new(&raw, false, None);
        let error = f.stage(header, &mut body).unwrap_err();
        body.removed();
        assert_eq!(error.code, Code::InvalidInput, "malformed case{case}");
        assert!(!error.unknown);
        assert_eq!(f.current_stage([81; 32]), original);
        assert_eq!(f.original_bytes(), ORIGINAL);
    }
}

#[test]
fn declared_overflow_refuses_without_body_or_new_scratch_and_spool_effects() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("admission");
    let (mut header, _) = f.prepared([91; 32], 1, &[(f.root_serial, Vec::new())], &[]);
    header.totals.names = 65_537;
    header.totals.name_bytes = 65_537 * 11;
    let before = std::fs::read_dir(f.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<BTreeSet<_>>();
    let mut body = Body::new(&[], false, None);
    let error = f.stage(header, &mut body).unwrap_err();
    body.removed();
    assert_eq!(error.code, Code::Capacity);
    assert_eq!(body.calls, 0);
    assert!(body.owned_spools.is_empty());
    assert_eq!(
        std::fs::read_dir(f.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<BTreeSet<_>>(),
        before
    );
}

#[test]
fn externally_truncated_or_corrupt_owned_spool_fails_before_stage_publication() {
    let _serial = SERIAL.lock().unwrap();
    let f = Fixture::new("native-input-corrupt");
    let (header, raw) = f.prepared(
        [101; 32],
        1,
        &[(f.root_serial, vec![(b"b".to_vec(), Some(f.file_serial))])],
        &[],
    );
    let mut body = Body::new(&raw, false, None);
    let original = f.stage(header, &mut body).unwrap();
    body.removed();
    for (index, fault) in [NativeFault::TruncateSlot, NativeFault::HeaderOffset]
        .into_iter()
        .enumerate()
    {
        let (header, raw) = f.prepared(
            [101; 32],
            index as u64 + 2,
            &[(f.root_serial, vec![(b"z".to_vec(), Some(f.file_serial))])],
            &[],
        );
        let mut body = Body::new(&raw, false, Some(fault));
        let error = f.stage(header, &mut body).unwrap_err();
        body.removed();
        assert!(
            body.eof,
            "native fault follows acknowledged end_directory at exact EOF"
        );
        assert_eq!(error.code, Code::InvalidInput);
        assert!(!error.unknown);
        assert_eq!(f.current_stage([101; 32]), original);
        assert_eq!(f.original_bytes(), ORIGINAL);
    }
}
