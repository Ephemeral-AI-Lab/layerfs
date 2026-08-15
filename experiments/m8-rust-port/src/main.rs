use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::{create_dir_all, remove_dir_all, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::Instant;

const MIB: usize = 1024 * 1024;
const PARAMETERS: (usize, usize, usize) = (32_768, 131_072, 524_288);
const LEAF_GROUPING: (usize, usize, usize) = (64, 128, 256);
const INTERNAL_GROUPING: (usize, usize, usize) = (32, 64, 128);
const SOURCE_WINDOW_BYTES: usize = 2 * MIB;
const SEED: u32 = 0x5eed;
const INSERT_BYTE: u8 = 1;
const CARRIER_HEADER_BYTES: usize = 56;
const CARRIER_IO_BUFFER_BYTES: usize = 128 * 1024;
const CARRIER_VERSION: u16 = 1;
const RANDOM_READS: usize = 1_000;
const MAX_PHASE2_MIB: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageMode {
    Sqlite,
    Hybrid,
}

impl StorageMode {
    fn name(self) -> &'static str {
        match self {
            Self::Sqlite => "R-SQLite",
            Self::Hybrid => "R-HYBRID",
        }
    }
}

#[derive(Clone, Debug)]
struct CarrierRecord {
    offset: usize,
    length: usize,
    checksum: [u8; 32],
}

#[derive(Clone, Debug)]
struct CarrierBatch {
    carrier_id: [u8; 32],
    relative_path: String,
    byte_length: usize,
    record_count: usize,
    offsets: HashMap<[u8; 32], (usize, usize)>,
    record_encode_ns: u128,
    digest_ns: u128,
    write_ns: u128,
    sync_ns: u128,
}

#[derive(Clone)]
struct DeterministicSource {
    state: u32,
    word: [u8; 4],
    next: usize,
}

#[derive(Clone)]
struct FixtureObject {
    hash: [u8; 32],
    length: usize,
    source: DeterministicSource,
}

#[derive(Clone)]
struct FixturePlan {
    entries: Vec<Entry>,
    objects: Vec<FixtureObject>,
    digest: [u8; 32],
}

enum Phase2ObjectSource<'a> {
    Map(&'a HashMap<[u8; 32], Vec<u8>>),
    Fixture(&'a FixturePlan),
}

impl<'a> Phase2ObjectSource<'a> {
    fn hashes(&self) -> Vec<[u8; 32]> {
        let mut hashes: Vec<[u8; 32]> = match self {
            Self::Map(objects) => objects.keys().copied().collect(),
            Self::Fixture(plan) => plan.objects.iter().map(|object| object.hash).collect(),
        };
        hashes.sort();
        hashes
    }

    fn payload(&self, hash: [u8; 32]) -> AppResult<Vec<u8>> {
        match self {
            Self::Map(objects) => objects
                .get(&hash)
                .cloned()
                .ok_or_else(|| "object source hash missing".into()),
            Self::Fixture(plan) => {
                let object = plan
                    .objects
                    .iter()
                    .find(|object| object.hash == hash)
                    .ok_or("fixture object hash missing")?;
                Ok(deterministic_from_source(
                    object.source.clone(),
                    object.length,
                ))
            }
        }
    }
}

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    hash: [u8; 32],
    length: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Child {
    hash: [u8; 32],
    span: usize,
    entry_count: usize,
}

#[derive(Clone, Debug)]
enum Node {
    Leaf {
        entries: Vec<Entry>,
        span: usize,
    },
    Internal {
        children: Vec<Child>,
        span: usize,
        entry_count: usize,
    },
}

#[derive(Clone, Debug)]
struct EncodedNode {
    hash: [u8; 32],
    encoded: Vec<u8>,
    node: Node,
}

#[derive(Clone, Debug)]
struct BuiltManifest {
    root_hash: [u8; 32],
    root: Vec<u8>,
    entries: Vec<Entry>,
    nodes: Vec<EncodedNode>,
    depth: usize,
    file_size: usize,
}

#[derive(Clone, Default)]
struct Counters {
    statements: usize,
    rows_read: usize,
    rows_inserted: usize,
    source_reads: usize,
    source_bytes: usize,
    source_transactions: usize,
    node_reads: usize,
    object_reads: usize,
    transactions: usize,
    commits: usize,
    checkpoint_bytes: i64,
    peak_disk_bytes: u64,
}

fn phase2_counters_json(counters: &Counters) -> serde_json::Value {
    json!({
        "statements": counters.statements,
        "rows_read": counters.rows_read,
        "rows_inserted": counters.rows_inserted,
        "source_reads": counters.source_reads,
        "source_bytes": counters.source_bytes,
        "source_transactions": counters.source_transactions,
        "node_reads": counters.node_reads,
        "object_reads": counters.object_reads,
        "transactions": counters.transactions,
        "commits": counters.commits,
        "peak_total_bytes": counters.peak_disk_bytes,
        "peak_total_storage_bytes": counters.peak_disk_bytes,
    })
}

fn phase2_counter_delta_json(before: &Counters, after: &Counters) -> serde_json::Value {
    json!({
        "statements": after.statements.saturating_sub(before.statements),
        "rows_read": after.rows_read.saturating_sub(before.rows_read),
        "rows_inserted": after.rows_inserted.saturating_sub(before.rows_inserted),
        "source_reads": after.source_reads.saturating_sub(before.source_reads),
        "source_bytes": after.source_bytes.saturating_sub(before.source_bytes),
        "source_transactions": after.source_transactions.saturating_sub(before.source_transactions),
        "node_reads": after.node_reads.saturating_sub(before.node_reads),
        "object_reads": after.object_reads.saturating_sub(before.object_reads),
        "transactions": after.transactions.saturating_sub(before.transactions),
        "commits": after.commits.saturating_sub(before.commits),
        "peak_total_bytes": after.peak_disk_bytes,
        "peak_total_storage_bytes": after.peak_disk_bytes,
    })
}

#[derive(Clone)]
struct Summary {
    object_members: Vec<[u8; 32]>,
    node_members: Vec<[u8; 32]>,
    object_bytes: usize,
    node_bytes: usize,
    closure_fold: [u8; 32],
    chain_digest: [u8; 32],
    object_bloom: Vec<u8>,
    node_bloom: Vec<u8>,
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.finalize().into()
}

fn sync_directory(path: &Path) -> AppResult<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

struct CarrierWriter {
    carrier_dir: PathBuf,
    temp_path: PathBuf,
    file: BufWriter<File>,
    carrier_digest: Sha256,
    byte_length: usize,
    record_count: usize,
    record_encode_ns: u128,
    write_started: Instant,
}

impl CarrierWriter {
    fn new(carrier_dir: &Path) -> AppResult<Self> {
        create_dir_all(carrier_dir)?;
        let final_name_placeholder = format!("carrier-pending-{}.lfc", process::id());
        let temp_path = carrier_dir.join(format!(".{}.tmp", final_name_placeholder));
        Ok(Self {
            carrier_dir: carrier_dir.to_path_buf(),
            temp_path: temp_path.clone(),
            file: BufWriter::with_capacity(CARRIER_IO_BUFFER_BYTES, File::create(temp_path)?),
            carrier_digest: Sha256::new(),
            byte_length: 0,
            record_count: 0,
            record_encode_ns: 0,
            write_started: Instant::now(),
        })
    }

    fn append(&mut self, hash: [u8; 32], bytes: &[u8]) -> AppResult<usize> {
        let offset = self.byte_length;
        let encode_started = Instant::now();
        let mut header = [0u8; CARRIER_HEADER_BYTES];
        header[..4].copy_from_slice(b"LFCR");
        header[4..6].copy_from_slice(&CARRIER_VERSION.to_le_bytes());
        header[6..8].copy_from_slice(&(CARRIER_HEADER_BYTES as u16).to_le_bytes());
        header[8..16].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
        header[16..48].copy_from_slice(&hash);
        header[48..52].copy_from_slice(&1u32.to_le_bytes());
        header[52..56].copy_from_slice(&0u32.to_le_bytes());
        self.record_encode_ns += encode_started.elapsed().as_nanos();
        self.carrier_digest.update(&header);
        self.carrier_digest.update(bytes);
        self.file.write_all(&header)?;
        self.file.write_all(bytes)?;
        self.byte_length = offset
            .checked_add(CARRIER_HEADER_BYTES)
            .and_then(|value| value.checked_add(bytes.len()))
            .ok_or("carrier length overflow")?;
        self.record_count += 1;
        Ok(offset)
    }

    fn finish(mut self) -> AppResult<CarrierBatch> {
        self.file.flush()?;
        let write_ns = self.write_started.elapsed().as_nanos();
        let digest_started = Instant::now();
        let carrier_id: [u8; 32] = self.carrier_digest.finalize().into();
        let digest_ns = digest_started.elapsed().as_nanos();
        let final_name = format!("carrier-{}.lfc", hex(&carrier_id));
        let final_path = self.carrier_dir.join(&final_name);
        let file = self.file.into_inner().map_err(|error| error.into_error())?;
        let sync_started = Instant::now();
        file.sync_all()?;
        let mut sync_ns = sync_started.elapsed().as_nanos();
        if final_path.exists() {
            if std::fs::metadata(&final_path)?.len() != self.byte_length as u64
                || validate_carrier_file(&final_path, carrier_id, None, true)? != self.record_count
            {
                let _ = std::fs::remove_file(&self.temp_path);
                return Err("deterministic carrier name collision".into());
            }
            std::fs::remove_file(&self.temp_path)?;
        } else {
            std::fs::rename(&self.temp_path, &final_path)?;
            let directory_sync_started = Instant::now();
            sync_directory(&self.carrier_dir)?;
            sync_ns += directory_sync_started.elapsed().as_nanos();
        }
        Ok(CarrierBatch {
            carrier_id,
            relative_path: final_name,
            byte_length: self.byte_length,
            record_count: self.record_count,
            offsets: HashMap::new(),
            record_encode_ns: self.record_encode_ns,
            digest_ns,
            write_ns,
            sync_ns,
        })
    }
}

fn stage_carrier(
    carrier_dir: &Path,
    objects: &Phase2ObjectSource<'_>,
    missing: &HashSet<[u8; 32]>,
) -> AppResult<Option<CarrierBatch>> {
    if missing.is_empty() {
        return Ok(None);
    }
    let ordered = objects.hashes();
    let mut writer = CarrierWriter::new(carrier_dir)?;
    let mut offsets = HashMap::new();
    for hash in ordered {
        if missing.contains(&hash) {
            let bytes = objects.payload(hash)?;
            let offset = writer.append(hash, &bytes)?;
            offsets.insert(hash, (offset, bytes.len()));
        }
    }
    let mut batch = writer.finish()?;
    batch.offsets = offsets;
    Ok(Some(batch))
}

fn read_carrier_record(
    path: &Path,
    record: &CarrierRecord,
    expected_hash: [u8; 32],
) -> AppResult<Vec<u8>> {
    let file_length = std::fs::metadata(path)?.len();
    let end = (record.offset as u64)
        .checked_add(CARRIER_HEADER_BYTES as u64)
        .and_then(|value| value.checked_add(record.length as u64))
        .ok_or("carrier offset overflow")?;
    if end > file_length || record.checksum != expected_hash {
        return Err("carrier bounds or checksum metadata invalid".into());
    }
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(record.offset as u64))?;
    let mut header = [0u8; CARRIER_HEADER_BYTES];
    file.read_exact(&mut header)?;
    if &header[..4] != b"LFCR"
        || u16::from_le_bytes(header[4..6].try_into()?) != CARRIER_VERSION
        || u16::from_le_bytes(header[6..8].try_into()?) as usize != CARRIER_HEADER_BYTES
        || u64::from_le_bytes(header[8..16].try_into()?) != record.length as u64
        || header[16..48] != expected_hash
        || u32::from_le_bytes(header[48..52].try_into()?) != 1
    {
        return Err("invalid or truncated carrier record".into());
    }
    let mut bytes = vec![0u8; record.length];
    file.read_exact(&mut bytes)?;
    if sha256(&bytes) != expected_hash {
        return Err("carrier payload hash mismatch".into());
    }
    Ok(bytes)
}

fn validate_carrier_file(
    path: &Path,
    carrier_id: [u8; 32],
    references: Option<(&Connection, i64)>,
    verify_payload_hash: bool,
) -> AppResult<usize> {
    let file_length = std::fs::metadata(path)?.len();
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(CARRIER_IO_BUFFER_BYTES, file);
    let mut carrier_hash = Sha256::new();
    let mut reference_statement = references
        .map(|(conn, _)| {
            conn.prepare(
                "SELECT carrier_offset,carrier_length,carrier_checksum,size,hash
                 FROM efs_cas_objects WHERE carrier_id=? ORDER BY carrier_offset",
            )
        })
        .transpose()?;
    let mut reference_rows = match reference_statement.as_mut() {
        Some(statement) => Some(statement.query(params![carrier_id.as_slice()])?),
        None => None,
    };
    let expected_record_count = references.map(|(_, count)| count);
    let mut offset = 0u64;
    let mut count = 0usize;
    let mut header = [0u8; CARRIER_HEADER_BYTES];
    let mut payload_buffer = [0u8; CARRIER_IO_BUFFER_BYTES];
    loop {
        let first = reader.read(&mut header[..1])?;
        if first == 0 {
            break;
        }
        reader.read_exact(&mut header[1..])?;
        carrier_hash.update(&header);
        if &header[..4] != b"LFCR"
            || u16::from_le_bytes(header[4..6].try_into()?) != CARRIER_VERSION
            || u16::from_le_bytes(header[6..8].try_into()?) as usize != CARRIER_HEADER_BYTES
            || u32::from_le_bytes(header[48..52].try_into()?) != 1
            || u32::from_le_bytes(header[52..56].try_into()?) != 0
        {
            return Err("carrier has a truncated record".into());
        }
        let length_u64 = u64::from_le_bytes(header[8..16].try_into()?);
        let end = offset
            .checked_add(CARRIER_HEADER_BYTES as u64)
            .and_then(|value| value.checked_add(length_u64))
            .ok_or("carrier length overflow")?;
        if end > file_length {
            return Err("carrier record bounds or hash invalid".into());
        }
        let expected_hash: [u8; 32] = header[16..48].try_into()?;
        let expected_reference = match reference_rows.as_mut() {
            Some(rows) => match rows.next()? {
                Some(row) => Some((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                )),
                None => None,
            },
            None => None,
        };
        if let Some((
            reference_offset,
            reference_length,
            reference_checksum,
            reference_size,
            reference_hash,
        )) = expected_reference
        {
            let reference_checksum: [u8; 32] = reference_checksum
                .try_into()
                .map_err(|_| "invalid carrier reference checksum")?;
            let reference_hash: [u8; 32] = reference_hash
                .try_into()
                .map_err(|_| "invalid carrier reference hash")?;
            if reference_offset < 0
                || reference_length < 0
                || reference_size < 0
                || reference_offset as u64 != offset
                || reference_length as u64 != length_u64
                || reference_size != reference_length
                || reference_checksum != expected_hash
                || reference_hash != expected_hash
            {
                return Err("carrier reference mismatch".into());
            }
        } else if references.is_some() {
            return Err("carrier record missing SQLite reference".into());
        }
        let mut payload_hash = Sha256::new();
        let mut remaining = length_u64;
        while remaining > 0 {
            let read_length = remaining.min(payload_buffer.len() as u64) as usize;
            reader.read_exact(&mut payload_buffer[..read_length])?;
            if verify_payload_hash {
                payload_hash.update(&payload_buffer[..read_length]);
            }
            carrier_hash.update(&payload_buffer[..read_length]);
            remaining -= read_length as u64;
        }
        if verify_payload_hash {
            let payload_digest: [u8; 32] = payload_hash.finalize().into();
            if payload_digest != expected_hash {
                return Err("carrier record bounds or hash invalid".into());
            }
        }
        offset = end;
        count += 1;
    }
    if offset != file_length {
        return Err("carrier trailing bytes".into());
    }
    if let Some(rows) = reference_rows.as_mut() {
        if rows.next()?.is_some() {
            return Err("SQLite reference missing carrier record".into());
        }
    }
    if expected_record_count != Some(count as i64) && expected_record_count.is_some() {
        return Err("carrier record count mismatch".into());
    }
    let actual_carrier_id: [u8; 32] = carrier_hash.finalize().into();
    if actual_carrier_id != carrier_id {
        return Err("carrier file digest mismatch".into());
    }
    Ok(count)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn phase2_hash_set_digest(hashes: &HashSet<[u8; 32]>) -> [u8; 32] {
    let mut ordered = hashes.iter().copied().collect::<Vec<_>>();
    ordered.sort();
    let mut digest = Sha256::new();
    for hash in ordered {
        digest.update(hash);
    }
    digest.finalize().into()
}

fn put_u32(out: &mut [u8], at: usize, value: usize) {
    out[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}

fn put_u64(out: &mut [u8], at: usize, value: usize) {
    out[at..at + 8].copy_from_slice(&(value as u64).to_le_bytes());
}

fn read_u32(bytes: &[u8], at: usize) -> AppResult<usize> {
    Ok(u32::from_le_bytes(bytes.get(at..at + 4).ok_or("truncated u32")?.try_into()?) as usize)
}

fn read_u64(bytes: &[u8], at: usize) -> AppResult<usize> {
    Ok(u64::from_le_bytes(bytes.get(at..at + 8).ok_or("truncated u64")?.try_into()?) as usize)
}

fn gear_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut seed = 0x9e37_79b9u32;
    for item in &mut table {
        seed = (seed ^ seed.wrapping_shl(13)).rotate_left(0);
        seed = seed ^ (seed >> 17);
        seed = seed ^ seed.wrapping_shl(5);
        *item = seed;
    }
    table
}

impl DeterministicSource {
    fn new(seed: u32) -> Self {
        Self {
            state: seed,
            word: [0; 4],
            next: 4,
        }
    }

    fn next_byte(&mut self) -> u8 {
        if self.next == 4 {
            self.state = self.state.wrapping_add(0x6d2b_79f5);
            let mut value = self.state;
            value = (value ^ (value >> 15)).wrapping_mul(value | 1);
            value ^= value.wrapping_add((value ^ (value >> 7)).wrapping_mul(value | 61));
            let word = value ^ (value >> 14);
            self.word = word.to_le_bytes();
            self.next = 0;
        }
        let byte = self.word[self.next];
        self.next += 1;
        byte
    }

    fn fill(&mut self, output: &mut [u8]) {
        for byte in output {
            *byte = self.next_byte();
        }
    }
}

fn deterministic_from_source(mut source: DeterministicSource, length: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; length];
    source.fill(&mut bytes);
    bytes
}

fn deterministic_bytes(seed: u32, length: usize) -> Vec<u8> {
    deterministic_from_source(DeterministicSource::new(seed), length)
}

