use blake3::Hasher;
use layerfs_core::{
    cas::{InMemoryCas, PutOutcome},
    cdc::FastCdc,
    decode_object, decode_object_from, encode_object, encode_object_to, CanonicalName,
    CanonicalPath, ChunkReference, CoreError, DirectoryEntry, EditCounters, FullReplaceTiming,
    LogicalFile, Object, ObjectId, ObjectKind, ObjectReference,
};
use layerfs_os::{probe, HostEnvironment};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::ops::Range;
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const FORMAT_VERSION: u32 = 1;
const SEED: u64 = 0x4c41594552534653;
const BUFFER_SIZE: usize = 1024 * 1024;
const SINGLE_FILES: &[(&str, u64)] = &[
    ("S1-16", 16 * 1024 * 1024),
    ("S1-100", 100 * 1024 * 1024),
    ("S1-512", 512 * 1024 * 1024),
];
const TREE_FILE_COUNT: usize = 10_000;
const PHASE2_LAYOUT_WARMUPS: usize = 1;
const PHASE2_LAYOUT_ITERATIONS: usize = 3;
const PHASE2_RANGE_BYTES: u64 = 64 * 1024;
const SEGMENT_CHUNKS: usize = 64;
const TREE_LEAF_CHUNKS: usize = 64;
const TREE_FANOUT: usize = 16;

type EvalResult<T> = Result<T, String>;

#[derive(Debug, Clone)]
struct FileManifest {
    path: String,
    size: u64,
    blake3: String,
}