fn find_boundary(input: &[u8], start: usize) -> usize {
    let (minimum, average, maximum) = PARAMETERS;
    let minimum_end = usize::min(start + minimum, input.len());
    let normal_end = usize::min(start + average, input.len());
    let maximum_end = usize::min(start + maximum, input.len());
    if minimum_end >= input.len() {
        return input.len();
    }
    let bits = average.trailing_zeros() as usize;
    let early_mask = (1u32 << usize::min(30, bits + 1)) - 1;
    let late_mask = (1u32 << usize::max(1, bits - 1)) - 1;
    let table = gear_table();
    let mut gear_hash = 0u32;
    for cursor in minimum_end..maximum_end {
        gear_hash = gear_hash
            .wrapping_shl(1)
            .wrapping_add(table[input[cursor] as usize]);
        let mask = if cursor < normal_end {
            early_mask
        } else {
            late_mask
        };
        if gear_hash & mask == 0 {
            return cursor + 1;
        }
    }
    maximum_end
}

fn chunk_fixture(bytes: &[u8]) -> (Vec<Entry>, Vec<(usize, Vec<u8>)>) {
    let mut entries = Vec::new();
    let mut objects = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let end = find_boundary(bytes, start);
        let chunk = bytes[start..end].to_vec();
        let hash = sha256(&chunk);
        entries.push(Entry {
            hash,
            length: chunk.len(),
        });
        objects.push((start, chunk));
        start = end;
    }
    (entries, objects)
}

fn stream_fixture(seed: u32, length: usize) -> AppResult<(FixturePlan, u128, u128)> {
    let started = Instant::now();
    let mut source_ns = 0u128;
    let (_, _, maximum) = PARAMETERS;
    let mut source = DeterministicSource::new(seed);
    let mut buffer = Vec::with_capacity(usize::min(maximum, length));
    let mut buffer_source = source.clone();
    let mut entries = Vec::new();
    let mut objects = Vec::new();
    let mut seen = HashSet::new();
    let mut digest = Sha256::new();
    let mut remaining = length;
    while remaining > 0 {
        let fill_length = usize::min(
            maximum.saturating_sub(buffer.len()),
            remaining - buffer.len(),
        );
        if fill_length > 0 {
            let old_length = buffer.len();
            buffer.resize(old_length + fill_length, 0);
            let source_started = Instant::now();
            source.fill(&mut buffer[old_length..]);
            source_ns += source_started.elapsed().as_nanos();
        }
        let end = find_boundary(&buffer, 0);
        let chunk = &buffer[..end];
        let hash = sha256(chunk);
        entries.push(Entry { hash, length: end });
        digest.update(chunk);
        if seen.insert(hash) {
            objects.push(FixtureObject {
                hash,
                length: end,
                source: buffer_source.clone(),
            });
        }
        buffer.drain(..end);
        buffer_source = buffer_source.clone();
        for _ in 0..end {
            let _ = buffer_source.next_byte();
        }
        remaining -= end;
    }
    let plan = FixturePlan {
        entries,
        objects,
        digest: digest.finalize().into(),
    };
    Ok((
        plan,
        source_ns,
        started.elapsed().as_nanos().saturating_sub(source_ns),
    ))
}

fn advance_group_state(
    mut state: u64,
    hash: &[u8; 32],
    span: usize,
    entry_count: Option<usize>,
) -> u64 {
    let table = gear_table();
    for byte in hash {
        state = (state << 1).wrapping_add(table[*byte as usize] as u64);
    }
    let mut value = span as u64;
    let span_bytes = if entry_count.is_some() { 8 } else { 4 };
    for _ in 0..span_bytes {
        state = (state << 1).wrapping_add(table[(value & 0xff) as usize] as u64);
        value >>= 8;
    }
    if let Some(count) = entry_count {
        let mut value = count as u64;
        for _ in 0..8 {
            state = (state << 1).wrapping_add(table[(value & 0xff) as usize] as u64);
            value >>= 8;
        }
    }
    state
}

fn is_boundary(count: usize, state: u64, grouping: (usize, usize, usize)) -> bool {
    let bits = grouping.1.trailing_zeros();
    let high = state >> (64 - bits);
    count >= grouping.2 || (count >= grouping.0 && high == 0)
}

fn encode_leaf(entries: &[Entry]) -> (Vec<u8>, usize) {
    let span = entries.iter().map(|entry| entry.length).sum();
    let mut encoded = vec![0u8; 32 + entries.len() * 36];
    encoded[..4].copy_from_slice(b"EAFN");
    encoded[4..6].copy_from_slice(&1u16.to_le_bytes());
    encoded[6] = 0;
    encoded[7] = 1;
    put_u32(&mut encoded, 8, entries.len());
    put_u64(&mut encoded, 16, span);
    put_u64(&mut encoded, 24, entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let at = 32 + index * 36;
        encoded[at..at + 32].copy_from_slice(&entry.hash);
        put_u32(&mut encoded, at + 32, entry.length);
    }
    (encoded, span)
}

fn encode_internal(children: &[Child]) -> (Vec<u8>, usize, usize) {
    let span = children.iter().map(|child| child.span).sum();
    let entry_count = children.iter().map(|child| child.entry_count).sum();
    let mut encoded = vec![0u8; 32 + children.len() * 48];
    encoded[..4].copy_from_slice(b"EAFN");
    encoded[4..6].copy_from_slice(&1u16.to_le_bytes());
    encoded[6] = 1;
    encoded[7] = 1;
    put_u32(&mut encoded, 8, children.len());
    put_u64(&mut encoded, 16, span);
    put_u64(&mut encoded, 24, entry_count);
    for (index, child) in children.iter().enumerate() {
        let at = 32 + index * 48;
        encoded[at..at + 32].copy_from_slice(&child.hash);
        put_u64(&mut encoded, at + 32, child.span);
        put_u64(&mut encoded, at + 40, child.entry_count);
    }
    (encoded, span, entry_count)
}

fn build_manifest(entries: &[Entry]) -> BuiltManifest {
    let mut nodes = Vec::new();
    let mut current = Vec::new();
    let mut group = Vec::new();
    let mut state = 0u64;
    for entry in entries {
        group.push(entry.clone());
        state = advance_group_state(state, &entry.hash, entry.length, None);
        if is_boundary(group.len(), state, LEAF_GROUPING) {
            let (encoded, span) = encode_leaf(&group);
            let hash = sha256(&encoded);
            let entry_count = group.len();
            nodes.push(EncodedNode {
                hash,
                encoded,
                node: Node::Leaf {
                    entries: group,
                    span,
                },
            });
            current.push(Child {
                hash,
                span,
                entry_count,
            });
            group = Vec::new();
            state = 0;
        }
    }
    if !group.is_empty() || current.is_empty() {
        let (encoded, span) = encode_leaf(&group);
        let hash = sha256(&encoded);
        let entry_count = group.len();
        nodes.push(EncodedNode {
            hash,
            encoded,
            node: Node::Leaf {
                entries: group,
                span,
            },
        });
        current.push(Child {
            hash,
            span,
            entry_count,
        });
    }
    let mut depth = 1;
    while current.len() > 1 {
        let mut next = Vec::new();
        let mut children = Vec::new();
        let mut state = 0u64;
        for child in current {
            state = advance_group_state(state, &child.hash, child.span, Some(child.entry_count));
            children.push(child);
            if is_boundary(children.len(), state, INTERNAL_GROUPING) {
                let (encoded, span, entry_count) = encode_internal(&children);
                let hash = sha256(&encoded);
                nodes.push(EncodedNode {
                    hash,
                    encoded,
                    node: Node::Internal {
                        children,
                        span,
                        entry_count,
                    },
                });
                next.push(Child {
                    hash,
                    span,
                    entry_count,
                });
                children = Vec::new();
                state = 0;
            }
        }
        if !children.is_empty() {
            let (encoded, span, entry_count) = encode_internal(&children);
            let hash = sha256(&encoded);
            nodes.push(EncodedNode {
                hash,
                encoded,
                node: Node::Internal {
                    children,
                    span,
                    entry_count,
                },
            });
            next.push(Child {
                hash,
                span,
                entry_count,
            });
        }
        current = next;
        depth += 1;
    }
    let root_node = &current[0];
    let mut root = vec![0u8; 68];
    root[..4].copy_from_slice(b"EAFR");
    root[4..6].copy_from_slice(&1u16.to_le_bytes());
    root[6] = 1;
    root[7] = 1;
    put_u32(&mut root, 8, PARAMETERS.0);
    put_u32(&mut root, 12, PARAMETERS.1);
    put_u32(&mut root, 16, PARAMETERS.2);
    put_u64(
        &mut root,
        20,
        entries.iter().map(|entry| entry.length).sum(),
    );
    put_u64(&mut root, 28, entries.len());
    root[36..68].copy_from_slice(&root_node.hash);
    BuiltManifest {
        root_hash: sha256(&root),
        root,
        entries: entries.to_vec(),
        nodes,
        depth,
        file_size: entries.iter().map(|entry| entry.length).sum(),
    }
}

impl EncodedNode {
    fn entry_count(&self) -> usize {
        match &self.node {
            Node::Leaf { entries, .. } => entries.len(),
            Node::Internal { entry_count, .. } => *entry_count,
        }
    }
    fn span(&self) -> usize {
        match &self.node {
            Node::Leaf { span, .. } | Node::Internal { span, .. } => *span,
        }
    }
    fn kind(&self) -> i64 {
        match self.node {
            Node::Leaf { .. } => 0,
            Node::Internal { .. } => 1,
        }
    }
}

fn node_map(built: &BuiltManifest) -> HashMap<[u8; 32], EncodedNode> {
    built
        .nodes
        .iter()
        .cloned()
        .map(|node| (node.hash, node))
        .collect()
}

fn decode_node(encoded: &[u8], expected: [u8; 32]) -> AppResult<Node> {
    if encoded.len() < 32 || &encoded[..4] != b"EAFN" || sha256(encoded) != expected {
        return Err("manifest node digest or envelope mismatch".into());
    }
    let count = read_u32(encoded, 8)?;
    let span = read_u64(encoded, 16)?;
    let entry_count = read_u64(encoded, 24)?;
    match encoded[6] {
        0 => {
            if encoded.len() != 32 + count * 36 || entry_count != count {
                return Err("invalid leaf encoding".into());
            }
            let mut entries = Vec::with_capacity(count);
            for index in 0..count {
                let at = 32 + index * 36;
                entries.push(Entry {
                    hash: encoded[at..at + 32].try_into()?,
                    length: read_u32(encoded, at + 32)?,
                });
            }
            Ok(Node::Leaf { entries, span })
        }
        1 => {
            if encoded.len() != 32 + count * 48 || count == 0 {
                return Err("invalid internal encoding".into());
            }
            let mut children = Vec::with_capacity(count);
            for index in 0..count {
                let at = 32 + index * 48;
                children.push(Child {
                    hash: encoded[at..at + 32].try_into()?,
                    span: read_u64(encoded, at + 32)?,
                    entry_count: read_u64(encoded, at + 40)?,
                });
            }
            Ok(Node::Internal {
                children,
                span,
                entry_count,
            })
        }
        _ => Err("invalid manifest node kind".into()),
    }
}

fn schema(conn: &Connection) -> AppResult<()> {
    for sql in [
        "CREATE TABLE efs_meta (singleton INTEGER PRIMARY KEY CHECK(singleton=1), schema_version INTEGER NOT NULL, filesystem_id TEXT NOT NULL UNIQUE, main_revision INTEGER NOT NULL, root_inode TEXT NOT NULL, root_mutation_generation INTEGER NOT NULL, next_allocation_sequence INTEGER NOT NULL, cow_page_bytes INTEGER NOT NULL CHECK(cow_page_bytes IN (4096,8192,16384)), created_at_ms INTEGER NOT NULL, max_manifest_entries INTEGER NOT NULL, max_manifest_depth INTEGER NOT NULL, max_file_bytes INTEGER NOT NULL, writer_profile TEXT NOT NULL, last_root_removal_generation INTEGER NOT NULL)",
        "CREATE TABLE efs_usage (singleton INTEGER PRIMARY KEY CHECK(singleton=1), object_count INTEGER NOT NULL, object_bytes INTEGER NOT NULL, manifest_root_count INTEGER NOT NULL, manifest_root_bytes INTEGER NOT NULL, manifest_node_count INTEGER NOT NULL, manifest_node_bytes INTEGER NOT NULL, page_count INTEGER NOT NULL, page_bytes INTEGER NOT NULL, patch_count INTEGER NOT NULL, patch_bytes INTEGER NOT NULL, staging_bytes INTEGER NOT NULL, result_bytes INTEGER NOT NULL, maintenance_bytes INTEGER NOT NULL, permanent_identifiers INTEGER NOT NULL, charged_metadata_bytes INTEGER NOT NULL, mutation_sequence INTEGER NOT NULL, ingest_reservation_bytes INTEGER NOT NULL, integrity_token TEXT NOT NULL)",
        "CREATE TABLE efs_cas_objects (hash BLOB PRIMARY KEY CHECK(length(hash)=32), size INTEGER NOT NULL CHECK(size>=0 AND size=length(bytes)), bytes BLOB NOT NULL, allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID",
        "CREATE TABLE efs_manifest_roots (hash BLOB PRIMARY KEY CHECK(length(hash)=32), root_node_hash BLOB NOT NULL CHECK(length(root_node_hash)=32), file_size INTEGER NOT NULL CHECK(file_size>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), chunk_min INTEGER NOT NULL, chunk_avg INTEGER NOT NULL, chunk_max INTEGER NOT NULL, encoded BLOB NOT NULL CHECK(length(encoded)=68), allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID",
        "CREATE TABLE efs_manifest_nodes (hash BLOB PRIMARY KEY CHECK(length(hash)=32), kind INTEGER NOT NULL CHECK(kind IN (0,1)), logical_bytes INTEGER NOT NULL CHECK(logical_bytes>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), encoded BLOB NOT NULL, allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID",
        "CREATE TABLE efs_manifest_validations (manifest_hash BLOB PRIMARY KEY REFERENCES efs_manifest_roots(hash) ON DELETE RESTRICT, tree_depth INTEGER NOT NULL CHECK(tree_depth BETWEEN 1 AND 64)) WITHOUT ROWID",
        "CREATE TABLE efs_root_journal (generation INTEGER PRIMARY KEY, kind INTEGER NOT NULL, root_id BLOB NOT NULL)",
        "CREATE TABLE efs_verification_state (singleton INTEGER PRIMARY KEY CHECK(singleton=1), root_hash BLOB NOT NULL CHECK(length(root_hash)=32), logical_digest BLOB NOT NULL CHECK(length(logical_digest)=32), verified_generation INTEGER NOT NULL)",
        "CREATE TABLE efs_manifest_subtree_summaries (node_hash BLOB PRIMARY KEY REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), closure_fold BLOB NOT NULL CHECK(length(closure_fold)=32), chain_digest BLOB NOT NULL CHECK(length(chain_digest)=32), object_bloom BLOB NOT NULL CHECK(length(object_bloom)=1024), node_bloom BLOB NOT NULL CHECK(length(node_bloom)=1024), object_members BLOB NOT NULL CHECK(length(object_members)%32=0), node_members BLOB NOT NULL CHECK(length(node_members)%32=0)) WITHOUT ROWID",
    ] {
        conn.execute(sql, [])?;
    }
    conn.execute("INSERT INTO efs_meta VALUES(1,13,'m8-rust',0,'m8',0,1,4096,1000,4294967295,8,17179869184,'m8-rust',0)", [])?;
    conn.execute(
        "INSERT INTO efs_usage VALUES(1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,'')",
        [],
    )?;
    Ok(())
}

fn schema_hybrid(conn: &Connection) -> AppResult<()> {
    for sql in [
        "CREATE TABLE efs_meta (singleton INTEGER PRIMARY KEY CHECK(singleton=1), schema_version INTEGER NOT NULL, filesystem_id TEXT NOT NULL UNIQUE, main_revision INTEGER NOT NULL, root_inode TEXT NOT NULL, root_mutation_generation INTEGER NOT NULL, next_allocation_sequence INTEGER NOT NULL, cow_page_bytes INTEGER NOT NULL CHECK(cow_page_bytes IN (4096,8192,16384)), created_at_ms INTEGER NOT NULL, max_manifest_entries INTEGER NOT NULL, max_manifest_depth INTEGER NOT NULL, max_file_bytes INTEGER NOT NULL, writer_profile TEXT NOT NULL, last_root_removal_generation INTEGER NOT NULL)",
        "CREATE TABLE efs_usage (singleton INTEGER PRIMARY KEY CHECK(singleton=1), object_count INTEGER NOT NULL, object_bytes INTEGER NOT NULL, manifest_root_count INTEGER NOT NULL, manifest_root_bytes INTEGER NOT NULL, manifest_node_count INTEGER NOT NULL, manifest_node_bytes INTEGER NOT NULL, page_count INTEGER NOT NULL, page_bytes INTEGER NOT NULL, patch_count INTEGER NOT NULL, patch_bytes INTEGER NOT NULL, staging_bytes INTEGER NOT NULL, result_bytes INTEGER NOT NULL, maintenance_bytes INTEGER NOT NULL, permanent_identifiers INTEGER NOT NULL, charged_metadata_bytes INTEGER NOT NULL, mutation_sequence INTEGER NOT NULL, ingest_reservation_bytes INTEGER NOT NULL, integrity_token TEXT NOT NULL)",
        "CREATE TABLE efs_carriers (carrier_id BLOB PRIMARY KEY CHECK(length(carrier_id)=32), relative_path TEXT NOT NULL UNIQUE, carrier_bytes INTEGER NOT NULL CHECK(carrier_bytes>=0), carrier_digest BLOB NOT NULL CHECK(length(carrier_digest)=32), record_count INTEGER NOT NULL CHECK(record_count>=0), format_version INTEGER NOT NULL CHECK(format_version=1)) WITHOUT ROWID",
        "CREATE TABLE efs_cas_objects (hash BLOB PRIMARY KEY CHECK(length(hash)=32), size INTEGER NOT NULL CHECK(size>=0), carrier_id BLOB NOT NULL REFERENCES efs_carriers(carrier_id) ON DELETE RESTRICT, carrier_offset INTEGER NOT NULL CHECK(carrier_offset>=0), carrier_length INTEGER NOT NULL CHECK(carrier_length=size), carrier_checksum BLOB NOT NULL CHECK(length(carrier_checksum)=32), allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID",
        "CREATE TABLE efs_manifest_roots (hash BLOB PRIMARY KEY CHECK(length(hash)=32), root_node_hash BLOB NOT NULL CHECK(length(root_node_hash)=32), file_size INTEGER NOT NULL CHECK(file_size>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), chunk_min INTEGER NOT NULL, chunk_avg INTEGER NOT NULL, chunk_max INTEGER NOT NULL, encoded BLOB NOT NULL CHECK(length(encoded)=68), allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID",
        "CREATE TABLE efs_manifest_nodes (hash BLOB PRIMARY KEY CHECK(length(hash)=32), kind INTEGER NOT NULL CHECK(kind IN (0,1)), logical_bytes INTEGER NOT NULL CHECK(logical_bytes>=0), entry_count INTEGER NOT NULL CHECK(entry_count>=0), encoded BLOB NOT NULL, allocation_sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID",
        "CREATE TABLE efs_manifest_validations (manifest_hash BLOB PRIMARY KEY REFERENCES efs_manifest_roots(hash) ON DELETE RESTRICT, tree_depth INTEGER NOT NULL CHECK(tree_depth BETWEEN 1 AND 64)) WITHOUT ROWID",
        "CREATE TABLE efs_root_journal (generation INTEGER PRIMARY KEY, kind INTEGER NOT NULL, root_id BLOB NOT NULL)",
        "CREATE TABLE efs_verification_state (singleton INTEGER PRIMARY KEY CHECK(singleton=1), root_hash BLOB NOT NULL CHECK(length(root_hash)=32), logical_digest BLOB NOT NULL CHECK(length(logical_digest)=32), verified_generation INTEGER NOT NULL)",
        "CREATE TABLE efs_manifest_subtree_summaries (node_hash BLOB PRIMARY KEY REFERENCES efs_manifest_nodes(hash) ON DELETE RESTRICT, object_count INTEGER NOT NULL CHECK(object_count>=0), object_bytes INTEGER NOT NULL CHECK(object_bytes>=0), node_count INTEGER NOT NULL CHECK(node_count>=0), node_bytes INTEGER NOT NULL CHECK(node_bytes>=0), membership_count INTEGER NOT NULL CHECK(membership_count>=0), closure_fold BLOB NOT NULL CHECK(length(closure_fold)=32), chain_digest BLOB NOT NULL CHECK(length(chain_digest)=32), object_bloom BLOB NOT NULL CHECK(length(object_bloom)=1024), node_bloom BLOB NOT NULL CHECK(length(node_bloom)=1024), object_members BLOB NOT NULL CHECK(length(object_members)%32=0), node_members BLOB NOT NULL CHECK(length(node_members)%32=0)) WITHOUT ROWID",
    ] {
        conn.execute(sql, [])?;
    }
    conn.execute("INSERT INTO efs_meta VALUES(1,13,'m8-rust-hybrid',0,'m8',0,1,4096,1000,4294967295,8,17179869184,'m8-rust-hybrid',0)", [])?;
    conn.execute(
        "INSERT INTO efs_usage VALUES(1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,'')",
        [],
    )?;
    Ok(())
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "FILE")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "mmap_size", 0i64)?;
    conn.pragma_update(None, "cache_size", -65_536i64)?;
    conn.pragma_update(None, "wal_autocheckpoint", 130_308i64)?;
    Ok(())
}

fn insert_summary(
    tx: &Transaction<'_>,
    node_hash: [u8; 32],
    summary: &Summary,
) -> AppResult<usize> {
    let object_members = summary
        .object_members
        .iter()
        .flat_map(|hash| hash.to_vec())
        .collect::<Vec<_>>();
    let node_members = summary
        .node_members
        .iter()
        .flat_map(|hash| hash.to_vec())
        .collect::<Vec<_>>();
    Ok(tx.execute("INSERT OR IGNORE INTO efs_manifest_subtree_summaries(node_hash,object_count,object_bytes,node_count,node_bytes,membership_count,closure_fold,chain_digest,object_bloom,node_bloom,object_members,node_members) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", params![node_hash.as_slice(), summary.object_members.len() as i64, summary.object_bytes as i64, summary.node_members.len() as i64, summary.node_bytes as i64, (summary.object_members.len() + summary.node_members.len()) as i64, summary.closure_fold.as_slice(), summary.chain_digest.as_slice(), &summary.object_bloom, &summary.node_bloom, object_members, node_members])?)
}

fn extend_chain(previous: [u8; 32], kind: u8, hash: [u8; 32], size: usize) -> [u8; 32] {
    let mut encoded = vec![0u8; 49];
    encoded[0] = kind;
    encoded[1..33].copy_from_slice(&hash);
    encoded[33..41].copy_from_slice(&(size as u64).to_le_bytes());
    let mut input = previous.to_vec();
    input.extend(encoded);
    sha256(&input)
}

fn extend_certificate_chain(previous: [u8; 32], sequence: usize, summary: &Summary) -> [u8; 32] {
    let mut encoded = vec![0u8; 49];
    encoded[0] = 3;
    encoded[1..33].copy_from_slice(&summary.chain_digest);
    encoded[33..41].copy_from_slice(&(sequence as u64).to_le_bytes());
    encoded[41..49].copy_from_slice(
        &((summary.object_members.len() + summary.node_members.len()) as u64).to_le_bytes(),
    );
    let mut input = previous.to_vec();
    input.extend(encoded);
    sha256(&input)
}

fn bloom_add(bloom: &mut [u8], hash: &[u8; 32]) {
    for index in 0..4 {
        let bit =
            (((hash[index * 2] as usize) << 8) | hash[index * 2 + 1] as usize) % (bloom.len() * 8);
        bloom[bit >> 3] |= 1 << (bit & 7);
    }
}

fn summary_for(
    hash: [u8; 32],
    map: &HashMap<[u8; 32], EncodedNode>,
    memo: &mut HashMap<[u8; 32], Summary>,
    seed: [u8; 32],
) -> AppResult<Summary> {
    if let Some(summary) = memo.get(&hash) {
        return Ok(summary.clone());
    }
    let value = map.get(&hash).ok_or("summary node missing")?;
    let mut objects = Vec::new();
    let mut object_seen = HashSet::new();
    let mut nodes = Vec::new();
    let mut node_seen = HashSet::new();
    let mut object_bytes = 0;
    let mut node_bytes = 0;
    let mut fold = [0u8; 32];
    let mut chain = seed;
    let mut object_bloom = vec![0u8; 1024];
    let mut node_bloom = vec![0u8; 1024];
    let mut sequence = 0;
    match &value.node {
        Node::Leaf { entries, .. } => {
            for entry in entries {
                if object_seen.insert(entry.hash) {
                    objects.push(entry.hash);
                    object_bytes += entry.length;
                    bloom_add(&mut object_bloom, &entry.hash);
                    for (left, right) in fold.iter_mut().zip(entry.hash) {
                        *left ^= right;
                    }
                    chain = extend_chain(chain, 0, entry.hash, entry.length);
                }
            }
        }
        Node::Internal { children, .. } => {
            for child in children {
                let child_summary = summary_for(child.hash, map, memo, seed)?;
                if node_seen.insert(child.hash) {
                    nodes.push(child.hash);
                    node_bytes += map
                        .get(&child.hash)
                        .ok_or("summary child missing")?
                        .encoded
                        .len();
                    bloom_add(&mut node_bloom, &child.hash);
                    for (left, right) in fold.iter_mut().zip(child.hash) {
                        *left ^= right;
                    }
                    chain = extend_chain(
                        chain,
                        1,
                        child.hash,
                        map.get(&child.hash)
                            .ok_or("summary child missing")?
                            .encoded
                            .len(),
                    );
                    sequence += 1;
                    for member in &child_summary.object_members {
                        if object_seen.insert(*member) {
                            objects.push(*member);
                        }
                    }
                    for member in &child_summary.node_members {
                        if node_seen.insert(*member) {
                            nodes.push(*member);
                        }
                    }
                    object_bytes += child_summary.object_bytes;
                    node_bytes += child_summary.node_bytes;
                    for index in 0..1024 {
                        object_bloom[index] |= child_summary.object_bloom[index];
                        node_bloom[index] |= child_summary.node_bloom[index];
                    }
                    for (left, right) in fold.iter_mut().zip(child_summary.closure_fold) {
                        *left ^= right;
                    }
                    chain = extend_certificate_chain(chain, sequence, &child_summary);
                    sequence +=
                        child_summary.object_members.len() + child_summary.node_members.len();
                }
            }
        }
    }
    let summary = Summary {
        object_members: objects,
        node_members: nodes,
        object_bytes,
        node_bytes,
        closure_fold: fold,
        chain_digest: chain,
        object_bloom,
        node_bloom,
    };
    memo.insert(hash, summary.clone());
    Ok(summary)
}

fn summary_from_row(
    object_count: i64,
    object_bytes: i64,
    node_count: i64,
    node_bytes: i64,
    closure_fold: Vec<u8>,
    chain_digest: Vec<u8>,
    object_bloom: Vec<u8>,
    node_bloom: Vec<u8>,
    object_members: Vec<u8>,
    node_members: Vec<u8>,
) -> AppResult<Summary> {
    if object_count < 0
        || node_count < 0
        || object_bytes < 0
        || node_bytes < 0
        || closure_fold.len() != 32
        || chain_digest.len() != 32
        || object_bloom.len() != 1024
        || node_bloom.len() != 1024
        || object_members.len() != object_count as usize * 32
        || node_members.len() != node_count as usize * 32
    {
        return Err("invalid stored subtree summary".into());
    }
    let hashes = |bytes: Vec<u8>| -> AppResult<Vec<[u8; 32]>> {
        bytes
            .chunks_exact(32)
            .map(|chunk| Ok(chunk.try_into()?))
            .collect()
    };
    Ok(Summary {
        object_members: hashes(object_members)?,
        node_members: hashes(node_members)?,
        object_bytes: object_bytes as usize,
        node_bytes: node_bytes as usize,
        closure_fold: closure_fold
            .try_into()
            .map_err(|_| "invalid closure fold")?,
        chain_digest: chain_digest
            .try_into()
            .map_err(|_| "invalid chain digest")?,
        object_bloom,
        node_bloom,
    })
}

fn load_summary(
    tx: &Transaction<'_>,
    hash: [u8; 32],
    counters: &mut Counters,
) -> AppResult<Summary> {
    let summary = tx.query_row(
        "SELECT object_count,object_bytes,node_count,node_bytes,closure_fold,chain_digest,object_bloom,node_bloom,object_members,node_members FROM efs_manifest_subtree_summaries WHERE node_hash=?",
        params![hash.as_slice()],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
            ))
        },
    )?;
    counters.statements += 1;
    counters.rows_read += 1;
    summary_from_row(
        summary.0, summary.1, summary.2, summary.3, summary.4, summary.5, summary.6, summary.7,
        summary.8, summary.9,
    )
}

fn next_sequence(tx: &Transaction<'_>) -> AppResult<i64> {
    Ok(tx.query_row(
        "SELECT next_allocation_sequence FROM efs_meta WHERE singleton=1",
        [],
        |row| row.get(0),
    )?)
}

fn update_sequence(tx: &Transaction<'_>, next: i64) -> AppResult<()> {
    tx.execute(
        "UPDATE efs_meta SET next_allocation_sequence=? WHERE singleton=1",
        [next],
    )?;
    Ok(())
}