#[derive(Debug, Clone)]
struct DatasetManifest {
    id: String,
    files: Vec<FileManifest>,
    empty_dirs: Vec<String>,
    root_input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeEntry {
    kind: String,
    size: u64,
    blake3: Option<String>,
    target: Option<String>,
}

#[derive(Debug)]
struct Mismatch {
    path: String,
    issue: String,
    expected: Option<TreeEntry>,
    actual: Option<TreeEntry>,
}

fn main() {
    let result = match env::args().nth(1).as_deref() {
        Some("b0") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval b0 <run-directory>".to_owned())
            .and_then(|path| run_b0(Path::new(&path)).map(|_| 0)),
        Some("dataset") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval dataset <dataset-directory>".to_owned())
            .and_then(|path| {
                let output = Path::new(&path);
                prepare_empty_directory(output)?;
                let manifest = generate_dataset_set(output)?;
                write_text(&output.join("dataset.json"), &dataset_set_json(&manifest))?;
                Ok(0)
            }),
        Some("probe") => {
            let directory = env::args().nth(2);
            let output = env::args().nth(3);
            match (directory, output) {
                (Some(directory), Some(output)) => {
                    write_probe(Path::new(&directory), Path::new(&output))
                }
                _ => Err("usage: layerfs-eval probe <directory> <output-json>".to_owned()),
            }
        }
        Some("phase1") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval phase1 <run-directory>".to_owned())
            .and_then(|path| run_phase1(Path::new(&path)).map(|_| 0)),
        Some("phase2-layout") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval phase2-layout <run-directory>".to_owned())
            .and_then(|path| run_phase2_layout(Path::new(&path)).map(|_| 0)),
        Some("phase2-edits") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval phase2-edits <run-directory>".to_owned())
            .and_then(|path| run_phase2_edits(Path::new(&path)).map(|_| 0)),
        Some("phase2-ingest-breakdown") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval phase2-ingest-breakdown <run-directory>".to_owned())
            .and_then(|path| run_phase2_ingest_breakdown(Path::new(&path)).map(|_| 0)),
        Some("phase2-ingest-file") => env::args()
            .nth(2)
            .ok_or_else(|| "usage: layerfs-eval phase2-ingest-file <run-directory>".to_owned())
            .and_then(|path| run_phase2_ingest_file(Path::new(&path)).map(|_| 0)),
        Some("oracle") => {
            let expected = env::args().nth(2);
            let actual = env::args().nth(3);
            let output = env::args().nth(4);
            match (expected, actual, output) {
                (Some(expected), Some(actual), Some(output)) => {
                    run_oracle(Path::new(&expected), Path::new(&actual), Path::new(&output))
                }
                _ => Err(
                    "usage: layerfs-eval oracle <expected-directory> <actual-directory> <output-json>"
                        .to_owned(),
                ),
            }
        }
        Some("help") | None => {
            print_help();
            Ok(0)
        }
        Some(command) => Err(format!("unknown command: {command}")),
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!("layerfs-eval b0 <run-directory>");
    println!("layerfs-eval dataset <dataset-directory>");
    println!("layerfs-eval probe <directory> <output-json>");
    println!("layerfs-eval phase1 <run-directory>");
    println!("layerfs-eval phase2-layout <run-directory>");
    println!("layerfs-eval phase2-edits <run-directory>");
    println!("layerfs-eval phase2-ingest-breakdown <run-directory>");
    println!("layerfs-eval phase2-ingest-file <run-directory>");
    println!("layerfs-eval oracle <expected-directory> <actual-directory> <output-json>");
}

const PHASE1_WARMUPS: usize = 1;
const PHASE1_ITERATIONS: usize = 5;
const PHASE1_BYTE_SIZES: &[usize] = &[1024, 1024 * 1024, 8 * 1024 * 1024];
const PHASE1_DIRECTORY_FANOUTS: &[usize] = &[16, 256, 4096];

struct LayoutFixture {
    cas: InMemoryCas,
    references: Vec<ChunkReference>,
    length: u64,
    source_fingerprint: String,
    cdc_bytes_scanned: u64,
    chunks_created: u64,
    cas_stored_bytes: u64,
}

struct DeterministicReader {
    total: u64,
    offset: u64,
    salt: String,
    hasher: Hasher,
    block: Vec<u8>,
    block_offset: u64,
    block_length: usize,
    block_position: usize,
}

impl DeterministicReader {
    fn new(total: u64, salt: &str) -> Self {
        Self {
            total,
            offset: 0,
            salt: salt.to_owned(),
            hasher: Hasher::new(),
            block: vec![0_u8; BUFFER_SIZE],
            block_offset: 0,
            block_length: 0,
            block_position: 0,
        }
    }

    fn fingerprint(&self) -> String {
        self.hasher.clone().finalize().to_hex().to_string()
    }
}

impl Read for DeterministicReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.offset == self.total || output.is_empty() {
            return Ok(0);
        }
        if self.block_position == self.block_length {
            let block_size = u64::try_from(BUFFER_SIZE)
                .map_err(|_| io::Error::other("deterministic reader buffer overflow"))?;
            self.block_offset = (self.offset / block_size)
                .checked_mul(block_size)
                .ok_or_else(|| io::Error::other("deterministic reader block overflow"))?;
            let remaining = self
                .total
                .checked_sub(self.block_offset)
                .ok_or_else(|| io::Error::other("deterministic reader block underflow"))?;
            self.block_length = usize::try_from(remaining.min(block_size))
                .map_err(|_| io::Error::other("deterministic reader length overflow"))?;
            fill_buffer(
                &mut self.block[..self.block_length],
                self.block_offset,
                &self.salt,
            );
            self.block_position = 0;
        }
        let length = output
            .len()
            .min(self.block_length.saturating_sub(self.block_position));
        output[..length]
            .copy_from_slice(&self.block[self.block_position..self.block_position + length]);
        self.hasher.update(&output[..length]);
        self.block_position += length;
        self.offset =
            self.offset
                .checked_add(u64::try_from(length).map_err(|_| {
                    io::Error::other("deterministic reader length conversion overflow")
                })?)
                .ok_or_else(|| io::Error::other("deterministic reader offset overflow"))?;
        Ok(length)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LayoutReadStats {
    metadata_nodes_visited: u64,
    chunk_refs_inspected: u64,
    chunks_read: u64,
    bytes_delivered: u64,
}

struct LayoutRead {
    bytes: Vec<u8>,
    stats: LayoutReadStats,
}

struct FlatManifest {
    references: Vec<ChunkReference>,
    length: u64,
}

impl FlatManifest {
    fn from_references(references: Vec<ChunkReference>, length: u64) -> Self {
        Self { references, length }
    }

    fn read_range(&self, cas: &InMemoryCas, range: Range<u64>) -> EvalResult<LayoutRead> {
        validate_layout_range(range.clone(), self.length)?;
        let mut stats = LayoutReadStats {
            metadata_nodes_visited: 1,
            ..LayoutReadStats::default()
        };
        let bytes = read_chunk_references(cas, &self.references, 0, range, &mut stats)?;
        finish_layout_read(bytes, stats)
    }
}

struct Segment {
    start: u64,
    end: u64,
    references: Vec<ChunkReference>,
}

struct SegmentedManifest {
    segments: Vec<Segment>,
    length: u64,
}

impl SegmentedManifest {
    fn from_references(references: &[ChunkReference]) -> EvalResult<Self> {
        let mut segments = Vec::new();
        let mut offset = 0_u64;
        for slice in references.chunks(SEGMENT_CHUNKS) {
            let start = offset;
            for reference in slice {
                offset = offset
                    .checked_add(reference.length())
                    .ok_or_else(|| "segmented layout length overflow".to_owned())?;
            }
            segments.push(Segment {
                start,
                end: offset,
                references: slice.to_vec(),
            });
        }
        Ok(Self {
            segments,
            length: offset,
        })
    }

    fn first_segment(&self, offset: u64, stats: &mut LayoutReadStats) -> usize {
        let mut low = 0;
        let mut high = self.segments.len();
        while low < high {
            let middle = low + (high - low) / 2;
            stats.metadata_nodes_visited += 1;
            if self.segments[middle].end <= offset {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    fn read_range(&self, cas: &InMemoryCas, range: Range<u64>) -> EvalResult<LayoutRead> {
        validate_layout_range(range.clone(), self.length)?;
        let mut stats = LayoutReadStats {
            metadata_nodes_visited: 1,
            ..LayoutReadStats::default()
        };
        let mut bytes = Vec::new();
        if range.start != range.end {
            let mut index = self.first_segment(range.start, &mut stats);
            while let Some(segment) = self.segments.get(index) {
                stats.metadata_nodes_visited = stats
                    .metadata_nodes_visited
                    .checked_add(1)
                    .ok_or_else(|| "segmented metadata counter overflow".to_owned())?;
                if segment.start >= range.end {
                    break;
                }
                let segment_range = range.start.max(segment.start)..range.end.min(segment.end);
                if segment_range.start < segment_range.end {
                    let segment_bytes = read_chunk_references(
                        cas,
                        &segment.references,
                        segment.start,
                        segment_range,
                        &mut stats,
                    )?;
                    bytes.extend_from_slice(&segment_bytes);
                }
                index += 1;
            }
        }
        finish_layout_read(bytes, stats)
    }
}

enum ContentCandidate {
    Flat(FlatManifest),
    Segmented(SegmentedManifest),
    Tree(FanoutTree),
}

impl ContentCandidate {
    fn name(&self) -> &'static str {
        match self {
            Self::Flat(_) => "flat-manifest",
            Self::Segmented(_) => "segmented-64-chunks",
            Self::Tree(_) => "fixed-fanout-16-tree",
        }
    }

    fn read_range(&self, cas: &InMemoryCas, range: Range<u64>) -> EvalResult<LayoutRead> {
        match self {
            Self::Flat(layout) => layout.read_range(cas, range),
            Self::Segmented(layout) => layout.read_range(cas, range),
            Self::Tree(layout) => layout.read_range(cas, range),
        }
    }
}

enum TreeNode {
    Leaf {
        start: u64,
        end: u64,
        references: Vec<ChunkReference>,
    },
    Branch {
        start: u64,
        end: u64,
        children: Vec<TreeNode>,
    },
}

impl TreeNode {
    fn start(&self) -> u64 {
        match self {
            Self::Leaf { start, .. } | Self::Branch { start, .. } => *start,
        }
    }

    fn end(&self) -> u64 {
        match self {
            Self::Leaf { end, .. } | Self::Branch { end, .. } => *end,
        }
    }

    fn read_into(
        &self,
        cas: &InMemoryCas,
        range: Range<u64>,
        stats: &mut LayoutReadStats,
        output: &mut Vec<u8>,
    ) -> EvalResult<()> {
        stats.metadata_nodes_visited = stats
            .metadata_nodes_visited
            .checked_add(1)
            .ok_or_else(|| "tree metadata counter overflow".to_owned())?;
        match self {
            Self::Leaf {
                start, references, ..
            } => {
                let bytes = read_chunk_references(cas, references, *start, range, stats)?;
                output.extend_from_slice(&bytes);
            }
            Self::Branch { children, .. } => {
                for child in children {
                    if child.end() <= range.start || child.start() >= range.end {
                        stats.metadata_nodes_visited = stats
                            .metadata_nodes_visited
                            .checked_add(1)
                            .ok_or_else(|| "tree metadata counter overflow".to_owned())?;
                        continue;
                    }
                    child.read_into(cas, range.clone(), stats, output)?;
                }
            }
        }
        Ok(())
    }
}

struct FanoutTree {
    root: TreeNode,
    length: u64,
}

impl FanoutTree {
    fn from_references(references: &[ChunkReference]) -> EvalResult<Self> {
        let mut level = Vec::new();
        let mut offset = 0_u64;
        for slice in references.chunks(TREE_LEAF_CHUNKS) {
            let start = offset;
            for reference in slice {
                offset = offset
                    .checked_add(reference.length())
                    .ok_or_else(|| "tree length overflow".to_owned())?;
            }
            level.push(TreeNode::Leaf {
                start,
                end: offset,
                references: slice.to_vec(),
            });
        }
        if level.is_empty() {
            level.push(TreeNode::Leaf {
                start: 0,
                end: 0,
                references: Vec::new(),
            });
        }
        while level.len() > 1 {
            let mut next = Vec::new();
            while !level.is_empty() {
                let take = TREE_FANOUT.min(level.len());
                let children = level.drain(..take).collect::<Vec<_>>();
                let start = children
                    .first()
                    .map(|child| child.start())
                    .ok_or_else(|| "tree branch without children".to_owned())?;
                let end = children
                    .last()
                    .map(|child| child.end())
                    .ok_or_else(|| "tree branch without children".to_owned())?;
                next.push(TreeNode::Branch {
                    start,
                    end,
                    children,
                });
            }
            level = next;
        }
        let root = level.pop().ok_or_else(|| "tree without root".to_owned())?;
        Ok(Self {
            root,
            length: offset,
        })
    }

    fn read_range(&self, cas: &InMemoryCas, range: Range<u64>) -> EvalResult<LayoutRead> {
        validate_layout_range(range.clone(), self.length)?;
        let capacity = usize::try_from(
            range
                .end
                .checked_sub(range.start)
                .ok_or_else(|| "tree range length underflow".to_owned())?,
        )
        .map_err(|_| "tree range length does not fit usize".to_owned())?;
        let mut bytes = Vec::with_capacity(capacity);
        let mut stats = LayoutReadStats::default();
        if range.start != range.end {
            self.root.read_into(cas, range, &mut stats, &mut bytes)?;
        }
        finish_layout_read(bytes, stats)
    }
}

fn build_layout_fixture(id: &str, size: u64) -> EvalResult<LayoutFixture> {
    let mut source = DeterministicReader::new(size, id);
    let mut cas = InMemoryCas::new();
    let mut references = Vec::new();
    let mut chunks_created = 0_u64;
    let cdc_counters = FastCdc::new()
        .scan(&mut source, |chunk| {
            let (id, outcome) = cas.put_chunk(chunk)?;
            if outcome == PutOutcome::Inserted {
                chunks_created = chunks_created
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
            }
            let length = u64::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?;
            references.push(ChunkReference::new(id, length));
            Ok(())
        })
        .map_err(|error| format!("build layout fixture: {error}"))?;
    Ok(LayoutFixture {
        cas_stored_bytes: cas.stored_bytes(),
        cas,
        references,
        length: size,
        source_fingerprint: source.fingerprint(),
        cdc_bytes_scanned: cdc_counters.bytes_scanned,
        chunks_created,
    })
}

fn validate_layout_range(range: Range<u64>, length: u64) -> EvalResult<()> {
    if range.start > range.end || range.end > length {
        return Err(format!(
            "invalid layout range {}..{} for length {length}",
            range.start, range.end
        ));
    }
    Ok(())
}

fn read_chunk_references(
    cas: &InMemoryCas,
    references: &[ChunkReference],
    start_offset: u64,
    range: Range<u64>,
    stats: &mut LayoutReadStats,
) -> EvalResult<Vec<u8>> {
    let requested = range
        .end
        .checked_sub(range.start)
        .ok_or_else(|| "layout range arithmetic underflow".to_owned())?;
    let capacity = usize::try_from(requested)
        .map_err(|_| "layout range length does not fit usize".to_owned())?;
    let mut output = Vec::with_capacity(capacity);
    let mut offset = start_offset;
    for reference in references {
        stats.chunk_refs_inspected = stats
            .chunk_refs_inspected
            .checked_add(1)
            .ok_or_else(|| "layout reference counter overflow".to_owned())?;
        let chunk_end = offset
            .checked_add(reference.length())
            .ok_or_else(|| "layout chunk offset overflow".to_owned())?;
        if offset >= range.end {
            break;
        }
        if chunk_end <= range.start {
            offset = chunk_end;
            continue;
        }
        let chunk = cas
            .get(reference.id())
            .map_err(|error| format!("layout CAS read: {error}"))?;
        let actual = u64::try_from(chunk.len())
            .map_err(|_| "layout chunk length does not fit u64".to_owned())?;
        if actual != reference.length() {
            return Err(format!(
                "layout chunk length mismatch: expected {}, got {actual}",
                reference.length()
            ));
        }
        let local_start = range.start.saturating_sub(offset);
        let local_end = range
            .end
            .min(chunk_end)
            .checked_sub(offset)
            .ok_or_else(|| "layout chunk range underflow".to_owned())?;
        let local_start = usize::try_from(local_start)
            .map_err(|_| "layout chunk start does not fit usize".to_owned())?;
        let local_end = usize::try_from(local_end)
            .map_err(|_| "layout chunk end does not fit usize".to_owned())?;
        output.extend_from_slice(&chunk[local_start..local_end]);
        stats.chunks_read = stats
            .chunks_read
            .checked_add(1)
            .ok_or_else(|| "layout chunk counter overflow".to_owned())?;
        offset = chunk_end;
    }
    Ok(output)
}

fn finish_layout_read(bytes: Vec<u8>, mut stats: LayoutReadStats) -> EvalResult<LayoutRead> {
    stats.bytes_delivered = u64::try_from(bytes.len())
        .map_err(|_| "layout delivered-byte counter overflow".to_owned())?;
    Ok(LayoutRead { bytes, stats })
}

fn layout_ranges(size: u64) -> EvalResult<Vec<(&'static str, Range<u64>)>> {
    let length = PHASE2_RANGE_BYTES.min(size);
    let middle_start = size
        .checked_div(2)
        .and_then(|offset| offset.checked_sub(length / 2))
        .ok_or_else(|| "layout middle range underflow".to_owned())?;
    let eof_start = size
        .checked_sub(length)
        .ok_or_else(|| "layout EOF range underflow".to_owned())?;
    let middle_end = middle_start
        .checked_add(length)
        .ok_or_else(|| "layout middle range overflow".to_owned())?;
    Ok(vec![
        ("prefix", 0..length),
        ("middle", middle_start..middle_end),
        ("eof", eof_start..size),
    ])
}

fn expected_layout_range(id: &str, range: Range<u64>) -> EvalResult<Vec<u8>> {
    let length = range
        .end
        .checked_sub(range.start)
        .ok_or_else(|| "layout expected range underflow".to_owned())?;
    let mut bytes = vec![
        0_u8;
        usize::try_from(length)
            .map_err(|_| "layout expected range does not fit usize".to_owned())?
    ];
    let block_size = u64::try_from(BUFFER_SIZE)
        .map_err(|_| "layout expected buffer size overflow".to_owned())?;
    let mut offset = range.start;
    let mut block = vec![0_u8; BUFFER_SIZE];
    while offset < range.end {
        let block_start = (offset / block_size)
            .checked_mul(block_size)
            .ok_or_else(|| "layout expected block overflow".to_owned())?;
        let block_end = range.end.min(
            block_start
                .checked_add(block_size)
                .ok_or_else(|| "layout expected block end overflow".to_owned())?,
        );
        let block_length = usize::try_from(
            block_end
                .checked_sub(block_start)
                .ok_or_else(|| "layout expected block underflow".to_owned())?,
        )
        .map_err(|_| "layout expected block length overflow".to_owned())?;
        fill_buffer(&mut block[..block_length], block_start, id);
        let source_start = usize::try_from(offset - block_start)
            .map_err(|_| "layout expected source offset overflow".to_owned())?;
        let output_start = usize::try_from(offset - range.start)
            .map_err(|_| "layout expected output offset overflow".to_owned())?;
        bytes[output_start..output_start + block_length - source_start]
            .copy_from_slice(&block[source_start..block_length]);
        offset = block_end;
    }
    Ok(bytes)
}

struct LayoutBenchmarkRun {
    dataset: String,
    layout: &'static str,
    range_name: &'static str,
    file_size: u64,
    range: Range<u64>,
    source_fingerprint: String,
    cdc_bytes_scanned: u64,
    chunks_created: u64,
    cas_stored_bytes: u64,
    stats: LayoutReadStats,
    elapsed_ns: Vec<u128>,
    correct: bool,
}

fn measure_layout(
    candidate: &ContentCandidate,
    fixture: &LayoutFixture,
    dataset: &str,
    range_name: &'static str,
    range: Range<u64>,
    expected: &[u8],
) -> EvalResult<LayoutBenchmarkRun> {
    let mut measured_stats = None;
    for _ in 0..PHASE2_LAYOUT_WARMUPS {
        let read = candidate.read_range(&fixture.cas, range.clone())?;
        if read.bytes != expected {
            return Err(format!(
                "layout correctness failed during warm-up: {} {range_name}: {}",
                candidate.name(),
                layout_mismatch(&read.bytes, expected)
            ));
        }
        measured_stats = Some(read.stats);
    }
    let mut elapsed_ns = Vec::with_capacity(PHASE2_LAYOUT_ITERATIONS);
    for _ in 0..PHASE2_LAYOUT_ITERATIONS {
        let start = Instant::now();
        let read = candidate.read_range(&fixture.cas, range.clone())?;
        elapsed_ns.push(start.elapsed().as_nanos());
        if read.bytes != expected {
            return Err(format!(
                "layout correctness failed: {} {range_name}: {}",
                candidate.name(),
                layout_mismatch(&read.bytes, expected)
            ));
        }
        if let Some(stats) = measured_stats {
            if stats != read.stats {
                return Err(format!(
                    "layout counters were nondeterministic: {} {range_name}",
                    candidate.name()
                ));
            }
        }
        measured_stats = Some(read.stats);
        std::hint::black_box(read.bytes.len());
    }
    Ok(LayoutBenchmarkRun {
        dataset: dataset.to_owned(),
        layout: candidate.name(),
        range,
        range_name,
        file_size: fixture.length,
        source_fingerprint: fixture.source_fingerprint.clone(),
        cdc_bytes_scanned: fixture.cdc_bytes_scanned,
        chunks_created: fixture.chunks_created,
        cas_stored_bytes: fixture.cas_stored_bytes,
        stats: measured_stats.ok_or_else(|| "layout benchmark produced no counters".to_owned())?,
        elapsed_ns,
        correct: true,
    })
}

fn layout_mismatch(actual: &[u8], expected: &[u8]) -> String {
    let first_difference = actual
        .iter()
        .zip(expected)
        .position(|(actual, expected)| actual != expected);
    format!(
        "actual_len={}, expected_len={}, first_difference={first_difference:?}",
        actual.len(),
        expected.len(),
    )
}

fn run_phase2_layout(run_directory: &Path) -> EvalResult<()> {
    prepare_empty_directory(run_directory)?;
    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;
    let source = git_metadata();
    let mut runs = Vec::new();
    for &(dataset, size) in SINGLE_FILES {
        let fixture = build_layout_fixture(dataset, size)?;
        let flat = ContentCandidate::Flat(FlatManifest::from_references(
            fixture.references.clone(),
            fixture.length,
        ));
        let segmented =
            ContentCandidate::Segmented(SegmentedManifest::from_references(&fixture.references)?);
        let tree = ContentCandidate::Tree(FanoutTree::from_references(&fixture.references)?);
        let candidates = [flat, segmented, tree];
        for (range_name, range) in layout_ranges(size)? {
            let expected = expected_layout_range(dataset, range.clone())?;
            for candidate in &candidates {
                runs.push(measure_layout(
                    candidate,
                    &fixture,
                    dataset,
                    range_name,
                    range.clone(),
                    &expected,
                )?);
            }
        }
    }
    let results = runs
        .iter()
        .map(|run| phase2_layout_run_json(run, &source))
        .collect::<Vec<_>>()
        .join("\n");
    write_text(
        &run_directory.join("results.jsonl"),
        &format!("{results}\n"),
    )?;
    write_text(
        &run_directory.join("summary.md"),
        &phase2_layout_summary(&runs),
    )?;
    Ok(())
}

fn phase2_layout_run_json(run: &LayoutBenchmarkRun, source: &GitMetadata) -> String {
    let elapsed = run
        .elapsed_ns
        .iter()
        .map(u128::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"format_version\":1,\"benchmark\":\"phase2-layout-selection-baseline\",\"case\":\"layout-range-read\",\"dataset\":{},\"layout\":{},\"range_name\":{},\"file_size_bytes\":{},\"range_start\":{},\"range_bytes\":{},\"source_fingerprint\":{},\"cdc_bytes_scanned\":{},\"chunks_created\":{},\"cas_stored_bytes\":{},\"metadata_nodes_visited\":{},\"chunk_refs_inspected\":{},\"chunks_read\":{},\"bytes_delivered\":{},\"iterations\":{},\"elapsed_ns\":[{}],\"peak_memory_bytes\":null,\"correct\":{},\"source_commit\":{},\"dirty_tree\":{},\"performance_claim\":\"layout-selection-baseline-only\"}}",
        json_string(&run.dataset),
        json_string(run.layout),
        json_string(run.range_name),
        run.file_size,
        run.range.start,
        run.range.end - run.range.start,
        json_string(&run.source_fingerprint),
        run.cdc_bytes_scanned,
        run.chunks_created,
        run.cas_stored_bytes,
        run.stats.metadata_nodes_visited,
        run.stats.chunk_refs_inspected,
        run.stats.chunks_read,
        run.stats.bytes_delivered,
        run.elapsed_ns.len(),
        elapsed,
        run.correct,
        json_option_string(source.commit.as_deref()),
        source.dirty_tree,
    )
}

fn phase2_layout_summary(runs: &[LayoutBenchmarkRun]) -> String {
    let mut summary = String::from(
        "# Phase 2 layout-selection baseline\n\n\
         This artifact compares three simple in-memory logical layouts over the\n\
         same authenticated CDC chunk references: a flat manifest, fixed-size\n\
         64-chunk segments, and a fixed-fanout-16 tree with 64-chunk leaves.\n\n\
         It measures deterministic 64 KiB prefix, middle, and EOF range reads\n\
         for S1-16, S1-100, and S1-512. `metadata_nodes_visited`,\n\
         `chunk_refs_inspected`, and `chunks_read` are layout-operation counters;\n\
         `cdc_bytes_scanned` and `chunks_created` describe fixture construction.\n\n\
         This is a layout-selection baseline, not a final performance claim. It\n\
         does not freeze canonical content encodings and does not qualify B6/B7/B8,\n\
         SQLite, concurrency, or process-wide memory. Peak memory remains an\n\
         external observation.\n\n\
         | Dataset | Layout | Range | Median ns | Metadata nodes | Chunk refs | Chunks read | Correct |\n|---|---|---|---:|---:|---:|---:|:---:|\n",
    );
    for run in runs {
        let _ = writeln!(
            summary,
            "| {} | `{}` | `{}` | {} | {} | {} | {} | {} |",
            run.dataset,
            run.layout,
            run.range_name,
            median(&run.elapsed_ns),
            run.stats.metadata_nodes_visited,
            run.stats.chunk_refs_inspected,
            run.stats.chunks_read,
            run.correct,
        );
    }
    summary.push_str(
        "\nEvery result includes the source fingerprint, source metadata, exact range\n\
         correctness, and the deterministic layout counters in `results.jsonl`.\n",
    );
    summary
}

struct EditRun {
    case: &'static str,
    dataset: String,
    operation: &'static str,
    file_size: u64,
    range: Range<u64>,
    replacement_bytes: u64,
    final_size: u64,
    source_fingerprint: String,
    counters: EditCounters,
    cas_stored_bytes: u64,
    elapsed_ns: u128,
    correct: bool,
}

fn run_phase2_edits(run_directory: &Path) -> EvalResult<()> {
    prepare_empty_directory(run_directory)?;
    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;
    let source = git_metadata();
    let mut runs = Vec::new();

    for &(dataset, size) in SINGLE_FILES {
        let mut cas = InMemoryCas::new();
        let mut reader = DeterministicReader::new(size, dataset);
        let start = Instant::now();
        let full_replace = LogicalFile::full_replace(&mut cas, &mut reader)
            .map_err(|error| format!("{dataset} B6 full replace: {error}"))?;
        let elapsed_ns = start.elapsed().as_nanos();
        let source_fingerprint = reader.fingerprint();
        let correct = verify_logical_file(&cas, full_replace.file(), &source_fingerprint)?;
        runs.push(EditRun {
            case: "B6",
            dataset: dataset.to_owned(),
            operation: "full-replace",
            file_size: size,
            range: 0..0,
            replacement_bytes: size,
            final_size: full_replace.file().length(),
            source_fingerprint: source_fingerprint.clone(),
            counters: full_replace.counters(),
            cas_stored_bytes: cas.stored_bytes(),
            elapsed_ns,
            correct,
        });

        let middle = size
            .checked_div(2)
            .ok_or_else(|| format!("{dataset} B7 offset arithmetic failed"))?;
        let replacement = [0xa5_u8];
        let edit_start = middle
            .checked_sub(1)
            .ok_or_else(|| format!("{dataset} B7 middle offset underflow"))?;
        let start = Instant::now();
        let edited = full_replace
            .file()
            .replace_range(&mut cas, edit_start..middle, &replacement)
            .map_err(|error| format!("{dataset} B7 middle edit: {error}"))?;
        let elapsed_ns = start.elapsed().as_nanos();
        let expected = edited_fingerprint(dataset, size, edit_start..middle, &replacement)?;
        let correct = verify_logical_file(&cas, edited.file(), &expected)?;
        runs.push(EditRun {
            case: "B7",
            dataset: dataset.to_owned(),
            operation: "one-byte-middle-replacement",
            file_size: size,
            range: edit_start..middle,
            replacement_bytes: 1,
            final_size: edited.file().length(),
            source_fingerprint: source_fingerprint.clone(),
            counters: edited.counters(),
            cas_stored_bytes: cas.stored_bytes(),
            elapsed_ns,
            correct,
        });

        if dataset == "S1-100" {
            for edit in phase2_edit_shapes(size)? {
                let start = Instant::now();
                let edited = full_replace
                    .file()
                    .replace_range(&mut cas, edit.range.clone(), &edit.replacement)
                    .map_err(|error| format!("{dataset} B8 {}: {error}", edit.operation))?;
                let elapsed_ns = start.elapsed().as_nanos();
                let expected =
                    edited_fingerprint(dataset, size, edit.range.clone(), &edit.replacement)?;
                let correct = verify_logical_file(&cas, edited.file(), &expected)?;
                runs.push(EditRun {
                    case: "B8",
                    dataset: dataset.to_owned(),
                    operation: edit.operation,
                    file_size: size,
                    range: edit.range,
                    replacement_bytes: u64::try_from(edit.replacement.len())
                        .map_err(|_| "B8 replacement length overflow".to_owned())?,
                    final_size: edited.file().length(),
                    source_fingerprint: source_fingerprint.clone(),
                    counters: edited.counters(),
                    cas_stored_bytes: cas.stored_bytes(),
                    elapsed_ns,
                    correct,
                });
            }
        }
    }

    let results = runs
        .iter()
        .map(|run| phase2_edit_run_json(run, &source))
        .collect::<Vec<_>>()
        .join("\n");
    write_text(
        &run_directory.join("results.jsonl"),
        &format!("{results}\n"),
    )?;
    write_text(
        &run_directory.join("summary.md"),
        &phase2_edit_summary(&runs),
    )?;
    Ok(())
}

struct IngestBreakdownRun {
    dataset: String,
    file_size: u64,
    chunks: usize,
    source_fingerprint: String,
    counters: EditCounters,
    cas_stored_bytes: u64,
    timing: FullReplaceTiming,
    component_total_ns: u128,
    outer_elapsed_ns: u128,
    correct: bool,
    pipeline: &'static str,
}

fn run_phase2_ingest_breakdown(run_directory: &Path) -> EvalResult<()> {
    prepare_empty_directory(run_directory)?;
    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;
    let source = git_metadata();
    let (dataset, size) = SINGLE_FILES
        .iter()
        .find(|&&(dataset, _)| dataset == "S1-100")
        .copied()
        .ok_or_else(|| "S1-100 dataset is not configured".to_owned())?;

    let mut cas = InMemoryCas::new();
    let mut reader = DeterministicReader::new(size, dataset);
    let outer_start = Instant::now();
    let (full_replace, timing) = LogicalFile::full_replace_timed(&mut cas, &mut reader)
        .map_err(|error| format!("{dataset} full ingest: {error}"))?;
    let outer_elapsed_ns = outer_start.elapsed().as_nanos();
    let source_fingerprint = reader.fingerprint();
    let correct = verify_logical_file(&cas, full_replace.file(), &source_fingerprint)?;
    let component_total_ns = timing_component_total_ns(&timing)?;
    let run = IngestBreakdownRun {
        dataset: dataset.to_owned(),
        file_size: size,
        chunks: full_replace.file().chunks().len(),
        source_fingerprint,
        counters: full_replace.counters(),
        cas_stored_bytes: cas.stored_bytes(),
        timing,
        component_total_ns,
        outer_elapsed_ns,
        correct,
        pipeline:
            "DeterministicReader -> FastCDC -> full InMemoryCas::put_chunk -> LogicalFile manifest",
    };

    write_text(
        &run_directory.join("results.jsonl"),
        &format!("{}\n", phase2_ingest_breakdown_json(&run, &source)),
    )?;
    write_text(
        &run_directory.join("summary.md"),
        &phase2_ingest_breakdown_summary(&run),
    )?;
    Ok(())
}

fn run_phase2_ingest_file(run_directory: &Path) -> EvalResult<()> {
    prepare_empty_directory(run_directory)?;
    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;
    let source = git_metadata();
    let (dataset, size) = SINGLE_FILES
        .iter()
        .find(|&&(dataset, _)| dataset == "S1-100")
        .copied()
        .ok_or_else(|| "S1-100 dataset is not configured".to_owned())?;
    let source_path = run_directory.join("S1-100.source");
    let source_fingerprint = write_deterministic_source(&source_path, size, dataset)?;

    let mut cas = InMemoryCas::new();
    let mut reader = File::open(&source_path).map_err(io_message)?;
    let outer_start = Instant::now();
    let (full_replace, timing) = LogicalFile::full_replace_timed(&mut cas, &mut reader)
        .map_err(|error| format!("{dataset} file ingest: {error}"))?;
    let outer_elapsed_ns = outer_start.elapsed().as_nanos();
    let correct = verify_logical_file(&cas, full_replace.file(), &source_fingerprint)?;
    let component_total_ns = timing_component_total_ns(&timing)?;
    let run = IngestBreakdownRun {
        dataset: dataset.to_owned(),
        file_size: size,
        chunks: full_replace.file().chunks().len(),
        source_fingerprint,
        counters: full_replace.counters(),
        cas_stored_bytes: cas.stored_bytes(),
        timing,
        component_total_ns,
        outer_elapsed_ns,
        correct,
        pipeline: "regular file -> FastCDC -> full InMemoryCas::put_chunk -> LogicalFile manifest",
    };

    write_text(
        &run_directory.join("results.jsonl"),
        &format!("{}\n", phase2_ingest_breakdown_json(&run, &source)),
    )?;
    write_text(
        &run_directory.join("summary.md"),
        &phase2_ingest_breakdown_summary(&run),
    )?;
    Ok(())
}

fn write_deterministic_source(path: &Path, size: u64, dataset: &str) -> EvalResult<String> {
    let mut reader = DeterministicReader::new(size, dataset);
    let mut file = File::create(path).map_err(io_message)?;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer).map_err(io_message)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(io_message)?;
    }
    file.sync_all().map_err(io_message)?;
    Ok(reader.fingerprint())
}

fn timing_component_total_ns(timing: &FullReplaceTiming) -> EvalResult<u128> {
    [
        timing.source_read_ns,
        timing.cdc_ns,
        timing.cas_publish_ns,
        timing.manifest_ns,
    ]
    .into_iter()
    .try_fold(0_u128, |total, stage| {
        total
            .checked_add(stage)
            .ok_or_else(|| "ingest timing total overflow".to_owned())
    })
}

fn phase2_ingest_breakdown_json(run: &IngestBreakdownRun, source: &GitMetadata) -> String {
    format!(
        "{{\"format_version\":1,\"benchmark\":\"phase2-ingest-breakdown\",\"dataset\":{},\"file_size_bytes\":{},\"chunks\":{},\"source_fingerprint\":{},\"cdc_bytes_scanned\":{},\"chunks_created\":{},\"cas_stored_bytes\":{},\"source_read_ns\":{},\"cdc_ns\":{},\"cas_publish_ns\":{},\"manifest_ns\":{},\"component_total_ns\":{},\"outer_elapsed_ns\":{},\"correct\":{},\"source_commit\":{},\"dirty_tree\":{},\"pipeline\":{},\"storage_engine\":\"in-memory-btreemap-cas\"}}",
        json_string(&run.dataset),
        run.file_size,
        run.chunks,
        json_string(&run.source_fingerprint),
        run.counters.cdc_bytes_scanned,
        run.counters.chunks_created,
        run.cas_stored_bytes,
        run.timing.source_read_ns,
        run.timing.cdc_ns,
        run.timing.cas_publish_ns,
        run.timing.manifest_ns,
        run.component_total_ns,
        run.outer_elapsed_ns,
        run.correct,
        json_option_string(source.commit.as_deref()),
        source.dirty_tree,
        json_string(run.pipeline),
    )
}

fn phase2_ingest_breakdown_summary(run: &IngestBreakdownRun) -> String {
    let component_total = run.component_total_ns as f64;
    let row = |name: &str, ns: u128| {
        let percent = if component_total == 0.0 {
            0.0
        } else {
            (ns as f64 * 100.0) / component_total
        };
        format!(
            "| {name} | {} | {:.3} | {:.2}% |\n",
            ns,
            ns as f64 / 1_000_000.0,
            percent,
        )
    };
    let mut summary = format!(
        "# Phase 2 100 MiB full-ingest timing breakdown\n\n\
         Dataset: `{}` ({} bytes / 100 MiB). This is one fresh in-memory run of\n\
         the production full-replace path: deterministic source generation,\n\
         FastCDC chunking, full CAS publication for every emitted chunk, and\n\
         final logical-file manifest construction. Correctness was checked by\n\
         reading the resulting logical file and comparing its BLAKE3 fingerprint.\n\n\
         | Step | Nanoseconds | Milliseconds | Share of component total |\n|---|---:|---:|---:|\n",
        run.dataset, run.file_size,
    );
    summary.push_str(&row("Source read / input", run.timing.source_read_ns));
    summary.push_str(&row(
        "CDC scanner and callback bookkeeping",
        run.timing.cdc_ns,
    ));
    summary.push_str(&row(
        "CAS publication (hash, lookup, copy, insert/reuse)",
        run.timing.cas_publish_ns,
    ));
    summary.push_str(&row(
        "Logical-file manifest finalization",
        run.timing.manifest_ns,
    ));
    summary.push_str(&format!(
        "| **Component total (sum)** | **{}** | **{:.3}** | **100.00%** |\n\n\
         Outer `full_replace_timed` elapsed: **{} ns ({:.3} ms)**. The outer\n\
         timer is a cross-check; the four rows above are the additive stage\n\
         total. The small difference is timer/instrumentation and orchestration\n\
         overhead.\n\n\
         - CDC bytes scanned: `{}`\n\
         - Chunks created: `{}`\n\
         - CAS objects: `{}`\n\
         - CAS bytes stored: `{}`\n\
         - Correct: `{}`\n\
         - Storage engine: `InMemoryCas` backed by an in-memory `BTreeMap`\n",
        run.component_total_ns,
        run.component_total_ns as f64 / 1_000_000.0,
        run.outer_elapsed_ns,
        run.outer_elapsed_ns as f64 / 1_000_000.0,
        run.counters.cdc_bytes_scanned,
        run.counters.chunks_created,
        run.chunks,
        run.cas_stored_bytes,
        run.correct,
    ));
    summary
}

struct EditShape {
    operation: &'static str,
    range: Range<u64>,
    replacement: Vec<u8>,
}

fn phase2_edit_shapes(size: u64) -> EvalResult<Vec<EditShape>> {
    let middle = size
        .checked_div(2)
        .ok_or_else(|| "B8 middle offset arithmetic failed".to_owned())?;
    let middle_start = middle
        .checked_sub(1)
        .ok_or_else(|| "B8 middle offset underflow".to_owned())?;
    let truncate_start = size
        .checked_sub(64 * 1024)
        .ok_or_else(|| "B8 truncate range underflow".to_owned())?;
    Ok(vec![
        EditShape {
            operation: "equal-length-middle-replacement",
            range: middle_start..middle,
            replacement: vec![0xa5],
        },
        EditShape {
            operation: "prepend",
            range: 0..0,
            replacement: b"prepend".to_vec(),
        },
        EditShape {
            operation: "append",
            range: size..size,
            replacement: b"append".to_vec(),
        },
        EditShape {
            operation: "truncate",
            range: truncate_start..size,
            replacement: Vec::new(),
        },
        EditShape {
            operation: "eof-no-op",
            range: size..size,
            replacement: Vec::new(),
        },
    ])
}

fn verify_logical_file(
    cas: &InMemoryCas,
    file: &LogicalFile,
    expected_fingerprint: &str,
) -> EvalResult<bool> {
    let mut hasher = Hasher::new();
    let mut offset = 0_u64;
    while offset < file.length() {
        let end = file.length().min(
            offset
                .checked_add(BUFFER_SIZE as u64)
                .ok_or_else(|| "logical-file verification overflow".to_owned())?,
        );
        let read = file
            .read_range(cas, offset..end)
            .map_err(|error| format!("logical-file verification read: {error}"))?;
        hasher.update(read.bytes());
        offset = end;
    }
    Ok(hasher.finalize().to_hex().to_string() == expected_fingerprint)
}

fn edited_fingerprint(
    dataset: &str,
    size: u64,
    range: Range<u64>,
    replacement: &[u8],
) -> EvalResult<String> {
    let mut hasher = Hasher::new();
    update_expected_range(&mut hasher, dataset, 0..range.start)?;
    hasher.update(replacement);
    update_expected_range(&mut hasher, dataset, range.end..size)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn update_expected_range(hasher: &mut Hasher, dataset: &str, range: Range<u64>) -> EvalResult<()> {
    let mut offset = range.start;
    while offset < range.end {
        let end = range.end.min(
            offset
                .checked_add(BUFFER_SIZE as u64)
                .ok_or_else(|| "expected edit fingerprint overflow".to_owned())?,
        );
        let bytes = expected_layout_range(dataset, offset..end)?;
        hasher.update(&bytes);
        offset = end;
    }
    Ok(())
}

fn phase2_edit_run_json(run: &EditRun, source: &GitMetadata) -> String {
    format!(
        "{{\"format_version\":1,\"benchmark\":\"phase2-edit-baseline\",\"case\":{},\"dataset\":{},\"operation\":{},\"file_size_bytes\":{},\"range_start\":{},\"range_end\":{},\"replacement_bytes\":{},\"final_size_bytes\":{},\"source_fingerprint\":{},\"cdc_bytes_scanned\":{},\"chunks_reused\":{},\"chunks_created\":{},\"bytes_hashed\":{},\"bytes_delivered\":{},\"cas_stored_bytes\":{},\"elapsed_ns\":{},\"correct\":{},\"source_commit\":{},\"dirty_tree\":{},\"performance_claim\":\"in-memory-phase2-baseline-only\"}}",
        json_string(run.case),
        json_string(&run.dataset),
        json_string(run.operation),
        run.file_size,
        run.range.start,
        run.range.end,
        run.replacement_bytes,
        run.final_size,
        json_string(&run.source_fingerprint),
        run.counters.cdc_bytes_scanned,
        run.counters.chunks_reused,
        run.counters.chunks_created,
        run.counters.bytes_hashed,
        run.counters.bytes_delivered,
        run.cas_stored_bytes,
        run.elapsed_ns,
        run.correct,
        json_option_string(source.commit.as_deref()),
        source.dirty_tree,
    )
}

fn phase2_edit_summary(runs: &[EditRun]) -> String {
    let mut summary = String::from(
        "# Phase 2 in-memory edit baseline\n\n\
         This artifact exercises the real streaming CDC/CAS full-replace path\n\
         (B6), one-byte middle replacement scaling (B7), and the five S1-100\n\
         edit shapes (B8). Each case is a single cold in-memory operation; final\n\
         bytes are verified by deterministic BLAKE3 fingerprints.\n\n\
         The counters are the acceptance evidence: B7 must scan bounded bytes\n\
         relative to file size, and B8 must remain exact without an unbounded\n\
         fallback. This is not a durable-storage, concurrency, or final\n\
         performance claim.\n\n\
         | Case | Dataset | Operation | File bytes | CDC scanned | Reused | Created | Elapsed ns | Correct |\n|---|---|---|---:|---:|---:|---:|---:|:---:|\n",
    );
    for run in runs {
        let _ = writeln!(
            summary,
            "| {} | {} | `{}` | {} | {} | {} | {} | {} | {} |",
            run.case,
            run.dataset,
            run.operation,
            run.file_size,
            run.counters.cdc_bytes_scanned,
            run.counters.chunks_reused,
            run.counters.chunks_created,
            run.elapsed_ns,
            run.correct,
        );
    }
    summary.push_str(
        "\n`environment.json` records host/source metadata. `results.jsonl` retains\n\
         the exact range, final size, authenticated source fingerprint, all\n\
         required B6/B7/B8 counters, and correctness result for each operation.\n",
    );
    summary
}

#[derive(Debug)]
struct Phase1Run {
    case: String,
    input_bytes: usize,
    output_bytes: usize,
    iterations: usize,
    elapsed_ns: Vec<u128>,
    correct: bool,
}

fn run_phase1(run_directory: &Path) -> EvalResult<()> {
    prepare_phase1_directory(run_directory)?;
    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;

    let mut runs = Vec::new();
    for &size in PHASE1_BYTE_SIZES {
        let object = phase1_bytes_object(size)?;
        let encoded =
            encode_object(&object).map_err(|error| format!("encode fixture: {error:?}"))?;
        let expected_id = ObjectId::for_bytes(&encoded);
        let label = format!("bytes-{size}");

        runs.push(measure_phase1(
            format!("{label}/encode_vec"),
            encoded.len(),
            encoded.len(),
            || {
                encode_object(&object)
                    .is_ok_and(|value| std::hint::black_box(value.len()) == encoded.len())
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/encode_writer"),
            encoded.len(),
            encoded.len(),
            || {
                let mut output = Vec::with_capacity(encoded.len());
                encode_object_to(&object, &mut output)
                    .is_ok_and(|_| std::hint::black_box(output.as_slice()) == encoded.as_slice())
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/decode_slice"),
            encoded.len(),
            size,
            || decode_object(&encoded).is_ok_and(|value| value == object),
        )?);
        runs.push(measure_phase1(
            format!("{label}/decode_reader"),
            encoded.len(),
            size,
            || {
                decode_object_from(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == object)
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/hash_slice"),
            encoded.len(),
            32,
            || std::hint::black_box(ObjectId::for_bytes(&encoded)) == expected_id,
        )?);
        runs.push(measure_phase1(
            format!("{label}/hash_reader"),
            encoded.len(),
            32,
            || {
                ObjectId::from_reader(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == expected_id)
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/object_id"),
            size,
            32,
            || object.id().is_ok_and(|value| value == expected_id),
        )?);
    }

    for &fanout in PHASE1_DIRECTORY_FANOUTS {
        let object = phase1_directory_object(fanout)?;
        let encoded =
            encode_object(&object).map_err(|error| format!("encode fixture: {error:?}"))?;
        let expected_id = ObjectId::for_bytes(&encoded);
        let label = format!("directory-{fanout}");
        runs.push(measure_phase1(
            format!("{label}/encode_vec"),
            encoded.len(),
            encoded.len(),
            || {
                encode_object(&object)
                    .is_ok_and(|value| std::hint::black_box(value.len()) == encoded.len())
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/decode_reader"),
            encoded.len(),
            encoded.len(),
            || {
                decode_object_from(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == object)
            },
        )?);
        runs.push(measure_phase1(
            format!("{label}/hash_reader"),
            encoded.len(),
            32,
            || {
                ObjectId::from_reader(Cursor::new(encoded.as_slice()))
                    .is_ok_and(|value| value == expected_id)
            },
        )?);
    }

    for (label, path) in [
        ("path-short", "a/b/c".to_owned()),
        ("path-max", phase1_max_path()),
    ] {
        let path_bytes = path.len();
        runs.push(measure_phase1(
            format!("{label}/validate"),
            path_bytes,
            path_bytes,
            || CanonicalPath::new(&path).is_ok(),
        )?);
    }

    let results = runs
        .iter()
        .map(phase1_run_json)
        .collect::<Vec<_>>()
        .join("\n");
    write_text(
        &run_directory.join("results.jsonl"),
        &format!("{results}\n"),
    )?;
    write_text(&run_directory.join("summary.md"), &phase1_summary(&runs))?;
    Ok(())
}

fn phase1_bytes_object(size: usize) -> EvalResult<Object> {
    let mut bytes = vec![0_u8; size];
    fill_buffer(&mut bytes, 0, "phase1-bytes");
    Object::bytes(bytes).map_err(|error| format!("bytes fixture: {error:?}"))
}

fn phase1_directory_object(fanout: usize) -> EvalResult<Object> {
    let id = ObjectId::for_bytes(b"phase1-directory-child");
    let mut entries = Vec::with_capacity(fanout);
    for index in 0..fanout {
        let name = CanonicalName::new(&format!("entry-{index:05}"))
            .map_err(|error| format!("directory name fixture: {error:?}"))?;
        entries.push(DirectoryEntry::new(
            name,
            ObjectReference::new(ObjectKind::Bytes, id),
        ));
    }
    Object::directory(entries).map_err(|error| format!("directory fixture: {error:?}"))
}

fn phase1_max_path() -> String {
    (0..256)
        .map(|_| "abcdefghijklmno")
        .collect::<Vec<_>>()
        .join("/")
}

fn measure_phase1<F>(
    case: String,
    input_bytes: usize,
    output_bytes: usize,
    mut operation: F,
) -> EvalResult<Phase1Run>
where
    F: FnMut() -> bool,
{
    for _ in 0..PHASE1_WARMUPS {
        if !operation() {
            return Err(format!(
                "Phase 1 benchmark correctness failed during warm-up: {case}"
            ));
        }
    }
    let mut elapsed_ns = Vec::with_capacity(PHASE1_ITERATIONS);
    for _ in 0..PHASE1_ITERATIONS {
        let start = Instant::now();
        let correct = operation();
        elapsed_ns.push(start.elapsed().as_nanos());
        if !correct {
            return Err(format!("Phase 1 benchmark correctness failed: {case}"));
        }
    }
    Ok(Phase1Run {
        case,
        input_bytes,
        output_bytes,
        iterations: PHASE1_ITERATIONS,
        elapsed_ns,
        correct: true,
    })
}

fn phase1_run_json(run: &Phase1Run) -> String {
    let elapsed = run
        .elapsed_ns
        .iter()
        .map(u128::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"format_version\":1,\"case\":{},\"input_bytes\":{},\"output_bytes\":{},\"iterations\":{},\"elapsed_ns\":[{}],\"peak_memory_bytes\":null,\"peak_memory_status\":\"external_observation_required\",\"correct\":{}}}",
        json_string(&run.case),
        run.input_bytes,
        run.output_bytes,
        run.iterations,
        elapsed,
        run.correct,
    )
}

fn phase1_summary(runs: &[Phase1Run]) -> String {
    let mut summary = String::from(
        "# Phase 1 canonical-object baseline\n\n\
         This is a correctness-preserving microbenchmark for the Phase 1 core.\n\n\
         It measures bounded path validation, canonical encode/decode, and\n\
         BLAKE3 identity work for representative byte and directory objects.\n\n\
         It does not measure CDC, CAS, SQLite, materialization, or large-file\n\
         small-edit behavior; those remain Phase 2 and later gates.\n\n\
         Each case has one warm-up and five measured iterations. `peak_memory_bytes`\n\
         remains explicitly unavailable because this process does not sample RSS.\n\
         Capture peak memory externally with `/usr/bin/time -l` or Instruments.\n\n\
         | Case | Median ns | Input bytes | Output bytes | Correct |\n|---|---:|---:|---:|:---:|\n",
    );
    for run in runs {
        let median = median(&run.elapsed_ns);
        let _ = writeln!(
            summary,
            "| `{}` | {} | {} | {} | {} |",
            run.case, median, run.input_bytes, run.output_bytes, run.correct
        );
    }
    summary.push_str(
        "\nPhase 1 is eligible to close only when all cases are correct, the\n\
         results and environment artifacts are retained, and the external peak\n\
         memory observation is recorded as a value or explicitly unavailable.\n",
    );
    summary
}

fn median(values: &[u128]) -> u128 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn run_b0(run_directory: &Path) -> EvalResult<()> {
    prepare_empty_directory(run_directory)?;
    let datasets_directory = run_directory.join("datasets");
    fs::create_dir(&datasets_directory).map_err(io_message)?;
    let manifests = generate_dataset_set(&datasets_directory)?;

    write_text(
        &run_directory.join("dataset.json"),
        &dataset_set_json(&manifests),
    )?;
    write_text(
        &run_directory.join("root_inputs.json"),
        &root_inputs_json(&manifests),
    )?;

    let environment = host_environment(run_directory)?;
    write_text(
        &run_directory.join("environment.json"),
        &environment_json(&environment),
    )?;

    let source = git_metadata();
    let command = env::args().collect::<Vec<_>>().join(" ");
    let mut results = String::new();
    for manifest in &manifests {
        writeln!(
            results,
            "{{\"format_version\":{FORMAT_VERSION},\"case\":\"B0\",\"dataset\":{},\"dataset_manifest\":{},\"root_input\":{},\"elapsed_ns\":null,\"timing_status\":\"not_applicable\",\"source_commit\":{},\"dirty_tree\":{},\"benchmark_command\":{},\"correct\":true}}",
            json_string(&manifest.id),
            json_string(&manifest.root_input),
            json_string(&manifest.root_input),
            json_option_string(source.commit.as_deref()),
            source.dirty_tree,
            json_string(&command),
        )
        .map_err(|error| error.to_string())?;
    }
    write_text(&run_directory.join("results.jsonl"), &results)?;

    let summary = ("# Phase 0 B0\n\n\
         B0 records deterministic dataset and manifest generation before the \
         production LayerFS roots exist.\n\n\
         The root_input field is the manifest digest for each dataset, not a \
         production LayerFS root ID. Replace it with the actual root once \
         layerfs-core is implemented. B0 does not measure product timing; its \
         timing_status is not_applicable.\n\n\
         - Dataset manifest: dataset.json\n\
         - Root inputs: root_inputs.json\n\
         - Environment: environment.json\n\
         - Results: results.jsonl\n")
        .to_owned();
    write_text(&run_directory.join("summary.md"), &summary)?;
    Ok(())
}

fn write_probe(directory: &Path, output: &Path) -> EvalResult<i32> {
    let environment = host_environment(directory)?;
    write_text(output, &environment_json(&environment))?;
    Ok(0)
}

fn host_environment(path: &Path) -> EvalResult<HostEnvironment> {
    probe(path).map_err(|error| error.to_string())
}

fn generate_dataset_set(root: &Path) -> EvalResult<Vec<DatasetManifest>> {
    let mut manifests = Vec::with_capacity(SINGLE_FILES.len() + 1);
    for &(id, size) in SINGLE_FILES {
        manifests.push(generate_single_dataset(root, id, size)?);
    }
    manifests.push(generate_tree_dataset(root)?);
    Ok(manifests)
}

fn generate_single_dataset(root: &Path, id: &str, size: u64) -> EvalResult<DatasetManifest> {
    let directory = root.join(id);
    fs::create_dir(&directory).map_err(io_message)?;
    let relative_path = format!("single/{}.bin", id.to_ascii_lowercase());
    let file_path = directory.join(&relative_path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(io_message)?;
    }
    let blake3 = write_deterministic_file(&file_path, size, id)?;
    let files = vec![FileManifest {
        path: relative_path,
        size,
        blake3,
    }];
    Ok(make_manifest(id, files, Vec::new()))
}

fn generate_tree_dataset(root: &Path) -> EvalResult<DatasetManifest> {
    let id = "S2-tree";
    let directory = root.join(id);
    fs::create_dir(&directory).map_err(io_message)?;
    let mut files = Vec::with_capacity(TREE_FILE_COUNT);
    for index in 0..TREE_FILE_COUNT {
        let relative_path = format!(
            "tree/dir-{}/sub-{}/file-{index:05}.bin",
            index % 100,
            index % 10
        );
        let size = tree_file_size(index);
        let file_path = directory.join(&relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(io_message)?;
        }
        let blake3 = write_deterministic_file(&file_path, size, id)?;
        files.push(FileManifest {
            path: relative_path,
            size,
            blake3,
        });
    }

    let mut empty_dirs = Vec::with_capacity(8);
    for index in 0..8 {
        let relative_path = format!("tree/empty/dir-{index:03}");
        fs::create_dir_all(directory.join(&relative_path)).map_err(io_message)?;
        empty_dirs.push(relative_path);
    }
    Ok(make_manifest(id, files, empty_dirs))
}

fn tree_file_size(index: usize) -> u64 {
    let base = 10 * 1024;
    if index % 100 == 0 {
        base + 32 * 1024
    } else if (1..=4).contains(&(index % 100)) {
        base + 8 * 1024
    } else {
        base
    }
}

fn make_manifest(id: &str, files: Vec<FileManifest>, empty_dirs: Vec<String>) -> DatasetManifest {
    let root_input = root_input_hash(id, &files, &empty_dirs);
    DatasetManifest {
        id: id.to_owned(),
        files,
        empty_dirs,
        root_input,
    }
}

fn root_input_hash(id: &str, files: &[FileManifest], empty_dirs: &[String]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"layerfs-phase0-root-input-v1\n");
    hasher.update(id.as_bytes());
    hasher.update(b"\n");
    for directory in empty_dirs {
        hasher.update(b"D\n");
        hasher.update(directory.as_bytes());
        hasher.update(b"\n");
    }
    for file in files {
        hasher.update(b"F\n");
        hasher.update(file.path.as_bytes());
        hasher.update(b"\n");
        hasher.update(file.size.to_string().as_bytes());
        hasher.update(b"\n");
        hasher.update(file.blake3.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

fn write_deterministic_file(path: &Path, size: u64, salt: &str) -> EvalResult<String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_message)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    let mut offset = 0_u64;
    while offset < size {
        let remaining = size - offset;
        let length = remaining.min(BUFFER_SIZE as u64) as usize;
        fill_buffer(&mut buffer[..length], offset, salt);
        file.write_all(&buffer[..length]).map_err(io_message)?;
        hasher.update(&buffer[..length]);
        offset += length as u64;
    }
    drop(file);
    Ok(hasher.finalize().to_hex().to_string())
}

fn fill_buffer(buffer: &mut [u8], offset: u64, salt: &str) {
    let mut state = SEED ^ salt_hash(salt) ^ offset;
    for (index, byte) in buffer.iter_mut().enumerate() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let position = offset.wrapping_add(index as u64);
        *byte = if (position / 8192) % 23 == 0 {
            (salt_hash(salt) as u8).wrapping_add((position / 8192) as u8)
        } else {
            (state >> 24) as u8
        };
    }
}

fn salt_hash(salt: &str) -> u64 {
    salt.bytes()
        .fold(0_u64, |value, byte| value.rotate_left(5) ^ u64::from(byte))
}

fn dataset_set_json(manifests: &[DatasetManifest]) -> String {
    let mut output = String::from("{\"format_version\":1,\"seed\":");
    output.push_str(&SEED.to_string());
    output.push_str(",\"datasets\":[");
    for (index, manifest) in manifests.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&manifest_json(manifest));
    }
    output.push_str("]}\n");
    output
}

fn manifest_json(manifest: &DatasetManifest) -> String {
    let mut output = String::from("{\"id\":");
    output.push_str(&json_string(&manifest.id));
    output.push_str(",\"root_input\":");
    output.push_str(&json_string(&manifest.root_input));
    output.push_str(",\"empty_dirs\":[");
    for (index, directory) in manifest.empty_dirs.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&json_string(directory));
    }
    output.push_str("],\"files\":[");
    for (index, file) in manifest.files.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str("{\"path\":");
        output.push_str(&json_string(&file.path));
        output.push_str(",\"size\":");
        output.push_str(&file.size.to_string());
        output.push_str(",\"blake3\":");
        output.push_str(&json_string(&file.blake3));
        output.push('}');
    }
    output.push_str("]}");
    output
}

fn root_inputs_json(manifests: &[DatasetManifest]) -> String {
    let mut output = String::from("{\"format_version\":1,\"root_inputs\":[");
    for (index, manifest) in manifests.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str("{\"dataset\":");
        output.push_str(&json_string(&manifest.id));
        output.push_str(",\"root_input\":");
        output.push_str(&json_string(&manifest.root_input));
        output.push('}');
    }
    output.push_str("]}\n");
    output
}

fn environment_json(environment: &HostEnvironment) -> String {
    let source = git_metadata();
    let command = env::args().collect::<Vec<_>>().join(" ");
    let timestamp_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(
        "{{\"format_version\":{FORMAT_VERSION},\"operating_system\":{},\"os_version\":{},\"architecture\":{},\"filesystem_type\":{},\"apfs_volume\":{},\"case_behavior\":{},\"cpu_model\":{},\"logical_cpu_count\":{},\"memory_bytes\":{},\"sqlite_version\":{},\"rust_version\":{},\"sqlite_journal_mode\":{},\"sqlite_synchronous\":{},\"sqlite_temp_store\":{},\"sqlite_mmap_size\":{},\"probe_path\":{},\"source_commit\":{},\"dirty_tree\":{},\"benchmark_command\":{},\"timestamp_unix_ns\":{timestamp_unix_ns}}}\n",
        json_string(&environment.operating_system),
        json_option_string(environment.os_version.as_deref()),
        json_option_string(environment.architecture.as_deref()),
        json_option_string(environment.filesystem_type.as_deref()),
        json_option_string(environment.apfs_volume.as_deref()),
        json_option_string(environment.case_behavior.as_deref()),
        json_option_string(environment.cpu_model.as_deref()),
        json_option_u64(environment.logical_cpu_count),
        json_option_u64(environment.memory_bytes),
        json_option_string(environment.sqlite_version.as_deref()),
        json_option_string(environment.rust_version.as_deref()),
        json_string(environment.journal_mode),
        json_string(environment.synchronous),
        json_string(environment.temp_store),
        json_string(environment.mmap_size),
        json_string(&environment.probe_path),
        json_option_string(source.commit.as_deref()),
        source.dirty_tree,
        json_string(&command),
    )
}

fn git_metadata() -> GitMetadata {
    GitMetadata {
        commit: command_text("git", &["rev-parse", "HEAD"]),
        dirty_tree: Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .map(|output| !output.stdout.is_empty())
            .unwrap_or(false),
    }
}

struct GitMetadata {
    commit: Option<String>,
    dirty_tree: bool,
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn run_oracle(expected: &Path, actual: &Path, output: &Path) -> EvalResult<i32> {
    let expected_tree = read_tree(expected)?;
    let actual_tree = read_tree(actual)?;
    let mut paths = BTreeSet::new();
    paths.extend(expected_tree.keys().cloned());
    paths.extend(actual_tree.keys().cloned());

    let mut mismatches = Vec::new();
    for path in paths {
        match (expected_tree.get(&path), actual_tree.get(&path)) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (Some(expected), Some(actual)) => mismatches.push(Mismatch {
                path,
                issue: "different".to_owned(),
                expected: Some(expected.clone()),
                actual: Some(actual.clone()),
            }),
            (Some(expected), None) => mismatches.push(Mismatch {
                path,
                issue: "missing".to_owned(),
                expected: Some(expected.clone()),
                actual: None,
            }),
            (None, Some(actual)) => mismatches.push(Mismatch {
                path,
                issue: "extra".to_owned(),
                expected: None,
                actual: Some(actual.clone()),
            }),
            (None, None) => {}
        }
    }