fn phase2_read_payload(
    conn: &Connection,
    mode: StorageMode,
    carrier_dir: &Path,
    hash: [u8; 32],
) -> AppResult<Vec<u8>> {
    match mode {
        StorageMode::Sqlite => {
            let (size, bytes): (i64, Vec<u8>) = conn.query_row(
                "SELECT size,bytes FROM efs_cas_objects WHERE hash=?",
                params![hash.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if size < 0 || size as usize != bytes.len() || sha256(&bytes) != hash {
                return Err("SQLite payload digest or length mismatch".into());
            }
            Ok(bytes)
        }
        StorageMode::Hybrid => {
            let (size, carrier_id, offset, length, checksum, relative_path): (i64, Vec<u8>, i64, i64, Vec<u8>, String) = conn.query_row(
                "SELECT o.size,o.carrier_id,o.carrier_offset,o.carrier_length,o.carrier_checksum,c.relative_path FROM efs_cas_objects o JOIN efs_carriers c ON c.carrier_id=o.carrier_id WHERE o.hash=?",
                params![hash.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )?;
            let carrier_id: [u8; 32] = carrier_id
                .try_into()
                .map_err(|_| "invalid carrier identifier")?;
            let checksum: [u8; 32] = checksum
                .try_into()
                .map_err(|_| "invalid carrier checksum")?;
            if size < 0 || offset < 0 || length < 0 || length != size || checksum != hash {
                return Err("invalid carrier reference metadata".into());
            }
            read_carrier_record(
                &carrier_dir.join(relative_path),
                &CarrierRecord {
                    offset: offset as usize,
                    length: length as usize,
                    checksum,
                },
                hash,
            )
            .and_then(|bytes| {
                if carrier_id == [0u8; 32] {
                    return Err("zero carrier identifier".into());
                }
                Ok(bytes)
            })
        }
    }
}

fn phase2_prepare_objects(
    conn: &Connection,
    objects: &Phase2ObjectSource<'_>,
    mode: StorageMode,
    counters: &mut Counters,
) -> AppResult<HashSet<[u8; 32]>> {
    let mut missing = HashSet::new();
    for hash in objects.hashes() {
        let found: Option<i64> = match mode {
            StorageMode::Sqlite => conn
                .query_row(
                    "SELECT size FROM efs_cas_objects WHERE hash=?",
                    params![hash.as_slice()],
                    |row| row.get(0),
                )
                .optional()?,
            StorageMode::Hybrid => conn
                .query_row(
                    "SELECT size FROM efs_cas_objects WHERE hash=?",
                    params![hash.as_slice()],
                    |row| row.get(0),
                )
                .optional()?,
        };
        counters.statements += 1;
        counters.rows_read += usize::from(found.is_some());
        if found.is_none() {
            missing.insert(hash);
        }
    }
    Ok(missing)
}

fn phase2_persist_manifest(
    tx: &Transaction<'_>,
    built: &BuiltManifest,
    objects: &Phase2ObjectSource<'_>,
    missing: &HashSet<[u8; 32]>,
    old_nodes: &HashSet<[u8; 32]>,
    counters: &mut Counters,
    publish_generation: i64,
    mode: StorageMode,
    carrier_dir: &Path,
    carrier: Option<&CarrierBatch>,
) -> AppResult<(usize, usize)> {
    let mut sequence = next_sequence(tx)?;
    if let Some(batch) = carrier {
        // ponytail: the staged bytes were hashed while building the deterministic carrier and
        // fsynced before this transaction; full carrier validation belongs to the shared
        // close/reopen verification boundary, not a duplicate pre-commit 100 MiB scan.
        tx.execute("INSERT OR IGNORE INTO efs_carriers(carrier_id,relative_path,carrier_bytes,carrier_digest,record_count,format_version) VALUES(?,?,?,?,?,?)", params![batch.carrier_id.as_slice(), batch.relative_path, batch.byte_length as i64, batch.carrier_id.as_slice(), batch.record_count as i64, CARRIER_VERSION as i64])?;
        counters.rows_inserted += 1;
    }
    let mut new_objects = 0;
    for hash in objects.hashes() {
        let bytes = objects.payload(hash)?;
        counters.statements += 1;
        let is_new = missing.contains(&hash);
        match mode {
            StorageMode::Sqlite => {
                let existing: Option<(i64, Vec<u8>)> = tx
                    .query_row(
                        "SELECT size,bytes FROM efs_cas_objects WHERE hash=?",
                        params![hash.as_slice()],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                counters.rows_read += usize::from(existing.is_some());
                if let Some((size, prior)) = existing {
                    if size < 0
                        || size as usize != bytes.len()
                        || prior != bytes
                        || sha256(&prior) != hash
                    {
                        return Err("CAS same-ID/different-bytes collision".into());
                    }
                } else {
                    tx.execute("INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,?)", params![hash.as_slice(), bytes.len() as i64, bytes, sequence])?;
                    counters.rows_inserted += 1;
                    new_objects += 1;
                    sequence += 1;
                }
            }
            StorageMode::Hybrid => {
                let existing: Option<(i64, Vec<u8>, i64, i64, Vec<u8>)> = tx.query_row("SELECT size,carrier_id,carrier_offset,carrier_length,carrier_checksum FROM efs_cas_objects WHERE hash=?", params![hash.as_slice()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))).optional()?;
                counters.rows_read += usize::from(existing.is_some());
                if let Some((size, carrier_id, offset, length, checksum)) = existing {
                    if size < 0 || offset < 0 || length != size || checksum != hash.to_vec() {
                        return Err("carrier reference collision".into());
                    }
                    let carrier_id: [u8; 32] = carrier_id
                        .try_into()
                        .map_err(|_| "invalid incumbent carrier ID")?;
                    let checksum: [u8; 32] = checksum
                        .try_into()
                        .map_err(|_| "invalid incumbent checksum")?;
                    let relative_path: String = tx.query_row(
                        "SELECT relative_path FROM efs_carriers WHERE carrier_id=?",
                        params![carrier_id.as_slice()],
                        |row| row.get(0),
                    )?;
                    let prior = read_carrier_record(
                        &carrier_dir.join(relative_path),
                        &CarrierRecord {
                            offset: offset as usize,
                            length: length as usize,
                            checksum,
                        },
                        hash,
                    )?;
                    if prior != bytes {
                        return Err("carrier same-ID/different-bytes collision".into());
                    }
                } else {
                    let batch = carrier.ok_or("new hybrid object has no durable carrier")?;
                    if !is_new {
                        return Err("new hybrid object missing from carrier admission".into());
                    }
                    let (carrier_offset, carrier_length) = batch
                        .offsets
                        .get(&hash)
                        .copied()
                        .ok_or("new hybrid object missing carrier offset")?;
                    tx.execute("INSERT INTO efs_cas_objects(hash,size,carrier_id,carrier_offset,carrier_length,carrier_checksum,allocation_sequence) VALUES(?,?,?,?,?,?,?)", params![hash.as_slice(), bytes.len() as i64, batch.carrier_id.as_slice(), carrier_offset as i64, carrier_length as i64, hash.as_slice(), sequence])?;
                    counters.rows_inserted += 1;
                    new_objects += 1;
                    sequence += 1;
                }
            }
        }
    }
    let mut new_nodes = 0;
    for node in &built.nodes {
        if old_nodes.contains(&node.hash) {
            continue;
        }
        counters.statements += 1;
        let prior: Option<Vec<u8>> = tx
            .query_row(
                "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
                params![node.hash.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        counters.rows_read += usize::from(prior.is_some());
        if let Some(prior) = prior {
            if prior != node.encoded {
                return Err("manifest node collision".into());
            }
        } else {
            tx.execute("INSERT INTO efs_manifest_nodes(hash,kind,logical_bytes,entry_count,encoded,allocation_sequence) VALUES(?,?,?,?,?,?)", params![node.hash.as_slice(), node.kind(), node.span() as i64, node.entry_count() as i64, &node.encoded, sequence])?;
            counters.rows_inserted += 1;
            new_nodes += 1;
            sequence += 1;
        }
    }
    update_sequence(tx, sequence)?;
    let mut summary_memo = HashMap::new();
    let summary_seed = sha256(b"efs-subtree-chain-v1");
    let built_nodes = node_map(built);
    // M7 path-copy only creates summaries for new spine nodes. Untouched
    // subtrees already have authenticated summaries in SQLite; loading those
    // rows avoids walking every payload again for a one-byte edit.
    for node in &built.nodes {
        if old_nodes.contains(&node.hash) {
            summary_memo.insert(node.hash, load_summary(tx, node.hash, counters)?);
        }
    }
    let mut summary_rows = 0;
    for node in &built.nodes {
        if old_nodes.contains(&node.hash) {
            continue;
        }
        let summary = summary_for(node.hash, &built_nodes, &mut summary_memo, summary_seed)?;
        counters.rows_inserted += insert_summary(tx, node.hash, &summary)?;
        summary_rows += 1;
    }
    counters.statements += summary_rows;
    let prior_root: Option<i64> = tx
        .query_row(
            "SELECT length(encoded) FROM efs_manifest_roots WHERE hash=?",
            params![built.root_hash.as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    counters.statements += 1;
    counters.rows_read += usize::from(prior_root.is_some());
    if prior_root.is_none() {
        let root_node_hash = built.root[36..68].to_vec();
        tx.execute("INSERT INTO efs_manifest_roots(hash,root_node_hash,file_size,entry_count,chunk_min,chunk_avg,chunk_max,encoded,allocation_sequence) VALUES(?,?,?,?,?,?,?,?,?)", params![built.root_hash.as_slice(), root_node_hash, built.file_size as i64, built.entries.len() as i64, PARAMETERS.0 as i64, PARAMETERS.1 as i64, PARAMETERS.2 as i64, &built.root, sequence])?;
        counters.rows_inserted += 1;
        sequence += 1;
        update_sequence(tx, sequence)?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
        params![built.root_hash.as_slice(), built.depth as i64],
    )?;
    tx.execute(
        "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)",
        params![publish_generation, 0i64, built.root_hash.as_slice()],
    )?;
    tx.execute("UPDATE efs_meta SET root_mutation_generation=?, last_root_removal_generation=0 WHERE singleton=1", [publish_generation])?;
    tx.execute("UPDATE efs_usage SET object_count=(SELECT count(*) FROM efs_cas_objects),object_bytes=(SELECT coalesce(sum(size),0) FROM efs_cas_objects),manifest_root_count=(SELECT count(*) FROM efs_manifest_roots),manifest_root_bytes=(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_roots),manifest_node_count=(SELECT count(*) FROM efs_manifest_nodes),manifest_node_bytes=(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_nodes) WHERE singleton=1", [])?;
    Ok((new_objects, new_nodes))
}

fn persist_manifest(
    tx: &Transaction<'_>,
    built: &BuiltManifest,
    objects: &HashMap<[u8; 32], Vec<u8>>,
    old_nodes: &HashSet<[u8; 32]>,
    old_objects: &HashSet<[u8; 32]>,
    counters: &mut Counters,
    publish_generation: i64,
) -> AppResult<(usize, usize)> {
    let mut sequence = next_sequence(tx)?;
    let mut new_objects = 0;
    for (hash, bytes) in objects {
        counters.statements += 1;
        let existing: Option<(i64, Vec<u8>)> = tx
            .query_row(
                "SELECT size,bytes FROM efs_cas_objects WHERE hash=?",
                params![hash.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        counters.rows_read += usize::from(existing.is_some());
        if let Some((size, prior)) = existing {
            if size as usize != bytes.len() || prior != *bytes {
                return Err("CAS same-ID/different-bytes collision".into());
            }
        } else {
            tx.execute(
                "INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,?)",
                params![hash.as_slice(), bytes.len() as i64, bytes, sequence],
            )?;
            counters.rows_inserted += 1;
            new_objects += 1;
            sequence += 1;
        }
    }
    let mut new_nodes = 0;
    for node in &built.nodes {
        if old_nodes.contains(&node.hash) {
            continue;
        }
        counters.statements += 1;
        let prior: Option<Vec<u8>> = tx
            .query_row(
                "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
                params![node.hash.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        counters.rows_read += usize::from(prior.is_some());
        if let Some(prior) = prior {
            if prior != node.encoded {
                return Err("manifest node collision".into());
            }
        } else {
            tx.execute("INSERT INTO efs_manifest_nodes(hash,kind,logical_bytes,entry_count,encoded,allocation_sequence) VALUES(?,?,?,?,?,?)", params![node.hash.as_slice(), node.kind(), node.span() as i64, node.entry_count() as i64, &node.encoded, sequence])?;
            counters.rows_inserted += 1;
            new_nodes += 1;
            sequence += 1;
        }
    }
    update_sequence(tx, sequence)?;
    let mut summary_memo = HashMap::new();
    let summary_seed = sha256(b"efs-subtree-chain-v1");
    let built_nodes = node_map(built);
    for node in &built.nodes {
        let summary = summary_for(node.hash, &built_nodes, &mut summary_memo, summary_seed)?;
        counters.rows_inserted += insert_summary(tx, node.hash, &summary)?;
    }
    counters.statements += built.nodes.len();
    let prior_root: Option<i64> = tx
        .query_row(
            "SELECT length(encoded) FROM efs_manifest_roots WHERE hash=?",
            params![built.root_hash.as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    counters.statements += 1;
    counters.rows_read += usize::from(prior_root.is_some());
    if prior_root.is_none() {
        let root_node_hash = built.root[36..68].to_vec();
        tx.execute("INSERT INTO efs_manifest_roots(hash,root_node_hash,file_size,entry_count,chunk_min,chunk_avg,chunk_max,encoded,allocation_sequence) VALUES(?,?,?,?,?,?,?,?,?)", params![built.root_hash.as_slice(), root_node_hash, built.file_size as i64, built.entries.len() as i64, PARAMETERS.0 as i64, PARAMETERS.1 as i64, PARAMETERS.2 as i64, &built.root, sequence])?;
        counters.rows_inserted += 1;
        sequence += 1;
        update_sequence(tx, sequence)?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
        params![built.root_hash.as_slice(), built.depth as i64],
    )?;
    tx.execute(
        "INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,?,?)",
        params![publish_generation, 0i64, built.root_hash.as_slice()],
    )?;
    tx.execute("UPDATE efs_meta SET root_mutation_generation=?, last_root_removal_generation=0 WHERE singleton=1", [publish_generation])?;
    tx.execute("UPDATE efs_usage SET object_count=(SELECT count(*) FROM efs_cas_objects),object_bytes=(SELECT coalesce(sum(size),0) FROM efs_cas_objects),manifest_root_count=(SELECT count(*) FROM efs_manifest_roots),manifest_root_bytes=(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_roots),manifest_node_count=(SELECT count(*) FROM efs_manifest_nodes),manifest_node_bytes=(SELECT coalesce(sum(length(encoded)),0) FROM efs_manifest_nodes) WHERE singleton=1", [])?;
    let _ = old_objects;
    Ok((new_objects, new_nodes))
}

fn initialize_db(
    path: &Path,
    built: &BuiltManifest,
    objects: &[(usize, Vec<u8>)],
    counters: &mut Counters,
) -> AppResult<Connection> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    schema(&conn)?;
    let tx = conn.transaction()?;
    counters.transactions += 1;
    let mut object_map = HashMap::new();
    for (_, bytes) in objects {
        object_map.insert(sha256(bytes), bytes.clone());
    }
    let empty = HashSet::new();
    persist_manifest(&tx, built, &object_map, &empty, &empty, counters, 0)?;
    tx.commit()?;
    counters.commits += 1;
    Ok(conn)
}

fn guard_checks(
    path: &Path,
    built: &BuiltManifest,
    objects: &[(usize, Vec<u8>)],
) -> AppResult<serde_json::Value> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    let bytes = &objects.first().ok_or("missing probe object")?.1;
    let hash = sha256(bytes);
    let stored: Vec<u8> = conn.query_row(
        "SELECT bytes FROM efs_cas_objects WHERE hash=?",
        params![hash.as_slice()],
        |row| row.get(0),
    )?;
    let same_id_reuse = stored == *bytes;
    let before_head: Vec<u8> = conn.query_row(
        "SELECT root_id FROM efs_root_journal ORDER BY generation DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    let mut different = bytes.clone();
    different[0] ^= 1;
    let mut mismatch = HashMap::new();
    mismatch.insert(hash, different);
    let empty = HashSet::new();
    let mut probe_counters = Counters::default();
    let tx = conn.transaction()?;
    let collision_rejected = persist_manifest(
        &tx,
        built,
        &mismatch,
        &empty,
        &empty,
        &mut probe_counters,
        1,
    )
    .is_err();
    drop(tx);
    let before_count: i64 =
        conn.query_row("SELECT count(*) FROM efs_cas_objects", [], |row| row.get(0))?;
    let tx = conn.transaction()?;
    let rollback_bytes = vec![7u8, 8, 9];
    let rollback_hash = sha256(&rollback_bytes);
    tx.execute("INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,(SELECT next_allocation_sequence FROM efs_meta WHERE singleton=1))", params![rollback_hash.as_slice(), rollback_bytes.len() as i64, rollback_bytes])?;
    drop(tx);
    let after_count: i64 =
        conn.query_row("SELECT count(*) FROM efs_cas_objects", [], |row| row.get(0))?;
    let after_head: Vec<u8> = conn.query_row(
        "SELECT root_id FROM efs_root_journal ORDER BY generation DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    Ok(
        json!({"sameIdReuse":same_id_reuse,"collisionRejected":collision_rejected,"rollback":before_count == after_count,"rootUnchanged":before_head == after_head}),
    )
}

fn query_node(conn: &Connection, hash: [u8; 32], counters: &mut Counters) -> AppResult<Node> {
    let encoded: Vec<u8> = conn.query_row(
        "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
        params![hash.as_slice()],
        |row| row.get(0),
    )?;
    counters.statements += 1;
    counters.rows_read += 1;
    counters.node_reads += 1;
    Ok(decode_node(&encoded, hash)?)
}

fn load_path(
    conn: &Connection,
    built: &BuiltManifest,
    offset: usize,
    counters: &mut Counters,
) -> AppResult<usize> {
    let root_node_hash: Vec<u8> = conn.query_row(
        "SELECT root_node_hash FROM efs_manifest_roots WHERE hash=?",
        params![built.root_hash.as_slice()],
        |row| row.get(0),
    )?;
    counters.statements += 1;
    counters.rows_read += 1;
    let mut hash: [u8; 32] = root_node_hash
        .try_into()
        .map_err(|_| "invalid root node hash")?;
    let mut remaining = offset;
    loop {
        match query_node(conn, hash, counters)? {
            Node::Leaf { .. } => return Ok(counters.node_reads),
            Node::Internal { children, .. } => {
                let mut selected = None;
                for child in children {
                    if remaining < child.span {
                        selected = Some(child);
                        break;
                    }
                    remaining -= child.span;
                }
                hash = selected
                    .ok_or("manifest path offset outside internal span")?
                    .hash;
            }
        }
    }
}

fn entry_offsets(entries: &[Entry]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(entries.len());
    let mut current = 0;
    for entry in entries {
        offsets.push(current);
        current += entry.length;
    }
    offsets
}

fn read_source_window(
    conn: &Connection,
    entries: &[Entry],
    start_index: usize,
    start_offset: usize,
    file_size: usize,
    counters: &mut Counters,
) -> AppResult<Vec<u8>> {
    let length = usize::min(SOURCE_WINDOW_BYTES, file_size - start_offset);
    let mut output = vec![0u8; length];
    let mut copied = 0;
    let mut index = start_index;
    let offsets = entry_offsets(entries);
    while copied < length {
        let bytes: Vec<u8> = conn.query_row(
            "SELECT bytes FROM efs_cas_objects WHERE hash=?",
            params![entries[index].hash.as_slice()],
            |row| row.get(0),
        )?;
        counters.statements += 1;
        counters.rows_read += 1;
        counters.object_reads += 1;
        if sha256(&bytes) != entries[index].hash || bytes.len() != entries[index].length {
            return Err("source object digest or length mismatch".into());
        }
        let object_start = if start_offset > offsets[index] {
            start_offset - offsets[index]
        } else {
            0
        };
        let take = usize::min(bytes.len() - object_start, length - copied);
        output[copied..copied + take].copy_from_slice(&bytes[object_start..object_start + take]);
        copied += take;
        index += 1;
    }
    counters.source_reads += 1;
    counters.source_transactions += 1;
    counters.source_bytes += length;
    Ok(output)
}

fn edit_entries(
    conn: &Connection,
    built: &BuiltManifest,
    offset: usize,
    counters: &mut Counters,
) -> AppResult<(BuiltManifest, HashMap<[u8; 32], Vec<u8>>, serde_json::Value)> {
    let path_nodes_before = counters.node_reads;
    let _ = load_path(conn, built, offset, counters)?;
    let (affected_leaf, affected_leaf_index) = phase2_leaf_context_at_offset(built, offset)?;
    let affected = affected_leaf.target_entry;
    let chunk_start = affected_leaf.target_offset;
    let window = read_source_window(
        conn,
        &built.entries,
        affected,
        chunk_start,
        built.file_size,
        counters,
    )?;
    let relative = offset - chunk_start;
    let mut edited = Vec::with_capacity(window.len());
    edited.extend_from_slice(&window[..relative]);
    edited.push(INSERT_BYTE);
    edited.extend_from_slice(&window[relative + 1..]);
    let mut output_entries = Vec::new();
    let mut output_objects = HashMap::new();
    let mut cursor = 0;
    let mut old_index = affected;
    let mut reconnect = None;
    while cursor < edited.len() {
        let end = find_boundary(&edited, cursor);
        if end == edited.len() && chunk_start + end < built.file_size {
            return Err(
                "M8 bounded source window did not reach an authenticated chunk boundary".into(),
            );
        }
        let bytes = edited[cursor..end].to_vec();
        let hash = sha256(&bytes);
        output_entries.push(Entry {
            hash,
            length: bytes.len(),
        });
        output_objects.entry(hash).or_insert(bytes);
        cursor = end;
        let output = output_entries
            .last()
            .ok_or("local rebuild emitted no output")?;
        if old_index < built.entries.len()
            && output.length == built.entries[old_index].length
            && output.hash == built.entries[old_index].hash
        {
            reconnect = Some(old_index + 1);
            break;
        }
        old_index += 1;
    }
    let reconnect_index = reconnect.unwrap_or(built.entries.len());
    if reconnect.is_none() && cursor < edited.len() {
        return Err("M8 bounded source window ended before reconnect".into());
    }
    let (rebuilt, new_manifest_node_count, _) =
        phase2_local_rebuild(built, affected, reconnect_index, &output_entries)?;
    if std::env::var_os("PHASE2_M7_CANONICAL_CHECK").is_some() {
        let mut canonical_bytes = deterministic_bytes(SEED, built.file_size);
        canonical_bytes[offset] = INSERT_BYTE;
        let (canonical_entries, _) = chunk_fixture(&canonical_bytes);
        let canonical = build_manifest(&canonical_entries);
        if canonical.root_hash != rebuilt.root_hash {
            return Err("bounded M7 spine root differs from canonical rebuild".into());
        }
    }
    let old_node_hashes: HashSet<_> = built.nodes.iter().map(|node| node.hash).collect();
    let reused_nodes = rebuilt
        .nodes
        .iter()
        .filter(|node| old_node_hashes.contains(&node.hash))
        .count();
    let old_object_hashes: HashSet<_> = built.entries.iter().map(|entry| entry.hash).collect();
    let new_object_count = output_objects
        .keys()
        .filter(|hash| !old_object_hashes.contains(*hash))
        .count();
    let old_reconnect_offset = if reconnect_index == built.entries.len() {
        built.file_size
    } else {
        phase2_leaf_context_for_entry(built, reconnect_index)?.target_offset
    };
    let metrics = json!({
        "loadedEntries": reconnect_index.saturating_sub(affected) + affected_leaf_index,
        "loadedNodes": counters.node_reads - path_nodes_before,
        "affectedEntries": output_entries.len(),
        "newObjectCount": new_object_count,
        "newManifestNodeCount": new_manifest_node_count,
        "reusedSubtrees": reused_nodes.saturating_sub(new_manifest_node_count),
        "reusedManifestNodeCount": reused_nodes,
        "scanWindowBytes": cursor,
        "reconnectOldOffset": old_reconnect_offset,
        "reconnectNewOffset": old_reconnect_offset,
    });
    Ok((rebuilt, output_objects, metrics))
}

fn phase2_read_source_window(
    conn: &Connection,
    mode: StorageMode,
    carrier_dir: &Path,
    entries: &[Entry],
    start_index: usize,
    start_offset: usize,
    file_size: usize,
    requested_length: usize,
    counters: &mut Counters,
) -> AppResult<Vec<u8>> {
    let length = usize::min(requested_length, file_size - start_offset);
    let mut output = vec![0u8; length];
    let mut copied = 0;
    let mut index = start_index;
    let mut object_offset = start_offset;
    while copied < length {
        let bytes = phase2_read_payload(conn, mode, carrier_dir, entries[index].hash)?;
        counters.statements += 1;
        counters.rows_read += 1;
        counters.object_reads += 1;
        if bytes.len() != entries[index].length {
            return Err("source object length mismatch".into());
        }
        let object_start = start_offset.saturating_sub(object_offset);
        let take = usize::min(bytes.len() - object_start, length - copied);
        output[copied..copied + take].copy_from_slice(&bytes[object_start..object_start + take]);
        copied += take;
        object_offset += bytes.len();
        index += 1;
    }
    counters.source_reads += 1;
    counters.source_transactions += 1;
    counters.source_bytes += length;
    Ok(output)
}

#[derive(Clone)]
struct Phase2LeafContext {
    leaf: EncodedNode,
    start_entry: usize,
    target_entry: usize,
    target_offset: usize,
}

fn phase2_leaf_context_for_entry(
    built: &BuiltManifest,
    target_entry: usize,
) -> AppResult<Phase2LeafContext> {
    if target_entry >= built.entries.len() {
        return Err("manifest entry index is outside the file".into());
    }
    let map = node_map(built);
    let mut hash: [u8; 32] = built.root[36..68].try_into()?;
    let mut remaining = target_entry;
    let mut start_entry = 0;
    let mut start_offset = 0;
    loop {
        let node = map.get(&hash).ok_or("manifest path node missing")?;
        match &node.node {
            Node::Leaf { entries, .. } => {
                let target_offset = start_offset
                    + entries[..remaining]
                        .iter()
                        .map(|entry| entry.length)
                        .sum::<usize>();
                return Ok(Phase2LeafContext {
                    leaf: node.clone(),
                    start_entry,
                    target_entry,
                    target_offset,
                });
            }
            Node::Internal { children, .. } => {
                let mut selected = None;
                for child in children {
                    if remaining < child.entry_count {
                        selected = Some(child);
                        break;
                    }
                    remaining -= child.entry_count;
                    start_entry += child.entry_count;
                    start_offset += child.span;
                }
                hash = selected
                    .ok_or("manifest entry path is outside the tree")?
                    .hash;
            }
        }
    }
}

fn phase2_leaf_context_at_offset(
    built: &BuiltManifest,
    offset: usize,
) -> AppResult<(Phase2LeafContext, usize)> {
    if offset >= built.file_size {
        let last = built.entries.len().checked_sub(1).ok_or("empty manifest")?;
        let context = phase2_leaf_context_for_entry(built, last)?;
        let affected = match &context.leaf.node {
            Node::Leaf { entries, .. } => entries.len() - 1,
            Node::Internal { .. } => return Err("leaf context is not a leaf".into()),
        };
        return Ok((context, affected));
    }
    let map = node_map(built);
    let mut hash: [u8; 32] = built.root[36..68].try_into()?;
    let mut remaining = offset;
    let mut start_entry = 0;
    let mut start_offset = 0;
    loop {
        let node = map.get(&hash).ok_or("manifest offset node missing")?;
        match &node.node {
            Node::Leaf { entries, .. } => {
                let mut relative = remaining;
                for (index, entry) in entries.iter().enumerate() {
                    if relative < entry.length {
                        return Ok((
                            Phase2LeafContext {
                                leaf: node.clone(),
                                start_entry,
                                target_entry: start_entry + index,
                                target_offset: start_offset
                                    + entries[..index]
                                        .iter()
                                        .map(|item| item.length)
                                        .sum::<usize>(),
                            },
                            index,
                        ));
                    }
                    relative -= entry.length;
                }
                return Err("manifest leaf does not contain the requested offset".into());
            }
            Node::Internal { children, .. } => {
                let mut selected = None;
                for child in children {
                    if remaining < child.span {
                        selected = Some(child);
                        break;
                    }
                    remaining -= child.span;
                    start_entry += child.entry_count;
                    start_offset += child.span;
                }
                hash = selected
                    .ok_or("manifest offset path is outside the tree")?
                    .hash;
            }
        }
    }
}

fn phase2_collect_leaf_children(
    map: &HashMap<[u8; 32], EncodedNode>,
    hash: [u8; 32],
    output: &mut Vec<Child>,
) -> AppResult<()> {
    let node = map.get(&hash).ok_or("leaf traversal node missing")?;
    match &node.node {
        Node::Leaf { entries, span } => output.push(Child {
            hash,
            span: *span,
            entry_count: entries.len(),
        }),
        Node::Internal { children, .. } => {
            for child in children {
                phase2_collect_leaf_children(map, child.hash, output)?;
            }
        }
    }
    Ok(())
}

fn phase2_leaf_index_at_entry(leaves: &[Child], target: usize) -> Option<usize> {
    let mut start = 0;
    for (index, leaf) in leaves.iter().enumerate() {
        if target < start + leaf.entry_count {
            return Some(index);
        }
        start += leaf.entry_count;
    }
    None
}

fn phase2_regroup_leaf_segment(records: &[Entry], at_true_end: bool) -> (Vec<EncodedNode>, bool) {
    let mut nodes = Vec::new();
    let mut group = Vec::new();
    let mut state = 0u64;
    for record in records {
        group.push(record.clone());
        state = advance_group_state(state, &record.hash, record.length, None);
        if is_boundary(group.len(), state, LEAF_GROUPING) {
            let (encoded, span) = encode_leaf(&group);
            let hash = sha256(&encoded);
            nodes.push(EncodedNode {
                hash,
                encoded,
                node: Node::Leaf {
                    entries: std::mem::take(&mut group),
                    span,
                },
            });
            state = 0;
        }
    }
    if !group.is_empty() {
        if !at_true_end {
            return (nodes, false);
        }
        let (encoded, span) = encode_leaf(&group);
        let hash = sha256(&encoded);
        nodes.push(EncodedNode {
            hash,
            encoded,
            node: Node::Leaf {
                entries: group,
                span,
            },
        });
    }
    (nodes, true)
}

fn phase2_build_internal_tree(leaves: &[Child]) -> AppResult<(Vec<EncodedNode>, Child, usize)> {
    if leaves.is_empty() {
        return Err("local rebuild produced no leaves".into());
    }
    let mut nodes = Vec::new();
    let mut current = leaves.to_vec();
    let mut depth = 1;
    while current.len() > 1 {
        let mut next = Vec::new();
        let mut group = Vec::new();
        let mut state = 0u64;
        for child in current {
            state = advance_group_state(state, &child.hash, child.span, Some(child.entry_count));
            group.push(child);
            if is_boundary(group.len(), state, INTERNAL_GROUPING) {
                let (encoded, span, entry_count) = encode_internal(&group);
                let hash = sha256(&encoded);
                nodes.push(EncodedNode {
                    hash,
                    encoded,
                    node: Node::Internal {
                        children: std::mem::take(&mut group),
                        span,
                        entry_count,
                    },
                });
                next.push(Child {
                    hash,
                    span,
                    entry_count,
                });
                state = 0;
            }
        }
        if !group.is_empty() {
            let (encoded, span, entry_count) = encode_internal(&group);
            let hash = sha256(&encoded);
            nodes.push(EncodedNode {
                hash,
                encoded,
                node: Node::Internal {
                    children: group,
                    span,
                    entry_count,
                },
            });
            next.push(Child {
                hash,
                span,
                entry_count,
            });
        }
        current = next;
        depth += 1;
    }
    Ok((
        nodes,
        current.pop().ok_or("local rebuild root missing")?,
        depth,
    ))
}

fn phase2_encode_root(file_size: usize, entry_count: usize, root_node: [u8; 32]) -> Vec<u8> {
    let mut root = vec![0u8; 68];
    root[..4].copy_from_slice(b"EAFR");
    root[4..6].copy_from_slice(&1u16.to_le_bytes());
    root[6] = 1;
    root[7] = 1;
    put_u32(&mut root, 8, PARAMETERS.0);
    put_u32(&mut root, 12, PARAMETERS.1);
    put_u32(&mut root, 16, PARAMETERS.2);
    put_u64(&mut root, 20, file_size);
    put_u64(&mut root, 28, entry_count);
    root[36..68].copy_from_slice(&root_node);
    root
}

fn phase2_local_rebuild(
    built: &BuiltManifest,
    affected: usize,
    reconnect_index: usize,
    replacement: &[Entry],
) -> AppResult<(BuiltManifest, usize, usize)> {
    let affected_leaf = phase2_leaf_context_for_entry(built, affected)?;
    let mut candidate_end = if reconnect_index == built.entries.len() {
        built.entries.len()
    } else {
        let reconnect_leaf = phase2_leaf_context_for_entry(built, reconnect_index)?;
        reconnect_leaf.start_entry + reconnect_leaf.leaf.entry_count()
    };
    let mut rebuilt_leaves = None;
    // ponytail: the current M7 fixtures reconnect within a few leaf groups;
    // the bounded ceiling prevents an accidental whole-file regroup.
    for _ in 0..64 {
        let prefix_count = affected - affected_leaf.start_entry;
        let mut records = Vec::new();
        if let Node::Leaf { entries, .. } = &affected_leaf.leaf.node {
            records.extend_from_slice(&entries[..prefix_count]);
        }
        records.extend_from_slice(replacement);
        records.extend_from_slice(&built.entries[reconnect_index..candidate_end]);
        let (leaves, complete) =
            phase2_regroup_leaf_segment(&records, candidate_end == built.entries.len());
        if complete {
            rebuilt_leaves = Some((candidate_end, leaves));
            break;
        }
        if candidate_end == built.entries.len() {
            break;
        }
        let next = phase2_leaf_context_for_entry(built, candidate_end)?;
        candidate_end = next.start_entry + next.leaf.entry_count();
    }
    let (candidate_end, rebuilt_leaves) =
        rebuilt_leaves.ok_or("M7 bounded leaf regroup did not reconnect")?;
    let mut entries = Vec::with_capacity(
        built
            .entries
            .len()
            .saturating_sub(reconnect_index - affected)
            + replacement.len(),
    );
    entries.extend_from_slice(&built.entries[..affected]);
    entries.extend_from_slice(replacement);
    entries.extend_from_slice(&built.entries[reconnect_index..]);

    let old_map = node_map(built);
    let mut old_leaves = Vec::new();
    phase2_collect_leaf_children(&old_map, built.root[36..68].try_into()?, &mut old_leaves)?;
    let start_leaf = phase2_leaf_index_at_entry(&old_leaves, affected)
        .ok_or("affected leaf is not in the current root")?;
    let reuse_leaf = if candidate_end == built.entries.len() {
        old_leaves.len()
    } else {
        let context = phase2_leaf_context_for_entry(built, candidate_end)?;
        phase2_leaf_index_at_entry(&old_leaves, context.start_entry)
            .ok_or("reconnect leaf is not in the current root")?
    };
    let rebuilt_leaf_children = rebuilt_leaves
        .iter()
        .map(|node| Child {
            hash: node.hash,
            span: node.span(),
            entry_count: node.entry_count(),
        })
        .collect::<Vec<_>>();
    let mut leaves = Vec::new();
    leaves.extend_from_slice(&old_leaves[..start_leaf]);
    leaves.extend(rebuilt_leaf_children);
    leaves.extend_from_slice(&old_leaves[reuse_leaf..]);
    let (mut generated, root_child, depth) = phase2_build_internal_tree(&leaves)?;
    generated.extend(rebuilt_leaves);
    let root = phase2_encode_root(
        entries.iter().map(|entry| entry.length).sum(),
        entries.len(),
        root_child.hash,
    );
    let root_hash = sha256(&root);
    let old_hashes = built
        .nodes
        .iter()
        .map(|node| node.hash)
        .collect::<HashSet<_>>();
    let new_nodes = generated
        .iter()
        .filter(|node| !old_hashes.contains(&node.hash))
        .count();
    let mut nodes = built.nodes.clone();
    for node in generated {
        if !nodes.iter().any(|existing| existing.hash == node.hash) {
            nodes.push(node);
        }
    }
    Ok((
        BuiltManifest {
            root_hash,
            root,
            entries,
            nodes,
            depth,
            file_size: built.file_size,
        },
        new_nodes,
        old_hashes.len(),
    ))
}

fn phase2_edit_entries(
    conn: &Connection,
    mode: StorageMode,
    carrier_dir: &Path,
    built: &BuiltManifest,
    canonical_source: &[u8],
    offset: usize,
    timings: &mut Phase2Timings,
    counters: &mut Counters,
) -> AppResult<(BuiltManifest, HashMap<[u8; 32], Vec<u8>>, serde_json::Value)> {
    let path_nodes_before = counters.node_reads;
    let _ = load_path(conn, built, offset, counters)?;
    let (affected_leaf, affected_leaf_index) = phase2_leaf_context_at_offset(built, offset)?;
    let affected = affected_leaf.target_entry;
    let chunk_start = affected_leaf.target_offset;
    let window = phase2_read_source_window(
        conn,
        mode,
        carrier_dir,
        &built.entries,
        affected,
        chunk_start,
        built.file_size,
        SOURCE_WINDOW_BYTES,
        counters,
    )?;
    let relative = offset - chunk_start;
    let mut edited = Vec::with_capacity(window.len());
    edited.extend_from_slice(&window[..relative]);
    edited.push(INSERT_BYTE);
    edited.extend_from_slice(&window[relative + 1..]);
    let mut output_entries = Vec::new();
    let mut output_objects = HashMap::new();
    let mut cursor = 0;
    let mut old_index = affected;
    let mut reconnect = None;
    let chunk_started = Instant::now();
    while cursor < edited.len() {
        let end = find_boundary(&edited, cursor);
        if end == edited.len() && chunk_start + end < built.file_size {
            return Err(
                "M7 bounded source window did not reach an authenticated chunk boundary".into(),
            );
        }
        let bytes = edited[cursor..end].to_vec();
        let hash = sha256(&bytes);
        output_entries.push(Entry {
            hash,
            length: bytes.len(),
        });
        output_objects.entry(hash).or_insert(bytes);
        cursor = end;
        let output = output_entries
            .last()
            .ok_or("local rebuild emitted no output")?;
        if old_index < built.entries.len()
            && output.length == built.entries[old_index].length
            && output.hash == built.entries[old_index].hash
        {
            reconnect = Some(old_index + 1);
            break;
        }
        old_index += 1;
    }
    timings.cdc_hash_ns += chunk_started.elapsed().as_nanos();
    let reconnect_index = reconnect.unwrap_or(built.entries.len());
    if reconnect.is_none() && cursor < edited.len() {
        return Err("M7 bounded source window ended before reconnect".into());
    }
    let tree_started = Instant::now();
    let (rebuilt, new_manifest_node_count, _) =
        phase2_local_rebuild(built, affected, reconnect_index, &output_entries)?;
    let replacement_end = affected
        .checked_add(output_entries.len())
        .ok_or("M7 replacement range overflow")?;
    if rebuilt.entries[..affected] != built.entries[..affected]
        || rebuilt.entries[replacement_end..] != built.entries[reconnect_index..]
    {
        return Err("M7 changed the identity outside the bounded edit region".into());
    }
    timings.tree_ns += tree_started.elapsed().as_nanos();
    if std::env::var_os("PHASE2_M7_CANONICAL_CHECK").is_some() {
        let canonical_started = Instant::now();
        let mut canonical_bytes = canonical_source.to_vec();
        canonical_bytes[offset] = INSERT_BYTE;
        let (canonical_entries, _) = chunk_fixture(&canonical_bytes);
        let canonical = build_manifest(&canonical_entries);
        if canonical.root_hash != rebuilt.root_hash {
            return Err("bounded M7 spine root differs from canonical rebuild".into());
        }
        timings.canonical_check_ns += canonical_started.elapsed().as_nanos();
    }
    let old_node_hashes: HashSet<_> = built.nodes.iter().map(|node| node.hash).collect();
    let reused_nodes = rebuilt
        .nodes
        .iter()
        .filter(|node| old_node_hashes.contains(&node.hash))
        .count();
    let old_object_hashes: HashSet<_> = built.entries[affected..reconnect_index]
        .iter()
        .map(|entry| entry.hash)
        .collect();
    let new_object_hashes: HashSet<_> = rebuilt.entries[affected..replacement_end]
        .iter()
        .map(|entry| entry.hash)
        .collect();
    let changed_object_hashes = old_object_hashes
        .symmetric_difference(&new_object_hashes)
        .copied()
        .collect::<HashSet<_>>();
    let all_old_object_hashes: HashSet<_> = built.entries.iter().map(|entry| entry.hash).collect();
    let mut unchanged_identity_digest = Sha256::new();
    for entry in built.entries[..affected]
        .iter()
        .chain(built.entries[reconnect_index..].iter())
    {
        unchanged_identity_digest.update(entry.hash);
        unchanged_identity_digest.update((entry.length as u64).to_le_bytes());
    }
    let unchanged_identity_set: [u8; 32] = unchanged_identity_digest.finalize().into();
    let changed_object_set = phase2_hash_set_digest(&changed_object_hashes);
    let new_object_count = output_objects
        .keys()
        .filter(|hash| !all_old_object_hashes.contains(*hash))
        .count();
    let old_reconnect_offset = if reconnect_index == built.entries.len() {
        built.file_size
    } else {
        phase2_leaf_context_for_entry(built, reconnect_index)?.target_offset
    };
    let metrics = json!({
        "loadedEntries": reconnect_index.saturating_sub(affected) + affected_leaf_index,
        "loadedNodes": counters.node_reads - path_nodes_before,
        "affectedEntries": output_entries.len(),
        "newObjectCount": new_object_count,
        "newManifestNodeCount": new_manifest_node_count,
        "reusedSubtrees": reused_nodes.saturating_sub(new_manifest_node_count),
        "reusedManifestNodeCount": reused_nodes,
        "scanWindowBytes": cursor,
        "reconnectOldOffset": old_reconnect_offset,
        "reconnectNewOffset": old_reconnect_offset,
        "changedObjectSet": hex(&changed_object_set),
        "changedObjectCount": changed_object_hashes.len(),
        "unchangedIdentitySet": hex(&unchanged_identity_set),
        "unchangedIdentityCount": built.entries.len() - (reconnect_index - affected),
    });
    Ok((rebuilt, output_objects, metrics))
}

fn materialized_digest(
    conn: &Connection,
    root_hash: [u8; 32],
    counters: &mut Counters,
) -> AppResult<[u8; 32]> {
    let root: Vec<u8> = conn.query_row(
        "SELECT encoded FROM efs_manifest_roots WHERE hash=?",
        params![root_hash.as_slice()],
        |row| row.get(0),
    )?;
    counters.statements += 1;
    counters.rows_read += 1;
    if sha256(&root) != root_hash {
        return Err("root digest mismatch after reopen".into());
    }
    let root_node: [u8; 32] = root[36..68].try_into()?;
    let mut hash = Sha256::new();
    fn visit(
        conn: &Connection,
        node_hash: [u8; 32],
        out: &mut Sha256,
        counters: &mut Counters,
    ) -> AppResult<()> {
        let encoded: Vec<u8> = conn.query_row(
            "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
            params![node_hash.as_slice()],
            |row| row.get(0),
        )?;
        counters.statements += 1;
        counters.rows_read += 1;
        let node = decode_node(&encoded, node_hash)?;
        match node {
            Node::Leaf { entries, .. } => {
                for entry in entries {
                    let bytes: Vec<u8> = conn.query_row(
                        "SELECT bytes FROM efs_cas_objects WHERE hash=?",
                        params![entry.hash.as_slice()],
                        |row| row.get(0),
                    )?;
                    counters.statements += 1;
                    counters.rows_read += 1;
                    if bytes.len() != entry.length || sha256(&bytes) != entry.hash {
                        return Err("object digest mismatch after reopen".into());
                    }
                    out.update(bytes);
                }
            }
            Node::Internal { children, .. } => {
                for child in children {
                    visit(conn, child.hash, out, counters)?;
                }
            }
        }
        Ok(())
    }
    visit(conn, root_node, &mut hash, counters)?;
    Ok(hash.finalize().into())
}

fn phase2_materialized_digest(
    conn: &Connection,
    mode: StorageMode,
    carrier_dir: &Path,
    root_hash: [u8; 32],
    counters: &mut Counters,
) -> AppResult<[u8; 32]> {
    let root: Vec<u8> = conn.query_row(
        "SELECT encoded FROM efs_manifest_roots WHERE hash=?",
        params![root_hash.as_slice()],
        |row| row.get(0),
    )?;
    counters.statements += 1;
    counters.rows_read += 1;
    if sha256(&root) != root_hash {
        return Err("root digest mismatch after reopen".into());
    }
    let root_node: [u8; 32] = root[36..68].try_into()?;
    let mut hash = Sha256::new();
    fn visit(
        conn: &Connection,
        mode: StorageMode,
        carrier_dir: &Path,
        node_hash: [u8; 32],
        out: &mut Sha256,
        counters: &mut Counters,
    ) -> AppResult<()> {
        let encoded: Vec<u8> = conn.query_row(
            "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
            params![node_hash.as_slice()],
            |row| row.get(0),
        )?;
        counters.statements += 1;
        counters.rows_read += 1;
        let node = decode_node(&encoded, node_hash)?;
        match node {
            Node::Leaf { entries, .. } => {
                for entry in entries {
                    let bytes = phase2_read_payload(conn, mode, carrier_dir, entry.hash)?;
                    counters.statements += 1;
                    counters.rows_read += 1;
                    if bytes.len() != entry.length || sha256(&bytes) != entry.hash {
                        return Err("payload digest mismatch after reopen".into());
                    }
                    out.update(bytes);
                }
            }
            Node::Internal { children, .. } => {
                for child in children {
                    visit(conn, mode, carrier_dir, child.hash, out, counters)?;
                }
            }
        }
        Ok(())
    }
    visit(conn, mode, carrier_dir, root_node, &mut hash, counters)?;
    Ok(hash.finalize().into())
}

fn expected_digest(size: usize, offset: usize) -> [u8; 32] {
    let mut bytes = deterministic_bytes(SEED, size);
    bytes[offset] = INSERT_BYTE;
    sha256(&bytes)
}

fn directory_bytes(path: &Path) -> AppResult<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total = total
                .checked_add(directory_bytes(&entry.path())?)
                .ok_or("directory size overflow")?;
        } else {
            total = total
                .checked_add(metadata.len())
                .ok_or("directory size overflow")?;
        }
    }
    Ok(total)
}

fn phase2_space(
    conn: &Connection,
    mode: StorageMode,
    directory: &Path,
    carrier_dir: &Path,
) -> AppResult<serde_json::Value> {
    let db = directory.join("fs.db");
    let db_bytes = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0);
    let wal_bytes = std::fs::metadata(directory.join("fs.db-wal"))
        .map(|m| m.len())
        .unwrap_or(0);
    let shm_bytes = std::fs::metadata(directory.join("fs.db-shm"))
        .map(|m| m.len())
        .unwrap_or(0);
    let mut carrier_bytes = 0u64;
    let mut temporary_bytes = 0u64;
    let mut carrier_names = HashSet::new();
    if carrier_dir.exists() {
        for entry in std::fs::read_dir(carrier_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let size = entry.metadata()?.len();
            if name.ends_with(".lfc") {
                carrier_bytes += size;
                carrier_names.insert(name);
            } else {
                temporary_bytes += size;
            }
        }
    }
    let mut live_payload = 0u64;
    let mut referenced_carriers = HashSet::new();
    match mode {
        StorageMode::Sqlite => {
            let mut stmt = conn.prepare("SELECT size FROM efs_cas_objects")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                live_payload += row.get::<_, i64>(0)?.max(0) as u64;
            }
        }
        StorageMode::Hybrid => {
            let mut stmt = conn.prepare("SELECT size,carrier_id FROM efs_cas_objects")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                live_payload += row.get::<_, i64>(0)?.max(0) as u64;
                referenced_carriers.insert(row.get::<_, Vec<u8>>(1)?);
            }
        }
    }
    let mut orphan_carrier_bytes = 0u64;
    if mode == StorageMode::Hybrid {
        let mut carriers = conn.prepare("SELECT carrier_id,relative_path FROM efs_carriers")?;
        let rows = carriers.query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, name) = row?;
            if !referenced_carriers.contains(&id) {
                orphan_carrier_bytes += std::fs::metadata(carrier_dir.join(name))
                    .map(|m| m.len())
                    .unwrap_or(0);
            }
        }
        for name in carrier_names {
            let known: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM efs_carriers WHERE relative_path=?)",
                [name.as_str()],
                |row| row.get(0),
            )?;
            if !known {
                orphan_carrier_bytes += std::fs::metadata(carrier_dir.join(name))
                    .map(|m| m.len())
                    .unwrap_or(0);
            }
        }
    }
    let total = directory_bytes(directory)?;
    Ok(json!({
        "sqlite_bytes": db_bytes,
        "database_bytes": db_bytes,
        "wal_bytes": wal_bytes,
        "shm_bytes": shm_bytes,
        "carrier_bytes": carrier_bytes,
        "temporary_bytes": temporary_bytes,
        "total_bytes": total,
        "steady_state_total_bytes": total,
        "live_payload_bytes": live_payload,
        "obsolete_or_unreferenced_carrier_bytes": orphan_carrier_bytes,
        "obsolete_carrier_bytes": orphan_carrier_bytes,
    }))
}

fn phase2_orphan_files(conn: &Connection, carrier_dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut orphans = Vec::new();
    if !carrier_dir.exists() {
        return Ok(orphans);
    }
    for entry in std::fs::read_dir(carrier_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".lfc") {
            continue;
        }
        let known: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM efs_carriers WHERE relative_path=?)",
            [name.as_str()],
            |row| row.get(0),
        )?;
        if !known {
            orphans.push(entry.path());
        }
    }
    Ok(orphans)
}

fn phase2_quarantine_orphans(conn: &Connection, carrier_dir: &Path) -> AppResult<usize> {
    let orphans = phase2_orphan_files(conn, carrier_dir)?;
    if orphans.is_empty() {
        return Ok(0);
    }
    let quarantine = carrier_dir.join("quarantine");
    create_dir_all(&quarantine)?;
    for path in &orphans {
        let destination = quarantine.join(path.file_name().ok_or("invalid orphan path")?);
        std::fs::rename(path, destination)?;
    }
    sync_directory(carrier_dir)?;
    Ok(orphans.len())
}

#[derive(Default)]
struct Phase2Timings {
    source_read_ns: u128,
    cdc_hash_ns: u128,
    tree_ns: u128,
    canonical_check_ns: u128,
    admission_ns: u128,
    storage_before_commit_ns: u128,
    carrier_record_encode_ns: u128,
    carrier_append_ns: u128,
    carrier_digest_ns: u128,
    carrier_write_ns: u128,
    carrier_fsync_ns: u128,
    carrier_verify_ns: u128,
    sqlite_metadata_ns: u128,
    sqlite_commit_ns: u128,
    close_reopen_ns: u128,
    verification_ns: u128,
    incremental_verification_ns: u128,
    checkpoint_ns: u128,
}