    let correct = mismatches.is_empty();
    write_text(output, &oracle_json(correct, &mismatches))?;
    Ok(if correct { 0 } else { 1 })
}

fn read_tree(root: &Path) -> EvalResult<BTreeMap<String, TreeEntry>> {
    if !root.is_dir() {
        return Err(format!(
            "oracle root is not a directory: {}",
            root.display()
        ));
    }
    let mut tree = BTreeMap::new();
    walk_tree(root, Path::new(""), &mut tree)?;
    Ok(tree)
}

fn walk_tree(
    root: &Path,
    relative: &Path,
    tree: &mut BTreeMap<String, TreeEntry>,
) -> EvalResult<()> {
    let full_path = root.join(relative);
    let mut entries = fs::read_dir(&full_path)
        .map_err(io_message)?
        .collect::<Result<Vec<_>, io::Error>>()
        .map_err(io_message)?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let child_relative = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(root.join(&child_relative)).map_err(io_message)?;
        let child_name = path_text(&child_relative);
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "directory".to_owned(),
                    size: 0,
                    blake3: None,
                    target: None,
                },
            );
            walk_tree(root, &child_relative, tree)?;
        } else if file_type.is_file() {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "file".to_owned(),
                    size: metadata.len(),
                    blake3: Some(hash_file(&root.join(&child_relative))?),
                    target: None,
                },
            );
        } else if file_type.is_symlink() {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "symlink".to_owned(),
                    size: 0,
                    blake3: None,
                    target: Some(
                        fs::read_link(root.join(&child_relative))
                            .map_err(io_message)?
                            .to_string_lossy()
                            .into_owned(),
                    ),
                },
            );
        } else {
            tree.insert(
                child_name,
                TreeEntry {
                    kind: "other".to_owned(),
                    size: 0,
                    blake3: None,
                    target: None,
                },
            );
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> EvalResult<String> {
    let mut file = File::open(path).map_err(io_message)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let length = file.read(&mut buffer).map_err(io_message)?;
        if length == 0 {
            break;
        }
        hasher.update(&buffer[..length]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn oracle_json(correct: bool, mismatches: &[Mismatch]) -> String {
    let mut output = format!(
        "{{\"format_version\":1,\"correct\":{correct},\"mismatch_count\":{},\"mismatches\":[",
        mismatches.len()
    );
    for (index, mismatch) in mismatches.iter().take(256).enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str("{\"path\":");
        output.push_str(&json_string(&mismatch.path));
        output.push_str(",\"issue\":");
        output.push_str(&json_string(&mismatch.issue));
        output.push_str(",\"expected\":");
        output.push_str(&tree_entry_json(mismatch.expected.as_ref()));
        output.push_str(",\"actual\":");
        output.push_str(&tree_entry_json(mismatch.actual.as_ref()));
        output.push('}');
    }
    output.push_str("]}\n");
    output
}

fn tree_entry_json(entry: Option<&TreeEntry>) -> String {
    let Some(entry) = entry else {
        return "null".to_owned();
    };
    format!(
        "{{\"kind\":{},\"size\":{},\"blake3\":{},\"target\":{}}}",
        json_string(&entry.kind),
        entry.size,
        json_option_string(entry.blake3.as_deref()),
        json_option_string(entry.target.as_deref()),
    )
}

fn prepare_empty_directory(path: &Path) -> EvalResult<()> {
    if path.exists() {
        let mut entries = fs::read_dir(path).map_err(io_message)?;
        if entries.next().transpose().map_err(io_message)?.is_some() {
            return Err(format!("output directory is not empty: {}", path.display()));
        }
    } else {
        fs::create_dir_all(path).map_err(io_message)?;
    }
    Ok(())
}

fn prepare_phase1_directory(path: &Path) -> EvalResult<()> {
    if path.exists() {
        let entries = fs::read_dir(path)
            .map_err(io_message)?
            .collect::<Result<Vec<_>, io::Error>>()
            .map_err(io_message)?;
        if entries.iter().any(|entry| entry.file_name() != "time.txt") {
            return Err(format!("output directory is not empty: {}", path.display()));
        }
    } else {
        fs::create_dir_all(path).map_err(io_message)?;
    }
    Ok(())
}

fn write_text(path: &Path, text: &str) -> EvalResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_message)?;
    }
    fs::write(path, text).map_err(io_message)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn json_option_string(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), json_string)
}