fn phase2_configure(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "FILE")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "page_size", 4096i64)?;
    conn.pragma_update(None, "mmap_size", 0i64)?;
    conn.pragma_update(None, "cache_size", -65_536i64)?;
    conn.pragma_update(None, "wal_autocheckpoint", 0i64)?;
    Ok(())
}

fn phase2_open(path: &Path, mode: StorageMode, fresh: bool) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    phase2_configure(&conn)?;
    if fresh {
        match mode {
            StorageMode::Sqlite => schema(&conn)?,
            StorageMode::Hybrid => schema_hybrid(&conn)?,
        }
    }
    Ok(conn)
}

fn phase2_store(
    conn: &mut Connection,
    mode: StorageMode,
    directory: &Path,
    built: &BuiltManifest,
    objects: Phase2ObjectSource<'_>,
    old_nodes: &HashSet<[u8; 32]>,
    counters: &mut Counters,
    timings: &mut Phase2Timings,
) -> AppResult<(usize, usize, Option<CarrierBatch>)> {
    let storage_started = Instant::now();
    let carrier_dir = directory.join("carriers");
    let admission_started = Instant::now();
    let missing = phase2_prepare_objects(conn, &objects, mode, counters)?;
    timings.admission_ns += admission_started.elapsed().as_nanos();
    let carrier_started = Instant::now();
    let carrier = match mode {
        StorageMode::Sqlite => None,
        StorageMode::Hybrid => stage_carrier(&carrier_dir, &objects, &missing)?,
    };
    timings.carrier_append_ns += carrier_started.elapsed().as_nanos();
    if let Some(batch) = &carrier {
        timings.carrier_record_encode_ns += batch.record_encode_ns;
        timings.carrier_write_ns += batch.write_ns;
        timings.carrier_digest_ns += batch.digest_ns;
        timings.carrier_append_ns = timings.carrier_append_ns.saturating_sub(batch.sync_ns);
        timings.carrier_fsync_ns += batch.sync_ns;
    }
    counters.peak_disk_bytes = counters.peak_disk_bytes.max(directory_bytes(directory)?);
    let generation: i64 = conn.query_row(
        "SELECT coalesce(max(generation),0)+1 FROM efs_root_journal",
        [],
        |row| row.get(0),
    )?;
    let metadata_started = Instant::now();
    let tx = conn.transaction()?;
    counters.transactions += 1;
    let (new_objects, new_nodes) = phase2_persist_manifest(
        &tx,
        built,
        &objects,
        &missing,
        old_nodes,
        counters,
        generation,
        mode,
        &carrier_dir,
        carrier.as_ref(),
    )?;
    timings.sqlite_metadata_ns += metadata_started.elapsed().as_nanos();
    let commit_started = Instant::now();
    timings.storage_before_commit_ns += commit_started.duration_since(storage_started).as_nanos();
    tx.commit()?;
    timings.sqlite_commit_ns += commit_started.elapsed().as_nanos();
    counters.peak_disk_bytes = counters.peak_disk_bytes.max(directory_bytes(directory)?);
    counters.commits += 1;
    Ok((new_objects, new_nodes, carrier))
}

fn phase2_verify(
    conn: &Connection,
    mode: StorageMode,
    directory: &Path,
    built: &BuiltManifest,
    expected_digest: [u8; 32],
    counters: &mut Counters,
    timings: &mut Phase2Timings,
) -> AppResult<()> {
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(format!("SQLite integrity_check failed: {integrity}").into());
    }
    let digest = phase2_materialized_digest(
        conn,
        mode,
        &directory.join("carriers"),
        built.root_hash,
        counters,
    )?;
    if digest != expected_digest {
        return Err("logical digest mismatch after reopen".into());
    }
    if mode == StorageMode::Hybrid {
        let carrier_verify_started = Instant::now();
        let mut statement =
            conn.prepare("SELECT carrier_id,relative_path,record_count FROM efs_carriers")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let carrier_id = row.get::<_, Vec<u8>>(0)?;
            let path = row.get::<_, String>(1)?;
            let record_count = row.get::<_, i64>(2)?;
            let id: [u8; 32] = carrier_id.try_into().map_err(|_| "invalid carrier ID")?;
            validate_carrier_file(
                &directory.join("carriers").join(path),
                id,
                Some((conn, record_count)),
                // phase2_materialized_digest already hashes every referenced payload;
                // this pass validates framing, references, and the carrier digest.
                false,
            )?;
        }
        if !phase2_orphan_files(conn, &directory.join("carriers"))?.is_empty() {
            return Err("unquarantined orphan carrier".into());
        }
        timings.carrier_verify_ns += carrier_verify_started.elapsed().as_nanos();
    }
    phase2_record_verification(conn, built.root_hash, expected_digest)?;
    Ok(())
}

fn phase2_record_verification(
    conn: &Connection,
    root_hash: [u8; 32],
    logical_digest: [u8; 32],
) -> AppResult<()> {
    let generation: i64 = conn.query_row(
        "SELECT generation FROM efs_root_journal WHERE root_id=? ORDER BY generation DESC LIMIT 1",
        params![root_hash.as_slice()],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO efs_verification_state(singleton,root_hash,logical_digest,verified_generation) VALUES(1,?,?,?) ON CONFLICT(singleton) DO UPDATE SET root_hash=excluded.root_hash,logical_digest=excluded.logical_digest,verified_generation=excluded.verified_generation",
        params![root_hash.as_slice(), logical_digest.as_slice(), generation],
    )?;
    Ok(())
}

fn phase2_verify_incremental(
    conn: &Connection,
    mode: StorageMode,
    directory: &Path,
    previous: &BuiltManifest,
    previous_digest: [u8; 32],
    built: &BuiltManifest,
    changed_objects: &HashMap<[u8; 32], Vec<u8>>,
    carrier: Option<&CarrierBatch>,
    expected_digest: [u8; 32],
    _counters: &mut Counters,
    timings: &mut Phase2Timings,
) -> AppResult<()> {
    let verification_started = Instant::now();
    let state: (Vec<u8>, Vec<u8>, i64) = conn.query_row(
        "SELECT root_hash,logical_digest,verified_generation FROM efs_verification_state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if state.0 != previous.root_hash.to_vec() || state.1 != previous_digest.to_vec() || state.2 < 0
    {
        return Err("incremental verification requires a matching full verification state".into());
    }
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(format!("SQLite integrity_check failed: {integrity}").into());
    }
    let stored_root: Vec<u8> = conn.query_row(
        "SELECT encoded FROM efs_manifest_roots WHERE hash=?",
        params![built.root_hash.as_slice()],
        |row| row.get(0),
    )?;
    if stored_root != built.root || sha256(&stored_root) != built.root_hash {
        return Err("incremental manifest root mismatch".into());
    }
    let journal_root: Vec<u8> = conn.query_row(
        "SELECT root_id FROM efs_root_journal ORDER BY generation DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    if journal_root != built.root_hash.to_vec() {
        return Err("incremental root journal mismatch".into());
    }
    let old_nodes: HashSet<_> = previous.nodes.iter().map(|node| node.hash).collect();
    for node in &built.nodes {
        if old_nodes.contains(&node.hash) {
            continue;
        }
        let stored: Vec<u8> = conn.query_row(
            "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
            params![node.hash.as_slice()],
            |row| row.get(0),
        )?;
        if stored != node.encoded {
            return Err("incremental manifest node mismatch".into());
        }
        decode_node(&stored, node.hash)?;
    }
    for hash in changed_objects.keys().copied() {
        let bytes = phase2_read_payload(conn, mode, &directory.join("carriers"), hash)?;
        if sha256(&bytes) != hash {
            return Err("incremental payload digest mismatch".into());
        }
    }
    if let (StorageMode::Hybrid, Some(batch)) = (mode, carrier) {
        let carrier_verify_started = Instant::now();
        validate_carrier_file(
            &directory.join("carriers").join(&batch.relative_path),
            batch.carrier_id,
            Some((conn, batch.record_count as i64)),
            true,
        )?;
        timings.carrier_verify_ns += carrier_verify_started.elapsed().as_nanos();
    }
    phase2_record_verification(conn, built.root_hash, expected_digest)?;
    let elapsed = verification_started.elapsed().as_nanos();
    timings.incremental_verification_ns += elapsed;
    Ok(())
}

fn phase2_checkpoint(path: &Path) -> AppResult<(u128, (i64, i64, i64))> {
    let conn = phase2_open(path, StorageMode::Sqlite, false)?;
    let started = Instant::now();
    let result: (i64, i64, i64) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    Ok((started.elapsed().as_nanos(), result))
}

fn phase2_base(
    directory: &Path,
    mode: StorageMode,
    size: usize,
) -> AppResult<(
    Connection,
    BuiltManifest,
    FixturePlan,
    Phase2Timings,
    Counters,
)> {
    create_dir_all(directory)?;
    let source_started = Instant::now();
    let fixture = stream_fixture(SEED, size)?;
    let source_read_ns = source_started.elapsed().as_nanos();
    let cdc_hash_ns = 0;
    let tree_started = Instant::now();
    let built = build_manifest(&fixture.entries);
    let tree_ns = tree_started.elapsed().as_nanos();
    let db_path = directory.join("fs.db");
    let mut conn = phase2_open(&db_path, mode, true)?;
    let mut counters = Counters::default();
    let mut timings = Phase2Timings {
        source_read_ns,
        cdc_hash_ns,
        tree_ns,
        ..Default::default()
    };
    let empty = HashSet::new();
    phase2_store(
        &mut conn,
        mode,
        directory,
        &built,
        Phase2ObjectSource::Fixture(&fixture),
        &empty,
        &mut counters,
        &mut timings,
    )?;
    Ok((conn, built, fixture, timings, counters))
}

fn phase2_reopen(
    conn: Connection,
    path: &Path,
    mode: StorageMode,
    directory: &Path,
    built: &BuiltManifest,
    expected: [u8; 32],
    timings: &mut Phase2Timings,
    counters: &mut Counters,
) -> AppResult<(Connection, u128, u128)> {
    let started = Instant::now();
    conn.close().map_err(|(_, error)| error)?;
    let reopened = phase2_open(path, mode, false)?;
    let reopen_ns = started.elapsed().as_nanos();
    let verification_started = Instant::now();
    phase2_verify(
        &reopened, mode, directory, built, expected, counters, timings,
    )?;
    let verification_ns = verification_started.elapsed().as_nanos();
    timings.verification_ns += verification_ns;
    Ok((reopened, reopen_ns, verification_ns))
}

fn phase2_reopen_incremental(
    conn: Connection,
    path: &Path,
    mode: StorageMode,
    directory: &Path,
    previous: &BuiltManifest,
    previous_digest: [u8; 32],
    built: &BuiltManifest,
    changed_objects: &HashMap<[u8; 32], Vec<u8>>,
    carrier: Option<&CarrierBatch>,
    expected_digest: [u8; 32],
    timings: &mut Phase2Timings,
    counters: &mut Counters,
) -> AppResult<(Connection, u128, u128)> {
    let started = Instant::now();
    conn.close().map_err(|(_, error)| error)?;
    let reopened = phase2_open(path, mode, false)?;
    let reopen_ns = started.elapsed().as_nanos();
    let verification_started = Instant::now();
    phase2_verify_incremental(
        &reopened,
        mode,
        directory,
        previous,
        previous_digest,
        built,
        changed_objects,
        carrier,
        expected_digest,
        counters,
        timings,
    )?;
    let verification_ns = verification_started.elapsed().as_nanos();
    Ok((reopened, reopen_ns, verification_ns))
}

fn phase2_materialize_to(
    conn: &Connection,
    mode: StorageMode,
    directory: &Path,
    built: &BuiltManifest,
    expected: [u8; 32],
    output: &Path,
    counters: &mut Counters,
) -> AppResult<u128> {
    let started = Instant::now();
    let root: Vec<u8> = conn.query_row(
        "SELECT encoded FROM efs_manifest_roots WHERE hash=?",
        params![built.root_hash.as_slice()],
        |row| row.get(0),
    )?;
    counters.statements += 1;
    counters.rows_read += 1;
    if sha256(&root) != built.root_hash {
        return Err("materialization root digest mismatch".into());
    }
    let root_node: [u8; 32] = root[36..68].try_into()?;
    let mut output_file = File::create(output)?;
    let mut digest = Sha256::new();
    fn visit(
        conn: &Connection,
        mode: StorageMode,
        directory: &Path,
        node_hash: [u8; 32],
        output: &mut File,
        digest: &mut Sha256,
        counters: &mut Counters,
    ) -> AppResult<()> {
        let encoded: Vec<u8> = conn.query_row(
            "SELECT encoded FROM efs_manifest_nodes WHERE hash=?",
            params![node_hash.as_slice()],
            |row| row.get(0),
        )?;
        counters.statements += 1;
        counters.rows_read += 1;
        match decode_node(&encoded, node_hash)? {
            Node::Leaf { entries, .. } => {
                for entry in entries {
                    let bytes =
                        phase2_read_payload(conn, mode, &directory.join("carriers"), entry.hash)?;
                    if bytes.len() != entry.length {
                        return Err("materialization object length mismatch".into());
                    }
                    output.write_all(&bytes)?;
                    digest.update(bytes);
                    counters.object_reads += 1;
                }
            }
            Node::Internal { children, .. } => {
                for child in children {
                    visit(conn, mode, directory, child.hash, output, digest, counters)?;
                }
            }
        }
        Ok(())
    }
    visit(
        conn,
        mode,
        directory,
        root_node,
        &mut output_file,
        &mut digest,
        counters,
    )?;
    output_file.sync_all()?;
    let materialized: [u8; 32] = digest.finalize().into();
    if materialized != expected || output_file.metadata()?.len() != built.file_size as u64 {
        return Err("materialized output digest mismatch".into());
    }
    Ok(started.elapsed().as_nanos())
}

fn phase2_random_reads(
    conn: &Connection,
    mode: StorageMode,
    directory: &Path,
    built: &BuiltManifest,
    expected_source: &[u8],
    reads: usize,
    counters: &mut Counters,
) -> AppResult<(u128, [u8; 32])> {
    if built.file_size < 4096 {
        return Err("random-read fixture is smaller than 4 KiB".into());
    }
    let started = Instant::now();
    let mut digest = Sha256::new();
    let range = built.file_size - 4096;
    for index in 0..reads {
        let offset = (index.wrapping_mul(1_048_573) + 17) % (range + 1);
        let (leaf, _) = phase2_leaf_context_at_offset(built, offset)?;
        let bytes = phase2_read_source_window(
            conn,
            mode,
            &directory.join("carriers"),
            &built.entries,
            leaf.target_entry,
            leaf.target_offset,
            built.file_size,
            offset - leaf.target_offset + 4096,
            counters,
        )?;
        let relative = offset - leaf.target_offset;
        if bytes.len() < relative + 4096
            || bytes[relative..relative + 4096] != expected_source[offset..offset + 4096]
        {
            return Err("random-read bytes mismatch".into());
        }
        digest.update(&bytes[relative..relative + 4096]);
    }
    Ok((started.elapsed().as_nanos(), digest.finalize().into()))
}

fn stage_compaction_carrier(conn: &Connection, carrier_dir: &Path) -> AppResult<CarrierBatch> {
    let mut writer = CarrierWriter::new(carrier_dir)?;
    let mut statement = conn.prepare("SELECT hash FROM efs_cas_objects ORDER BY hash")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let hash: [u8; 32] = row
            .get::<_, Vec<u8>>(0)?
            .try_into()
            .map_err(|_| "invalid object hash")?;
        let bytes = phase2_read_payload(conn, StorageMode::Hybrid, carrier_dir, hash)?;
        writer.append(hash, &bytes)?;
    }
    drop(rows);
    drop(statement);
    if writer.record_count == 0 {
        let temp_path = writer.temp_path.clone();
        drop(writer.file);
        std::fs::remove_file(temp_path)?;
        return Err("cannot compact empty carrier set".into());
    }
    Ok(writer.finish()?)
}

fn phase2_compact(
    conn: &mut Connection,
    directory: &Path,
    counters: &mut Counters,
) -> AppResult<u128> {
    let started = Instant::now();
    let carrier_dir = directory.join("carriers");
    let batch = stage_compaction_carrier(conn, &carrier_dir)?;
    let tx = conn.transaction()?;
    counters.transactions += 1;
    tx.execute("INSERT OR IGNORE INTO efs_carriers(carrier_id,relative_path,carrier_bytes,carrier_digest,record_count,format_version) VALUES(?,?,?,?,?,?)", params![batch.carrier_id.as_slice(), batch.relative_path, batch.byte_length as i64, batch.carrier_id.as_slice(), batch.record_count as i64, CARRIER_VERSION as i64])?;
    let mut carrier_offset = 0usize;
    let mut statement = tx.prepare("SELECT hash,size FROM efs_cas_objects ORDER BY hash")?;
    let updates = statement
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for (hash, size) in updates {
        if size < 0 {
            return Err("negative object size during compaction".into());
        }
        tx.execute("UPDATE efs_cas_objects SET carrier_id=?,carrier_offset=?,carrier_length=?,carrier_checksum=? WHERE hash=?", params![batch.carrier_id.as_slice(), carrier_offset as i64, size, hash.as_slice(), hash.as_slice()])?;
        carrier_offset = carrier_offset
            .checked_add(CARRIER_HEADER_BYTES)
            .and_then(|value| value.checked_add(size as usize))
            .ok_or("carrier offset overflow")?;
    }
    tx.execute(
        "DELETE FROM efs_carriers WHERE carrier_id NOT IN (SELECT DISTINCT carrier_id FROM efs_cas_objects)",
        [],
    )?;
    tx.commit()?;
    counters.peak_disk_bytes = counters.peak_disk_bytes.max(directory_bytes(directory)?);
    counters.commits += 1;
    let mut old = Vec::new();
    for entry in std::fs::read_dir(&carrier_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".lfc") && name != batch.relative_path {
            old.push(entry.path());
        }
    }
    for path in old {
        std::fs::remove_file(path)?;
    }
    sync_directory(&carrier_dir)?;
    Ok(started.elapsed().as_nanos())
}

fn phase2_json_timing(t: &Phase2Timings) -> serde_json::Value {
    json!({
        "source_read_ms": t.source_read_ns as f64 / 1e6,
        "fastcdc_hash_ms": t.cdc_hash_ns as f64 / 1e6,
        "tree_member_ms": t.tree_ns as f64 / 1e6,
        "m7_canonical_check_ms": t.canonical_check_ns as f64 / 1e6,
        "object_admission_ms": t.admission_ns as f64 / 1e6,
        "payload_persistence_ms": t.storage_before_commit_ns as f64 / 1e6,
        "carrier_record_encode_ms": t.carrier_record_encode_ns as f64 / 1e6,
        "carrier_append_ms": t.carrier_append_ns as f64 / 1e6,
        "carrier_digest_ms": t.carrier_digest_ns as f64 / 1e6,
        "carrier_write_ms": t.carrier_write_ns as f64 / 1e6,
        "carrier_fsync_ms": t.carrier_fsync_ns as f64 / 1e6,
        "carrier_sync_ms": t.carrier_fsync_ns as f64 / 1e6,
        "carrier_verify_ms": t.carrier_verify_ns as f64 / 1e6,
        "sqlite_metadata_ms": t.sqlite_metadata_ns as f64 / 1e6,
        "sqlite_commit_ms": t.sqlite_commit_ns as f64 / 1e6,
        "close_reopen_ms": t.close_reopen_ns as f64 / 1e6,
        "full_verification_ms": t.verification_ns as f64 / 1e6,
        "incremental_verification_ms": t.incremental_verification_ns as f64 / 1e6,
        "checkpoint_ms": t.checkpoint_ns as f64 / 1e6,
    })
}

fn phase2_directory(mode: StorageMode, workload: &str, size: usize) -> PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-phase2-{}-{}-{}-{}",
        process::id(),
        mode.name(),
        workload,
        size
    ))
}

fn phase2_create_record(mode: StorageMode, size: usize) -> AppResult<serde_json::Value> {
    let workload = "create";
    let directory = phase2_directory(mode, workload, size);
    let _ = remove_dir_all(&directory);
    create_dir_all(&directory)?;
    let started = Instant::now();
    let (conn, built, fixture, mut timings, mut counters) = phase2_base(&directory, mode, size)?;
    let expected = fixture.digest;
    let s1 = phase2_space(&conn, mode, &directory, &directory.join("carriers"))?;
    let db_path = directory.join("fs.db");
    let (conn, reopen_ns, _) = phase2_reopen(
        conn,
        &db_path,
        mode,
        &directory,
        &built,
        expected,
        &mut timings,
        &mut counters,
    )?;
    timings.close_reopen_ns += reopen_ns;
    let s2 = phase2_space(&conn, mode, &directory, &directory.join("carriers"))?;
    conn.close().map_err(|(_, error)| error)?;
    let (checkpoint_ns, checkpoint) = phase2_checkpoint(&db_path)?;
    timings.checkpoint_ns += checkpoint_ns;
    let mut steady = phase2_open(&db_path, mode, false)?;
    let s3 = phase2_space(&steady, mode, &directory, &directory.join("carriers"))?;
    let primary_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let primary_counters = counters.clone();
    let post_compaction = if mode == StorageMode::Hybrid {
        let compact_ns = phase2_compact(&mut steady, &directory, &mut counters)?;
        phase2_verify(
            &steady,
            mode,
            &directory,
            &built,
            expected,
            &mut counters,
            &mut timings,
        )?;
        let checkpoint_after_compaction = phase2_checkpoint(&db_path)?;
        let compact_space = phase2_space(&steady, mode, &directory, &directory.join("carriers"))?;
        json!({"status":"pass","compact_ms":compact_ns as f64 / 1e6,"checkpoint_ms":checkpoint_after_compaction.0 as f64 / 1e6,"space":compact_space})
    } else {
        json!("not_applicable")
    };
    let s4 = phase2_space(&steady, mode, &directory, &directory.join("carriers"))?;
    let post_compaction_total_bytes = post_compaction
        .get("space")
        .and_then(|space| space.get("total_bytes"))
        .cloned()
        .unwrap_or_else(|| s4["total_bytes"].clone());
    steady.close().map_err(|(_, error)| error)?;
    let result = json!({
        "phase2":"pass",
        "candidate":mode.name(),
        "workload":workload,
        "size_bytes":size,
        "size_mib":size / MIB,
        "base_root":hex(&built.root_hash),
        "root":hex(&built.root_hash),
        "logical_digest":hex(&expected),
        "object_count":built.entries.len(),
        "manifest_node_count":built.nodes.len(),
        "elapsed_ms":primary_elapsed_ms,
        "throughput_mib_s":(size as f64 / MIB as f64) / (primary_elapsed_ms / 1000.0),
        "timing":phase2_json_timing(&timings),
        "counters":phase2_counters_json(&counters),
        "primary_counters":phase2_counters_json(&primary_counters),
        "post_compaction_counters":phase2_counter_delta_json(&primary_counters, &counters),
        "checkpoint":{"busy":checkpoint.0,"log_frames":checkpoint.1,"checkpointed_frames":checkpoint.2},
        "space":{"before":json!({"total_bytes":0}),"after_commit_before_close":s1,"after_reopen":s2,"after_checkpoint":s3,"steady_state":s4,"post_compaction":post_compaction},
        "storage":{"database_bytes":s4["database_bytes"],"wal_bytes":s4["wal_bytes"],"shm_bytes":s4["shm_bytes"],"carrier_bytes":s4["carrier_bytes"],"temporary_bytes":s4["temporary_bytes"],"live_payload_bytes":s4["live_payload_bytes"],"obsolete_carrier_bytes":s4["obsolete_carrier_bytes"],"peak_total_storage_bytes":counters.peak_disk_bytes,"steady_state_total_bytes":s4["total_bytes"],"post_compaction_total_bytes":post_compaction_total_bytes},
        "peak_total_bytes":counters.peak_disk_bytes,
        "correctness":{"self_check":"pass","read_after_write":"pass","close_reopen":"pass","root":"pass","digest":"pass","sqlite_integrity":"pass","carrier":"pass"},
    });
    remove_dir_all(&directory)?;
    Ok(result)
}

fn phase2_edit_record(
    mode: StorageMode,
    size: usize,
    count: usize,
) -> AppResult<serde_json::Value> {
    let workload = if count == 1 {
        "edit1"
    } else {
        "edit3-m7-bounded"
    };
    let directory = phase2_directory(mode, workload, size);
    let _ = remove_dir_all(&directory);
    create_dir_all(&directory)?;
    let (conn, base, fixture, mut timings, mut counters) = phase2_base(&directory, mode, size)?;
    let expected_base = fixture.digest;
    let db_path = directory.join("fs.db");
    let (mut conn, reopen_ns, _) = phase2_reopen(
        conn,
        &db_path,
        mode,
        &directory,
        &base,
        expected_base,
        &mut timings,
        &mut counters,
    )?;
    timings.close_reopen_ns += reopen_ns;
    let offsets = if count == 1 {
        vec![size / 2]
    } else {
        vec![0, size / 2, size - 1]
    };
    let mut expected_bytes = deterministic_bytes(SEED, size);
    let base_root = base.root_hash;
    let mut current_expected = expected_base;
    let mut current = base;
    let mut edits = Vec::new();
    let operation_start_counters = counters.clone();
    let mut operation_bounded_local_edit_ns = 0u128;
    let mut operation_canonical_check_ns = 0u128;
    let mut operation_payload_persistence_ns = 0u128;
    let mut operation_sqlite_commit_ns = 0u128;
    let mut operation_close_reopen_ns = 0u128;
    let mut operation_full_verification_ns = 0u128;
    let operation_started = Instant::now();
    for offset in offsets {
        let edit_started = Instant::now();
        let canonical_check_before = timings.canonical_check_ns;
        let bounded_edit_started = Instant::now();
        let (rebuilt, output_objects, metrics) = phase2_edit_entries(
            &conn,
            mode,
            &directory.join("carriers"),
            &current,
            &expected_bytes,
            offset,
            &mut timings,
            &mut counters,
        )?;
        let canonical_check_ns = timings
            .canonical_check_ns
            .saturating_sub(canonical_check_before);
        let bounded_local_edit_ns = bounded_edit_started
            .elapsed()
            .as_nanos()
            .saturating_sub(canonical_check_ns);
        let old_nodes: HashSet<_> = current.nodes.iter().map(|node| node.hash).collect();
        let mut edit_timings = Phase2Timings::default();
        let (new_objects, new_nodes, carrier) = phase2_store(
            &mut conn,
            mode,
            &directory,
            &rebuilt,
            Phase2ObjectSource::Map(&output_objects),
            &old_nodes,
            &mut counters,
            &mut edit_timings,
        )?;
        timings.admission_ns += edit_timings.admission_ns;
        timings.storage_before_commit_ns += edit_timings.storage_before_commit_ns;
        timings.carrier_record_encode_ns += edit_timings.carrier_record_encode_ns;
        timings.carrier_append_ns += edit_timings.carrier_append_ns;
        timings.carrier_digest_ns += edit_timings.carrier_digest_ns;
        timings.carrier_write_ns += edit_timings.carrier_write_ns;
        timings.carrier_fsync_ns += edit_timings.carrier_fsync_ns;
        timings.sqlite_metadata_ns += edit_timings.sqlite_metadata_ns;
        timings.sqlite_commit_ns += edit_timings.sqlite_commit_ns;
        expected_bytes[offset] = INSERT_BYTE;
        let expected = sha256(&expected_bytes);
        let (reopened, close_reopen_ns, incremental_verification_ns) = phase2_reopen_incremental(
            conn,
            &db_path,
            mode,
            &directory,
            &current,
            current_expected,
            &rebuilt,
            &output_objects,
            carrier.as_ref(),
            expected,
            &mut timings,
            &mut counters,
        )?;
        timings.close_reopen_ns += close_reopen_ns;
        conn = reopened;
        let total_end_to_end_ns = edit_started.elapsed().as_nanos();
        operation_bounded_local_edit_ns += bounded_local_edit_ns;
        operation_canonical_check_ns += canonical_check_ns;
        operation_payload_persistence_ns += edit_timings.storage_before_commit_ns;
        operation_sqlite_commit_ns += edit_timings.sqlite_commit_ns;
        operation_close_reopen_ns += close_reopen_ns;
        operation_full_verification_ns += incremental_verification_ns;
        edits.push(json!({
            "offset":offset,
            "elapsed_ms":total_end_to_end_ns as f64/1e6,
            "new_objects":new_objects,
            "new_nodes":new_nodes,
            "changed_object_set":metrics["changedObjectSet"],
            "unchanged_identity_set":metrics["unchangedIdentitySet"],
            "metrics":metrics,
            "digest":hex(&expected),
            "timing": {
                "m7_bounded_local_edit_ms": bounded_local_edit_ns as f64/1e6,
                "m7_canonical_check_ms": canonical_check_ns as f64/1e6,
                "payload_persistence_ms": edit_timings.storage_before_commit_ns as f64/1e6,
                "sqlite_commit_ms": edit_timings.sqlite_commit_ns as f64/1e6,
                "close_reopen_ms": close_reopen_ns as f64/1e6,
                "verification_mode": "incremental",
                "incremental_verification_ms": incremental_verification_ns as f64/1e6,
                "total_end_to_end_ms": total_end_to_end_ns as f64/1e6
            }
        }));
        current_expected = expected;
        current = rebuilt;
    }
    let total_ms = operation_started.elapsed().as_secs_f64() * 1000.0;
    let space = phase2_space(&conn, mode, &directory, &directory.join("carriers"))?;
    let result = json!({
        "phase2":"pass","candidate":mode.name(),"workload":workload,"size_bytes":size,"size_mib":size/MIB,
        "elapsed_ms":total_ms,"total_end_to_end_ms":total_ms,"throughput_mib_s":(size as f64 / MIB as f64)/(total_ms/1000.0),"edits":edits,
        "base_root":hex(&base_root),
        "root":hex(&current.root_hash),
        "logical_digest":hex(&sha256(&expected_bytes)),
        "object_count":current.entries.len(),
        "manifest_node_count":current.nodes.len(),
        "operation_timing": {
            "m7_bounded_local_edit_ms": operation_bounded_local_edit_ns as f64/1e6,
            "m7_canonical_check_ms": operation_canonical_check_ns as f64/1e6,
            "payload_persistence_ms": operation_payload_persistence_ns as f64/1e6,
            "sqlite_commit_ms": operation_sqlite_commit_ns as f64/1e6,
            "close_reopen_ms": operation_close_reopen_ns as f64/1e6,
            "verification_mode": "incremental",
            "incremental_verification_ms": operation_full_verification_ns as f64/1e6,
            "total_end_to_end_ms": total_ms
        },
        "timing":phase2_json_timing(&timings),
        "counters":phase2_counters_json(&counters),
        "operation_counters":phase2_counter_delta_json(&operation_start_counters, &counters),
        "space":{"steady_state":space.clone()},"storage":{"steady_state":space,"peak_total_storage_bytes":counters.peak_disk_bytes},"correctness":{"changed_object_set":"pass","unchanged_identity_set":"pass","close_reopen":"pass","digest":"pass","carrier":"pass","sqlite_integrity":"pass"}
    });
    conn.close().map_err(|(_, error)| error)?;
    remove_dir_all(&directory)?;
    Ok(result)
}

fn phase2_read_record(mode: StorageMode, size: usize) -> AppResult<serde_json::Value> {
    let workload = "read";
    let directory = phase2_directory(mode, workload, size);
    let _ = remove_dir_all(&directory);
    create_dir_all(&directory)?;
    let (conn, built, fixture, mut timings, mut counters) = phase2_base(&directory, mode, size)?;
    let expected = fixture.digest;
    let original = deterministic_bytes(SEED, size);
    let db_path = directory.join("fs.db");
    let (conn, reopen_ns, _) = phase2_reopen(
        conn,
        &db_path,
        mode,
        &directory,
        &built,
        expected,
        &mut timings,
        &mut counters,
    )?;
    timings.close_reopen_ns += reopen_ns;
    let read_start_counters = counters.clone();
    let cold_started = Instant::now();
    let cold_digest = phase2_materialized_digest(
        &conn,
        mode,
        &directory.join("carriers"),
        built.root_hash,
        &mut counters,
    )?;
    let cold_ns = cold_started.elapsed().as_nanos();
    let warm_started = Instant::now();
    let warm_digest = phase2_materialized_digest(
        &conn,
        mode,
        &directory.join("carriers"),
        built.root_hash,
        &mut counters,
    )?;
    let warm_ns = warm_started.elapsed().as_nanos();
    let (random_ns, random_digest) = phase2_random_reads(
        &conn,
        mode,
        &directory,
        &built,
        &original,
        RANDOM_READS,
        &mut counters,
    )?;
    if cold_digest != expected || warm_digest != expected || random_digest == [0u8; 32] {
        return Err("read verification failed".into());
    }
    let storage = phase2_space(&conn, mode, &directory, &directory.join("carriers"))?;
    let result = json!({"phase2":"pass","candidate":mode.name(),"workload":workload,"size_bytes":size,"size_mib":size/MIB,
        "base_root":hex(&built.root_hash),"root":hex(&built.root_hash),"logical_digest":hex(&expected),"object_count":built.entries.len(),"manifest_node_count":built.nodes.len(),
        "cold_label":"apfs_direct_reopened_namespace","warm_label":"apfs_direct_same_process_repeat","cold_read_ms":cold_ns as f64/1e6,"warm_read_ms":warm_ns as f64/1e6,"cold_mib_s":(size as f64/MIB as f64)/(cold_ns as f64/1e9),"warm_mib_s":(size as f64/MIB as f64)/(warm_ns as f64/1e9),"random_reads":RANDOM_READS,"random_4k_total_ms":random_ns as f64/1e6,"random_4k_us":random_ns as f64/RANDOM_READS as f64/1e3,"storage":{"steady_state":storage,"peak_total_storage_bytes":counters.peak_disk_bytes},"timing":phase2_json_timing(&timings),"counters":phase2_counters_json(&counters),"operation_counters":phase2_counter_delta_json(&read_start_counters, &counters),"correctness":{"cold_digest":"pass","warm_digest":"pass","random_bytes":"pass","close_reopen":"pass","sqlite_integrity":"pass","carrier":"pass"}});
    conn.close().map_err(|(_, error)| error)?;
    remove_dir_all(&directory)?;
    Ok(result)
}