fn json_option_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn io_message(error: io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escaping_is_valid_and_stable() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn root_input_hash_is_order_sensitive_and_deterministic() {
        let files = vec![FileManifest {
            path: "a".to_owned(),
            size: 1,
            blake3: "hash".to_owned(),
        }];
        let first = root_input_hash("test", &files, &[]);
        let second = root_input_hash("test", &files, &[]);
        assert_eq!(first, second);
        let changed = vec![FileManifest {
            path: "b".to_owned(),
            size: 1,
            blake3: "hash".to_owned(),
        }];
        assert_ne!(first, root_input_hash("test", &changed, &[]));
    }

    #[test]
    fn deterministic_payload_depends_on_position_and_salt() {
        let mut first = vec![0_u8; 8192];
        let mut second = vec![0_u8; 8192];
        fill_buffer(&mut first, 0, "same");
        fill_buffer(&mut second, 0, "same");
        assert_eq!(first, second);

        let mut changed = vec![0_u8; 8192];
        fill_buffer(&mut changed, 1, "same");
        assert_ne!(first, changed);
    }

    #[test]
    fn tree_sizes_match_phase_zero_shape() {
        assert_eq!(tree_file_size(0), 42 * 1024);
        assert_eq!(tree_file_size(1), 18 * 1024);
        assert_eq!(tree_file_size(5), 10 * 1024);
    }

    #[test]
    fn oracle_detects_all_phase_zero_mismatch_classes() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("layerfs-eval-oracle-{stamp}"));
        let expected = root.join("expected");
        let actual = root.join("actual");
        let report = root.join("report.json");
        fs::create_dir_all(&expected).expect("expected fixture directory should be created");
        fs::create_dir_all(&actual).expect("actual fixture directory should be created");

        fs::write(expected.join("changed"), b"expected")
            .expect("changed fixture should be written");
        fs::write(actual.join("changed"), b"actual").expect("changed fixture should be written");
        fs::write(expected.join("missing"), b"missing").expect("missing fixture should be written");
        fs::write(expected.join("kind"), b"file").expect("kind fixture should be written");
        fs::create_dir(actual.join("kind")).expect("kind directory should be created");
        fs::write(expected.join("length"), b"1234").expect("length fixture should be written");
        fs::write(actual.join("length"), b"1").expect("length fixture should be written");
        fs::write(actual.join("extra"), b"extra").expect("extra fixture should be written");

        assert_eq!(
            run_oracle(&expected, &actual, &report).expect("oracle should run"),
            1
        );
        let report_text = fs::read_to_string(&report).expect("oracle report should be readable");
        assert!(report_text.contains("\"mismatch_count\":5"));
        assert_eq!(report_text.matches("\"issue\":\"different\"").count(), 3);
        assert!(report_text.contains("\"issue\":\"missing\""));
        assert!(report_text.contains("\"issue\":\"extra\""));
        for path in ["changed", "missing", "kind", "length", "extra"] {
            assert!(report_text.contains(&format!("\"path\":\"{path}\"")));
        }

        fs::remove_dir_all(root).expect("oracle fixture should be cleaned up");
    }
}