fn phase2_materialize_record(mode: StorageMode, size: usize) -> AppResult<serde_json::Value> {
    let workload = "materialize";
    let directory = phase2_directory(mode, workload, size);
    let _ = remove_dir_all(&directory);
    create_dir_all(&directory)?;
    let (conn, built, fixture, mut timings, mut counters) = phase2_base(&directory, mode, size)?;
    let expected = fixture.digest;
    let db_path = directory.join("fs.db");
    let (conn, reopen_ns, _) = phase2_reopen(
        conn,
        &db_path,
        mode,
        &directory,
        &built,
        expected,
        &mut timings,
        &mut counters,
    )?;
    timings.close_reopen_ns += reopen_ns;
    let materialize_start_counters = counters.clone();
    let output = directory.join("materialized.bin");
    let materialize_ns = phase2_materialize_to(
        &conn,
        mode,
        &directory,
        &built,
        expected,
        &output,
        &mut counters,
    )?;
    let storage = phase2_space(&conn, mode, &directory, &directory.join("carriers"))?;
    let result = json!({"phase2":"pass","candidate":mode.name(),"workload":workload,"size_bytes":size,"size_mib":size/MIB,
        "base_root":hex(&built.root_hash),"root":hex(&built.root_hash),"logical_digest":hex(&expected),"object_count":built.entries.len(),"manifest_node_count":built.nodes.len(),
        "materialize_ms":materialize_ns as f64/1e6,"materialize_mib_s":(size as f64/MIB as f64)/(materialize_ns as f64/1e9),"storage":{"steady_state":storage,"peak_total_storage_bytes":counters.peak_disk_bytes},"timing":phase2_json_timing(&timings),"counters":phase2_counters_json(&counters),"operation_counters":phase2_counter_delta_json(&materialize_start_counters, &counters),"correctness":{"output_digest":"pass","output_bytes":"pass","close_reopen":"pass","carrier":"pass","sqlite_integrity":"pass"}});
    conn.close().map_err(|(_, error)| error)?;
    remove_dir_all(&directory)?;
    Ok(result)
}

fn phase2_many_materialize_record(mode: StorageMode) -> AppResult<serde_json::Value> {
    let size = MIB;
    let workload = "materialize-100x1m";
    let directory = phase2_directory(mode, workload, size);
    let _ = remove_dir_all(&directory);
    create_dir_all(&directory)?;
    let (conn, built, fixture, mut timings, mut counters) = phase2_base(&directory, mode, size)?;
    let expected = fixture.digest;
    let db_path = directory.join("fs.db");
    let (conn, _, _) = phase2_reopen(
        conn,
        &db_path,
        mode,
        &directory,
        &built,
        expected,
        &mut timings,
        &mut counters,
    )?;
    let materialize_start_counters = counters.clone();
    let output = directory.join("materialized-1m.bin");
    let mut samples = Vec::new();
    for _ in 0..100 {
        samples.push(
            phase2_materialize_to(
                &conn,
                mode,
                &directory,
                &built,
                expected,
                &output,
                &mut counters,
            )? as f64
                / 1e6,
        );
    }
    let total_ms: f64 = samples.iter().sum();
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    let storage = phase2_space(&conn, mode, &directory, &directory.join("carriers"))?;
    let result = json!({"phase2":"pass","candidate":mode.name(),"workload":workload,"count":100,"logical_bytes":100*MIB,
        "base_root":hex(&built.root_hash),"root":hex(&built.root_hash),"logical_digest":hex(&expected),"object_count":built.entries.len(),"manifest_node_count":built.nodes.len(),
        "total_ms":total_ms,"median_ms":sorted[50],"min_ms":sorted[0],"max_ms":sorted[99],"storage":{"steady_state":storage,"peak_total_storage_bytes":counters.peak_disk_bytes},"timing":phase2_json_timing(&timings),"counters":phase2_counters_json(&counters),"operation_counters":phase2_counter_delta_json(&materialize_start_counters, &counters),"correctness":{"all_outputs":"pass","close_reopen":"pass","carrier":"pass"}});
    conn.close().map_err(|(_, error)| error)?;
    remove_dir_all(&directory)?;
    Ok(result)
}

fn phase2_crash_child(
    mode: StorageMode,
    db_path: &Path,
    size: usize,
    boundary: &str,
) -> AppResult<()> {
    let directory = db_path.parent().ok_or("database has no parent")?;
    let mut conn = phase2_open(db_path, mode, false)?;
    let original = deterministic_bytes(SEED, size);
    let (entries, _) = chunk_fixture(&original);
    let built = build_manifest(&entries);
    let mut counters = Counters::default();
    let mut timings = Phase2Timings::default();
    let (rebuilt, objects, _) = phase2_edit_entries(
        &conn,
        mode,
        &directory.join("carriers"),
        &built,
        &original,
        size / 2,
        &mut timings,
        &mut counters,
    )?;
    let old_nodes: HashSet<_> = built.nodes.iter().map(|node| node.hash).collect();
    let carrier_dir = directory.join("carriers");
    let object_source = Phase2ObjectSource::Map(&objects);
    let missing = phase2_prepare_objects(&conn, &object_source, mode, &mut counters)?;
    let carrier = match mode {
        StorageMode::Sqlite => None,
        StorageMode::Hybrid => stage_carrier(&carrier_dir, &object_source, &missing)?,
    };
    let generation: i64 = conn.query_row(
        "SELECT coalesce(max(generation),0)+1 FROM efs_root_journal",
        [],
        |row| row.get(0),
    )?;
    let tx = conn.transaction()?;
    counters.transactions += 1;
    phase2_persist_manifest(
        &tx,
        &rebuilt,
        &object_source,
        &missing,
        &old_nodes,
        &mut counters,
        generation,
        mode,
        &carrier_dir,
        carrier.as_ref(),
    )?;
    if boundary == "before-commit" {
        process::exit(91);
    }
    tx.commit()?;
    if boundary == "after-commit" {
        process::exit(92);
    }
    Ok(())
}

fn phase2_recovery_record(mode: StorageMode, size: usize) -> AppResult<serde_json::Value> {
    let workload = "recovery";
    let directory = phase2_directory(mode, workload, size);
    let _ = remove_dir_all(&directory);
    create_dir_all(&directory)?;
    let (conn, built, fixture, mut timings, mut counters) = phase2_base(&directory, mode, size)?;
    let old_expected = fixture.digest;
    let db_path = directory.join("fs.db");
    conn.close().map_err(|(_, error)| error)?;
    let exe = std::env::current_exe()?;
    let run_child = |boundary: &str| -> AppResult<i32> {
        let size_mib = (size / MIB).to_string();
        Ok(Command::new(&exe)
            .args([
                "phase2-crash",
                match mode {
                    StorageMode::Sqlite => "sqlite",
                    StorageMode::Hybrid => "hybrid",
                },
                db_path.to_str().ok_or("non-UTF8 database path")?,
                &size_mib,
                boundary,
            ])
            .status()?
            .code()
            .unwrap_or(-1))
    };
    let before_status = run_child("before-commit")?;
    let recovered_before = phase2_open(&db_path, mode, false)?;
    let orphan_count = if mode == StorageMode::Hybrid {
        phase2_quarantine_orphans(&recovered_before, &directory.join("carriers"))?
    } else {
        0
    };
    phase2_verify(
        &recovered_before,
        mode,
        &directory,
        &built,
        old_expected,
        &mut counters,
        &mut timings,
    )?;
    let baseline_counts: (i64, i64, i64) = recovered_before.query_row("SELECT (SELECT count(*) FROM efs_cas_objects),(SELECT count(*) FROM efs_manifest_roots),(SELECT count(*) FROM efs_root_journal)", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    recovered_before.close().map_err(|(_, error)| error)?;
    let after_status = run_child("after-commit")?;
    let after = phase2_open(&db_path, mode, false)?;
    let mut expected = deterministic_bytes(SEED, size);
    expected[size / 2] = INSERT_BYTE;
    let (new_entries, _) = chunk_fixture(&expected);
    let new_built = build_manifest(&new_entries);
    let new_expected = sha256(&expected);
    phase2_verify(
        &after,
        mode,
        &directory,
        &new_built,
        new_expected,
        &mut counters,
        &mut timings,
    )?;
    let after_counts: (i64, i64, i64) = after.query_row("SELECT (SELECT count(*) FROM efs_cas_objects),(SELECT count(*) FROM efs_manifest_roots),(SELECT count(*) FROM efs_root_journal)", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    after.close().map_err(|(_, error)| error)?;
    let result = json!({"phase2":"pass","candidate":mode.name(),"workload":workload,"size_mib":size/MIB,"crash_before_commit_status":before_status,"crash_after_commit_status":after_status,"old_or_new_before":before_status==91,"old_or_new_after":after_status==92,"orphan_files_quarantined":orphan_count,"baseline_counts":baseline_counts,"after_counts":after_counts,"correctness":{"crash_before_commit":"pass","crash_after_commit":"pass","orphan_detection":"pass","orphan_quarantine":"pass","referenced_payloads":"pass","sqlite_integrity":"pass"}});
    remove_dir_all(&directory)?;
    Ok(result)
}

fn phase2_carrier_check() -> AppResult<serde_json::Value> {
    if parse_phase2_size("100")? != 100 * MIB || parse_phase2_size("101").is_ok() {
        return Err("phase2 size guard failed".into());
    }
    let directory =
        std::env::temp_dir().join(format!("layerfs-phase2-carrier-check-{}", process::id()));
    let _ = remove_dir_all(&directory);
    let carrier_dir = directory.join("carriers");
    let bytes = b"carrier-format-check".to_vec();
    let hash = sha256(&bytes);
    let mut objects = HashMap::new();
    objects.insert(hash, bytes.clone());
    let missing = HashSet::from([hash]);
    let object_source = Phase2ObjectSource::Map(&objects);
    let batch = stage_carrier(&carrier_dir, &object_source, &missing)?
        .ok_or("carrier self-check did not stage")?;
    if validate_carrier_file(
        &carrier_dir.join(&batch.relative_path),
        batch.carrier_id,
        None,
        true,
    )? != 1
    {
        return Err("carrier self-check record count failed".into());
    }
    let duplicate = stage_carrier(&carrier_dir, &object_source, &missing)?
        .ok_or("carrier idempotence check did not stage")?;
    if duplicate.carrier_id != batch.carrier_id
        || duplicate.byte_length != batch.byte_length
        || duplicate.record_count != batch.record_count
    {
        return Err("carrier idempotence check failed".into());
    }
    if read_carrier_record(
        &carrier_dir.join(&batch.relative_path),
        &CarrierRecord {
            offset: 0,
            length: bytes.len(),
            checksum: hash,
        },
        hash,
    )? != bytes
    {
        return Err("carrier self-check read failed".into());
    }
    let corrupt = carrier_dir.join("corrupt.lfc");
    std::fs::copy(carrier_dir.join(&batch.relative_path), &corrupt)?;
    OpenOptions::new().write(true).open(&corrupt)?.set_len(3)?;
    if validate_carrier_file(&corrupt, batch.carrier_id, None, true).is_ok() {
        return Err("truncated carrier accepted".into());
    }
    remove_dir_all(&directory)?;
    Ok(
        json!({"carrier_format_check":"pass","truncated_record":"rejected","payload_hash":"verified","bounds":"verified"}),
    )
}

fn pragmas(conn: &Connection) -> AppResult<serde_json::Value> {
    let sqlite_version: String = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let journal: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    let mmap: i64 = conn.pragma_query_value(None, "mmap_size", |row| row.get(0))?;
    let cache: i64 = conn.pragma_query_value(None, "cache_size", |row| row.get(0))?;
    let wal_auto: i64 = conn.pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))?;
    let foreign_keys: i64 = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    Ok(
        json!({"sqlite":sqlite_version,"journal_mode":journal,"synchronous":synchronous,"mmap_size":mmap,"cache_size":cache,"wal_autocheckpoint":wal_auto,"foreign_keys":foreign_keys}),
    )
}

fn run_size(size: usize) -> AppResult<serde_json::Value> {
    let started = Instant::now();
    let original = deterministic_bytes(SEED, size);
    let (entries, objects) = chunk_fixture(&original);
    let built = build_manifest(&entries);
    let base_digest = sha256(&original);
    drop(original);
    let directory =
        std::env::temp_dir().join(format!("layerfs-m8-rust-{}-{}", std::process::id(), size));
    let _ = remove_dir_all(&directory);
    create_dir_all(&directory)?;
    let db_path = directory.join("fs.db");
    let mut counters = Counters::default();
    let mut conn = initialize_db(&db_path, &built, &objects, &mut counters)?;
    let guards = guard_checks(&db_path, &built, &objects)?;
    if [
        "sameIdReuse",
        "collisionRejected",
        "rollback",
        "rootUnchanged",
    ]
    .iter()
    .any(|key| guards[*key].as_bool() != Some(true))
    {
        return Err("CAS identity or rollback guard failed".into());
    }
    let settings = pragmas(&conn)?;
    let mut edits = Vec::new();
    for offset in [0usize, size / 2, size - 1] {
        let edit_started = Instant::now();
        let (rebuilt, output_objects, metrics) =
            edit_entries(&conn, &built, offset, &mut counters)?;
        let old_nodes: HashSet<_> = built.nodes.iter().map(|node| node.hash).collect();
        let old_objects: HashSet<_> = built.entries.iter().map(|entry| entry.hash).collect();
        let generation: i64 = conn.query_row(
            "SELECT coalesce(max(generation),0)+1 FROM efs_root_journal",
            [],
            |row| row.get(0),
        )?;
        let tx = conn.transaction()?;
        counters.transactions += 1;
        let (new_objects, new_nodes) = persist_manifest(
            &tx,
            &rebuilt,
            &output_objects,
            &old_nodes,
            &old_objects,
            &mut counters,
            generation,
        )?;
        tx.commit()?;
        counters.commits += 1;
        let prepare_ms = edit_started.elapsed().as_secs_f64() * 1000.0;
        let reopen = Connection::open(&db_path)?;
        configure(&reopen)?;
        let verify_started = Instant::now();
        let digest = materialized_digest(&reopen, rebuilt.root_hash, &mut counters)?;
        if digest != expected_digest(size, offset) {
            return Err("reopened logical digest mismatch".into());
        }
        let verify_ms = verify_started.elapsed().as_secs_f64() * 1000.0;
        edits.push(json!({"offset":offset,"root":hex(&rebuilt.root_hash),"digest":hex(&digest),"prepare_ms":prepare_ms,"reopen_verify_ms":verify_ms,"mode":"local-rebuild","new_objects":new_objects,"new_nodes":new_nodes,"source_reads":counters.source_reads,"source_bytes":counters.source_bytes,"source_transactions":counters.source_transactions,"transactions":counters.transactions,"commits":counters.commits,"metrics":metrics}));
        reopen.close().map_err(|(_, err)| err)?;
    }
    let final_root: Vec<u8> = conn.query_row(
        "SELECT root_id FROM efs_root_journal ORDER BY generation DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    let final_root: [u8; 32] = final_root
        .try_into()
        .map_err(|_| "invalid published root")?;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let checkpoint: (i64, i64, i64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    counters.checkpoint_bytes = checkpoint.2;
    remove_dir_all(&directory)?;
    Ok(
        json!({"candidate":"rust","size":size,"size_mib":size/MIB,"base_root":hex(&built.root_hash),"base_digest":hex(&base_digest),"final_root":hex(&final_root),"manifest_entries":entries.len(),"manifest_nodes":built.nodes.len(),"settings":settings,"guards":guards,"elapsed_ms":elapsed_ms,"checkpoint":{"busy":checkpoint.0,"log_frames":checkpoint.1,"checkpointed_frames":checkpoint.2},"counters":{"statements":counters.statements,"rows_read":counters.rows_read,"rows_inserted":counters.rows_inserted,"source_reads":counters.source_reads,"source_bytes":counters.source_bytes,"source_transactions":counters.source_transactions,"node_reads":counters.node_reads,"object_reads":counters.object_reads,"transactions":counters.transactions,"commits":counters.commits},"edits":edits}),
    )
}

fn parse_storage_mode(value: &str) -> AppResult<StorageMode> {
    match value {
        "sqlite" | "r-sqlite" => Ok(StorageMode::Sqlite),
        "hybrid" | "r-hybrid" => Ok(StorageMode::Hybrid),
        _ => Err(format!("unknown storage mode: {value}").into()),
    }
}

fn parse_phase2_size(value: &str) -> AppResult<usize> {
    let size_mib = value.parse::<usize>()?;
    if size_mib == 0 || size_mib > MAX_PHASE2_MIB {
        return Err(format!("phase2 size must be 1..={MAX_PHASE2_MIB} MiB").into());
    }
    size_mib
        .checked_mul(MIB)
        .ok_or_else(|| "phase2 size overflow".into())
}

fn main() -> AppResult<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("phase2-carrier-check") => {
            println!("{}", phase2_carrier_check()?);
            return Ok(());
        }
        Some("phase2-crash") => {
            let mode = parse_storage_mode(args.get(2).ok_or("phase2-crash requires mode")?)?;
            let path = PathBuf::from(args.get(3).ok_or("phase2-crash requires database")?);
            let size = parse_phase2_size(args.get(4).ok_or("phase2-crash requires size MiB")?)?;
            return phase2_crash_child(
                mode,
                &path,
                size,
                args.get(5).ok_or("phase2-crash requires boundary")?,
            );
        }
        Some("phase2-recovery") => {
            let mode = parse_storage_mode(args.get(2).ok_or("phase2-recovery requires mode")?)?;
            let size = parse_phase2_size(args.get(3).ok_or("phase2-recovery requires size MiB")?)?;
            println!("{}", phase2_recovery_record(mode, size)?);
            return Ok(());
        }
        Some("phase2") => {
            let mode = parse_storage_mode(args.get(2).ok_or("phase2 requires mode")?)?;
            let workload = args.get(3).map(String::as_str).unwrap_or("create");
            let size = parse_phase2_size(args.get(4).map(String::as_str).unwrap_or("1"))?;
            let result = match workload {
                "create" => phase2_create_record(mode, size)?,
                "edit1" => phase2_edit_record(mode, size, 1)?,
                "edit3" | "edit3-m7-bounded" => phase2_edit_record(mode, size, 3)?,
                "read" => phase2_read_record(mode, size)?,
                "materialize" => phase2_materialize_record(mode, size)?,
                "materialize-100x1m" => phase2_many_materialize_record(mode)?,
                _ => return Err(format!("unknown phase2 workload: {workload}").into()),
            };
            println!("{result}");
            return Ok(());
        }
        _ => {}
    }
    let sizes = std::env::var("M8_SIZES").unwrap_or_else(|_| "1,10,20,100".to_string());
    for value in sizes.split(',') {
        let size = parse_phase2_size(value.trim())?;
        println!("{}", run_size(size)?);
    }
    Ok(())
}
