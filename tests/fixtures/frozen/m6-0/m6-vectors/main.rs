//! DOCUMENTATION-ONLY VERIFICATION TOOLING — NOT PRODUCT IMPLEMENTATION.
//! This independent reference generator imports no product code and does not
//! prescribe the product module or file structure.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const SCHEMA: u16 = 1;
const ROOT_MODE_SENTINEL: u16 = 0x1000;
const MAX_RECEIPT: usize = 1_048_576;
const MAX_FACTS: usize = 4_096;
const MAX_PATH: usize = 4_096;
const MAX_FACT: usize = 16_384;
const MAX_RANGES_PER_FACT: usize = 64;
const MAX_TOTAL_RANGES: usize = 16_384;
const MAX_DECODER_OWNED: usize = 2_097_152;
const MAX_ERROR_DISCLOSURE: usize = 4_096;
const MAX_FILE: u64 = 8_589_934_592;
const MAX_PACK_BYTES: usize = 67_108_864;
const PACK_HEADER_BYTES: usize = 64;
const PACK_INDEX_ENTRY_BYTES: usize = 80;
const PACK_TRAILER_BYTES: usize = 80;
const PACK_RECORDS_MAX: usize = 466_032;
const PACK_INDEX_BYTES_MAX: usize = 37_282_560;
const PACK_SORT_RUN_ENTRIES: usize = 46_604;
const PACK_SORT_INITIAL_RUNS_MAX: usize = 10;
const PACK_SORT_FILES_MAX: usize = 15;
const PACK_SORT_BYTE_WINDOW: usize = 3_728_384;
const PACK_SORT_PHYSICAL_SPILL_MAX: usize = 74_687_985;
const ALLOCATION_HEADER_BYTES: usize = 64;
const GROUP_PRIOR_PACK_DESCRIPTOR_MAX: usize = 1_024;
const GROUP_PRIOR_PACK_DESCRIPTOR_OWNED_BYTES: usize = 172_096;
const PACK_VALIDATOR_FIXED_CAPACITY_BYTES: usize = 4_096;
const PACK_VALIDATOR_FIXED_RECEIPT_BYTES: usize = 4_160;
const STANDALONE_VALIDATOR_RECEIPT_BYTES: usize = 3_994_944;
const RECOVERY_GC_BUCKET_MIN_BYTES: usize = 4_194_304;
const RECOVERY_GC_BUCKET_SLACK_BYTES: usize = 199_360;
const WRITER_VALIDATOR_DESCRIPTOR_RECEIPT_BYTES: usize = 4_691_584;
const FOREGROUND_BUCKET_MIN_BYTES: usize = 10_485_760;
const FOREGROUND_BUCKET_SLACK_BYTES: usize = 5_794_176;
const JOURNAL_HEADER_BYTES: usize = 76;
const JOURNAL_CHECKSUM_BYTES: usize = 32;
const JOURNAL_PAYLOAD_MAX: usize = 4_096;
const JOURNAL_FRAME_BYTES_MAX: usize = 4_204;
const QREC_BYTES: usize = 284;
const QREC_CHECKSUM_AT: usize = 252;
const CATALOG_DESCRIPTOR_BYTES: usize = 168;
const RECEIPT_PROCESSING_MAX: usize = MAX_RECEIPT + MAX_DECODER_OWNED + MAX_ERROR_DISCLOSURE;
const GENERATED_BEGIN: &str = "<!-- BEGIN GENERATED M6.0 VECTORS -->";
const GENERATED_END: &str = "<!-- END GENERATED M6.0 VECTORS -->";

const REGULAR: u8 = 0x01;
const DIRECTORY: u8 = 0x02;
const SYMLINK: u8 = 0x03;
const CREATE: u8 = 0x01;
const MODIFY: u8 = 0x02;
const DELETE: u8 = 0x03;
const RENAME: u8 = 0x04;
const METADATA: u8 = 0x05;
const TRUNCATE: u8 = 0x06;
const SPARSE_MAP: u8 = 0x07;
const REPLACE: u8 = 0x08;

#[derive(Clone)]
struct Fact {
    kind: u8,
    before_kind: u8,
    after_kind: u8,
    path: String,
    prior: Option<String>,
    before_generation: Option<u64>,
    after_generation: Option<u64>,
    before_size: Option<u64>,
    after_size: Option<u64>,
    metadata_mode: Option<u16>,
    ranges: Vec<(u64, u64)>,
}

#[derive(Clone)]
struct ReceiptFields {
    sandbox: [u8; 16],
    issuer: [u8; 16],
    epoch: u64,
    custody: [u8; 16],
    snapshot: [u8; 16],
    binding: [u8; 32],
    generation: u64,
    sequence_first: u64,
    sequence_final: u64,
    finality: u8,
    overflowed: u8,
    facts: Vec<Fact>,
    key_id: u32,
    key: [u8; 32],
}

#[derive(Clone)]
struct ValidationContext {
    sandbox: [u8; 16],
    issuer: [u8; 16],
    epoch: u64,
    custody: [u8; 16],
    snapshot: [u8; 16],
    binding: [u8; 32],
    key_id: u32,
    key: [u8; 32],
    last_generation: u64,
    last_sequence_final: u64,
    replay_tuple: Option<(u64, u64, u64, [u8; 32])>,
    replay_outcome_known: bool,
    accepted_binding: bool,
    binding_unrevoked: bool,
    binding_closure_authenticated: bool,
    binding_locator_current: bool,
    issuer_key_enabled: bool,
    epoch_registered_and_nonreused: bool,
    custody_current_and_immutable: bool,
    immediate_revalidation_succeeds: bool,
    authoritative_resync_witness: bool,
    decoder_owned_request: usize,
    error_disclosure_request: usize,
    reservation: ReservationState,
    authoritative_metadata_by_path: BTreeMap<String, SourceMetadataFact>,
}

#[derive(Clone, Copy)]
struct SourceMetadataFact {
    kind: u8,
    mode: Option<u16>,
}

#[derive(Clone, Copy)]
enum ReservationState {
    Available { bytes: usize },
    Unavailable,
    DeadlineExpired,
    Cancelled,
}

struct IdentityVector {
    name: &'static str,
    preimage: Vec<u8>,
    digest: [u8; 32],
}

struct HostileVector {
    name: &'static str,
    base: &'static str,
    mutation: &'static str,
    bytes: Vec<u8>,
    expected: &'static str,
    context: ValidationContext,
}

struct DispositionVector {
    name: &'static str,
    base: &'static str,
    mutation: &'static str,
    bytes: Vec<u8>,
    expected: &'static str,
    is_ok: bool,
    context: ValidationContext,
}

struct PackVector {
    name: &'static str,
    bytes: Vec<u8>,
    physical_keys: Vec<(u8, [u8; 32])>,
    index_keys: Vec<(u8, [u8; 32])>,
}

struct PackHostile {
    name: &'static str,
    base: &'static str,
    mutation: &'static str,
    bytes: Vec<u8>,
    expected: &'static str,
}

struct ModelVector {
    name: &'static str,
    input: String,
    expected: &'static str,
    actual: &'static str,
}

struct BinaryVector {
    name: &'static str,
    base: &'static str,
    mutation: &'static str,
    bytes: Vec<u8>,
    expected: &'static str,
    actual: Result<&'static str, &'static str>,
    render_exact: bool,
}

#[derive(Clone)]
struct PackMetadata {
    kind: u8,
    id: [u8; 32],
    absolute_offset: u64,
    object_len: u32,
    object_checksum: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
enum StructuralType {
    LogicalChunk,
    LogicalFile,
    FileNode,
    SymlinkNode,
    DirectoryNode,
    VersionRoot,
}

#[derive(Default)]
struct TypedRegistry {
    logical_chunks: BTreeMap<[u8; 32], u64>,
    logical_files: BTreeMap<[u8; 32], u64>,
    file_nodes: BTreeSet<[u8; 32]>,
    symlink_nodes: BTreeSet<[u8; 32]>,
    explicit_directory_nodes: BTreeSet<[u8; 32]>,
    root_directory_nodes: BTreeSet<[u8; 32]>,
}

struct StructuralHostile {
    name: &'static str,
    bytes: Vec<u8>,
    expected: &'static str,
    object_type: StructuralType,
    implicit_root: bool,
}

struct OccupiedVector {
    name: &'static str,
    claimed_id: [u8; 32],
    expected_bytes: Vec<u8>,
    stored_bytes: Vec<u8>,
    oracle: OccupiedIdOracle,
    object_type: StructuralType,
    implicit_root: bool,
    expected: Result<(), &'static str>,
    expected_label: &'static str,
}

#[derive(Clone, Copy)]
enum OccupiedIdOracle {
    Blake3,
    ForcedSameId,
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_be_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_be_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_be_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, &'static str> {
    let end = at.checked_add(2).ok_or("E_INTEGER_OVERFLOW")?;
    let raw = bytes.get(at..end).ok_or("E_TRUNCATED")?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, &'static str> {
    let end = at.checked_add(4).ok_or("E_INTEGER_OVERFLOW")?;
    let raw = bytes.get(at..end).ok_or("E_TRUNCATED")?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, &'static str> {
    let end = at.checked_add(8).ok_or("E_INTEGER_OVERFLOW")?;
    let raw = bytes.get(at..end).ok_or("E_TRUNCATED")?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

fn read_be_u16(bytes: &[u8], at: usize) -> Result<u16, &'static str> {
    let raw = take(bytes, at, 2)?;
    Ok(u16::from_be_bytes([raw[0], raw[1]]))
}

fn read_be_u32(bytes: &[u8], at: usize) -> Result<u32, &'static str> {
    let raw = take(bytes, at, 4)?;
    Ok(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_be_u64(bytes: &[u8], at: usize) -> Result<u64, &'static str> {
    let raw = take(bytes, at, 8)?;
    Ok(u64::from_be_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

fn take<'a>(bytes: &'a [u8], at: usize, len: usize) -> Result<&'a [u8], &'static str> {
    let end = at.checked_add(len).ok_or("E_INTEGER_OVERFLOW")?;
    bytes.get(at..end).ok_or("E_TRUNCATED")
}

fn advance(cursor: &mut usize, amount: usize) -> Result<(), &'static str> {
    *cursor = cursor.checked_add(amount).ok_or("E_INTEGER_OVERFLOW")?;
    Ok(())
}

fn digest(preimage: &[u8]) -> [u8; 32] {
    *blake3::hash(preimage).as_bytes()
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for index in 0..32 {
        difference |= left[index] ^ right[index];
    }
    difference == 0
}

fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").expect("String writes are infallible");
    }
    result
}

fn seq16(start: u8) -> [u8; 16] {
    let mut result = [0u8; 16];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = start.wrapping_add(index as u8);
    }
    result
}

fn seq32(start: u8) -> [u8; 32] {
    let mut result = [0u8; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = start.wrapping_add(index as u8);
    }
    result
}

fn domain(name: &[u8]) -> Vec<u8> {
    let mut result = name.to_vec();
    result.push(0);
    push_u16(&mut result, SCHEMA);
    result
}

fn logical_chunk(payload: &[u8]) -> IdentityVector {
    let mut preimage = domain(b"ESV2-LCHUNK");
    push_u64(
        &mut preimage,
        u64::try_from(payload.len()).expect("logical chunk length fits u64"),
    );
    preimage.extend_from_slice(payload);
    IdentityVector {
        name: "logical_chunk_abc",
        digest: digest(&preimage),
        preimage,
    }
}

fn logical_file(chunks: &[([u8; 32], u64)], logical_len: u64) -> Vec<u8> {
    let mut preimage = domain(b"ESV2-LFILE");
    push_u64(&mut preimage, logical_len);
    push_u32(
        &mut preimage,
        u32::try_from(chunks.len()).expect("logical chunk count fits u32"),
    );
    for (chunk_id, chunk_len) in chunks {
        preimage.extend_from_slice(chunk_id);
        push_u64(&mut preimage, *chunk_len);
    }
    preimage
}

fn file_node(mode: u16, file_id: &[u8; 32], logical_len: u64) -> Vec<u8> {
    let mut preimage = domain(b"ESV2-FNODE");
    push_u16(&mut preimage, mode);
    preimage.extend_from_slice(file_id);
    push_u64(&mut preimage, logical_len);
    preimage
}

fn symlink_node(target: &[u8]) -> Vec<u8> {
    let mut preimage = domain(b"ESV2-SNODE");
    push_u32(
        &mut preimage,
        u32::try_from(target.len()).expect("symlink target length fits u32"),
    );
    preimage.extend_from_slice(target);
    preimage
}

fn directory_node(mode: u16, children: &[(&[u8], u8, [u8; 32])]) -> Vec<u8> {
    let mut preimage = domain(b"ESV2-DNODE");
    push_u16(&mut preimage, mode);
    push_u32(
        &mut preimage,
        u32::try_from(children.len()).expect("directory child count fits u32"),
    );
    for (name, kind, child) in children {
        push_u32(
            &mut preimage,
            u32::try_from(name.len()).expect("directory name length fits u32"),
        );
        preimage.extend_from_slice(name);
        preimage.push(*kind);
        preimage.extend_from_slice(child);
    }
    preimage
}

fn version_root(root_id: &[u8; 32]) -> Vec<u8> {
    let mut preimage = domain(b"ESV2-VROOT");
    preimage.extend_from_slice(root_id);
    preimage
}

fn identity_vectors() -> Vec<IdentityVector> {
    let chunk = logical_chunk(b"abc");
    let logical_file_preimage = logical_file(&[(chunk.digest, 3)], 3);
    let logical_file_id = digest(&logical_file_preimage);
    let file_preimage = file_node(0o644, &logical_file_id, 3);
    let file_id = digest(&file_preimage);
    let symlink_preimage = symlink_node(b"file.txt");
    let symlink_id = digest(&symlink_preimage);
    let nested_preimage = directory_node(0o755, &[(b"data".as_slice(), REGULAR, file_id)]);
    let nested_id = digest(&nested_preimage);
    let empty_root_preimage = directory_node(ROOT_MODE_SENTINEL, &[]);
    let empty_root_id = digest(&empty_root_preimage);
    let empty_version_preimage = version_root(&empty_root_id);
    let composite_root_preimage = directory_node(
        ROOT_MODE_SENTINEL,
        &[
            (b"file.txt".as_slice(), REGULAR, file_id),
            (b"link".as_slice(), SYMLINK, symlink_id),
            (b"nested".as_slice(), DIRECTORY, nested_id),
        ],
    );
    let composite_root_id = digest(&composite_root_preimage);
    let composite_version_preimage = version_root(&composite_root_id);

    vec![
        chunk,
        IdentityVector {
            name: "logical_file_abc",
            digest: logical_file_id,
            preimage: logical_file_preimage,
        },
        IdentityVector {
            name: "file_node_0644_abc",
            digest: file_id,
            preimage: file_preimage,
        },
        IdentityVector {
            name: "symlink_node_file_txt",
            digest: symlink_id,
            preimage: symlink_preimage,
        },
        IdentityVector {
            name: "directory_explicit_0755_nested_file",
            digest: nested_id,
            preimage: nested_preimage,
        },
        IdentityVector {
            name: "directory_implicit_empty_root_1000",
            digest: empty_root_id,
            preimage: empty_root_preimage,
        },
        IdentityVector {
            name: "version_empty_root",
            digest: digest(&empty_version_preimage),
            preimage: empty_version_preimage,
        },
        IdentityVector {
            name: "directory_implicit_composite_root_1000",
            digest: composite_root_id,
            preimage: composite_root_preimage,
        },
        IdentityVector {
            name: "version_composite",
            digest: digest(&composite_version_preimage),
            preimage: composite_version_preimage,
        },
    ]
}

fn metadata_digest(kind: u8, mode: Option<u16>) -> [u8; 32] {
    let mut preimage = domain(b"ESV2-CHGMETA");
    preimage.push(kind);
    match mode {
        Some(value) => {
            preimage.push(1);
            push_u16(&mut preimage, value);
        }
        None => preimage.push(0),
    }
    digest(&preimage)
}

fn selected_metadata_fact(fact: &Fact) -> SourceMetadataFact {
    SourceMetadataFact {
        kind: if fact.kind == DELETE {
            fact.before_kind
        } else {
            fact.after_kind
        },
        mode: fact.metadata_mode,
    }
}

fn source_metadata_digest(fact: SourceMetadataFact) -> Result<[u8; 32], &'static str> {
    match (fact.kind, fact.mode) {
        (REGULAR, Some(mode)) | (DIRECTORY, Some(mode)) if mode <= 0x0fff => {
            Ok(metadata_digest(fact.kind, Some(mode)))
        }
        (SYMLINK, None) => Ok(metadata_digest(SYMLINK, None)),
        _ => Err("E_METADATA"),
    }
}

fn fact_presence(fact: &Fact) -> u8 {
    u8::from(fact.prior.is_some())
        | (u8::from(fact.before_generation.is_some()) << 1)
        | (u8::from(fact.after_generation.is_some()) << 2)
        | (u8::from(fact.before_size.is_some()) << 3)
        | (u8::from(fact.after_size.is_some()) << 4)
        | 0x20
}

fn encode_fact(fact: &Fact) -> Vec<u8> {
    let mut out = Vec::new();
    push_u32(&mut out, 0);
    out.push(fact.kind);
    out.push(fact.before_kind);
    out.push(fact.after_kind);
    out.push(fact_presence(fact));
    push_u32(
        &mut out,
        u32::try_from(fact.path.len()).expect("fact path length fits u32"),
    );
    out.extend_from_slice(fact.path.as_bytes());
    if let Some(prior) = &fact.prior {
        push_u32(
            &mut out,
            u32::try_from(prior.len()).expect("prior path length fits u32"),
        );
        out.extend_from_slice(prior.as_bytes());
    }
    if let Some(value) = fact.before_generation {
        push_u64(&mut out, value);
    }
    if let Some(value) = fact.after_generation {
        push_u64(&mut out, value);
    }
    if let Some(value) = fact.before_size {
        push_u64(&mut out, value);
    }
    if let Some(value) = fact.after_size {
        push_u64(&mut out, value);
    }
    out.extend_from_slice(
        &source_metadata_digest(selected_metadata_fact(fact))
            .expect("generator facts have canonical source metadata"),
    );
    push_u16(
        &mut out,
        u16::try_from(fact.ranges.len()).expect("range count fits u16"),
    );
    for (start, length) in &fact.ranges {
        push_u64(&mut out, *start);
        push_u64(&mut out, *length);
    }
    let encoded_len = u32::try_from(out.len()).expect("fact length fits u32");
    out[0..4].copy_from_slice(&encoded_len.to_le_bytes());
    out
}

fn encode_receipt(fields: &ReceiptFields) -> Vec<u8> {
    let fact_bytes: Vec<Vec<u8>> = fields.facts.iter().map(encode_fact).collect();
    let facts_len: usize = fact_bytes.iter().map(Vec::len).sum();
    let total_len = 225usize.checked_add(facts_len).expect("receipt length");
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(b"ESV2-CHGREC\0");
    push_u16(&mut out, SCHEMA);
    push_u32(
        &mut out,
        u32::try_from(total_len).expect("receipt length fits u32"),
    );
    out.extend_from_slice(&fields.sandbox);
    out.extend_from_slice(&fields.issuer);
    push_u64(&mut out, fields.epoch);
    out.extend_from_slice(&fields.custody);
    out.extend_from_slice(&fields.snapshot);
    out.extend_from_slice(&fields.binding);
    push_u64(&mut out, fields.generation);
    push_u64(&mut out, fields.sequence_first);
    push_u64(&mut out, fields.sequence_final);
    out.push(fields.finality);
    out.push(fields.overflowed);
    push_u32(
        &mut out,
        u32::try_from(fields.facts.len()).expect("fact count fits u32"),
    );
    push_u32(
        &mut out,
        u32::try_from(facts_len).expect("facts length fits u32"),
    );
    for fact in fact_bytes {
        out.extend_from_slice(&fact);
    }
    let mut coverage_preimage = domain(b"ESV2-CHGCOV");
    coverage_preimage.extend_from_slice(&out[14..]);
    out.extend_from_slice(&digest(&coverage_preimage));
    out.push(1);
    push_u32(&mut out, fields.key_id);
    let mut auth_preimage = domain(b"ESV2-CHGMAC");
    auth_preimage.extend_from_slice(&out);
    out.extend_from_slice(blake3::keyed_hash(&fields.key, &auth_preimage).as_bytes());
    assert_eq!(out.len(), total_len);
    out
}

fn receipt_context(fields: &ReceiptFields) -> ValidationContext {
    let authoritative_metadata_by_path = fields
        .facts
        .iter()
        .map(|fact| (fact.path.clone(), selected_metadata_fact(fact)))
        .collect();
    ValidationContext {
        sandbox: fields.sandbox,
        issuer: fields.issuer,
        epoch: fields.epoch,
        custody: fields.custody,
        snapshot: fields.snapshot,
        binding: fields.binding,
        key_id: fields.key_id,
        key: fields.key,
        last_generation: 0,
        last_sequence_final: 0,
        replay_tuple: None,
        replay_outcome_known: true,
        accepted_binding: true,
        binding_unrevoked: true,
        binding_closure_authenticated: true,
        binding_locator_current: true,
        issuer_key_enabled: true,
        epoch_registered_and_nonreused: true,
        custody_current_and_immutable: true,
        immediate_revalidation_succeeds: true,
        authoritative_resync_witness: true,
        decoder_owned_request: MAX_DECODER_OWNED,
        error_disclosure_request: MAX_ERROR_DISCLOSURE,
        reservation: ReservationState::Available {
            bytes: RECEIPT_PROCESSING_MAX,
        },
        authoritative_metadata_by_path,
    }
}

fn base_receipt() -> ReceiptFields {
    ReceiptFields {
        sandbox: seq16(0x00),
        issuer: seq16(0x10),
        epoch: 1,
        custody: seq16(0x20),
        snapshot: seq16(0x30),
        binding: seq32(0x40),
        generation: 1,
        sequence_first: 1,
        sequence_final: 1,
        finality: 1,
        overflowed: 0,
        facts: Vec::new(),
        key_id: 7,
        key: seq32(0xa0),
    }
}

fn nontrivial_receipt() -> ReceiptFields {
    let mut fields = base_receipt();
    fields.generation = 9;
    fields.sequence_first = 100;
    fields.sequence_final = 102;
    fields.facts = vec![
        Fact {
            kind: RENAME,
            before_kind: SYMLINK,
            after_kind: SYMLINK,
            path: "docs/current".into(),
            prior: Some("docs/latest".into()),
            before_generation: Some(8),
            after_generation: Some(9),
            before_size: None,
            after_size: None,
            metadata_mode: None,
            ranges: Vec::new(),
        },
        Fact {
            kind: MODIFY,
            before_kind: REGULAR,
            after_kind: REGULAR,
            path: "src/lib.rs".into(),
            prior: None,
            before_generation: Some(41),
            after_generation: Some(42),
            before_size: Some(3),
            after_size: Some(5),
            metadata_mode: Some(0o644),
            ranges: vec![(1, 2)],
        },
    ];
    fields
}

fn simple_symlink_rename(path: &str, prior: &str) -> Fact {
    Fact {
        kind: RENAME,
        before_kind: SYMLINK,
        after_kind: SYMLINK,
        path: path.into(),
        prior: Some(prior.into()),
        before_generation: Some(1),
        after_generation: Some(2),
        before_size: None,
        after_size: None,
        metadata_mode: None,
        ranges: Vec::new(),
    }
}

fn validate_path(bytes: &[u8]) -> Result<&str, &'static str> {
    if bytes.len() > MAX_PATH {
        return Err("E_PATH_CAP");
    }
    if bytes.is_empty() || bytes.contains(&0) {
        return Err("E_PATH");
    }
    let path = std::str::from_utf8(bytes).map_err(|_| "E_PATH")?;
    if path.starts_with('/') || path.ends_with('/') || path.contains("//") {
        return Err("E_PATH");
    }
    let mut components = 0usize;
    for component in path.split('/') {
        components += 1;
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.as_bytes().len() > 255
        {
            return Err("E_PATH");
        }
    }
    if components > 256 {
        return Err("E_PATH");
    }
    Ok(path)
}

fn compare_paths(left: &str, right: &str) -> Ordering {
    let mut left_components = left.as_bytes().split(|byte| *byte == b'/');
    let mut right_components = right.as_bytes().split(|byte| *byte == b'/');
    loop {
        match (left_components.next(), right_components.next()) {
            (Some(left_component), Some(right_component)) => {
                let order = left_component.cmp(right_component);
                if order != Ordering::Equal {
                    return order;
                }
            }
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn array16(bytes: &[u8], at: usize) -> Result<[u8; 16], &'static str> {
    take(bytes, at, 16)?.try_into().map_err(|_| "E_TRUNCATED")
}

fn array32(bytes: &[u8], at: usize) -> Result<[u8; 32], &'static str> {
    take(bytes, at, 32)?.try_into().map_err(|_| "E_TRUNCATED")
}

fn precharge_receipt(context: &ValidationContext) -> Result<(), &'static str> {
    if context.decoder_owned_request > MAX_DECODER_OWNED {
        return Err("E_DECODER_OWNED_CAP");
    }
    if context.error_disclosure_request > MAX_ERROR_DISCLOSURE {
        return Err("E_ERROR_DISCLOSURE_CAP");
    }
    match context.reservation {
        ReservationState::Available { bytes } if bytes >= RECEIPT_PROCESSING_MAX => Ok(()),
        ReservationState::Available { .. } | ReservationState::Unavailable => {
            Err("RECEIPT_RESOURCE_REFUSED_UNAVAILABLE")
        }
        ReservationState::DeadlineExpired => Err("RECEIPT_RESOURCE_REFUSED_DEADLINE"),
        ReservationState::Cancelled => Err("RECEIPT_RESOURCE_REFUSED_CANCELLED"),
    }
}

fn validate_receipt(
    bytes: &[u8],
    context: &ValidationContext,
) -> Result<&'static str, &'static str> {
    if bytes.len() > MAX_RECEIPT {
        return Err("E_TOTAL_CAP");
    }
    // This reservation happens before any decoder-owned map/set allocation or
    // authentication work. The receipt is a caller-borrowed slice here.
    precharge_receipt(context)?;
    if bytes.len() < 225 {
        return Err("E_TRUNCATED");
    }
    if bytes.get(0..12) != Some(b"ESV2-CHGREC\0") {
        return Err("E_DOMAIN");
    }
    if read_u16(bytes, 12)? != SCHEMA {
        return Err("E_SCHEMA");
    }
    let declared_total = read_u32(bytes, 14)? as usize;
    if declared_total > MAX_RECEIPT {
        return Err("E_TOTAL_CAP");
    }
    if declared_total != bytes.len() {
        return Err("E_TOTAL_LENGTH");
    }
    let facts_count = read_u32(bytes, 148)? as usize;
    if facts_count > MAX_FACTS {
        return Err("E_FACT_COUNT_CAP");
    }
    let facts_len = read_u32(bytes, 152)? as usize;
    if 225usize
        .checked_add(facts_len)
        .ok_or("E_INTEGER_OVERFLOW")?
        != bytes.len()
    {
        return Err("E_FACTS_LENGTH");
    }
    let facts_end = 156usize
        .checked_add(facts_len)
        .ok_or("E_INTEGER_OVERFLOW")?;
    // Stream both independent hash states in the same forward pass as semantic
    // validation. The fixed header is consumed once here; each fact is added to
    // both states immediately after that fact has been parsed and validated.
    let mut coverage_hasher = blake3::Hasher::new();
    coverage_hasher.update(&domain(b"ESV2-CHGCOV"));
    coverage_hasher.update(&bytes[14..156]);
    let mut auth_hasher = blake3::Hasher::new_keyed(&context.key);
    auth_hasher.update(&domain(b"ESV2-CHGMAC"));
    auth_hasher.update(&bytes[..156]);
    let mut cursor = 156usize;
    let mut previous_path: Option<&str> = None;
    let mut primary_paths: BTreeSet<&str> = BTreeSet::new();
    let mut rename_sources: BTreeSet<&str> = BTreeSet::new();
    let mut rename_destinations: BTreeSet<&str> = BTreeSet::new();
    let mut rename_edges: BTreeMap<&str, &str> = BTreeMap::new();
    let mut nonrename_paths: BTreeSet<&str> = BTreeSet::new();
    let mut total_ranges = 0usize;
    let mut has_changed_regular_file = false;

    for _ in 0..facts_count {
        if cursor >= facts_end {
            return Err("E_FACT_COUNT");
        }
        let fact_start = cursor;
        let fact_len = read_u32(bytes, cursor)? as usize;
        if fact_len < 47 || fact_len > MAX_FACT {
            return Err("E_FACT_SIZE_CAP");
        }
        let fact_end = fact_start
            .checked_add(fact_len)
            .ok_or("E_INTEGER_OVERFLOW")?;
        if fact_end > facts_end {
            return Err("E_FACT_LENGTH");
        }
        let kind = *take(bytes, cursor, 8)?.get(4).ok_or("E_TRUNCATED")?;
        let before_kind = *take(bytes, cursor, 8)?.get(5).ok_or("E_TRUNCATED")?;
        let after_kind = *take(bytes, cursor, 8)?.get(6).ok_or("E_TRUNCATED")?;
        let presence = *take(bytes, cursor, 8)?.get(7).ok_or("E_TRUNCATED")?;
        if presence & 0xc0 != 0 {
            return Err("E_PRESENCE");
        }
        if !matches!(before_kind, 0 | REGULAR | DIRECTORY | SYMLINK)
            || !matches!(after_kind, 0 | REGULAR | DIRECTORY | SYMLINK)
        {
            return Err("E_ENTRY_KIND");
        }
        let kind_ok = match kind {
            CREATE => before_kind == 0 && after_kind != 0,
            MODIFY | TRUNCATE | SPARSE_MAP => before_kind == REGULAR && after_kind == REGULAR,
            DELETE => before_kind != 0 && after_kind == 0,
            RENAME => before_kind != 0 && before_kind == after_kind,
            METADATA => matches!(before_kind, REGULAR | DIRECTORY) && before_kind == after_kind,
            REPLACE => {
                before_kind != 0
                    && after_kind != 0
                    && (before_kind != after_kind || before_kind == SYMLINK)
            }
            _ => return Err("E_FACT_KIND"),
        };
        if !kind_ok {
            return Err("E_FACT_KIND_COMBINATION");
        }
        let has_prior = presence & 0x01 != 0;
        let has_before_generation = presence & 0x02 != 0;
        let has_after_generation = presence & 0x04 != 0;
        let has_before_size = presence & 0x08 != 0;
        let has_after_size = presence & 0x10 != 0;
        let has_metadata = presence & 0x20 != 0;
        if has_prior != (kind == RENAME)
            || has_before_generation != (before_kind != 0)
            || has_after_generation != (after_kind != 0)
            || has_before_size != (before_kind == REGULAR)
            || has_after_size != (after_kind == REGULAR)
            || !has_metadata
        {
            return Err("E_PRESENCE");
        }
        advance(&mut cursor, 8)?;
        let path_len = read_u32(bytes, cursor)? as usize;
        advance(&mut cursor, 4)?;
        let path_bytes = take(bytes, cursor, path_len)?;
        let path = validate_path(path_bytes)?;
        advance(&mut cursor, path_len)?;
        if let Some(previous) = previous_path {
            if compare_paths(previous, path) != Ordering::Less {
                return Err(if previous == path {
                    "E_DUPLICATE"
                } else {
                    "E_FACT_ORDER"
                });
            }
        }
        previous_path = Some(path);
        if !primary_paths.insert(path) {
            return Err("E_DUPLICATE");
        }
        let prior = if has_prior {
            let prior_len = read_u32(bytes, cursor)? as usize;
            advance(&mut cursor, 4)?;
            let prior_bytes = take(bytes, cursor, prior_len)?;
            let value = validate_path(prior_bytes)?;
            advance(&mut cursor, prior_len)?;
            if value == path || !rename_sources.insert(value) {
                return Err("E_RENAME_DUPLICATE");
            }
            rename_destinations.insert(path);
            rename_edges.insert(value, path);
            Some(value)
        } else {
            nonrename_paths.insert(path);
            None
        };
        let before_generation = if has_before_generation {
            let value = read_u64(bytes, cursor)?;
            advance(&mut cursor, 8)?;
            if value == 0 {
                return Err("E_GENERATION_ZERO");
            }
            Some(value)
        } else {
            None
        };
        let after_generation = if has_after_generation {
            let value = read_u64(bytes, cursor)?;
            advance(&mut cursor, 8)?;
            if value == 0 {
                return Err("E_GENERATION_ZERO");
            }
            Some(value)
        } else {
            None
        };
        if let (Some(before), Some(after)) = (before_generation, after_generation) {
            if after <= before {
                return Err("E_ENTRY_GENERATION");
            }
        }
        let _before_size = if has_before_size {
            let value = read_u64(bytes, cursor)?;
            advance(&mut cursor, 8)?;
            if value > MAX_FILE {
                return Err("E_SIZE_CAP");
            }
            Some(value)
        } else {
            None
        };
        let after_size = if has_after_size {
            let value = read_u64(bytes, cursor)?;
            advance(&mut cursor, 8)?;
            if value > MAX_FILE {
                return Err("E_SIZE_CAP");
            }
            Some(value)
        } else {
            None
        };
        let received_metadata = array32(bytes, cursor)?;
        let source_metadata = context
            .authoritative_metadata_by_path
            .get(path)
            .copied()
            .ok_or("E_METADATA")?;
        let selected_kind = if kind == DELETE {
            before_kind
        } else {
            after_kind
        };
        if source_metadata.kind != selected_kind
            || received_metadata != source_metadata_digest(source_metadata)?
        {
            return Err("E_METADATA");
        }
        advance(&mut cursor, 32)?;
        let range_count = read_u16(bytes, cursor)? as usize;
        advance(&mut cursor, 2)?;
        if range_count > MAX_RANGES_PER_FACT {
            return Err("E_RANGE_COUNT_CAP");
        }
        total_ranges = total_ranges
            .checked_add(range_count)
            .ok_or("E_INTEGER_OVERFLOW")?;
        if total_ranges > MAX_TOTAL_RANGES {
            return Err("E_TOTAL_RANGE_CAP");
        }
        if range_count > 0
            && !(after_kind == REGULAR
                && matches!(
                    kind,
                    CREATE | MODIFY | RENAME | TRUNCATE | SPARSE_MAP | REPLACE
                ))
        {
            return Err("E_RANGE_COMBINATION");
        }
        let mut previous_end: Option<u64> = None;
        for _ in 0..range_count {
            let start = read_u64(bytes, cursor)?;
            let length_at = cursor.checked_add(8).ok_or("E_INTEGER_OVERFLOW")?;
            let length = read_u64(bytes, length_at)?;
            advance(&mut cursor, 16)?;
            if length == 0 {
                return Err("E_RANGE_ZERO");
            }
            let end = start.checked_add(length).ok_or("E_RANGE_OVERFLOW")?;
            if end > after_size.ok_or("E_RANGE_COMBINATION")? {
                return Err("E_RANGE_SIZE");
            }
            if let Some(prior_end) = previous_end {
                if prior_end >= start {
                    return Err(if prior_end == start {
                        "E_RANGE_ADJACENT"
                    } else {
                        "E_RANGE_OVERLAP"
                    });
                }
            }
            previous_end = Some(end);
        }
        if cursor != fact_end {
            return Err("E_FACT_EOF");
        }
        if prior.is_none() && kind == RENAME {
            return Err("E_PRESENCE");
        }
        if after_kind == REGULAR
            && matches!(
                kind,
                CREATE | MODIFY | RENAME | TRUNCATE | SPARSE_MAP | REPLACE
            )
        {
            has_changed_regular_file = true;
        }
        coverage_hasher.update(&bytes[fact_start..fact_end]);
        auth_hasher.update(&bytes[fact_start..fact_end]);
        cursor = fact_end;
    }
    if cursor != facts_end {
        return Err("E_FACT_COUNT");
    }
    for source in &rename_sources {
        if nonrename_paths.contains(source) {
            return Err("E_RENAME_AMBIGUOUS");
        }
        if rename_destinations.contains(source) {
            let start = *source;
            let mut current = start;
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            loop {
                if !seen.insert(current) {
                    if current != start {
                        return Err("E_RENAME_CHAIN");
                    }
                    break;
                }
                current = rename_edges.get(current).copied().ok_or("E_RENAME_CHAIN")?;
            }
        }
    }

    let coverage_at = facts_end;
    let received_coverage = array32(bytes, coverage_at)?;
    let computed_coverage = *coverage_hasher.finalize().as_bytes();
    if !constant_time_eq(&received_coverage, &computed_coverage) {
        return Err("E_COVERAGE");
    }
    let scheme_at = coverage_at + 32;
    if bytes[scheme_at] != 1 {
        return Err("E_AUTH_SCHEME");
    }
    let key_id = read_u32(bytes, scheme_at + 1)?;
    if key_id == 0 || key_id != context.key_id || !context.issuer_key_enabled {
        return Err("E_AUTH_KEY");
    }
    let auth_at = scheme_at + 5;
    if auth_at + 32 != bytes.len() {
        return Err("E_EXACT_EOF");
    }
    let received_auth = array32(bytes, auth_at)?;
    auth_hasher.update(&bytes[facts_end..auth_at]);
    let computed_auth = *auth_hasher.finalize().as_bytes();
    if !constant_time_eq(&received_auth, &computed_auth) {
        return Err("E_AUTH");
    }
    if array16(bytes, 18)? != context.sandbox || array16(bytes, 34)? != context.issuer {
        return Err("E_ISSUER_AUTHORITY");
    }
    let receipt_epoch = read_u64(bytes, 50)?;
    if receipt_epoch == 0
        || receipt_epoch != context.epoch
        || !context.epoch_registered_and_nonreused
    {
        return Err("E_EPOCH");
    }
    if array16(bytes, 58)? != context.custody
        || array16(bytes, 74)? != context.snapshot
        || !context.custody_current_and_immutable
    {
        return Err("E_CUSTODY");
    }
    let binding = array32(bytes, 90)?;
    if binding == [0u8; 32]
        || binding != context.binding
        || !context.accepted_binding
        || !context.binding_unrevoked
        || !context.binding_closure_authenticated
        || !context.binding_locator_current
    {
        return Err("E_BINDING_AUTHORITY");
    }
    let generation = read_u64(bytes, 122)?;
    let sequence_first = read_u64(bytes, 130)?;
    let sequence_final = read_u64(bytes, 138)?;
    if generation == 0 || sequence_first == 0 || sequence_final < sequence_first {
        return Err("E_SEQUENCE");
    }
    let receipt_digest = digest(bytes);
    if let Some((prior_generation, prior_first, prior_final, prior_digest)) = context.replay_tuple {
        if generation == prior_generation
            && sequence_first == prior_first
            && sequence_final == prior_final
        {
            return if constant_time_eq(&receipt_digest, &prior_digest) {
                if context.replay_outcome_known {
                    Ok("IDEMPOTENT_REPLAY")
                } else {
                    Err("E_REPLAY_RECOVERY_REQUIRED")
                }
            } else {
                Err("E_REPLAY_DIVERGENCE")
            };
        }
    }
    if generation <= context.last_generation {
        return Err("E_SOURCE_GENERATION");
    }
    let expected_first = context
        .last_sequence_final
        .checked_add(1)
        .ok_or("E_SEQUENCE_OVERFLOW")?;
    if sequence_first != expected_first {
        return Err("FULL_ENUM_SEQUENCE_GAP");
    }
    match bytes[146] {
        0 => return Err("FULL_ENUM_NON_FINAL"),
        1 => {}
        _ => return Err("E_FINALITY_TAG"),
    }
    match bytes[147] {
        0 => {}
        1 => return Err("FULL_ENUM_PRODUCER_OVERFLOW"),
        _ => return Err("E_OVERFLOW_TAG"),
    }
    if !context.immediate_revalidation_succeeds {
        return Err("E_IMMEDIATE_REVALIDATION");
    }
    if has_changed_regular_file && !context.authoritative_resync_witness {
        return Ok("FULL_CHANGED_FILE_REQUIRED");
    }
    Ok("ACCEPTED")
}

fn reseal(bytes: &mut [u8], key: &[u8; 32]) {
    let facts_len = read_u32(bytes, 152).expect("facts length") as usize;
    let coverage_at = 156 + facts_len;
    let mut coverage_preimage = domain(b"ESV2-CHGCOV");
    coverage_preimage.extend_from_slice(&bytes[14..coverage_at]);
    bytes[coverage_at..coverage_at + 32].copy_from_slice(&digest(&coverage_preimage));
    let auth_at = coverage_at + 37;
    let mut auth_preimage = domain(b"ESV2-CHGMAC");
    auth_preimage.extend_from_slice(&bytes[..auth_at]);
    bytes[auth_at..auth_at + 32]
        .copy_from_slice(blake3::keyed_hash(key, &auth_preimage).as_bytes());
}

fn reauthenticate_without_coverage(bytes: &mut [u8], key: &[u8; 32]) {
    let facts_len = read_u32(bytes, 152).expect("facts length") as usize;
    let auth_at = 156 + facts_len + 37;
    let mut auth_preimage = domain(b"ESV2-CHGMAC");
    auth_preimage.extend_from_slice(&bytes[..auth_at]);
    bytes[auth_at..auth_at + 32]
        .copy_from_slice(blake3::keyed_hash(key, &auth_preimage).as_bytes());
}

fn structural_domain(object_type: StructuralType) -> &'static [u8] {
    match object_type {
        StructuralType::LogicalChunk => b"ESV2-LCHUNK\0",
        StructuralType::LogicalFile => b"ESV2-LFILE\0",
        StructuralType::FileNode => b"ESV2-FNODE\0",
        StructuralType::SymlinkNode => b"ESV2-SNODE\0",
        StructuralType::DirectoryNode => b"ESV2-DNODE\0",
        StructuralType::VersionRoot => b"ESV2-VROOT\0",
    }
}

fn structural_prefix(bytes: &[u8], object_type: StructuralType) -> Result<usize, &'static str> {
    let expected = structural_domain(object_type);
    if take(bytes, 0, expected.len()).map_err(|_| "S_TRUNCATED")? != expected {
        return Err("S_TYPE_DOMAIN");
    }
    if read_u16(bytes, expected.len()).map_err(|_| "S_TRUNCATED")? != SCHEMA {
        return Err("S_SCHEMA");
    }
    expected.len().checked_add(2).ok_or("S_INTEGER_OVERFLOW")
}

fn structural_take<'a>(bytes: &'a [u8], at: usize, len: usize) -> Result<&'a [u8], &'static str> {
    let end = at.checked_add(len).ok_or("S_INTEGER_OVERFLOW")?;
    bytes.get(at..end).ok_or("S_TRUNCATED")
}

fn structural_read_u16(bytes: &[u8], at: usize) -> Result<u16, &'static str> {
    let raw = structural_take(bytes, at, 2)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn structural_read_u32(bytes: &[u8], at: usize) -> Result<u32, &'static str> {
    let raw = structural_take(bytes, at, 4)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn structural_read_u64(bytes: &[u8], at: usize) -> Result<u64, &'static str> {
    let raw = structural_take(bytes, at, 8)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

fn structural_advance(cursor: &mut usize, amount: usize) -> Result<(), &'static str> {
    *cursor = cursor.checked_add(amount).ok_or("S_INTEGER_OVERFLOW")?;
    Ok(())
}

fn validate_structural_object(
    bytes: &[u8],
    object_type: StructuralType,
    implicit_root: bool,
    registry: &TypedRegistry,
) -> Result<(), &'static str> {
    let mut cursor = structural_prefix(bytes, object_type)?;
    match object_type {
        StructuralType::LogicalChunk => {
            let payload_len = structural_read_u64(bytes, cursor)?;
            if payload_len > 32_768 {
                return Err("S_CHUNK_CAP");
            }
            structural_advance(&mut cursor, 8)?;
            let payload_len = usize::try_from(payload_len).map_err(|_| "S_INTEGER_OVERFLOW")?;
            structural_take(bytes, cursor, payload_len)?;
            structural_advance(&mut cursor, payload_len)?;
        }
        StructuralType::LogicalFile => {
            let logical_len = structural_read_u64(bytes, cursor)?;
            if logical_len > MAX_FILE {
                return Err("S_LOGICAL_LENGTH");
            }
            structural_advance(&mut cursor, 8)?;
            let chunk_count = structural_read_u32(bytes, cursor)? as usize;
            if chunk_count > 1_000_000 {
                return Err("S_COUNT_CAP");
            }
            structural_advance(&mut cursor, 4)?;
            if (logical_len == 0) != (chunk_count == 0) {
                return Err("S_LOGICAL_LENGTH");
            }
            let mut reconstructed = 0u64;
            for _ in 0..chunk_count {
                let chunk_id: [u8; 32] = structural_take(bytes, cursor, 32)?
                    .try_into()
                    .map_err(|_| "S_TRUNCATED")?;
                structural_advance(&mut cursor, 32)?;
                let chunk_len = structural_read_u64(bytes, cursor)?;
                if chunk_len == 0 || chunk_len > 32_768 {
                    return Err("S_CHUNK_LENGTH");
                }
                if registry.logical_chunks.get(&chunk_id) != Some(&chunk_len) {
                    return Err("S_TYPED_EDGE");
                }
                reconstructed = reconstructed
                    .checked_add(chunk_len)
                    .ok_or("S_INTEGER_OVERFLOW")?;
                structural_advance(&mut cursor, 8)?;
            }
            if reconstructed != logical_len {
                return Err("S_LOGICAL_LENGTH");
            }
        }
        StructuralType::FileNode => {
            let mode = structural_read_u16(bytes, cursor)?;
            if mode > 0x0fff {
                return Err("S_FILE_MODE");
            }
            structural_advance(&mut cursor, 2)?;
            let logical_file_id: [u8; 32] = structural_take(bytes, cursor, 32)?
                .try_into()
                .map_err(|_| "S_TRUNCATED")?;
            structural_advance(&mut cursor, 32)?;
            let logical_len = structural_read_u64(bytes, cursor)?;
            if logical_len > MAX_FILE
                || registry.logical_files.get(&logical_file_id) != Some(&logical_len)
            {
                return Err("S_TYPED_EDGE");
            }
            structural_advance(&mut cursor, 8)?;
        }
        StructuralType::SymlinkNode => {
            let target_len = structural_read_u32(bytes, cursor)? as usize;
            structural_advance(&mut cursor, 4)?;
            if target_len == 0 || target_len > MAX_PATH {
                return Err("S_TARGET");
            }
            let target = structural_take(bytes, cursor, target_len)?;
            if target.contains(&0) || std::str::from_utf8(target).is_err() {
                return Err("S_TARGET");
            }
            structural_advance(&mut cursor, target_len)?;
        }
        StructuralType::DirectoryNode => {
            let mode = structural_read_u16(bytes, cursor)?;
            if implicit_root {
                if mode != ROOT_MODE_SENTINEL {
                    return Err("S_ROOT_SENTINEL");
                }
            } else if mode > 0x0fff {
                return Err("S_CHILD_MODE");
            }
            structural_advance(&mut cursor, 2)?;
            let child_count = structural_read_u32(bytes, cursor)? as usize;
            if child_count > 1_000_000 {
                return Err("S_COUNT_CAP");
            }
            structural_advance(&mut cursor, 4)?;
            let mut previous: Option<&[u8]> = None;
            for _ in 0..child_count {
                let name_len = structural_read_u32(bytes, cursor)? as usize;
                structural_advance(&mut cursor, 4)?;
                let name = structural_take(bytes, cursor, name_len)?;
                structural_advance(&mut cursor, name_len)?;
                if name.is_empty()
                    || name.len() > 255
                    || name.contains(&0)
                    || name.contains(&b'/')
                    || name == b"."
                    || name == b".."
                    || std::str::from_utf8(name).is_err()
                {
                    return Err("S_NAME");
                }
                if previous.is_some_and(|prior| prior >= name) {
                    return Err("S_ORDER_DUPLICATE");
                }
                previous = Some(name);
                let kind = *structural_take(bytes, cursor, 1)?
                    .first()
                    .ok_or("S_TRUNCATED")?;
                structural_advance(&mut cursor, 1)?;
                let child_id: [u8; 32] = structural_take(bytes, cursor, 32)?
                    .try_into()
                    .map_err(|_| "S_TRUNCATED")?;
                structural_advance(&mut cursor, 32)?;
                let typed = match kind {
                    REGULAR => registry.file_nodes.contains(&child_id),
                    DIRECTORY => registry.explicit_directory_nodes.contains(&child_id),
                    SYMLINK => registry.symlink_nodes.contains(&child_id),
                    _ => return Err("S_UNKNOWN_KIND"),
                };
                if !typed {
                    return Err("S_TYPED_EDGE");
                }
            }
        }
        StructuralType::VersionRoot => {
            let root_id: [u8; 32] = structural_take(bytes, cursor, 32)?
                .try_into()
                .map_err(|_| "S_TRUNCATED")?;
            if !registry.root_directory_nodes.contains(&root_id) {
                return Err("S_TYPED_EDGE");
            }
            structural_advance(&mut cursor, 32)?;
        }
    }
    if cursor != bytes.len() {
        return Err("S_EXACT_EOF");
    }
    Ok(())
}

fn register_identity(
    vector: &IdentityVector,
    object_type: StructuralType,
    implicit_root: bool,
    registry: &mut TypedRegistry,
) -> Result<(), &'static str> {
    validate_structural_object(&vector.preimage, object_type, implicit_root, registry)?;
    match object_type {
        StructuralType::LogicalChunk => {
            let domain_len = structural_domain(object_type).len();
            let payload_len = structural_read_u64(&vector.preimage, domain_len + 2)?;
            registry.logical_chunks.insert(vector.digest, payload_len);
        }
        StructuralType::LogicalFile => {
            let domain_len = structural_domain(object_type).len();
            let logical_len = structural_read_u64(&vector.preimage, domain_len + 2)?;
            registry.logical_files.insert(vector.digest, logical_len);
        }
        StructuralType::FileNode => {
            registry.file_nodes.insert(vector.digest);
        }
        StructuralType::SymlinkNode => {
            registry.symlink_nodes.insert(vector.digest);
        }
        StructuralType::DirectoryNode => {
            if implicit_root {
                registry.root_directory_nodes.insert(vector.digest);
            } else {
                registry.explicit_directory_nodes.insert(vector.digest);
            }
        }
        StructuralType::VersionRoot => {}
    }
    Ok(())
}

fn compare_two_windows(expected: &[u8], stored: &[u8]) -> bool {
    const WINDOW: usize = 65_536;
    let limit = expected.len().max(stored.len());
    let mut offset = 0usize;
    let mut equal = expected.len() == stored.len();
    while offset < limit {
        let expected_end = offset.saturating_add(WINDOW).min(expected.len());
        let stored_end = offset.saturating_add(WINDOW).min(stored.len());
        if expected.get(offset..expected_end) != stored.get(offset..stored_end) {
            equal = false;
        }
        offset = offset.saturating_add(WINDOW);
    }
    equal
}

fn occupied_compare(
    claimed_id: &[u8; 32],
    expected: &[u8],
    stored: &[u8],
    oracle: OccupiedIdOracle,
    object_type: StructuralType,
    implicit_root: bool,
    registry: &TypedRegistry,
) -> Result<(), &'static str> {
    // Structural/type/custody analogues outrank the remembered inequality.
    validate_structural_object(expected, object_type, implicit_root, registry)?;
    validate_structural_object(stored, object_type, implicit_root, registry)?;
    let recomputed_expected = digest(expected);
    let recomputed_stored = digest(stored);
    let (observed_expected, observed_stored) = match oracle {
        OccupiedIdOracle::Blake3 => (recomputed_expected, recomputed_stored),
        // A real BLAKE3 collision is computationally infeasible to manufacture
        // as a fixture. This test-only oracle injects equal observed IDs only
        // after both canonical byte strings have been parsed and independently
        // hashed; every ordinary vector uses the real BLAKE3 observations.
        OccupiedIdOracle::ForcedSameId => (*claimed_id, *claimed_id),
    };
    if observed_expected != *claimed_id || observed_stored != *claimed_id {
        return Err("S_ID_MISMATCH");
    }
    if !compare_two_windows(expected, stored) {
        return Err("S_OCCUPIED_SAME_ID_DIFFERENT_BYTES");
    }
    Ok(())
}

fn occupied_vectors(identities: &[IdentityVector]) -> Vec<OccupiedVector> {
    let empty_root = identities
        .iter()
        .find(|vector| vector.name == "directory_implicit_empty_root_1000")
        .expect("empty root");
    let composite_root = identities
        .iter()
        .find(|vector| vector.name == "directory_implicit_composite_root_1000")
        .expect("composite root");
    let empty_version = identities
        .iter()
        .find(|vector| vector.name == "version_empty_root")
        .expect("empty version");
    let mut malformed_root = empty_root.preimage.clone();
    malformed_root.push(0);

    vec![
        OccupiedVector {
            name: "occupied_exact_identical",
            claimed_id: empty_root.digest,
            expected_bytes: empty_root.preimage.clone(),
            stored_bytes: empty_root.preimage.clone(),
            oracle: OccupiedIdOracle::Blake3,
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
            expected: Ok(()),
            expected_label: "ACCEPTED_IDENTICAL",
        },
        OccupiedVector {
            name: "occupied_same_id_different_bytes",
            claimed_id: empty_root.digest,
            expected_bytes: empty_root.preimage.clone(),
            stored_bytes: composite_root.preimage.clone(),
            oracle: OccupiedIdOracle::ForcedSameId,
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
            expected: Err("S_OCCUPIED_SAME_ID_DIFFERENT_BYTES"),
            expected_label: "S_OCCUPIED_SAME_ID_DIFFERENT_BYTES",
        },
        OccupiedVector {
            name: "occupied_malformed_outranks_inequality",
            claimed_id: empty_root.digest,
            expected_bytes: empty_root.preimage.clone(),
            stored_bytes: malformed_root,
            oracle: OccupiedIdOracle::Blake3,
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
            expected: Err("S_EXACT_EOF"),
            expected_label: "S_EXACT_EOF",
        },
        OccupiedVector {
            name: "occupied_cross_type_outranks_inequality",
            claimed_id: empty_root.digest,
            expected_bytes: empty_root.preimage.clone(),
            stored_bytes: empty_version.preimage.clone(),
            oracle: OccupiedIdOracle::Blake3,
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
            expected: Err("S_TYPE_DOMAIN"),
            expected_label: "S_TYPE_DOMAIN",
        },
        OccupiedVector {
            name: "occupied_recomputed_id_mismatch",
            claimed_id: empty_root.digest,
            expected_bytes: empty_root.preimage.clone(),
            stored_bytes: composite_root.preimage.clone(),
            oracle: OccupiedIdOracle::Blake3,
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
            expected: Err("S_ID_MISMATCH"),
            expected_label: "S_ID_MISMATCH",
        },
    ]
}

fn path_of_len(prefix: &str, target: usize) -> String {
    assert!(target >= prefix.len() && target <= MAX_PATH);
    let mut result = prefix.to_owned();
    while result.len() < target {
        let component_len = result.rsplit('/').next().expect("component").len();
        let component_capacity = 255 - component_len;
        let remaining = target - result.len();
        if remaining <= component_capacity {
            result.extend(std::iter::repeat_n('a', remaining));
        } else if remaining == component_capacity + 1 {
            // Reserve one byte for a non-empty final component rather than
            // producing a path whose last byte is the separator.
            result.extend(std::iter::repeat_n('a', component_capacity - 1));
            result.push('/');
            result.push('a');
        } else {
            result.extend(std::iter::repeat_n('a', component_capacity));
            result.push('/');
            result.push('a');
        }
    }
    assert_eq!(result.len(), target);
    assert!(validate_path(result.as_bytes()).is_ok());
    result
}

fn create_symlink_fact(path: String, generation: u64) -> Fact {
    Fact {
        kind: CREATE,
        before_kind: 0,
        after_kind: SYMLINK,
        path,
        prior: None,
        before_generation: None,
        after_generation: Some(generation),
        before_size: None,
        after_size: None,
        metadata_mode: None,
        ranges: Vec::new(),
    }
}

fn modify_fact(path: String, ranges: Vec<(u64, u64)>) -> Fact {
    Fact {
        kind: MODIFY,
        before_kind: REGULAR,
        after_kind: REGULAR,
        path,
        prior: None,
        before_generation: Some(1),
        after_generation: Some(2),
        before_size: Some(128),
        after_size: Some(128),
        metadata_mode: Some(0o644),
        ranges,
    }
}

fn rename_fact(path: String, prior: String) -> Fact {
    Fact {
        kind: RENAME,
        before_kind: REGULAR,
        after_kind: REGULAR,
        path,
        prior: Some(prior),
        before_generation: Some(1),
        after_generation: Some(2),
        before_size: Some(MAX_FILE),
        after_size: Some(MAX_FILE),
        metadata_mode: Some(0o644),
        ranges: (0..64).map(|index| (index * 2, 1)).collect(),
    }
}

fn all_fact_kinds_receipt() -> ReceiptFields {
    let mut fields = base_receipt();
    fields.generation = 2;
    fields.facts = vec![
        create_symlink_fact("a-create".into(), 1),
        modify_fact("b-modify".into(), vec![(1, 2)]),
        Fact {
            kind: DELETE,
            before_kind: DIRECTORY,
            after_kind: 0,
            path: "c-delete".into(),
            prior: None,
            before_generation: Some(1),
            after_generation: None,
            before_size: None,
            after_size: None,
            metadata_mode: Some(0o755),
            ranges: Vec::new(),
        },
        simple_symlink_rename("d-rename", "old-d-rename"),
        Fact {
            kind: METADATA,
            before_kind: DIRECTORY,
            after_kind: DIRECTORY,
            path: "e-metadata".into(),
            prior: None,
            before_generation: Some(1),
            after_generation: Some(2),
            before_size: None,
            after_size: None,
            metadata_mode: Some(0o700),
            ranges: Vec::new(),
        },
        Fact {
            kind: TRUNCATE,
            before_kind: REGULAR,
            after_kind: REGULAR,
            path: "f-truncate".into(),
            prior: None,
            before_generation: Some(1),
            after_generation: Some(2),
            before_size: Some(10),
            after_size: Some(4),
            metadata_mode: Some(0o644),
            ranges: Vec::new(),
        },
        Fact {
            kind: SPARSE_MAP,
            before_kind: REGULAR,
            after_kind: REGULAR,
            path: "g-sparse".into(),
            prior: None,
            before_generation: Some(1),
            after_generation: Some(2),
            before_size: Some(128),
            after_size: Some(128),
            metadata_mode: Some(0o600),
            ranges: vec![(64, 16)],
        },
        Fact {
            kind: REPLACE,
            before_kind: SYMLINK,
            after_kind: SYMLINK,
            path: "h-replace-symlink".into(),
            prior: None,
            before_generation: Some(1),
            after_generation: Some(2),
            before_size: None,
            after_size: None,
            metadata_mode: None,
            ranges: Vec::new(),
        },
    ];
    fields
}

fn boundary_receipts() -> BTreeMap<&'static str, (Vec<u8>, ValidationContext, &'static str)> {
    let mut result = BTreeMap::new();

    let first_epoch = base_receipt();
    result.insert(
        "valid_first_receipt_in_new_epoch_sequence_one",
        (
            encode_receipt(&first_epoch),
            receipt_context(&first_epoch),
            "ACCEPTED",
        ),
    );

    let all_kinds = all_fact_kinds_receipt();
    let all_kinds_context = receipt_context(&all_kinds);
    let all_kinds_bytes = encode_receipt(&all_kinds);
    result.insert(
        "valid_all_fact_kinds",
        (all_kinds_bytes, all_kinds_context, "ACCEPTED"),
    );

    let mut rename_cycle = base_receipt();
    rename_cycle.generation = 2;
    rename_cycle.facts = vec![
        simple_symlink_rename("a", "b"),
        simple_symlink_rename("b", "a"),
    ];
    let rename_cycle_context = receipt_context(&rename_cycle);
    let rename_cycle_bytes = encode_receipt(&rename_cycle);
    result.insert(
        "valid_closed_rename_cycle",
        (rename_cycle_bytes, rename_cycle_context, "ACCEPTED"),
    );

    let mut max_facts = base_receipt();
    max_facts.generation = 2;
    max_facts.facts = (0..MAX_FACTS)
        .map(|index| create_symlink_fact(format!("p{index:04}"), index as u64 + 1))
        .collect();
    let max_facts_context = receipt_context(&max_facts);
    let max_facts_bytes = encode_receipt(&max_facts);
    result.insert(
        "boundary_4096_facts",
        (max_facts_bytes, max_facts_context, "ACCEPTED"),
    );

    let mut max_path = base_receipt();
    max_path.generation = 2;
    max_path.facts = vec![create_symlink_fact(path_of_len("z", MAX_PATH), 1)];
    let max_path_context = receipt_context(&max_path);
    let max_path_bytes = encode_receipt(&max_path);
    result.insert(
        "boundary_4096_path_bytes",
        (max_path_bytes, max_path_context, "ACCEPTED"),
    );

    let max_legal_fact = rename_fact(path_of_len("x", MAX_PATH), path_of_len("y", MAX_PATH));
    assert_eq!(encode_fact(&max_legal_fact).len(), 9_298);
    let mut max_legal_fact_receipt = base_receipt();
    max_legal_fact_receipt.generation = 2;
    max_legal_fact_receipt.facts = vec![max_legal_fact];
    let max_legal_fact_context = receipt_context(&max_legal_fact_receipt);
    let max_legal_fact_bytes = encode_receipt(&max_legal_fact_receipt);
    result.insert(
        "boundary_9298_maximum_reachable_legal_fact_bytes",
        (max_legal_fact_bytes, max_legal_fact_context, "ACCEPTED"),
    );

    let mut max_ranges = base_receipt();
    max_ranges.generation = 2;
    max_ranges.facts = (0..256)
        .map(|index| {
            modify_fact(
                format!("r{index:04}"),
                (0..64).map(|range| (range * 2, 1)).collect(),
            )
        })
        .collect();
    let max_ranges_context = receipt_context(&max_ranges);
    let max_ranges_bytes = encode_receipt(&max_ranges);
    result.insert(
        "boundary_16384_total_and_64_per_fact_ranges",
        (max_ranges_bytes, max_ranges_context, "ACCEPTED"),
    );

    let target_fact_bytes = MAX_RECEIPT - 225;
    let mut facts = Vec::new();
    for index in 0..112 {
        facts.push(rename_fact(
            path_of_len(&format!("n{index:04}"), MAX_PATH),
            path_of_len(&format!("o{index:04}"), MAX_PATH),
        ));
    }
    let used: usize = facts.iter().map(|fact| encode_fact(fact).len()).sum();
    let needed = target_fact_bytes - used;
    let probe = rename_fact("n0112".into(), "o0112".into());
    let minimum = encode_fact(&probe).len();
    let extra = needed - minimum;
    let path_extra = extra.min(MAX_PATH - 5);
    let prior_extra = extra - path_extra;
    assert!(prior_extra <= MAX_PATH - 5);
    facts.push(rename_fact(
        path_of_len("n0112", 5 + path_extra),
        path_of_len("o0112", 5 + prior_extra),
    ));
    let mut max_total = base_receipt();
    max_total.generation = 2;
    max_total.facts = facts;
    let max_total_context = receipt_context(&max_total);
    let max_total_bytes = encode_receipt(&max_total);
    assert_eq!(max_total_bytes.len(), MAX_RECEIPT);
    result.insert(
        "boundary_1048576_total_bytes",
        (max_total_bytes, max_total_context, "ACCEPTED"),
    );

    let replay_fields = nontrivial_receipt();
    let replay_bytes = encode_receipt(&replay_fields);
    let mut replay_context = receipt_context(&replay_fields);
    replay_context.replay_tuple = Some((9, 100, 102, digest(&replay_bytes)));
    result.insert(
        "valid_exact_idempotent_replay",
        (replay_bytes, replay_context, "IDEMPOTENT_REPLAY"),
    );

    result
}

fn swap_two_facts(bytes: &[u8]) -> Vec<u8> {
    let first_len = read_u32(bytes, 156).expect("first fact") as usize;
    let second_at = 156 + first_len;
    let second_len = read_u32(bytes, second_at).expect("second fact") as usize;
    let facts_end = second_at + second_len;
    let mut result = Vec::with_capacity(bytes.len());
    result.extend_from_slice(&bytes[..156]);
    result.extend_from_slice(&bytes[second_at..facts_end]);
    result.extend_from_slice(&bytes[156..second_at]);
    result.extend_from_slice(&bytes[facts_end..]);
    result
}

fn hostile_vectors() -> Vec<HostileVector> {
    let base_fields = nontrivial_receipt();
    let base_bytes = encode_receipt(&base_fields);
    let context = receipt_context(&base_fields);
    let mut vectors = Vec::new();

    let mut length = base_bytes.clone();
    let shorter_length = (length.len() - 1) as u32;
    length[14..18].copy_from_slice(&shorter_length.to_le_bytes());
    vectors.push(HostileVector {
        name: "receipt_total_length",
        base: "receipt_nontrivial",
        mutation: "total_encoded_bytes -= 1",
        bytes: length,
        expected: "E_TOTAL_LENGTH",
        context: context.clone(),
    });

    let mut count = base_bytes.clone();
    count[148..152].copy_from_slice(&3u32.to_le_bytes());
    reseal(&mut count, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_fact_count",
        base: "receipt_nontrivial",
        mutation: "facts_count 2 -> 3; reseal",
        bytes: count,
        expected: "E_FACT_COUNT",
        context: context.clone(),
    });

    let mut count_cap = base_bytes.clone();
    count_cap[148..152].copy_from_slice(&4_097u32.to_le_bytes());
    reseal(&mut count_cap, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_fact_count_cap",
        base: "receipt_nontrivial",
        mutation: "facts_count 2 -> 4097; reseal",
        bytes: count_cap,
        expected: "E_FACT_COUNT_CAP",
        context: context.clone(),
    });

    let mut presence = base_bytes.clone();
    presence[163] |= 0x40;
    reseal(&mut presence, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_reserved_presence",
        base: "receipt_nontrivial",
        mutation: "first fact presence |= 0x40; reseal",
        bytes: presence,
        expected: "E_PRESENCE",
        context: context.clone(),
    });

    let mut missing_presence = base_bytes.clone();
    missing_presence[163] &= !0x20;
    reseal(&mut missing_presence, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_missing_required_presence",
        base: "receipt_nontrivial",
        mutation: "first fact metadata presence bit cleared; reseal",
        bytes: missing_presence,
        expected: "E_PRESENCE",
        context: context.clone(),
    });

    let first_fact_len = read_u32(&base_bytes, 156).expect("first fact") as usize;
    let first_metadata_at = 156 + first_fact_len - 34;
    let mut arbitrary_metadata = base_bytes.clone();
    arbitrary_metadata[first_metadata_at] ^= 1;
    reseal(&mut arbitrary_metadata, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_metadata_arbitrary_digest",
        base: "receipt_nontrivial",
        mutation: "first CHGMETA digest byte toggled; receipt coverage/authentication resealed while immutable-source context remains unchanged",
        bytes: arbitrary_metadata,
        expected: "E_METADATA",
        context: context.clone(),
    });

    let mut wrong_metadata_kind = context.clone();
    wrong_metadata_kind.authoritative_metadata_by_path.insert(
        "docs/current".into(),
        SourceMetadataFact {
            kind: DIRECTORY,
            mode: Some(0o755),
        },
    );
    vectors.push(HostileVector {
        name: "receipt_metadata_wrong_source_kind",
        base: "receipt_nontrivial",
        mutation: "immutable-source metadata says DIRECTORY for the after-SYMLINK fact",
        bytes: base_bytes.clone(),
        expected: "E_METADATA",
        context: wrong_metadata_kind,
    });

    let mut wrong_metadata_mode = context.clone();
    wrong_metadata_mode.authoritative_metadata_by_path.insert(
        "src/lib.rs".into(),
        SourceMetadataFact {
            kind: REGULAR,
            mode: Some(0o600),
        },
    );
    vectors.push(HostileVector {
        name: "receipt_metadata_wrong_source_mode",
        base: "receipt_nontrivial",
        mutation: "immutable-source regular-file mode is 0600 while receipt CHGMETA commits 0644",
        bytes: base_bytes.clone(),
        expected: "E_METADATA",
        context: wrong_metadata_mode,
    });

    let mut sentinel_metadata_mode = context.clone();
    sentinel_metadata_mode
        .authoritative_metadata_by_path
        .insert(
            "src/lib.rs".into(),
            SourceMetadataFact {
                kind: REGULAR,
                mode: Some(ROOT_MODE_SENTINEL),
            },
        );
    vectors.push(HostileVector {
        name: "receipt_metadata_root_sentinel_mode",
        base: "receipt_nontrivial",
        mutation: "immutable-source regular-file metadata attempts reserved structural root sentinel 0x1000",
        bytes: base_bytes.clone(),
        expected: "E_METADATA",
        context: sentinel_metadata_mode,
    });

    let mut path_cap_fields = base_receipt();
    path_cap_fields.generation = 2;
    path_cap_fields.facts = vec![create_symlink_fact("z".repeat(MAX_PATH + 1), 1)];
    let path_cap_bytes = encode_receipt(&path_cap_fields);
    vectors.push(HostileVector {
        name: "receipt_path_cap",
        base: "one_create_fact",
        mutation: "primary path is 4097 bytes with valid framing/authentication",
        bytes: path_cap_bytes,
        expected: "E_PATH_CAP",
        context: receipt_context(&path_cap_fields),
    });

    for (name, path, expected) in [
        ("receipt_path_dot_component", ".", "E_PATH"),
        ("receipt_path_dot_dot_component", "..", "E_PATH"),
        ("receipt_path_leading_slash", "/a", "E_PATH"),
        ("receipt_path_repeated_slash", "a//b", "E_PATH"),
        ("receipt_path_trailing_slash", "a/", "E_PATH"),
    ] {
        let mut fields = base_receipt();
        fields.generation = 2;
        fields.facts = vec![create_symlink_fact(path.into(), 1)];
        vectors.push(HostileVector {
            name,
            base: "one_create_fact",
            mutation: "encode the named non-canonical OD-01 primary path",
            bytes: encode_receipt(&fields),
            expected,
            context: receipt_context(&fields),
        });
    }
    for (name, path, mutation) in [
        (
            "receipt_path_component_length_cap",
            "a".repeat(256),
            "single primary-path component is 256 bytes",
        ),
        (
            "receipt_path_component_count_cap",
            std::iter::repeat_n("a", 257).collect::<Vec<_>>().join("/"),
            "primary path contains 257 components",
        ),
    ] {
        let mut fields = base_receipt();
        fields.generation = 2;
        fields.facts = vec![create_symlink_fact(path, 1)];
        vectors.push(HostileVector {
            name,
            base: "one_create_fact",
            mutation,
            bytes: encode_receipt(&fields),
            expected: "E_PATH",
            context: receipt_context(&fields),
        });
    }
    let mut invalid_utf8_fields = base_receipt();
    invalid_utf8_fields.generation = 2;
    invalid_utf8_fields.facts = vec![create_symlink_fact("a".into(), 1)];
    let mut invalid_utf8 = encode_receipt(&invalid_utf8_fields);
    invalid_utf8[168] = 0xff;
    reseal(&mut invalid_utf8, &invalid_utf8_fields.key);
    vectors.push(HostileVector {
        name: "receipt_path_invalid_utf8",
        base: "one_create_fact",
        mutation: "one-byte primary path a -> ff; reseal",
        bytes: invalid_utf8,
        expected: "E_PATH",
        context: receipt_context(&invalid_utf8_fields),
    });
    let mut embedded_nul = encode_receipt(&invalid_utf8_fields);
    embedded_nul[168] = 0;
    reseal(&mut embedded_nul, &invalid_utf8_fields.key);
    vectors.push(HostileVector {
        name: "receipt_path_embedded_nul",
        base: "one_create_fact",
        mutation: "one-byte primary path a -> 00; reseal",
        bytes: embedded_nul,
        expected: "E_PATH",
        context: receipt_context(&invalid_utf8_fields),
    });

    let mut order = swap_two_facts(&base_bytes);
    reseal(&mut order, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_fact_order",
        base: "receipt_nontrivial",
        mutation: "swap the two complete fact records; reseal",
        bytes: order,
        expected: "E_FACT_ORDER",
        context: context.clone(),
    });

    let mut duplicate_fields = base_receipt();
    duplicate_fields.generation = 2;
    duplicate_fields.facts = vec![
        create_symlink_fact("same1".into(), 1),
        create_symlink_fact("same2".into(), 2),
    ];
    let duplicate_context = receipt_context(&duplicate_fields);
    let mut duplicate = encode_receipt(&duplicate_fields);
    let first_len = read_u32(&duplicate, 156).expect("fact") as usize;
    let second_path_at = 156 + first_len + 12;
    duplicate[second_path_at..second_path_at + 5].copy_from_slice(b"same1");
    reseal(&mut duplicate, &duplicate_fields.key);
    vectors.push(HostileVector {
        name: "receipt_duplicate_path",
        base: "two_create_facts",
        mutation: "second primary path same2 -> same1; reseal",
        bytes: duplicate,
        expected: "E_DUPLICATE",
        context: duplicate_context,
    });

    let mut rename_chain_fields = base_receipt();
    rename_chain_fields.generation = 2;
    rename_chain_fields.facts = vec![
        simple_symlink_rename("b", "a"),
        simple_symlink_rename("c", "b"),
    ];
    let rename_chain_bytes = encode_receipt(&rename_chain_fields);
    vectors.push(HostileVector {
        name: "receipt_uncoalesced_rename_chain",
        base: "two_rename_facts",
        mutation: "encode a->b followed by b->c instead of the coalesced a->c fact",
        bytes: rename_chain_bytes,
        expected: "E_RENAME_CHAIN",
        context: receipt_context(&rename_chain_fields),
    });

    let mut rename_ambiguous_fields = base_receipt();
    rename_ambiguous_fields.generation = 2;
    rename_ambiguous_fields.facts = vec![
        create_symlink_fact("a".into(), 1),
        simple_symlink_rename("b", "a"),
    ];
    let rename_ambiguous_bytes = encode_receipt(&rename_ambiguous_fields);
    vectors.push(HostileVector {
        name: "receipt_ambiguous_rename_source",
        base: "create_and_rename_facts",
        mutation: "encode non-rename primary path a and rename source a",
        bytes: rename_ambiguous_bytes,
        expected: "E_RENAME_AMBIGUOUS",
        context: receipt_context(&rename_ambiguous_fields),
    });

    let mut equal_generation_fields = base_receipt();
    equal_generation_fields.generation = 2;
    let mut equal_generation_fact = simple_symlink_rename("new", "old");
    equal_generation_fact.after_generation = equal_generation_fact.before_generation;
    equal_generation_fields.facts = vec![equal_generation_fact];
    vectors.push(HostileVector {
        name: "receipt_equal_entry_generation",
        base: "one_rename_fact",
        mutation: "before_generation == after_generation == 1",
        bytes: encode_receipt(&equal_generation_fields),
        expected: "E_ENTRY_GENERATION",
        context: receipt_context(&equal_generation_fields),
    });

    let mut source_fan_out_fields = base_receipt();
    source_fan_out_fields.generation = 2;
    source_fan_out_fields.facts = vec![
        simple_symlink_rename("b", "a"),
        simple_symlink_rename("c", "a"),
    ];
    vectors.push(HostileVector {
        name: "receipt_rename_source_fan_out",
        base: "two_rename_facts",
        mutation: "the same prior path a is the source for destinations b and c",
        bytes: encode_receipt(&source_fan_out_fields),
        expected: "E_RENAME_DUPLICATE",
        context: receipt_context(&source_fan_out_fields),
    });

    for (name, ranges, expected) in [
        (
            "receipt_range_overlap",
            vec![(0, 2), (1, 2)],
            "E_RANGE_OVERLAP",
        ),
        (
            "receipt_range_adjacency",
            vec![(0, 1), (1, 1)],
            "E_RANGE_ADJACENT",
        ),
        (
            "receipt_range_overflow",
            vec![(u64::MAX, 2)],
            "E_RANGE_OVERFLOW",
        ),
        ("receipt_range_zero", vec![(0, 0)], "E_RANGE_ZERO"),
        ("receipt_range_after_size", vec![(127, 2)], "E_RANGE_SIZE"),
    ] {
        let mut fields = base_receipt();
        fields.generation = 2;
        let mut fact = modify_fact("range".into(), ranges);
        if name == "receipt_range_overflow" {
            fact.after_size = Some(MAX_FILE);
        }
        fields.facts = vec![fact];
        let bytes = encode_receipt(&fields);
        vectors.push(HostileVector {
            name,
            base: "one_modify_fact",
            mutation: "encode named hostile range set with valid authentication",
            bytes,
            expected,
            context: receipt_context(&fields),
        });
    }

    let mut range_count_cap_fields = base_receipt();
    range_count_cap_fields.generation = 2;
    range_count_cap_fields.facts = vec![modify_fact(
        "range-cap".into(),
        (0..65).map(|range| (range * 2, 1)).collect(),
    )];
    let range_count_cap_bytes = encode_receipt(&range_count_cap_fields);
    vectors.push(HostileVector {
        name: "receipt_range_count_cap",
        base: "one_modify_fact",
        mutation: "range_count=65 with valid framing/authentication",
        bytes: range_count_cap_bytes,
        expected: "E_RANGE_COUNT_CAP",
        context: receipt_context(&range_count_cap_fields),
    });

    let mut total_range_cap_fields = base_receipt();
    total_range_cap_fields.generation = 2;
    total_range_cap_fields.facts = (0..257)
        .map(|index| {
            let count = if index == 256 { 1 } else { 64 };
            modify_fact(
                format!("total-range-{index:03}"),
                (0..count).map(|range| (range * 2, 1)).collect(),
            )
        })
        .collect();
    let total_range_cap_bytes = encode_receipt(&total_range_cap_fields);
    vectors.push(HostileVector {
        name: "receipt_total_range_cap",
        base: "257_modify_facts",
        mutation: "total range count=16385 with per-fact count <=64",
        bytes: total_range_cap_bytes,
        expected: "E_TOTAL_RANGE_CAP",
        context: receipt_context(&total_range_cap_fields),
    });

    let mut size_cap_fields = base_receipt();
    size_cap_fields.generation = 2;
    let mut size_cap_fact = modify_fact("size-cap".into(), Vec::new());
    size_cap_fact.before_size = Some(MAX_FILE + 1);
    size_cap_fields.facts = vec![size_cap_fact];
    vectors.push(HostileVector {
        name: "receipt_file_size_cap",
        base: "one_modify_fact",
        mutation: "before_size is 8589934593",
        bytes: encode_receipt(&size_cap_fields),
        expected: "E_SIZE_CAP",
        context: receipt_context(&size_cap_fields),
    });

    let mut forbidden_range_fields = base_receipt();
    forbidden_range_fields.generation = 2;
    forbidden_range_fields.facts = vec![Fact {
        kind: METADATA,
        before_kind: DIRECTORY,
        after_kind: DIRECTORY,
        path: "dir".into(),
        prior: None,
        before_generation: Some(1),
        after_generation: Some(2),
        before_size: None,
        after_size: None,
        metadata_mode: Some(0o755),
        ranges: vec![(0, 1)],
    }];
    vectors.push(HostileVector {
        name: "receipt_forbidden_range_combination",
        base: "one_metadata_fact",
        mutation: "directory METADATA fact carries one change range",
        bytes: encode_receipt(&forbidden_range_fields),
        expected: "E_RANGE_COMBINATION",
        context: receipt_context(&forbidden_range_fields),
    });

    let mut unknown_kind = base_bytes.clone();
    unknown_kind[160] = 0xff;
    reseal(&mut unknown_kind, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_unknown_fact_kind",
        base: "receipt_nontrivial",
        mutation: "first fact_kind -> 0xff; reseal",
        bytes: unknown_kind,
        expected: "E_FACT_KIND",
        context: context.clone(),
    });

    let mut unknown_entry_kind = base_bytes.clone();
    unknown_entry_kind[161] = 0xff;
    reseal(&mut unknown_entry_kind, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_unknown_entry_kind",
        base: "receipt_nontrivial",
        mutation: "first before_entry_kind -> 0xff; reseal",
        bytes: unknown_entry_kind,
        expected: "E_ENTRY_KIND",
        context: context.clone(),
    });

    let mut missing_prior = base_bytes.clone();
    missing_prior[163] &= !0x01;
    reseal(&mut missing_prior, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_missing_rename_prior_path",
        base: "receipt_nontrivial",
        mutation: "first RENAME fact prior_path presence cleared; reseal",
        bytes: missing_prior,
        expected: "E_PRESENCE",
        context: context.clone(),
    });

    let mut minimum_fact = base_bytes.clone();
    minimum_fact[156..160].copy_from_slice(&46u32.to_le_bytes());
    reseal(&mut minimum_fact, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_fact_below_defensive_minimum",
        base: "receipt_nontrivial",
        mutation: "first fact_encoded_bytes -> 46; reseal",
        bytes: minimum_fact,
        expected: "E_FACT_SIZE_CAP",
        context: context.clone(),
    });

    let mut fact_cap = base_bytes.clone();
    fact_cap[156..160].copy_from_slice(&16_385u32.to_le_bytes());
    reseal(&mut fact_cap, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_fact_size_cap",
        base: "receipt_nontrivial",
        mutation: "first fact_encoded_bytes -> 16385; reseal",
        bytes: fact_cap,
        expected: "E_FACT_SIZE_CAP",
        context: context.clone(),
    });

    let mut fact_beyond_block = base_bytes.clone();
    let fact_block_len = read_u32(&fact_beyond_block, 152).expect("facts");
    fact_beyond_block[156..160].copy_from_slice(&(fact_block_len + 1).to_le_bytes());
    reseal(&mut fact_beyond_block, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_fact_extends_beyond_facts_block",
        base: "receipt_nontrivial",
        mutation: "first fact length exceeds the complete facts block by one; reseal",
        bytes: fact_beyond_block,
        expected: "E_FACT_LENGTH",
        context: context.clone(),
    });

    let mut fact_trailing = base_bytes.clone();
    let first_fact_len = read_u32(&fact_trailing, 156).expect("first fact") as usize;
    let second_fact_at = 156 + first_fact_len;
    fact_trailing.insert(second_fact_at, 0);
    fact_trailing[156..160]
        .copy_from_slice(&(u32::try_from(first_fact_len).expect("fact") + 1).to_le_bytes());
    let original_facts_len = read_u32(&base_bytes, 152).expect("facts");
    fact_trailing[152..156].copy_from_slice(&(original_facts_len + 1).to_le_bytes());
    let original_total = read_u32(&base_bytes, 14).expect("total");
    fact_trailing[14..18].copy_from_slice(&(original_total + 1).to_le_bytes());
    reseal(&mut fact_trailing, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_fact_local_trailing_byte",
        base: "receipt_nontrivial",
        mutation: "insert one byte at first fact EOF and extend its/block/total lengths; reseal",
        bytes: fact_trailing,
        expected: "E_FACT_EOF",
        context: context.clone(),
    });

    let mut facts_length = base_bytes.clone();
    let declared_facts = read_u32(&facts_length, 152).expect("facts");
    facts_length[152..156].copy_from_slice(&(declared_facts - 1).to_le_bytes());
    vectors.push(HostileVector {
        name: "receipt_facts_length",
        base: "receipt_nontrivial",
        mutation: "facts_encoded_bytes -= 1 without changing total bytes",
        bytes: facts_length,
        expected: "E_FACTS_LENGTH",
        context: context.clone(),
    });

    let mut zero_binding = base_bytes.clone();
    zero_binding[90..122].fill(0);
    reseal(&mut zero_binding, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_zero_binding_token",
        base: "receipt_nontrivial",
        mutation: "base AcceptedBinding token set to all-zero; reseal",
        bytes: zero_binding,
        expected: "E_BINDING_AUTHORITY",
        context: context.clone(),
    });

    let mut zero_epoch = base_bytes.clone();
    zero_epoch[50..58].fill(0);
    reseal(&mut zero_epoch, &base_fields.key);
    let mut zero_epoch_context = context.clone();
    zero_epoch_context.epoch = 0;
    vectors.push(HostileVector {
        name: "receipt_zero_epoch",
        base: "receipt_nontrivial",
        mutation: "issuer_instance_epoch and registered context epoch both set to zero; reseal",
        bytes: zero_epoch,
        expected: "E_EPOCH",
        context: zero_epoch_context,
    });

    let mut foreign_sandbox_context = context.clone();
    foreign_sandbox_context.sandbox[0] ^= 1;
    vectors.push(HostileVector {
        name: "receipt_foreign_sandbox",
        base: "receipt_nontrivial",
        mutation: "store-held authority is for a different sandbox_id",
        bytes: base_bytes.clone(),
        expected: "E_ISSUER_AUTHORITY",
        context: foreign_sandbox_context,
    });

    let mut issuer_context = context.clone();
    issuer_context.issuer[0] ^= 1;
    vectors.push(HostileVector {
        name: "receipt_issuer_mismatch",
        base: "receipt_nontrivial",
        mutation: "registered issuer_id differs",
        bytes: base_bytes.clone(),
        expected: "E_ISSUER_AUTHORITY",
        context: issuer_context,
    });

    let mut unknown_key_context = context.clone();
    unknown_key_context.key_id = 8;
    vectors.push(HostileVector {
        name: "receipt_unknown_nonzero_key_id",
        base: "receipt_nontrivial",
        mutation: "registry exposes key id 8 while receipt names key id 7",
        bytes: base_bytes.clone(),
        expected: "E_AUTH_KEY",
        context: unknown_key_context,
    });

    let mut sequence_zero = base_bytes.clone();
    sequence_zero[130..138].fill(0);
    reseal(&mut sequence_zero, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_sequence_first_zero",
        base: "receipt_nontrivial",
        mutation: "sequence_first -> 0; reseal",
        bytes: sequence_zero,
        expected: "E_SEQUENCE",
        context: context.clone(),
    });

    let mut sequence_reverse = base_bytes.clone();
    sequence_reverse[138..146].copy_from_slice(&99u64.to_le_bytes());
    reseal(&mut sequence_reverse, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_sequence_final_before_first",
        base: "receipt_nontrivial",
        mutation: "sequence_final 102 -> 99 while sequence_first is 100; reseal",
        bytes: sequence_reverse,
        expected: "E_SEQUENCE",
        context: context.clone(),
    });

    let mut sequence_context = context.clone();
    sequence_context.last_sequence_final = 98;
    vectors.push(HostileVector {
        name: "receipt_sequence_gap",
        base: "receipt_nontrivial",
        mutation: "validate against last_sequence_final=98, expected first=99",
        bytes: base_bytes.clone(),
        expected: "FULL_ENUM_SEQUENCE_GAP",
        context: sequence_context,
    });

    let mut sequence_overflow_context = context.clone();
    sequence_overflow_context.last_sequence_final = u64::MAX;
    vectors.push(HostileVector {
        name: "receipt_sequence_overflow",
        base: "receipt_nontrivial",
        mutation: "validate after last_sequence_final=u64::MAX",
        bytes: base_bytes.clone(),
        expected: "E_SEQUENCE_OVERFLOW",
        context: sequence_overflow_context,
    });

    let mut generation_context = context.clone();
    generation_context.last_generation = 9;
    generation_context.last_sequence_final = 99;
    vectors.push(HostileVector {
        name: "receipt_stale_generation",
        base: "receipt_nontrivial",
        mutation: "validate against last_generation=9",
        bytes: base_bytes.clone(),
        expected: "E_SOURCE_GENERATION",
        context: generation_context,
    });

    let mut custody_context = context.clone();
    custody_context.custody[0] ^= 1;
    vectors.push(HostileVector {
        name: "receipt_custody_mismatch",
        base: "receipt_nontrivial",
        mutation: "registered custody_id differs",
        bytes: base_bytes.clone(),
        expected: "E_CUSTODY",
        context: custody_context,
    });

    let mut replay_variant = base_bytes.clone();
    replay_variant[147] = 1;
    reseal(&mut replay_variant, &base_fields.key);
    let mut replay_context = context.clone();
    replay_context.replay_tuple = Some((9, 100, 102, digest(&base_bytes)));
    vectors.push(HostileVector {
        name: "receipt_replay_divergence",
        base: "receipt_nontrivial",
        mutation: "same generation/sequence tuple, different validly authenticated bytes",
        bytes: replay_variant,
        expected: "E_REPLAY_DIVERGENCE",
        context: replay_context,
    });

    let mut auth = base_bytes.clone();
    let last = auth.len() - 1;
    auth[last] ^= 1;
    vectors.push(HostileVector {
        name: "receipt_authentication",
        base: "receipt_nontrivial",
        mutation: "issuer_authentication final byte ^= 1",
        bytes: auth,
        expected: "E_AUTH",
        context: context.clone(),
    });

    let mut coverage = base_bytes.clone();
    let coverage_at = 156 + read_u32(&coverage, 152).expect("facts") as usize;
    coverage[coverage_at] ^= 1;
    reauthenticate_without_coverage(&mut coverage, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_coverage_digest",
        base: "receipt_nontrivial",
        mutation: "coverage_digest byte 0 ^= 1; recompute only MAC",
        bytes: coverage,
        expected: "E_COVERAGE",
        context: context.clone(),
    });

    let mut non_final = base_bytes.clone();
    non_final[146] = 0;
    reseal(&mut non_final, &base_fields.key);
    let mut finality_context = context.clone();
    finality_context.last_sequence_final = 99;
    vectors.push(HostileVector {
        name: "receipt_non_final",
        base: "receipt_nontrivial",
        mutation: "finality FINAL -> NON_FINAL; reseal",
        bytes: non_final,
        expected: "FULL_ENUM_NON_FINAL",
        context: finality_context,
    });

    let mut overflowed = base_bytes.clone();
    overflowed[147] = 1;
    reseal(&mut overflowed, &base_fields.key);
    let mut overflow_context = context.clone();
    overflow_context.last_sequence_final = 99;
    vectors.push(HostileVector {
        name: "receipt_producer_overflow",
        base: "receipt_nontrivial",
        mutation: "overflowed COMPLETE -> PRODUCER_OVERFLOW; reseal",
        bytes: overflowed,
        expected: "FULL_ENUM_PRODUCER_OVERFLOW",
        context: overflow_context,
    });

    let mut unknown_finality = base_bytes.clone();
    unknown_finality[146] = 2;
    reseal(&mut unknown_finality, &base_fields.key);
    let mut unknown_finality_context = context.clone();
    unknown_finality_context.last_sequence_final = 99;
    vectors.push(HostileVector {
        name: "receipt_unknown_finality",
        base: "receipt_nontrivial",
        mutation: "finality -> 2; reseal",
        bytes: unknown_finality,
        expected: "E_FINALITY_TAG",
        context: unknown_finality_context,
    });

    let mut unknown_overflow = base_bytes.clone();
    unknown_overflow[147] = 2;
    reseal(&mut unknown_overflow, &base_fields.key);
    let mut unknown_overflow_context = context.clone();
    unknown_overflow_context.last_sequence_final = 99;
    vectors.push(HostileVector {
        name: "receipt_unknown_overflow_tag",
        base: "receipt_nontrivial",
        mutation: "overflowed -> 2; reseal",
        bytes: unknown_overflow,
        expected: "E_OVERFLOW_TAG",
        context: unknown_overflow_context,
    });

    let mut unknown_scheme = base_bytes.clone();
    let scheme_at = 156 + read_u32(&unknown_scheme, 152).expect("facts") as usize + 32;
    unknown_scheme[scheme_at] = 2;
    vectors.push(HostileVector {
        name: "receipt_unknown_auth_scheme",
        base: "receipt_nontrivial",
        mutation: "authentication_scheme -> 2",
        bytes: unknown_scheme,
        expected: "E_AUTH_SCHEME",
        context: context.clone(),
    });

    let mut zero_key_id = base_bytes.clone();
    let zero_key_at = 156 + read_u32(&zero_key_id, 152).expect("facts") as usize + 33;
    zero_key_id[zero_key_at..zero_key_at + 4].fill(0);
    reseal(&mut zero_key_id, &base_fields.key);
    vectors.push(HostileVector {
        name: "receipt_zero_key_id",
        base: "receipt_nontrivial",
        mutation: "issuer_key_id set to zero; reseal",
        bytes: zero_key_id,
        expected: "E_AUTH_KEY",
        context: context.clone(),
    });

    let mut trailing = base_bytes.clone();
    trailing.push(0);
    vectors.push(HostileVector {
        name: "receipt_trailing_byte",
        base: "receipt_nontrivial",
        mutation: "append 00 without changing total_encoded_bytes",
        bytes: trailing,
        expected: "E_TOTAL_LENGTH",
        context: context.clone(),
    });

    let over_cap = vec![0u8; MAX_RECEIPT + 1];
    vectors.push(HostileVector {
        name: "receipt_total_cap",
        base: "synthetic",
        mutation: "borrowed input length = 1048577",
        bytes: over_cap,
        expected: "E_TOTAL_CAP",
        context,
    });

    vectors
}

fn disposition_vectors() -> Vec<DispositionVector> {
    let fields = nontrivial_receipt();
    let bytes = encode_receipt(&fields);
    let mut base = receipt_context(&fields);
    base.last_sequence_final = 99;
    let mut vectors = Vec::new();
    let mut push = |name, mutation, expected, is_ok, context| {
        vectors.push(DispositionVector {
            name,
            base: "receipt_nontrivial",
            mutation,
            bytes: bytes.clone(),
            expected,
            is_ok,
            context,
        });
    };

    let mut full_changed = base.clone();
    full_changed.authoritative_resync_witness = false;
    push(
        "disposition_full_changed_file_required",
        "valid changed-path coverage; no authoritative resync witness",
        "FULL_CHANGED_FILE_REQUIRED",
        true,
        full_changed,
    );
    let mut unavailable = base.clone();
    unavailable.reservation = ReservationState::Unavailable;
    push(
        "disposition_resource_unavailable",
        "shared B_s reservation unavailable before parse/authentication",
        "RECEIPT_RESOURCE_REFUSED_UNAVAILABLE",
        false,
        unavailable,
    );
    let mut deadline = base.clone();
    deadline.reservation = ReservationState::DeadlineExpired;
    push(
        "disposition_resource_deadline",
        "reservation deadline already expired before parse/authentication",
        "RECEIPT_RESOURCE_REFUSED_DEADLINE",
        false,
        deadline,
    );
    let mut cancelled = base.clone();
    cancelled.reservation = ReservationState::Cancelled;
    push(
        "disposition_resource_cancelled",
        "reservation cancelled before parse/authentication",
        "RECEIPT_RESOURCE_REFUSED_CANCELLED",
        false,
        cancelled,
    );
    let mut short_budget = base.clone();
    short_budget.reservation = ReservationState::Available {
        bytes: RECEIPT_PROCESSING_MAX - 1,
    };
    push(
        "disposition_resource_short_precharge",
        "reservation is one byte below exact combined precharge",
        "RECEIPT_RESOURCE_REFUSED_UNAVAILABLE",
        false,
        short_budget,
    );
    let mut decoder_cap = base.clone();
    decoder_cap.decoder_owned_request = MAX_DECODER_OWNED + 1;
    push(
        "decoder_owned_capacity_cap",
        "decoder-owned capacity request is 2097153 bytes",
        "E_DECODER_OWNED_CAP",
        false,
        decoder_cap,
    );
    let mut disclosure_cap = base.clone();
    disclosure_cap.error_disclosure_request = MAX_ERROR_DISCLOSURE + 1;
    push(
        "error_disclosure_cap",
        "error disclosure request is 4097 bytes",
        "E_ERROR_DISCLOSURE_CAP",
        false,
        disclosure_cap,
    );
    let mut raw_token = base.clone();
    raw_token.accepted_binding = false;
    push(
        "authority_raw_token_cannot_mint",
        "matching raw token lacks a store-held accepted capability",
        "E_BINDING_AUTHORITY",
        false,
        raw_token,
    );
    let mut revoked = base.clone();
    revoked.binding_unrevoked = false;
    push(
        "authority_revoked_binding",
        "store-held binding is revoked",
        "E_BINDING_AUTHORITY",
        false,
        revoked,
    );
    let mut stale_locator = base.clone();
    stale_locator.binding_locator_current = false;
    push(
        "authority_stale_locator",
        "store-held binding locator proof is stale",
        "E_BINDING_AUTHORITY",
        false,
        stale_locator,
    );
    let mut bad_closure = base.clone();
    bad_closure.binding_closure_authenticated = false;
    push(
        "authority_unauthenticated_closure",
        "accepted-binding closure authentication is absent",
        "E_BINDING_AUTHORITY",
        false,
        bad_closure,
    );
    let mut disabled_key = base.clone();
    disabled_key.issuer_key_enabled = false;
    push(
        "authority_disabled_issuer_key",
        "registered issuer key is disabled",
        "E_AUTH_KEY",
        false,
        disabled_key,
    );
    let mut reused_epoch = base.clone();
    reused_epoch.epoch_registered_and_nonreused = false;
    push(
        "authority_epoch_reuse_or_rollback",
        "epoch history marks the current numeric epoch reused or rolled back",
        "E_EPOCH",
        false,
        reused_epoch,
    );
    let mut transferred = base.clone();
    transferred.custody_current_and_immutable = false;
    push(
        "custody_transferred_or_mutable_snapshot",
        "snapshot custody was transferred or is no longer immutable",
        "E_CUSTODY",
        false,
        transferred,
    );
    let mut revalidation = base.clone();
    revalidation.immediate_revalidation_succeeds = false;
    push(
        "custody_immediate_revalidation_failure",
        "authority/custody revalidation fails immediately before effects",
        "E_IMMEDIATE_REVALIDATION",
        false,
        revalidation,
    );
    let mut replay_unknown = base.clone();
    replay_unknown.replay_tuple = Some((9, 100, 102, digest(&bytes)));
    replay_unknown.replay_outcome_known = false;
    push(
        "replay_unknown_outcome_requires_recovery",
        "exact replay exists but prior outcome is unknown",
        "E_REPLAY_RECOVERY_REQUIRED",
        false,
        replay_unknown,
    );
    vectors
}

fn identity_layout(name: &str) -> (StructuralType, bool) {
    match name {
        "logical_chunk_abc" => (StructuralType::LogicalChunk, false),
        "logical_file_abc" => (StructuralType::LogicalFile, false),
        "file_node_0644_abc" => (StructuralType::FileNode, false),
        "symlink_node_file_txt" => (StructuralType::SymlinkNode, false),
        "directory_explicit_0755_nested_file" => (StructuralType::DirectoryNode, false),
        "directory_implicit_empty_root_1000" | "directory_implicit_composite_root_1000" => {
            (StructuralType::DirectoryNode, true)
        }
        "version_empty_root" | "version_composite" => (StructuralType::VersionRoot, false),
        _ => panic!("unknown identity vector {name}"),
    }
}

fn registry_from_identities(vectors: &[IdentityVector]) -> TypedRegistry {
    let mut registry = TypedRegistry::default();
    for vector in vectors {
        let (object_type, implicit_root) = identity_layout(vector.name);
        register_identity(vector, object_type, implicit_root, &mut registry)
            .unwrap_or_else(|error| panic!("{} failed structural parse: {error}", vector.name));
    }
    registry
}

fn structural_hostiles(vectors: &[IdentityVector]) -> Vec<StructuralHostile> {
    let find = |name: &str| {
        vectors
            .iter()
            .find(|vector| vector.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    let empty_root = vectors
        .iter()
        .find(|vector| vector.name == "directory_implicit_empty_root_1000")
        .expect("empty root");
    let explicit = vectors
        .iter()
        .find(|vector| vector.name == "directory_explicit_0755_nested_file")
        .expect("explicit directory");
    let mut root_normal_mode = empty_root.preimage.clone();
    root_normal_mode[13..15].copy_from_slice(&0o755u16.to_le_bytes());
    let mut child_sentinel = explicit.preimage.clone();
    child_sentinel[13..15].copy_from_slice(&ROOT_MODE_SENTINEL.to_le_bytes());
    let mut unknown_kind = explicit.preimage.clone();
    let kind_at = 19 + 4 + 4;
    unknown_kind[kind_at] = 0xff;
    let mut trailing = empty_root.preimage.clone();
    trailing.push(0);
    let truncated = explicit.preimage[..explicit.preimage.len() - 1].to_vec();
    let logical_chunk = find("logical_chunk_abc");
    let logical_file = find("logical_file_abc");
    let file_node = find("file_node_0644_abc");
    let symlink = find("symlink_node_file_txt");
    let version = find("version_empty_root");
    let mut chunk_schema = logical_chunk.preimage.clone();
    chunk_schema[12..14].copy_from_slice(&2u16.to_le_bytes());
    let mut chunk_length = logical_chunk.preimage.clone();
    chunk_length[14..22].copy_from_slice(&4u64.to_le_bytes());
    let mut chunk_trailing = logical_chunk.preimage.clone();
    chunk_trailing.push(0);
    let mut logical_file_sum = logical_file.preimage.clone();
    logical_file_sum[13..21].copy_from_slice(&4u64.to_le_bytes());
    let mut logical_file_chunk_length = logical_file.preimage.clone();
    logical_file_chunk_length[13..21].copy_from_slice(&4u64.to_le_bytes());
    logical_file_chunk_length[57..65].copy_from_slice(&4u64.to_le_bytes());
    let mut logical_file_edge = logical_file.preimage.clone();
    logical_file_edge[25..57].copy_from_slice(&symlink.digest);
    let mut logical_file_trailing = logical_file.preimage.clone();
    logical_file_trailing.push(0);
    let mut file_mode = file_node.preimage.clone();
    file_mode[13..15].copy_from_slice(&ROOT_MODE_SENTINEL.to_le_bytes());
    let mut file_edge = file_node.preimage.clone();
    file_edge[15..47].copy_from_slice(&symlink.digest);
    let mut file_length = file_node.preimage.clone();
    file_length[47..55].copy_from_slice(&4u64.to_le_bytes());
    let mut file_trailing = file_node.preimage.clone();
    file_trailing.push(0);
    let mut symlink_utf8 = symlink.preimage.clone();
    symlink_utf8[17] = 0xff;
    let mut symlink_nul = symlink.preimage.clone();
    symlink_nul[17] = 0;
    let mut symlink_empty = symlink.preimage.clone();
    symlink_empty[13..17].fill(0);
    symlink_empty.truncate(17);
    let mut symlink_trailing = symlink.preimage.clone();
    symlink_trailing.push(0);
    let mut directory_utf8 = explicit.preimage.clone();
    directory_utf8[23] = 0xff;
    let mut directory_nul = explicit.preimage.clone();
    directory_nul[23] = 0;
    let mut directory_edge = explicit.preimage.clone();
    directory_edge[28..60].copy_from_slice(&symlink.digest);
    let mut implicit_root_as_child = explicit.preimage.clone();
    implicit_root_as_child[27] = DIRECTORY;
    implicit_root_as_child[28..60].copy_from_slice(&empty_root.digest);
    let duplicate_directory = directory_node(
        ROOT_MODE_SENTINEL,
        &[
            (b"a".as_slice(), REGULAR, file_node.digest),
            (b"a".as_slice(), REGULAR, file_node.digest),
        ],
    );
    let descending_directory = directory_node(
        ROOT_MODE_SENTINEL,
        &[
            (b"z".as_slice(), REGULAR, file_node.digest),
            (b"a".as_slice(), REGULAR, file_node.digest),
        ],
    );
    let dot_directory = directory_node(
        ROOT_MODE_SENTINEL,
        &[(b".".as_slice(), REGULAR, file_node.digest)],
    );
    let mut version_edge = version.preimage.clone();
    version_edge[13..45].copy_from_slice(&file_node.digest);
    let mut explicit_directory_as_root = version.preimage.clone();
    explicit_directory_as_root[13..45].copy_from_slice(&explicit.digest);
    let mut version_trailing = version.preimage.clone();
    version_trailing.push(0);
    let mut version_domain = version.preimage.clone();
    version_domain[0] ^= 1;

    vec![
        StructuralHostile {
            name: "structural_chunk_unknown_schema",
            bytes: chunk_schema,
            expected: "S_SCHEMA",
            object_type: StructuralType::LogicalChunk,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_chunk_length_mismatch",
            bytes: chunk_length,
            expected: "S_TRUNCATED",
            object_type: StructuralType::LogicalChunk,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_chunk_trailing_byte",
            bytes: chunk_trailing,
            expected: "S_EXACT_EOF",
            object_type: StructuralType::LogicalChunk,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_logical_file_length_mismatch",
            bytes: logical_file_sum,
            expected: "S_LOGICAL_LENGTH",
            object_type: StructuralType::LogicalFile,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_logical_file_chunk_declared_length_mismatch",
            bytes: logical_file_chunk_length,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::LogicalFile,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_logical_file_wrong_typed_edge",
            bytes: logical_file_edge,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::LogicalFile,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_logical_file_trailing_byte",
            bytes: logical_file_trailing,
            expected: "S_EXACT_EOF",
            object_type: StructuralType::LogicalFile,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_file_mode_sentinel",
            bytes: file_mode,
            expected: "S_FILE_MODE",
            object_type: StructuralType::FileNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_file_wrong_typed_edge",
            bytes: file_edge,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::FileNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_file_length_mismatch",
            bytes: file_length,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::FileNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_file_trailing_byte",
            bytes: file_trailing,
            expected: "S_EXACT_EOF",
            object_type: StructuralType::FileNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_symlink_invalid_utf8",
            bytes: symlink_utf8,
            expected: "S_TARGET",
            object_type: StructuralType::SymlinkNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_symlink_embedded_nul",
            bytes: symlink_nul,
            expected: "S_TARGET",
            object_type: StructuralType::SymlinkNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_symlink_empty",
            bytes: symlink_empty,
            expected: "S_TARGET",
            object_type: StructuralType::SymlinkNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_symlink_trailing_byte",
            bytes: symlink_trailing,
            expected: "S_EXACT_EOF",
            object_type: StructuralType::SymlinkNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_root_without_sentinel",
            bytes: root_normal_mode,
            expected: "S_ROOT_SENTINEL",
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
        },
        StructuralHostile {
            name: "structural_child_with_sentinel",
            bytes: child_sentinel,
            expected: "S_CHILD_MODE",
            object_type: StructuralType::DirectoryNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_directory_invalid_utf8",
            bytes: directory_utf8,
            expected: "S_NAME",
            object_type: StructuralType::DirectoryNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_directory_embedded_nul",
            bytes: directory_nul,
            expected: "S_NAME",
            object_type: StructuralType::DirectoryNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_unknown_child_kind",
            bytes: unknown_kind,
            expected: "S_UNKNOWN_KIND",
            object_type: StructuralType::DirectoryNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_directory_wrong_typed_edge",
            bytes: directory_edge,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::DirectoryNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_implicit_root_rejected_as_explicit_child",
            bytes: implicit_root_as_child,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::DirectoryNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_directory_duplicate_name",
            bytes: duplicate_directory,
            expected: "S_ORDER_DUPLICATE",
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
        },
        StructuralHostile {
            name: "structural_directory_descending_name",
            bytes: descending_directory,
            expected: "S_ORDER_DUPLICATE",
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
        },
        StructuralHostile {
            name: "structural_directory_dot_name",
            bytes: dot_directory,
            expected: "S_NAME",
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
        },
        StructuralHostile {
            name: "structural_directory_trailing_byte",
            bytes: trailing,
            expected: "S_EXACT_EOF",
            object_type: StructuralType::DirectoryNode,
            implicit_root: true,
        },
        StructuralHostile {
            name: "structural_truncated_child_id",
            bytes: truncated,
            expected: "S_TRUNCATED",
            object_type: StructuralType::DirectoryNode,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_version_wrong_typed_edge",
            bytes: version_edge,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::VersionRoot,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_explicit_directory_rejected_as_version_root",
            bytes: explicit_directory_as_root,
            expected: "S_TYPED_EDGE",
            object_type: StructuralType::VersionRoot,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_version_trailing_byte",
            bytes: version_trailing,
            expected: "S_EXACT_EOF",
            object_type: StructuralType::VersionRoot,
            implicit_root: false,
        },
        StructuralHostile {
            name: "structural_version_wrong_domain",
            bytes: version_domain,
            expected: "S_TYPE_DOMAIN",
            object_type: StructuralType::VersionRoot,
            implicit_root: false,
        },
    ]
}

fn hash_frame(tag: u8, payload: &[u8]) -> [u8; 32] {
    let mut frame = Vec::with_capacity(20 + payload.len());
    frame.extend_from_slice(b"ELSHASH1");
    frame.push(tag);
    frame.push(0);
    push_be_u16(&mut frame, 0);
    push_be_u64(&mut frame, payload.len() as u64);
    frame.extend_from_slice(payload);
    digest(&frame)
}

fn physical_object(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut object = Vec::with_capacity(52 + payload.len());
    object.extend_from_slice(b"ELSOBJ01");
    push_be_u16(&mut object, 1);
    object.push(kind);
    object.push(0);
    object.extend_from_slice(&[0x44; 32]);
    push_be_u64(&mut object, payload.len() as u64);
    object.extend_from_slice(payload);
    object
}

fn object_id(kind: u8, object: &[u8]) -> Result<[u8; 32], &'static str> {
    let tag = match kind {
        0x01 => 0x11,
        0x02 => 0x12,
        0x03 => 0x13,
        0x04 => 0x14,
        0x05 => 0x10,
        _ => return Err("P_OBJECT_KIND"),
    };
    Ok(hash_frame(tag, object))
}

fn record_bytes(object: &[u8]) -> usize {
    let unpadded = 4usize.checked_add(object.len()).expect("record length");
    unpadded + ((8 - (unpadded % 8)) % 8)
}

fn build_pack(objects: &[Vec<u8>]) -> PackVector {
    assert!(!objects.is_empty());
    let mut next_offset = PACK_HEADER_BYTES;
    let mut metadata = Vec::with_capacity(objects.len());
    for object in objects {
        let kind = object[10];
        metadata.push(PackMetadata {
            kind,
            id: object_id(kind, object).expect("supported object kind"),
            absolute_offset: next_offset as u64,
            object_len: object.len() as u32,
            object_checksum: hash_frame(0x21, object),
        });
        next_offset = next_offset
            .checked_add(record_bytes(object))
            .expect("pack record bytes");
    }
    let index_offset = next_offset;
    let index_len = objects
        .len()
        .checked_mul(PACK_INDEX_ENTRY_BYTES)
        .expect("index bytes");
    let pack_len = index_offset
        .checked_add(index_len)
        .and_then(|value| value.checked_add(PACK_TRAILER_BYTES))
        .expect("pack bytes");
    assert!(pack_len <= MAX_PACK_BYTES);

    let mut pack = Vec::with_capacity(pack_len);
    pack.extend_from_slice(b"ELSPACK1");
    push_be_u16(&mut pack, 1);
    push_be_u16(&mut pack, PACK_HEADER_BYTES as u16);
    push_be_u32(&mut pack, 0);
    pack.extend_from_slice(&[0x44; 32]);
    push_be_u32(&mut pack, objects.len() as u32);
    push_be_u16(&mut pack, PACK_INDEX_ENTRY_BYTES as u16);
    push_be_u16(&mut pack, 0);
    push_be_u64(&mut pack, index_offset as u64);
    assert_eq!(pack.len(), PACK_HEADER_BYTES);

    for object in objects {
        push_be_u32(&mut pack, object.len() as u32);
        pack.extend_from_slice(object);
        while pack.len() % 8 != 0 {
            pack.push(0);
        }
    }
    assert_eq!(pack.len(), index_offset);

    let mut sorted = metadata.clone();
    sorted.sort_by(|left, right| (left.kind, left.id).cmp(&(right.kind, right.id)));
    for entry in &sorted {
        pack.push(entry.kind);
        pack.push(0);
        push_be_u16(&mut pack, 0);
        pack.extend_from_slice(&entry.id);
        push_be_u64(&mut pack, entry.absolute_offset);
        push_be_u32(&mut pack, entry.object_len);
        pack.extend_from_slice(&entry.object_checksum);
    }

    pack.extend_from_slice(b"ELSPEND1");
    push_be_u16(&mut pack, 1);
    push_be_u16(&mut pack, PACK_TRAILER_BYTES as u16);
    push_be_u32(&mut pack, 0);
    push_be_u64(&mut pack, pack_len as u64);
    push_be_u64(&mut pack, index_offset as u64);
    push_be_u64(&mut pack, index_len as u64);
    push_be_u32(&mut pack, objects.len() as u32);
    push_be_u32(&mut pack, 0);
    let checksum = hash_frame(0x20, &pack);
    pack.extend_from_slice(&checksum);
    assert_eq!(pack.len(), pack_len);

    PackVector {
        name: "",
        bytes: pack,
        physical_keys: metadata
            .iter()
            .map(|entry| (entry.kind, entry.id))
            .collect(),
        index_keys: sorted.iter().map(|entry| (entry.kind, entry.id)).collect(),
    }
}

fn validate_physical_object(
    object: &[u8],
    expected_profile: &[u8],
) -> Result<(u8, [u8; 32]), &'static str> {
    if object.len() < 52 {
        return Err("P_OBJECT_TRUNCATED");
    }
    if take(object, 0, 8)? != b"ELSOBJ01" {
        return Err("P_OBJECT_MAGIC");
    }
    if read_be_u16(object, 8)? != 1 {
        return Err("P_OBJECT_SCHEMA");
    }
    let kind = object[10];
    if object[11] != 0 {
        return Err("P_OBJECT_RESERVED");
    }
    if take(object, 12, 32)? != expected_profile {
        return Err("P_OBJECT_PROFILE");
    }
    let payload_len =
        usize::try_from(read_be_u64(object, 44)?).map_err(|_| "P_INTEGER_OVERFLOW")?;
    if 52usize
        .checked_add(payload_len)
        .ok_or("P_INTEGER_OVERFLOW")?
        != object.len()
    {
        return Err("P_OBJECT_EXACT_EOF");
    }
    if kind == 0x05 && !(1..=32_768).contains(&payload_len) {
        return Err("P_CHUNK_LENGTH");
    }
    let id = object_id(kind, object)?;
    Ok((kind, id))
}

fn validate_pack(bytes: &[u8]) -> Result<&'static str, &'static str> {
    if bytes.len() > MAX_PACK_BYTES {
        return Err("P_PACK_CAP");
    }
    if bytes.len() < PACK_HEADER_BYTES + PACK_TRAILER_BYTES {
        return Err("P_TRUNCATED");
    }
    if take(bytes, 0, 8)? != b"ELSPACK1" {
        return Err("P_HEADER_MAGIC");
    }
    if read_be_u16(bytes, 8)? != 1 || read_be_u16(bytes, 10)? != 64 {
        return Err("P_HEADER_SCHEMA_LENGTH");
    }
    if read_be_u32(bytes, 12)? != 0 || read_be_u16(bytes, 54)? != 0 {
        return Err("P_HEADER_RESERVED");
    }
    let profile = take(bytes, 16, 32)?;
    let record_count = read_be_u32(bytes, 48)? as usize;
    if record_count == 0 || record_count > PACK_RECORDS_MAX {
        return Err("P_RECORD_COUNT");
    }
    if read_be_u16(bytes, 52)? as usize != PACK_INDEX_ENTRY_BYTES {
        return Err("P_INDEX_ENTRY_SIZE");
    }
    let index_offset =
        usize::try_from(read_be_u64(bytes, 56)?).map_err(|_| "P_INTEGER_OVERFLOW")?;
    let index_len = record_count
        .checked_mul(PACK_INDEX_ENTRY_BYTES)
        .ok_or("P_INTEGER_OVERFLOW")?;
    let trailer_at = index_offset
        .checked_add(index_len)
        .ok_or("P_INTEGER_OVERFLOW")?;
    let computed_pack_len = trailer_at
        .checked_add(PACK_TRAILER_BYTES)
        .ok_or("P_INTEGER_OVERFLOW")?;
    if computed_pack_len != bytes.len() {
        return Err("P_EXACT_EOF");
    }
    if index_offset < PACK_HEADER_BYTES || trailer_at > bytes.len() {
        return Err("P_INDEX_LENGTH");
    }
    if take(bytes, trailer_at, 8)? != b"ELSPEND1"
        || read_be_u16(bytes, trailer_at + 8)? != 1
        || read_be_u16(bytes, trailer_at + 10)? as usize != PACK_TRAILER_BYTES
    {
        return Err("P_TRAILER_SCHEMA_LENGTH");
    }
    if read_be_u32(bytes, trailer_at + 12)? != 0 || read_be_u32(bytes, trailer_at + 44)? != 0 {
        return Err("P_TRAILER_RESERVED");
    }
    if read_be_u64(bytes, trailer_at + 16)? as usize != bytes.len()
        || read_be_u64(bytes, trailer_at + 24)? as usize != index_offset
        || read_be_u64(bytes, trailer_at + 32)? as usize != index_len
        || read_be_u32(bytes, trailer_at + 40)? as usize != record_count
    {
        return Err("P_TRAILER_FIELDS");
    }
    let expected_pack_checksum = hash_frame(0x20, take(bytes, 0, bytes.len() - 32)?);
    if take(bytes, bytes.len() - 32, 32)? != expected_pack_checksum {
        return Err("P_PACK_CHECKSUM");
    }

    let mut metadata = Vec::with_capacity(record_count);
    let mut previous_key: Option<(u8, [u8; 32])> = None;
    for ordinal in 0..record_count {
        let at = index_offset
            .checked_add(
                ordinal
                    .checked_mul(PACK_INDEX_ENTRY_BYTES)
                    .ok_or("P_INTEGER_OVERFLOW")?,
            )
            .ok_or("P_INTEGER_OVERFLOW")?;
        let kind = bytes[at];
        if object_id(kind, b"").is_err() {
            return Err("P_OBJECT_KIND");
        }
        if bytes[at + 1] != 0 || read_be_u16(bytes, at + 2)? != 0 {
            return Err("P_INDEX_RESERVED");
        }
        let id = array32(bytes, at + 4)?;
        let key = (kind, id);
        if previous_key
            .as_ref()
            .is_some_and(|previous| previous >= &key)
        {
            return Err("P_INDEX_ORDER_DUPLICATE");
        }
        previous_key = Some(key);
        metadata.push(PackMetadata {
            kind,
            id,
            absolute_offset: read_be_u64(bytes, at + 36)?,
            object_len: read_be_u32(bytes, at + 44)?,
            object_checksum: array32(bytes, at + 48)?,
        });
    }

    metadata.sort_by_key(|entry| entry.absolute_offset);
    if metadata
        .windows(2)
        .any(|pair| pair[0].absolute_offset >= pair[1].absolute_offset)
    {
        return Err("P_RECORD_BIJECTION");
    }
    let mut cursor = PACK_HEADER_BYTES;
    for entry in &metadata {
        let offset = usize::try_from(entry.absolute_offset).map_err(|_| "P_INTEGER_OVERFLOW")?;
        if offset != cursor {
            return Err("P_RECORD_BIJECTION");
        }
        let prefix_len = read_be_u32(bytes, cursor)? as usize;
        if prefix_len != entry.object_len as usize {
            return Err("P_RECORD_LENGTH");
        }
        let object_at = cursor.checked_add(4).ok_or("P_INTEGER_OVERFLOW")?;
        let object = take(bytes, object_at, prefix_len)?;
        let (kind, id) = validate_physical_object(object, profile)?;
        if kind != entry.kind || id != entry.id {
            return Err("P_TYPED_ID");
        }
        if hash_frame(0x21, object) != entry.object_checksum {
            return Err("P_OBJECT_CHECKSUM");
        }
        let unpadded_end = object_at
            .checked_add(prefix_len)
            .ok_or("P_INTEGER_OVERFLOW")?;
        let padded_end = cursor
            .checked_add(record_bytes(object))
            .ok_or("P_INTEGER_OVERFLOW")?;
        if padded_end > index_offset
            || take(bytes, unpadded_end, padded_end - unpadded_end)?
                .iter()
                .any(|byte| *byte != 0)
        {
            return Err("P_PADDING");
        }
        cursor = padded_end;
    }
    if cursor != index_offset {
        return Err("P_RECORD_BIJECTION");
    }
    Ok("PACK_ACCEPTED")
}

fn write_be_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_be_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_be_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_be_bytes());
}

fn reseal_pack(bytes: &mut [u8]) {
    let checksum_at = bytes.len() - 32;
    let checksum = hash_frame(0x20, &bytes[..checksum_at]);
    bytes[checksum_at..].copy_from_slice(&checksum);
}

fn pack_vectors() -> Vec<PackVector> {
    let mut minimal = build_pack(&[physical_object(0x05, &[0])]);
    minimal.name = "pack_minimal_one_chunk";

    let mut objects = vec![physical_object(0x05, &[0]), physical_object(0x05, &[1])];
    objects.sort_by(|left, right| {
        object_id(left[10], left)
            .expect("id")
            .cmp(&object_id(right[10], right).expect("id"))
            .reverse()
    });
    let mut discovery = build_pack(&objects);
    discovery.name = "pack_discovery_order_differs_from_index_order";
    assert_ne!(discovery.physical_keys, discovery.index_keys);
    assert!(discovery.physical_keys[0] > discovery.physical_keys[1]);
    assert!(discovery.index_keys[0] < discovery.index_keys[1]);
    vec![minimal, discovery]
}

fn pack_hostiles(valid: &[PackVector]) -> Vec<PackHostile> {
    let base = &valid[1].bytes;
    let index_at = read_be_u64(base, 56).expect("index offset") as usize;
    let mut vectors = Vec::new();

    let mut object_start = base.clone();
    let offset = read_be_u64(&object_start, index_at + 36).expect("offset");
    write_be_u64(&mut object_start, index_at + 36, offset + 4);
    reseal_pack(&mut object_start);
    vectors.push(PackHostile {
        name: "pack_index_object_start_offset",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "first index absolute_offset += 4 (object start instead of record-length prefix); reseal",
        bytes: object_start,
        expected: "P_RECORD_BIJECTION",
    });

    let mut duplicate = base.clone();
    duplicate[index_at + PACK_INDEX_ENTRY_BYTES] = duplicate[index_at];
    let first_id = duplicate[index_at + 4..index_at + 36].to_vec();
    duplicate[index_at + PACK_INDEX_ENTRY_BYTES + 4..index_at + PACK_INDEX_ENTRY_BYTES + 36]
        .copy_from_slice(&first_id);
    reseal_pack(&mut duplicate);
    vectors.push(PackHostile {
        name: "pack_duplicate_typed_index_key",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "second typed index key = first typed index key; reseal",
        bytes: duplicate,
        expected: "P_INDEX_ORDER_DUPLICATE",
    });

    let mut descending = base.clone();
    let first = descending[index_at..index_at + PACK_INDEX_ENTRY_BYTES].to_vec();
    let second = descending
        [index_at + PACK_INDEX_ENTRY_BYTES..index_at + 2 * PACK_INDEX_ENTRY_BYTES]
        .to_vec();
    descending[index_at..index_at + PACK_INDEX_ENTRY_BYTES].copy_from_slice(&second);
    descending[index_at + PACK_INDEX_ENTRY_BYTES..index_at + 2 * PACK_INDEX_ENTRY_BYTES]
        .copy_from_slice(&first);
    reseal_pack(&mut descending);
    vectors.push(PackHostile {
        name: "pack_descending_index",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "swap the two complete sorted index entries; reseal",
        bytes: descending,
        expected: "P_INDEX_ORDER_DUPLICATE",
    });

    let mut overlap = base.clone();
    let first_offset = read_be_u64(&overlap, index_at + 36).expect("offset");
    let second_offset =
        read_be_u64(&overlap, index_at + PACK_INDEX_ENTRY_BYTES + 36).expect("offset");
    let later_entry = if first_offset > second_offset {
        index_at
    } else {
        index_at + PACK_INDEX_ENTRY_BYTES
    };
    write_be_u64(&mut overlap, later_entry + 36, PACK_HEADER_BYTES as u64);
    reseal_pack(&mut overlap);
    vectors.push(PackHostile {
        name: "pack_overlapping_record_offsets",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "later physical record absolute_offset -> 64; reseal",
        bytes: overlap,
        expected: "P_RECORD_BIJECTION",
    });

    let mut prefix = base.clone();
    let original_len = read_be_u32(&prefix, PACK_HEADER_BYTES).expect("object len");
    write_be_u32(&mut prefix, PACK_HEADER_BYTES, original_len + 1);
    reseal_pack(&mut prefix);
    vectors.push(PackHostile {
        name: "pack_record_length_prefix_mismatch",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "first physical object_len prefix += 1; reseal",
        bytes: prefix,
        expected: "P_RECORD_LENGTH",
    });

    let mut unknown_kind = base.clone();
    unknown_kind[PACK_HEADER_BYTES + 4 + 10] = 0xff;
    reseal_pack(&mut unknown_kind);
    vectors.push(PackHostile {
        name: "pack_unknown_physical_object_kind",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "first physical ELSOBJ01 kind -> ff; reseal",
        bytes: unknown_kind,
        expected: "P_OBJECT_KIND",
    });

    let mut padding = base.clone();
    let object_len = read_be_u32(&padding, PACK_HEADER_BYTES).expect("object len") as usize;
    padding[PACK_HEADER_BYTES + 4 + object_len] = 1;
    reseal_pack(&mut padding);
    vectors.push(PackHostile {
        name: "pack_nonzero_minimum_padding",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "first minimum-padding byte 00 -> 01; reseal",
        bytes: padding,
        expected: "P_PADDING",
    });

    let mut object_checksum = base.clone();
    object_checksum[index_at + 48] ^= 1;
    reseal_pack(&mut object_checksum);
    vectors.push(PackHostile {
        name: "pack_object_checksum_mismatch",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "first index object_checksum byte ^= 1; reseal pack",
        bytes: object_checksum,
        expected: "P_OBJECT_CHECKSUM",
    });

    let mut auth = base.clone();
    let last = auth.len() - 1;
    auth[last] ^= 1;
    vectors.push(PackHostile {
        name: "pack_authentication_mismatch",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "final pack_checksum byte ^= 1",
        bytes: auth,
        expected: "P_PACK_CHECKSUM",
    });

    let mut trailing = base.clone();
    trailing.push(0);
    vectors.push(PackHostile {
        name: "pack_trailing_byte",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "append 00 after declared pack EOF",
        bytes: trailing,
        expected: "P_EXACT_EOF",
    });

    let mut omitted = base.clone();
    omitted.drain(index_at + PACK_INDEX_ENTRY_BYTES..index_at + 2 * PACK_INDEX_ENTRY_BYTES);
    write_be_u32(&mut omitted, 48, 1);
    let omitted_trailer = index_at + PACK_INDEX_ENTRY_BYTES;
    let omitted_len = omitted.len() as u64;
    write_be_u64(&mut omitted, omitted_trailer + 16, omitted_len);
    write_be_u64(
        &mut omitted,
        omitted_trailer + 32,
        PACK_INDEX_ENTRY_BYTES as u64,
    );
    write_be_u32(&mut omitted, omitted_trailer + 40, 1);
    reseal_pack(&mut omitted);
    vectors.push(PackHostile {
        name: "pack_physical_record_omitted_from_index",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "remove one index entry; set header/trailer count=1, index_len=80, pack_len exact; reseal",
        bytes: omitted,
        expected: "P_RECORD_BIJECTION",
    });

    let minimal = &valid[0].bytes;
    let minimal_index = read_be_u64(minimal, 56).expect("minimal index") as usize;
    let mut extra = minimal.clone();
    let mut extra_entry = extra[minimal_index..minimal_index + PACK_INDEX_ENTRY_BYTES].to_vec();
    extra_entry[0] = 0x04;
    extra_entry[1] = 0;
    extra_entry[2] = 0;
    extra_entry[3] = 0;
    extra_entry[4..36].fill(0);
    extra.splice(minimal_index..minimal_index, extra_entry);
    write_be_u32(&mut extra, 48, 2);
    let extra_trailer = minimal_index + 2 * PACK_INDEX_ENTRY_BYTES;
    let extra_len = extra.len() as u64;
    write_be_u64(&mut extra, extra_trailer + 16, extra_len);
    write_be_u64(
        &mut extra,
        extra_trailer + 32,
        (2 * PACK_INDEX_ENTRY_BYTES) as u64,
    );
    write_be_u32(&mut extra, extra_trailer + 40, 2);
    reseal_pack(&mut extra);
    vectors.push(PackHostile {
        name: "pack_extra_index_entry_without_physical_record",
        base: "pack_minimal_one_chunk",
        mutation: "insert sorted kind-04 index entry sharing the physical offset; set count/index_len/pack_len; reseal",
        bytes: extra,
        expected: "P_RECORD_BIJECTION",
    });

    for (name, at, value, mutation) in [
        (
            "pack_trailer_record_count_disagrees",
            index_at + 2 * PACK_INDEX_ENTRY_BYTES + 40,
            3u64,
            "trailer record_count 2 -> 3; reseal",
        ),
        (
            "pack_trailer_index_offset_disagrees",
            index_at + 2 * PACK_INDEX_ENTRY_BYTES + 24,
            (index_at + 8) as u64,
            "trailer index_offset += 8; reseal",
        ),
        (
            "pack_trailer_index_length_disagrees",
            index_at + 2 * PACK_INDEX_ENTRY_BYTES + 32,
            (3 * PACK_INDEX_ENTRY_BYTES) as u64,
            "trailer index_len 160 -> 240; reseal",
        ),
    ] {
        let mut bytes = base.clone();
        if name == "pack_trailer_record_count_disagrees" {
            write_be_u32(&mut bytes, at, value as u32);
        } else {
            write_be_u64(&mut bytes, at, value);
        }
        reseal_pack(&mut bytes);
        vectors.push(PackHostile {
            name,
            base: "pack_discovery_order_differs_from_index_order",
            mutation,
            bytes,
            expected: "P_TRAILER_FIELDS",
        });
    }

    let mut index_position = base.clone();
    write_be_u64(&mut index_position, 56, PACK_HEADER_BYTES as u64);
    reseal_pack(&mut index_position);
    vectors.push(PackHostile {
        name: "pack_index_position_overlaps_physical_records",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "header index_offset -> 64 without relocating bytes; reseal",
        bytes: index_position,
        expected: "P_EXACT_EOF",
    });

    let mut trailer_position = base.clone();
    trailer_position.insert(index_at + 2 * PACK_INDEX_ENTRY_BYTES, 0);
    vectors.push(PackHostile {
        name: "pack_trailer_position_has_unindexed_gap",
        base: "pack_discovery_order_differs_from_index_order",
        mutation: "insert 00 between index and trailer",
        bytes: trailer_position,
        expected: "P_EXACT_EOF",
    });

    let offsets = (0..2)
        .map(|ordinal| {
            read_be_u64(base, index_at + ordinal * PACK_INDEX_ENTRY_BYTES + 36)
                .expect("index offset")
        })
        .collect::<Vec<_>>();
    let later = if offsets[0] > offsets[1] { 0 } else { 1 };
    let later_at = index_at + later * PACK_INDEX_ENTRY_BYTES + 36;
    for (name, replacement, mutation) in [
        (
            "pack_offset_relative_to_record_region",
            offsets[later] - PACK_HEADER_BYTES as u64,
            "later absolute_offset encoded relative to byte 64; reseal",
        ),
        (
            "pack_offset_relative_to_index",
            offsets[later] + index_at as u64,
            "later absolute_offset incorrectly adds index origin; reseal",
        ),
        (
            "pack_offset_one_based",
            offsets[later] + 1,
            "later absolute_offset += 1; reseal",
        ),
    ] {
        let mut bytes = base.clone();
        write_be_u64(&mut bytes, later_at, replacement);
        reseal_pack(&mut bytes);
        vectors.push(PackHostile {
            name,
            base: "pack_discovery_order_differs_from_index_order",
            mutation,
            bytes,
            expected: "P_RECORD_BIJECTION",
        });
    }
    vectors
}

#[derive(Clone)]
struct JournalFields {
    frame_type: u8,
    operation: u8,
    flags: u32,
    profile: [u8; 32],
    transaction_id: [u8; 16],
    sequence: u64,
    payload: Vec<u8>,
}

struct JournalFrame {
    frame_type: u8,
    profile: [u8; 32],
    transaction_id: [u8; 16],
    sequence: u64,
    payload: Vec<u8>,
}

fn journal_intent_payload(
    old_generation: u64,
    old_catalog: [u8; 32],
    new_generation: u64,
    new_catalog: [u8; 32],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(80);
    push_be_u64(&mut payload, old_generation);
    payload.extend_from_slice(&old_catalog);
    push_be_u64(&mut payload, new_generation);
    payload.extend_from_slice(&new_catalog);
    assert_eq!(payload.len(), 80);
    payload
}

fn journal_outcome_payload(
    result: u8,
    selected_generation: u64,
    selected_catalog: [u8; 32],
    error_code: u16,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(43);
    payload.push(result);
    push_be_u64(&mut payload, selected_generation);
    payload.extend_from_slice(&selected_catalog);
    push_be_u16(&mut payload, error_code);
    assert_eq!(payload.len(), 43);
    payload
}

fn encode_journal_frame(fields: &JournalFields) -> Vec<u8> {
    let mut bytes =
        Vec::with_capacity(JOURNAL_HEADER_BYTES + fields.payload.len() + JOURNAL_CHECKSUM_BYTES);
    bytes.extend_from_slice(b"ELSJRN01");
    push_be_u16(&mut bytes, 1);
    bytes.push(fields.frame_type);
    bytes.push(fields.operation);
    push_be_u32(&mut bytes, fields.flags);
    bytes.extend_from_slice(&fields.profile);
    bytes.extend_from_slice(&fields.transaction_id);
    push_be_u64(&mut bytes, fields.sequence);
    push_be_u32(&mut bytes, fields.payload.len() as u32);
    assert_eq!(bytes.len(), JOURNAL_HEADER_BYTES);
    bytes.extend_from_slice(&fields.payload);
    let checksum = hash_frame(0x24, &bytes);
    bytes.extend_from_slice(&checksum);
    bytes
}

fn reseal_journal(bytes: &mut [u8]) {
    let checksum_at = bytes.len() - JOURNAL_CHECKSUM_BYTES;
    let checksum = hash_frame(0x24, &bytes[..checksum_at]);
    bytes[checksum_at..].copy_from_slice(&checksum);
}

fn parse_journal_envelope(bytes: &[u8]) -> Result<JournalFrame, &'static str> {
    if bytes.len() < JOURNAL_HEADER_BYTES + JOURNAL_CHECKSUM_BYTES {
        return Err("J_TRUNCATED");
    }
    if take(bytes, 0, 8)? != b"ELSJRN01" {
        return Err("J_MAGIC");
    }
    if read_be_u16(bytes, 8)? != 1 {
        return Err("J_SCHEMA");
    }
    let frame_type = bytes[10];
    if !matches!(frame_type, 1 | 2) {
        return Err("J_FRAME_TYPE");
    }
    if bytes[11] != 1 {
        return Err("J_OPERATION");
    }
    if read_be_u32(bytes, 12)? != 0 {
        return Err("J_FLAGS");
    }
    let profile = array32(bytes, 16)?;
    let mut transaction_id = [0u8; 16];
    transaction_id.copy_from_slice(take(bytes, 48, 16)?);
    let sequence = read_be_u64(bytes, 64)?;
    let payload_len = read_be_u32(bytes, 72)? as usize;
    if payload_len > JOURNAL_PAYLOAD_MAX {
        return Err("J_PAYLOAD_CAP");
    }
    let expected_len = JOURNAL_HEADER_BYTES
        .checked_add(payload_len)
        .and_then(|value| value.checked_add(JOURNAL_CHECKSUM_BYTES))
        .ok_or("J_INTEGER_OVERFLOW")?;
    match bytes.len().cmp(&expected_len) {
        Ordering::Less => return Err("J_TRUNCATED"),
        Ordering::Greater => return Err("J_TRAILING"),
        Ordering::Equal => {}
    }
    let checksum_at = JOURNAL_HEADER_BYTES + payload_len;
    let expected_checksum = hash_frame(0x24, take(bytes, 0, checksum_at)?);
    if take(bytes, checksum_at, JOURNAL_CHECKSUM_BYTES)? != expected_checksum {
        return Err("J_CHECKSUM");
    }
    Ok(JournalFrame {
        frame_type,
        profile,
        transaction_id,
        sequence,
        payload: take(bytes, JOURNAL_HEADER_BYTES, payload_len)?.to_vec(),
    })
}

fn validate_journal_frame(bytes: &[u8]) -> Result<&'static str, &'static str> {
    let frame = parse_journal_envelope(bytes)?;
    match frame.frame_type {
        1 => {
            if frame.sequence != 0 {
                return Err("J_SEQUENCE");
            }
            if frame.payload.len() != 80 {
                return Err("J_INTENT_PAYLOAD_SHAPE");
            }
            let old_generation = read_be_u64(&frame.payload, 0)?;
            let new_generation = read_be_u64(&frame.payload, 40)?;
            if old_generation.checked_add(1) != Some(new_generation) {
                return Err("J_INTENT_GENERATION");
            }
            Ok("J_INTENT_ACCEPTED")
        }
        2 => {
            if frame.sequence != 1 {
                return Err("J_SEQUENCE");
            }
            if frame.payload.len() != 43 {
                return Err("J_OUTCOME_PAYLOAD_SHAPE");
            }
            if !matches!(frame.payload[0], 1 | 2 | 3) {
                return Err("J_RESULT");
            }
            Ok("J_OUTCOME_ACCEPTED")
        }
        _ => unreachable!("envelope rejects unknown frame types"),
    }
}

fn validate_journal_transaction(frames: &[Vec<u8>]) -> Result<&'static str, &'static str> {
    let mut intent: Option<JournalFrame> = None;
    let mut outcome: Option<JournalFrame> = None;
    let mut sequences = BTreeSet::new();
    for bytes in frames {
        let parsed = parse_journal_envelope(bytes)?;
        validate_journal_frame(bytes)?;
        if !sequences.insert(parsed.sequence) {
            return Err("J_DUPLICATE_SEQUENCE");
        }
        match parsed.frame_type {
            1 => intent = Some(parsed),
            2 => {
                if intent.is_none() {
                    return Err("J_OUTCOME_WITHOUT_INTENT");
                }
                outcome = Some(parsed);
            }
            _ => unreachable!(),
        }
    }
    let intent = intent.ok_or("J_INTENT_MISSING")?;
    let Some(outcome) = outcome else {
        return Ok("J_INTENT_ONLY_RECOVERABLE");
    };
    if intent.profile != outcome.profile {
        return Err("J_PROFILE_MISMATCH");
    }
    if intent.transaction_id != outcome.transaction_id {
        return Err("J_TRANSACTION_MISMATCH");
    }
    let result = outcome.payload[0];
    let selected_generation = read_be_u64(&outcome.payload, 1)?;
    let selected_catalog = array32(&outcome.payload, 9)?;
    let old_generation = read_be_u64(&intent.payload, 0)?;
    let old_catalog = array32(&intent.payload, 8)?;
    let new_generation = read_be_u64(&intent.payload, 40)?;
    let new_catalog = array32(&intent.payload, 48)?;
    if (result == 1 && (selected_generation != new_generation || selected_catalog != new_catalog))
        || (result == 2
            && (selected_generation != old_generation || selected_catalog != old_catalog))
    {
        return Err("J_OUTCOME_CONTRADICTION");
    }
    Ok("J_TRANSACTION_ACCEPTED")
}

fn journal_vectors() -> (Vec<BinaryVector>, Vec<Vec<u8>>) {
    let profile = [0x44; 32];
    let transaction_id = [0x11; 16];
    let intent_fields = JournalFields {
        frame_type: 1,
        operation: 1,
        flags: 0,
        profile,
        transaction_id,
        sequence: 0,
        payload: journal_intent_payload(7, [0x22; 32], 8, [0x33; 32]),
    };
    let outcome_fields = JournalFields {
        frame_type: 2,
        operation: 1,
        flags: 0,
        profile,
        transaction_id,
        sequence: 1,
        payload: journal_outcome_payload(1, 8, [0x33; 32], 0),
    };
    let intent = encode_journal_frame(&intent_fields);
    let outcome = encode_journal_frame(&outcome_fields);
    let mut vectors = vec![
        BinaryVector {
            name: "journal_valid_intent",
            base: "exact ELSJRN01 intent",
            mutation: "none",
            bytes: intent.clone(),
            expected: "J_INTENT_ACCEPTED",
            actual: validate_journal_frame(&intent),
            render_exact: true,
        },
        BinaryVector {
            name: "journal_valid_committed_outcome",
            base: "exact ELSJRN01 outcome",
            mutation: "none",
            bytes: outcome.clone(),
            expected: "J_OUTCOME_ACCEPTED",
            actual: validate_journal_frame(&outcome),
            render_exact: true,
        },
    ];

    let mut push = |name, mutation, bytes: Vec<u8>, expected| {
        let actual = validate_journal_frame(&bytes);
        vectors.push(BinaryVector {
            name,
            base: "journal_valid_intent",
            mutation,
            bytes,
            expected,
            actual,
            render_exact: false,
        });
    };
    let mutate_reseal = |at: usize, value: u8| {
        let mut bytes = intent.clone();
        bytes[at] = value;
        reseal_journal(&mut bytes);
        bytes
    };
    push(
        "journal_bad_magic",
        "magic[0] ^= 1",
        mutate_reseal(0, b'D'),
        "J_MAGIC",
    );
    let mut schema = intent.clone();
    write_be_u16(&mut schema, 8, 2);
    reseal_journal(&mut schema);
    push(
        "journal_bad_schema",
        "schema -> 2; reseal",
        schema,
        "J_SCHEMA",
    );
    push(
        "journal_unknown_type",
        "frame_type -> 3; reseal",
        mutate_reseal(10, 3),
        "J_FRAME_TYPE",
    );
    push(
        "journal_unknown_operation",
        "operation -> 2; reseal",
        mutate_reseal(11, 2),
        "J_OPERATION",
    );
    let mut flags = intent.clone();
    write_be_u32(&mut flags, 12, 1);
    reseal_journal(&mut flags);
    push(
        "journal_nonzero_flags",
        "flags -> 1; reseal",
        flags,
        "J_FLAGS",
    );
    let mut sequence = intent.clone();
    write_be_u64(&mut sequence, 64, 1);
    reseal_journal(&mut sequence);
    push(
        "journal_wrong_sequence",
        "intent sequence -> 1; reseal",
        sequence,
        "J_SEQUENCE",
    );

    for (name, payload_len, expected) in [
        (
            "journal_payload_zero_shape",
            0usize,
            "J_INTENT_PAYLOAD_SHAPE",
        ),
        (
            "journal_payload_4096_shape",
            4_096usize,
            "J_INTENT_PAYLOAD_SHAPE",
        ),
        ("journal_payload_4097_refused", 4_097usize, "J_PAYLOAD_CAP"),
    ] {
        let fields = JournalFields {
            payload: vec![0; payload_len],
            ..intent_fields.clone()
        };
        push(
            name,
            "replace declared payload and reseal",
            encode_journal_frame(&fields),
            expected,
        );
    }

    let mut checksum = intent.clone();
    *checksum.last_mut().expect("checksum byte") ^= 1;
    push(
        "journal_checksum_mismatch",
        "checksum[-1] ^= 1",
        checksum,
        "J_CHECKSUM",
    );
    let mut truncated = intent.clone();
    truncated.pop();
    push(
        "journal_truncation",
        "remove final checksum byte",
        truncated,
        "J_TRUNCATED",
    );
    let mut trailing = intent.clone();
    trailing.push(0);
    push(
        "journal_trailing_byte",
        "append 00 after exact EOF",
        trailing,
        "J_TRAILING",
    );
    let mut unknown_result = outcome.clone();
    unknown_result[JOURNAL_HEADER_BYTES] = 0;
    reseal_journal(&mut unknown_result);
    let unknown_result_actual = validate_journal_frame(&unknown_result);
    vectors.push(BinaryVector {
        name: "journal_unknown_result",
        base: "journal_valid_committed_outcome",
        mutation: "result -> 0; reseal",
        bytes: unknown_result,
        expected: "J_RESULT",
        actual: unknown_result_actual,
        render_exact: false,
    });
    for (name, payload_len) in [
        ("journal_payload_zero_envelope_boundary", 0usize),
        ("journal_payload_4096_envelope_boundary", 4_096usize),
    ] {
        let bytes = encode_journal_frame(&JournalFields {
            payload: vec![0; payload_len],
            ..intent_fields.clone()
        });
        let actual = parse_journal_envelope(&bytes).map(|_| "J_ENVELOPE_ACCEPTED");
        vectors.push(BinaryVector {
            name,
            base: "journal_valid_intent",
            mutation: "replace declared payload and reseal; envelope-only boundary oracle",
            bytes,
            expected: "J_ENVELOPE_ACCEPTED",
            actual,
            render_exact: false,
        });
    }
    (vectors, vec![intent, outcome])
}

#[derive(Clone)]
struct QrecFields {
    reason: u16,
    flags: u16,
    profile: [u8; 32],
    generation: u64,
    catalog: [u8; 32],
    carrier_kind: u8,
    object_kind: u8,
    reserved: u16,
    pack: [u8; 32],
    object: [u8; 32],
    object_offset: u64,
    encoded_len: u32,
    reserved2: u32,
    expected_checksum: [u8; 32],
    observed_checksum: [u8; 32],
    journal_txid: [u8; 16],
}

struct QrecParsed {
    reason: u16,
    profile: [u8; 32],
    carrier_kind: u8,
    object_kind: u8,
    pack: [u8; 32],
    object: [u8; 32],
    object_offset: u64,
    encoded_len: u32,
    expected_checksum: [u8; 32],
    observed_checksum: [u8; 32],
    journal_txid: [u8; 16],
}

fn encode_qrec(fields: &QrecFields) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(QREC_BYTES);
    bytes.extend_from_slice(b"ELSQRN01");
    push_be_u16(&mut bytes, 1);
    push_be_u16(&mut bytes, QREC_BYTES as u16);
    push_be_u16(&mut bytes, fields.reason);
    push_be_u16(&mut bytes, fields.flags);
    bytes.extend_from_slice(&fields.profile);
    push_be_u64(&mut bytes, fields.generation);
    bytes.extend_from_slice(&fields.catalog);
    bytes.push(fields.carrier_kind);
    bytes.push(fields.object_kind);
    push_be_u16(&mut bytes, fields.reserved);
    bytes.extend_from_slice(&fields.pack);
    bytes.extend_from_slice(&fields.object);
    push_be_u64(&mut bytes, fields.object_offset);
    push_be_u32(&mut bytes, fields.encoded_len);
    push_be_u32(&mut bytes, fields.reserved2);
    bytes.extend_from_slice(&fields.expected_checksum);
    bytes.extend_from_slice(&fields.observed_checksum);
    bytes.extend_from_slice(&fields.journal_txid);
    assert_eq!(bytes.len(), QREC_CHECKSUM_AT);
    let checksum = hash_frame(0x25, &bytes);
    bytes.extend_from_slice(&checksum);
    assert_eq!(bytes.len(), QREC_BYTES);
    bytes
}

fn reseal_qrec(bytes: &mut [u8]) {
    let checksum = hash_frame(0x25, &bytes[..QREC_CHECKSUM_AT]);
    bytes[QREC_CHECKSUM_AT..].copy_from_slice(&checksum);
}

fn all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn parse_qrec(bytes: &[u8], carrier_len: Option<u32>) -> Result<QrecParsed, &'static str> {
    if bytes.len() < QREC_BYTES {
        return Err("Q_TRUNCATED");
    }
    if bytes.len() > QREC_BYTES {
        return Err("Q_TRAILING");
    }
    if take(bytes, 0, 8)? != b"ELSQRN01" {
        return Err("Q_MAGIC");
    }
    if read_be_u16(bytes, 8)? != 1 {
        return Err("Q_SCHEMA");
    }
    if read_be_u16(bytes, 10)? as usize != QREC_BYTES {
        return Err("Q_RECORD_LENGTH");
    }
    let reason = read_be_u16(bytes, 12)?;
    if !(1..=5).contains(&reason) {
        return Err("Q_REASON");
    }
    if read_be_u16(bytes, 14)? != 0 {
        return Err("Q_FLAGS");
    }
    let profile = array32(bytes, 16)?;
    let carrier_kind = bytes[88];
    if carrier_kind > 3 {
        return Err("Q_CARRIER_KIND");
    }
    let object_kind = bytes[89];
    if object_kind > 5 {
        return Err("Q_OBJECT_KIND");
    }
    if read_be_u16(bytes, 90)? != 0 || read_be_u32(bytes, 168)? != 0 {
        return Err("Q_RESERVED");
    }
    let pack = array32(bytes, 92)?;
    let object = array32(bytes, 124)?;
    let object_offset = read_be_u64(bytes, 156)?;
    let encoded_len = read_be_u32(bytes, 164)?;
    let expected_carrier_checksum = array32(bytes, 172)?;
    let observed_checksum = array32(bytes, 204)?;
    let mut journal_txid = [0u8; 16];
    journal_txid.copy_from_slice(take(bytes, 236, 16)?);
    let expected_checksum = hash_frame(0x25, take(bytes, 0, QREC_CHECKSUM_AT)?);
    if take(bytes, QREC_CHECKSUM_AT, 32)? != expected_checksum {
        return Err("Q_CHECKSUM");
    }

    match carrier_kind {
        0 | 3 => {
            if object_kind != 0
                || !all_zero(&pack)
                || !all_zero(&object)
                || object_offset != 0
                || encoded_len != 0
            {
                return Err("Q_PRESENCE");
            }
        }
        1 => {
            if object_kind == 0 {
                if !all_zero(&object) || object_offset != 0 || encoded_len != 0 {
                    return Err("Q_PRESENCE");
                }
            } else {
                if object_offset < (PACK_HEADER_BYTES + 4) as u64 || encoded_len < 52 {
                    return Err("Q_OBJECT_EXTENT");
                }
                let end = object_offset
                    .checked_add(encoded_len as u64)
                    .ok_or("Q_INTEGER_OVERFLOW")?;
                if carrier_len.is_some_and(|length| end > length as u64) {
                    return Err("Q_OBJECT_EXTENT");
                }
            }
        }
        2 => {
            if object_kind == 0 && !all_zero(&object) {
                return Err("Q_PRESENCE");
            }
            if object_offset > u32::MAX as u64 {
                return Err("Q_PRIVATE_ORDINAL");
            }
            if encoded_len == 0 || carrier_len.is_some_and(|length| encoded_len != length) {
                return Err("Q_PRIVATE_LENGTH");
            }
        }
        _ => unreachable!(),
    }
    let allowed_scope = match reason {
        1 => matches!(carrier_kind, 1..=3) && object_kind == 0,
        2 => carrier_kind == 2,
        3 => matches!(carrier_kind, 1..=3),
        4 => matches!(carrier_kind, 0 | 3) && object_kind == 0,
        5 => carrier_kind == 1 && object_kind == 1,
        _ => false,
    };
    if !allowed_scope {
        return Err("Q_REASON_SCOPE");
    }
    Ok(QrecParsed {
        reason,
        profile,
        carrier_kind,
        object_kind,
        pack,
        object,
        object_offset,
        encoded_len,
        expected_checksum: expected_carrier_checksum,
        observed_checksum,
        journal_txid,
    })
}

fn validate_qrec(bytes: &[u8], carrier_len: Option<u32>) -> Result<&'static str, &'static str> {
    parse_qrec(bytes, carrier_len).map(|_| "QREC_ACCEPTED")
}

fn qrec_vectors(private_pack: &[u8]) -> (Vec<BinaryVector>, Vec<u8>) {
    assert_eq!(private_pack.len(), 288);
    let private_pack_id = array32(private_pack, private_pack.len() - 32).expect("pack checksum");
    let fields = QrecFields {
        reason: 2,
        flags: 0,
        profile: [0x44; 32],
        generation: 8,
        catalog: [0x33; 32],
        carrier_kind: 2,
        object_kind: 5,
        reserved: 0,
        pack: private_pack_id,
        object: [0x88; 32],
        object_offset: 42,
        encoded_len: private_pack.len() as u32,
        reserved2: 0,
        expected_checksum: [0x99; 32],
        observed_checksum: private_pack_id,
        journal_txid: [0x11; 16],
    };
    let valid = encode_qrec(&fields);
    let mut vectors = vec![BinaryVector {
        name: "qrec_valid_reason2_private_pack_object",
        base: "exact ELSQRN01 reason-2 private-pack record",
        mutation: "none",
        bytes: valid.clone(),
        expected: "QREC_ACCEPTED",
        actual: validate_qrec(&valid, Some(288)),
        render_exact: true,
    }];
    let mut push = |name, mutation, bytes: Vec<u8>, expected, carrier_len| {
        let actual = validate_qrec(&bytes, carrier_len);
        vectors.push(BinaryVector {
            name,
            base: "qrec_valid_reason2_private_pack_object",
            mutation,
            bytes,
            expected,
            actual,
            render_exact: false,
        });
    };
    let mutate_reseal = |at: usize, value: u8| {
        let mut bytes = valid.clone();
        bytes[at] = value;
        reseal_qrec(&mut bytes);
        bytes
    };
    push(
        "qrec_bad_magic",
        "magic[0] ^= 1; reseal",
        mutate_reseal(0, b'D'),
        "Q_MAGIC",
        Some(288),
    );
    let mut schema = valid.clone();
    write_be_u16(&mut schema, 8, 2);
    reseal_qrec(&mut schema);
    push(
        "qrec_bad_schema",
        "schema -> 2; reseal",
        schema,
        "Q_SCHEMA",
        Some(288),
    );
    let mut record_len = valid.clone();
    write_be_u16(&mut record_len, 10, 283);
    reseal_qrec(&mut record_len);
    push(
        "qrec_bad_record_len",
        "record_len -> 283; reseal",
        record_len,
        "Q_RECORD_LENGTH",
        Some(288),
    );
    let mut reason = valid.clone();
    write_be_u16(&mut reason, 12, 6);
    reseal_qrec(&mut reason);
    push(
        "qrec_unknown_reason",
        "reason -> 6; reseal",
        reason,
        "Q_REASON",
        Some(288),
    );
    let mut flags = valid.clone();
    write_be_u16(&mut flags, 14, 1);
    reseal_qrec(&mut flags);
    push(
        "qrec_nonzero_flags",
        "flags -> 1; reseal",
        flags,
        "Q_FLAGS",
        Some(288),
    );
    push(
        "qrec_unknown_carrier",
        "carrier_kind -> 4; reseal",
        mutate_reseal(88, 4),
        "Q_CARRIER_KIND",
        Some(288),
    );
    push(
        "qrec_unknown_object_kind",
        "object_kind -> 6; reseal",
        mutate_reseal(89, 6),
        "Q_OBJECT_KIND",
        Some(288),
    );
    let mut reserved = valid.clone();
    write_be_u16(&mut reserved, 90, 1);
    reseal_qrec(&mut reserved);
    push(
        "qrec_nonzero_reserved",
        "reserved -> 1; reseal",
        reserved,
        "Q_RESERVED",
        Some(288),
    );
    let mut reserved2 = valid.clone();
    write_be_u32(&mut reserved2, 168, 1);
    reseal_qrec(&mut reserved2);
    push(
        "qrec_nonzero_reserved2",
        "reserved2 -> 1; reseal",
        reserved2,
        "Q_RESERVED",
        Some(288),
    );
    let mut absent = encode_qrec(&QrecFields {
        reason: 4,
        carrier_kind: 0,
        object_kind: 0,
        pack: [1; 32],
        object: [0; 32],
        object_offset: 0,
        encoded_len: 0,
        ..fields.clone()
    });
    reseal_qrec(&mut absent);
    push(
        "qrec_absent_pack_field_nonzero",
        "Metadata carrier with nonzero PackId_or_zero",
        absent,
        "Q_PRESENCE",
        None,
    );
    let mut wide_object = valid.clone();
    wide_object[89] = 0;
    reseal_qrec(&mut wide_object);
    push(
        "qrec_carrier_wide_object_field_present",
        "object_kind -> 0 but ObjectId remains present",
        wide_object,
        "Q_PRESENCE",
        Some(288),
    );
    let mut ordinal = valid.clone();
    write_be_u64(&mut ordinal, 156, u32::MAX as u64 + 1);
    reseal_qrec(&mut ordinal);
    push(
        "qrec_private_ordinal_overflow",
        "private ordinal -> u32::MAX+1",
        ordinal,
        "Q_PRIVATE_ORDINAL",
        Some(288),
    );
    let mut private_length = valid.clone();
    write_be_u32(&mut private_length, 164, 287);
    reseal_qrec(&mut private_length);
    push(
        "qrec_private_complete_length_mismatch",
        "encoded_len 287 != retained carrier 288",
        private_length,
        "Q_PRIVATE_LENGTH",
        Some(288),
    );
    let final_fields = QrecFields {
        reason: 3,
        carrier_kind: 1,
        object_kind: 5,
        object_offset: 68,
        encoded_len: 53,
        ..fields.clone()
    };
    let mut final_offset = encode_qrec(&final_fields);
    write_be_u64(&mut final_offset, 156, 67);
    reseal_qrec(&mut final_offset);
    push(
        "qrec_final_object_offset_before_elsobj",
        "object_offset -> 67",
        final_offset,
        "Q_OBJECT_EXTENT",
        Some(288),
    );
    let mut final_extent = encode_qrec(&final_fields);
    write_be_u32(&mut final_extent, 164, 221);
    reseal_qrec(&mut final_extent);
    push(
        "qrec_final_object_extent_past_eof",
        "offset 68 + len 221 > carrier 288",
        final_extent,
        "Q_OBJECT_EXTENT",
        Some(288),
    );
    let mut checksum = valid.clone();
    checksum[QREC_BYTES - 1] ^= 1;
    push(
        "qrec_checksum_mismatch",
        "record_checksum[-1] ^= 1",
        checksum,
        "Q_CHECKSUM",
        Some(288),
    );
    let mut truncated = valid.clone();
    truncated.pop();
    push(
        "qrec_truncation",
        "remove final checksum byte",
        truncated,
        "Q_TRUNCATED",
        Some(288),
    );
    let mut trailing = valid.clone();
    trailing.push(0);
    push(
        "qrec_trailing_byte",
        "append 00 after exact EOF",
        trailing,
        "Q_TRAILING",
        Some(288),
    );
    (vectors, valid)
}

fn lower_hex_exact(value: &str, width: usize) -> bool {
    value.len() == width
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validated_journal_path_class(
    path: &str,
    authenticated_frame: &[u8],
) -> Result<bool, &'static str> {
    if validate_journal_frame(authenticated_frame).is_err() {
        return Err("JOURNAL_FRAME_INVALID");
    }
    let Ok(frame) = parse_journal_envelope(authenticated_frame) else {
        return Err("JOURNAL_FRAME_INVALID");
    };
    let Some(rest) = path.strip_prefix("journal/v1/") else {
        return Err("PATH_PREFIX");
    };
    let Some((profile, file)) = rest.split_once('/') else {
        return Err("PATH_GRAMMAR");
    };
    if !lower_hex_exact(profile, 64) {
        return Err("PATH_PROFILE_LOWERHEX");
    }
    if profile != hex(&frame.profile) {
        return Err("JOURNAL_PROFILE_NAME_MISMATCH");
    }
    let (file, final_path) = match file.strip_suffix(".tmp") {
        Some(file) => (file, false),
        None => (file, true),
    };
    let Some(stem) = file.strip_suffix(".frame") else {
        return Err("PATH_SUFFIX");
    };
    let Some((txid, sequence)) = stem.split_once('-') else {
        return Err("PATH_GRAMMAR");
    };
    if !lower_hex_exact(txid, 32) {
        return Err("PATH_TXID_LOWERHEX");
    }
    if txid != hex(&frame.transaction_id) {
        return Err("JOURNAL_TRANSACTION_NAME_MISMATCH");
    }
    if !lower_hex_exact(sequence, 16) {
        return Err("PATH_SEQUENCE_LOWERHEX");
    }
    let parsed = u64::from_str_radix(sequence, 16).expect("validated hex");
    if parsed > 1 {
        return Err("JOURNAL_SEQUENCE");
    }
    if parsed != frame.sequence {
        return Err("JOURNAL_SEQUENCE_NAME_MISMATCH");
    }
    Ok(final_path)
}

fn journal_path_status(path: &str, authenticated_frame: &[u8]) -> &'static str {
    match validated_journal_path_class(path, authenticated_frame) {
        Ok(_) => "JOURNAL_PATH_ACCEPTED",
        Err(status) => status,
    }
}

fn journal_collision_status(
    path: &str,
    claimed_final_path: bool,
    intended_frame: &[u8],
    occupied_bytes: &[u8],
) -> &'static str {
    let final_path = match validated_journal_path_class(path, intended_frame) {
        Ok(final_path) => final_path,
        Err(status) => return status,
    };
    if claimed_final_path != final_path {
        return "JOURNAL_PATH_CLASS_MISMATCH";
    }
    if validate_journal_frame(occupied_bytes).is_ok() && occupied_bytes == intended_frame {
        if final_path {
            "JOURNAL_FINAL_IDEMPOTENT_REUSE"
        } else {
            "JOURNAL_PRIVATE_RESUME"
        }
    } else {
        "JournalPathCollision"
    }
}

fn quarantine_pair_status(private_path: &str, final_path: &str) -> &'static str {
    let Some(private_rest) = private_path.strip_prefix("quarantine/v1/") else {
        return "QUARANTINE_PRIVATE_GRAMMAR";
    };
    let Some(final_rest) = final_path.strip_prefix("quarantine/v1/") else {
        return "QUARANTINE_FINAL_GRAMMAR";
    };
    let Some((private_group, private_name)) = private_rest.split_once('/') else {
        return "QUARANTINE_PRIVATE_GRAMMAR";
    };
    let Some((final_group, final_name)) = final_rest.split_once('/') else {
        return "QUARANTINE_FINAL_GRAMMAR";
    };
    if private_name.contains('/') || final_name.contains('/') {
        return "QUARANTINE_PATH_DEPTH";
    }
    if !lower_hex_exact(private_group, 32) || !lower_hex_exact(final_group, 32) {
        return "QUARANTINE_GROUP_LOWERHEX";
    }
    if private_group != final_group {
        return "QUARANTINE_GROUP_RELATION";
    }
    let Some(private_ordinal) = private_name
        .strip_prefix('.')
        .and_then(|value| value.strip_suffix(".qrec.tmp"))
    else {
        return "QUARANTINE_PRIVATE_GRAMMAR";
    };
    let Some(final_ordinal) = final_name.strip_suffix(".qrec") else {
        return "QUARANTINE_FINAL_GRAMMAR";
    };
    if !lower_hex_exact(private_ordinal, 8) || !lower_hex_exact(final_ordinal, 8) {
        return "QUARANTINE_ORDINAL_LOWERHEX";
    }
    if private_ordinal != final_ordinal {
        return "QUARANTINE_FINAL_RELATION";
    }
    "QUARANTINE_PATH_PAIR_ACCEPTED"
}

fn private_pack_path_status(path: &str, record: &QrecParsed) -> &'static str {
    if record.reason != 2 || record.carrier_kind != 2 {
        return "Q_REASON2_PRIVATE_CARRIER_REQUIRED";
    }
    let expected = format!(
        "packs/v1/{}/.tmp/{}-{:08x}.pack",
        hex(&record.profile),
        hex(&record.journal_txid),
        record.object_offset,
    );
    if path == expected {
        "Q_REASON2_PRIVATE_PATH_ACCEPTED"
    } else {
        "Q_REASON2_PRIVATE_PATH_RELATION"
    }
}

fn occupied_final_pack_path_status(path: &str, record: &QrecParsed) -> &'static str {
    if record.reason != 2 || record.carrier_kind != 2 {
        return "Q_REASON2_PRIVATE_CARRIER_REQUIRED";
    }
    let expected = format!(
        "packs/v1/{}/{}.pack",
        hex(&record.profile),
        hex(&record.pack)
    );
    if path == expected {
        "Q_REASON2_OCCUPIED_FINAL_PACK_IDENTIFIED"
    } else {
        "Q_REASON2_OCCUPIED_FINAL_PACK_RELATION"
    }
}

fn reason2_private_carrier_status(
    record: &QrecParsed,
    private_pack: Option<&[u8]>,
    custody_owner_count: usize,
) -> &'static str {
    let Some(private_pack) = private_pack else {
        return "Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE";
    };
    if custody_owner_count != 1
        || private_pack.len() != record.encoded_len as usize
        || validate_pack(private_pack).is_err()
    {
        return "Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE";
    }
    let authenticated_pack_id =
        array32(private_pack, private_pack.len() - 32).expect("validated pack trailer");
    if authenticated_pack_id != record.pack
        || authenticated_pack_id != record.observed_checksum
        || record.expected_checksum == record.observed_checksum
    {
        return "Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE";
    }
    "Q_REASON2_EXISTING_PRIVATE_CARRIER_AUTHENTICATED"
}

fn quarantine_collision_status(
    final_path: bool,
    authenticated_284_exact_eof: bool,
    byte_identical: bool,
) -> &'static str {
    if authenticated_284_exact_eof && byte_identical {
        if final_path {
            "QUARANTINE_FINAL_IDEMPOTENT_REUSE"
        } else {
            "QUARANTINE_PRIVATE_RESUME"
        }
    } else {
        "QuarantinePathCollision"
    }
}

#[derive(Clone, Copy)]
enum GateClass {
    CatalogSwitch,
    ReadPin,
    Reclamation,
}

fn gate_trace_status(class: GateClass, trace: &[(&str, bool)]) -> &'static str {
    for (operation, under_gate) in trace {
        if !*under_gate {
            continue;
        }
        let allowed = match class {
            GateClass::CatalogSwitch => matches!(
                *operation,
                "catalog_compare_fixed_old_tuple_epoch"
                    | "catalog_lease_install"
                    | "catalog_lease_validate"
                    | "catalog_lease_clear_select_fixed_old_new_or_authority_unavailable"
            ),
            GateClass::ReadPin => matches!(
                *operation,
                "read_compare_binding_locator" | "read_pin_install" | "read_pin_release"
            ),
            GateClass::Reclamation => matches!(
                *operation,
                "snapshot_pin_install"
                    | "snapshot_pin_release"
                    | "snapshot_generation_epoch_compare_exact_g0_e0"
            ),
        };
        if !allowed {
            return "LIFECYCLE_GATE_EFFECT_FORBIDDEN";
        }
    }
    "LIFECYCLE_GATE_TRACE_ACCEPTED"
}

fn catalog_lease_clear_select_status(
    authenticated_readback: &str,
    caller_supplied_tuple: Option<&str>,
) -> &'static str {
    if caller_supplied_tuple.is_some() {
        return "CALLER_SUPPLIED_CATALOG_TUPLE_FORBIDDEN";
    }
    match authenticated_readback {
        "exact_old" => "FIXED_OLD_SELECTED_LEASE_CLEARED",
        "exact_new" => "FIXED_NEW_SELECTED_LEASE_CLEARED",
        "inconclusive" => "FIXED_AUTHORITY_UNAVAILABLE_SELECTED_LEASE_CLEARED",
        _ => "AUTHENTICATED_READBACK_RESULT_UNMODELED",
    }
}

fn continuous_gate_purpose_transition_status(
    common_gate_released_between_purposes: bool,
    common_gate_released_after_install: bool,
    snapshot_pin_has_lease_authority: bool,
) -> &'static str {
    let reclamation_finish = [
        ("snapshot_generation_epoch_compare_exact_g0_e0", true),
        ("snapshot_pin_release", true),
    ];
    let catalog_switch_enter = [
        ("catalog_compare_fixed_old_tuple_epoch", true),
        ("catalog_lease_install", true),
    ];
    if common_gate_released_between_purposes {
        return "COMMON_GATE_RACE_GAP";
    }
    if !common_gate_released_after_install {
        return "COMMON_GATE_NOT_RELEASED_AFTER_INSTALL";
    }
    if snapshot_pin_has_lease_authority {
        return "SNAPSHOT_PIN_LEASE_AUTHORITY_CONFLATION";
    }
    if gate_trace_status(GateClass::Reclamation, &reclamation_finish)
        != "LIFECYCLE_GATE_TRACE_ACCEPTED"
        || gate_trace_status(GateClass::CatalogSwitch, &catalog_switch_enter)
            != "LIFECYCLE_GATE_TRACE_ACCEPTED"
    {
        return "PURPOSE_CLASS_TRACE_FORBIDDEN";
    }
    "CONTINUOUS_COMMON_GATE_PURPOSE_TRANSITION_ACCEPTED"
}

fn catalog_switch_admission_status(
    expected_old_matches: bool,
    fixed_lease_available: bool,
) -> &'static str {
    if !expected_old_matches {
        return "CatalogSwitchConflict_NO_JOURNAL_NO_SELECTOR_EFFECT";
    }
    if !fixed_lease_available {
        return "CatalogSwitchBusy_PRE_IO";
    }
    "CATALOG_SWITCH_ADMITTED_IO_STILL_OUTSIDE_GATE"
}

fn catalog_recovery_status(selector: &str, bootstrap_old: bool) -> &'static str {
    match selector {
        "authenticated_exact_old" => "ROLL_BACK_AND_WRITE_CONCLUSIVE_OUTCOME",
        "authenticated_exact_new" => "COMMIT_AND_WRITE_CONCLUSIVE_OUTCOME",
        "absent" if bootstrap_old => "BOOTSTRAP_ABSENT_IS_EXACT_OLD_ROLLBACK",
        "absent" => "NONBOOTSTRAP_ABSENT_QUARANTINE_BLOCK_READINESS",
        "invalid" | "truncated" | "unrelated" | "contradictory" => {
            "OutcomeUnknown_QUARANTINE_BLOCK_READINESS"
        }
        _ => "SELECTOR_OBSERVATION_UNMODELED",
    }
}

fn journal_retirement_status(
    outcome_unlinked: bool,
    outcome_directory_fenced: bool,
    intent_unlinked: bool,
    intent_directory_fenced: bool,
) -> &'static str {
    if outcome_unlinked && outcome_directory_fenced && !intent_unlinked && !intent_directory_fenced
    {
        return "INTENT_ONLY_RECOVERABLE";
    }
    if outcome_unlinked && outcome_directory_fenced && intent_unlinked && intent_directory_fenced {
        return "ABSENT_RELEASE_CHARGES_AFTER_SECOND_FENCE";
    }
    "JOURNAL_RETIREMENT_ORDER_OR_FENCE_VIOLATION"
}

#[derive(Clone)]
struct CatalogDescriptor {
    pack_id: [u8; 32],
    pack_len: u64,
    record_count: u32,
    index_offset: u64,
    index_len: u64,
    min_key: (u8, [u8; 32]),
    max_key: (u8, [u8; 32]),
    pack_checksum: [u8; 32],
}

fn encode_catalog_descriptor(descriptor: &CatalogDescriptor) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CATALOG_DESCRIPTOR_BYTES);
    bytes.extend_from_slice(&descriptor.pack_id);
    push_be_u64(&mut bytes, descriptor.pack_len);
    push_be_u32(&mut bytes, descriptor.record_count);
    push_be_u32(&mut bytes, 0);
    push_be_u64(&mut bytes, descriptor.index_offset);
    push_be_u64(&mut bytes, descriptor.index_len);
    bytes.push(descriptor.min_key.0);
    bytes.extend_from_slice(&[0; 3]);
    bytes.extend_from_slice(&descriptor.min_key.1);
    bytes.push(descriptor.max_key.0);
    bytes.extend_from_slice(&[0; 3]);
    bytes.extend_from_slice(&descriptor.max_key.1);
    bytes.extend_from_slice(&descriptor.pack_checksum);
    assert_eq!(bytes.len(), CATALOG_DESCRIPTOR_BYTES);
    bytes
}

fn decode_catalog_descriptor(bytes: &[u8]) -> Result<CatalogDescriptor, &'static str> {
    if bytes.len() != CATALOG_DESCRIPTOR_BYTES {
        return Err("C_DESCRIPTOR_LENGTH");
    }
    if read_be_u32(bytes, 44)? != 0 {
        return Err("C_DESCRIPTOR_FLAGS");
    }
    if !all_zero(take(bytes, 65, 3)?) || !all_zero(take(bytes, 101, 3)?) {
        return Err("C_KEY_RESERVED");
    }
    let min_key = (bytes[64], array32(bytes, 68)?);
    let max_key = (bytes[100], array32(bytes, 104)?);
    if min_key > max_key {
        return Err("C_DESCRIPTOR_RANGE_ORDER");
    }
    Ok(CatalogDescriptor {
        pack_id: array32(bytes, 0)?,
        pack_len: read_be_u64(bytes, 32)?,
        record_count: read_be_u32(bytes, 40)?,
        index_offset: read_be_u64(bytes, 48)?,
        index_len: read_be_u64(bytes, 56)?,
        min_key,
        max_key,
        pack_checksum: array32(bytes, 136)?,
    })
}

fn pack_index_keys(bytes: &[u8]) -> Result<Vec<(u8, [u8; 32])>, &'static str> {
    validate_pack(bytes)?;
    let count = read_be_u32(bytes, 48)? as usize;
    let index_offset = read_be_u64(bytes, 56)? as usize;
    let mut keys = Vec::with_capacity(count);
    for ordinal in 0..count {
        let at = index_offset
            .checked_add(
                ordinal
                    .checked_mul(PACK_INDEX_ENTRY_BYTES)
                    .ok_or("C_INTEGER_OVERFLOW")?,
            )
            .ok_or("C_INTEGER_OVERFLOW")?;
        keys.push((bytes[at], array32(bytes, at + 4)?));
    }
    Ok(keys)
}

fn descriptor_for_pack(pack: &[u8]) -> CatalogDescriptor {
    let keys = pack_index_keys(pack).expect("authenticated pack");
    let index_offset = read_be_u64(pack, 56).expect("index offset");
    CatalogDescriptor {
        pack_id: array32(pack, pack.len() - 32).expect("pack id"),
        pack_len: pack.len() as u64,
        record_count: keys.len() as u32,
        index_offset,
        index_len: (keys.len() * PACK_INDEX_ENTRY_BYTES) as u64,
        min_key: *keys.first().expect("nonempty pack"),
        max_key: *keys.last().expect("nonempty pack"),
        pack_checksum: array32(pack, pack.len() - 32).expect("pack checksum"),
    }
}

fn validate_catalog_descriptors(
    descriptor_bytes: &[Vec<u8>],
    packs: &[PackVector],
) -> Result<&'static str, &'static str> {
    if descriptor_bytes.is_empty() || descriptor_bytes.len() > 1_024 {
        return Err("C_PACK_COUNT");
    }
    if descriptor_bytes.len() != packs.len() {
        return Err("C_DESCRIPTOR_PACK_BIJECTION");
    }
    let mut descriptors = Vec::with_capacity(descriptor_bytes.len());
    let mut previous_pack: Option<[u8; 32]> = None;
    for bytes in descriptor_bytes {
        let descriptor = decode_catalog_descriptor(bytes)?;
        if previous_pack.is_some_and(|previous| previous >= descriptor.pack_id) {
            return Err("C_PACK_ID_ORDER_DUPLICATE");
        }
        previous_pack = Some(descriptor.pack_id);
        descriptors.push(descriptor);
    }

    let mut all_keys = BTreeSet::new();
    for descriptor in &descriptors {
        let pack = packs
            .iter()
            .find(|pack| {
                array32(&pack.bytes, pack.bytes.len() - 32).expect("pack id") == descriptor.pack_id
            })
            .ok_or("C_DESCRIPTOR_PACK_BIJECTION")?;
        let exact = descriptor_for_pack(&pack.bytes);
        if descriptor.pack_len != exact.pack_len
            || descriptor.record_count != exact.record_count
            || descriptor.index_offset != exact.index_offset
            || descriptor.index_len != exact.index_len
            || descriptor.min_key != exact.min_key
            || descriptor.max_key != exact.max_key
            || descriptor.pack_checksum != exact.pack_checksum
        {
            return Err("C_DESCRIPTOR_PACK_MISMATCH");
        }
        for key in pack_index_keys(&pack.bytes)? {
            if !all_keys.insert(key) {
                return Err("C_CROSS_PACK_DUPLICATE_KEY");
            }
        }
    }
    Ok("CATALOG_ACCEPTED")
}

fn sorted_catalog_case(mut packs: Vec<PackVector>) -> (Vec<Vec<u8>>, Vec<PackVector>) {
    packs.sort_by_key(|pack| array32(&pack.bytes, pack.bytes.len() - 32).expect("pack id"));
    let descriptors = packs
        .iter()
        .map(|pack| encode_catalog_descriptor(&descriptor_for_pack(&pack.bytes)))
        .collect();
    (descriptors, packs)
}

fn catalog_vectors() -> (Vec<BinaryVector>, Vec<ModelVector>) {
    let example = build_pack(&[physical_object(0x05, &[0])]);
    let valid = encode_catalog_descriptor(&descriptor_for_pack(&example.bytes));
    let mut binaries = vec![BinaryVector {
        name: "catalog_valid_exact_168_byte_descriptor",
        base: "authenticated pack descriptor",
        mutation: "none",
        bytes: valid.clone(),
        expected: "C_DESCRIPTOR_ACCEPTED",
        actual: decode_catalog_descriptor(&valid).map(|_| "C_DESCRIPTOR_ACCEPTED"),
        render_exact: true,
    }];
    let mut flags = valid.clone();
    write_be_u32(&mut flags, 44, 1);
    binaries.push(BinaryVector {
        name: "catalog_descriptor_nonzero_flags",
        base: "catalog_valid_exact_168_byte_descriptor",
        mutation: "flags -> 1",
        bytes: flags.clone(),
        expected: "C_DESCRIPTOR_FLAGS",
        actual: decode_catalog_descriptor(&flags).map(|_| "C_DESCRIPTOR_ACCEPTED"),
        render_exact: false,
    });
    let mut reserved = valid.clone();
    reserved[65] = 1;
    binaries.push(BinaryVector {
        name: "catalog_descriptor_nonzero_key_reserved",
        base: "catalog_valid_exact_168_byte_descriptor",
        mutation: "min_key.reserved[0] -> 1",
        bytes: reserved.clone(),
        expected: "C_KEY_RESERVED",
        actual: decode_catalog_descriptor(&reserved).map(|_| "C_DESCRIPTOR_ACCEPTED"),
        render_exact: false,
    });

    let mut objects = (0u8..8)
        .map(|byte| physical_object(0x05, &[byte]))
        .collect::<Vec<_>>();
    objects.sort_by_key(|object| object_id(object[10], object).expect("object id"));
    let (overlap_descriptors, overlap_packs) = sorted_catalog_case(vec![
        build_pack(&[objects[0].clone(), objects[4].clone()]),
        build_pack(&[objects[2].clone(), objects[6].clone()]),
    ]);
    let overlap_actual = validate_catalog_descriptors(&overlap_descriptors, &overlap_packs)
        .unwrap_or_else(|error| error);

    let (duplicate_descriptors, duplicate_packs) = sorted_catalog_case(vec![
        build_pack(&[objects[0].clone()]),
        build_pack(&[objects[0].clone(), objects[7].clone()]),
    ]);
    let duplicate_actual = validate_catalog_descriptors(&duplicate_descriptors, &duplicate_packs)
        .expect_err("cross-pack duplicate must fail");

    let mut mismatch_descriptors = overlap_descriptors.clone();
    let first = decode_catalog_descriptor(&mismatch_descriptors[0]).expect("descriptor");
    mismatch_descriptors[0][68..100].copy_from_slice(&first.max_key.1);
    let mismatch_actual = validate_catalog_descriptors(&mismatch_descriptors, &overlap_packs)
        .expect_err("descriptor min mismatch must fail");
    let models = vec![
        ModelVector {
            name: "catalog_overlapping_descriptor_ranges_without_duplicate",
            input: "two exact descriptors have overlapping min/max ranges; global typed keys are disjoint".into(),
            expected: "CATALOG_ACCEPTED",
            actual: overlap_actual,
        },
        ModelVector {
            name: "catalog_cross_pack_duplicate_typed_key",
            input: "two authenticated packs contain the same typed (kind,id)".into(),
            expected: "C_CROSS_PACK_DUPLICATE_KEY",
            actual: duplicate_actual,
        },
        ModelVector {
            name: "catalog_descriptor_min_key_crosscheck",
            input: "descriptor min_key replaced by its max_key while pack bytes remain authenticated".into(),
            expected: "C_DESCRIPTOR_PACK_MISMATCH",
            actual: mismatch_actual,
        },
    ];
    (binaries, models)
}

fn checked_minimal_pack_len(record_count: usize) -> Result<usize, &'static str> {
    let record_bytes = record_count
        .checked_mul(64)
        .ok_or("PACK_ARITHMETIC_OVERFLOW")?;
    let index_bytes = record_count
        .checked_mul(PACK_INDEX_ENTRY_BYTES)
        .ok_or("PACK_ARITHMETIC_OVERFLOW")?;
    PACK_HEADER_BYTES
        .checked_add(record_bytes)
        .and_then(|value| value.checked_add(index_bytes))
        .and_then(|value| value.checked_add(PACK_TRAILER_BYTES))
        .ok_or("PACK_ARITHMETIC_OVERFLOW")
}

fn pack_len_status(pack_len: usize) -> &'static str {
    if pack_len <= MAX_PACK_BYTES {
        "PACK_CAP_ACCEPTED"
    } else {
        "PACK_CAP_REFUSED_PREALLOCATION"
    }
}

fn record_count_status(count: usize) -> &'static str {
    if count <= PACK_RECORDS_MAX {
        "PACK_RECORD_CAP_ACCEPTED"
    } else {
        "PACK_RECORD_CAP_REFUSED_PREALLOCATION"
    }
}

fn index_len_status(index_len: usize) -> &'static str {
    if index_len <= PACK_INDEX_BYTES_MAX && index_len % PACK_INDEX_ENTRY_BYTES == 0 {
        "PACK_INDEX_CAP_ACCEPTED"
    } else {
        "PACK_INDEX_CAP_REFUSED_PREALLOCATION"
    }
}

fn run_count(entries: usize) -> Result<usize, &'static str> {
    entries
        .checked_add(PACK_SORT_RUN_ENTRIES - 1)
        .ok_or("PACK_ARITHMETIC_OVERFLOW")
        .map(|rounded| rounded / PACK_SORT_RUN_ENTRIES)
}

fn bounded_offset_validation_status(record_count: usize) -> &'static str {
    let mut cursor = PACK_HEADER_BYTES;
    let mut max_run_entries = 0usize;
    for ordinal in 0..record_count {
        let expected = PACK_HEADER_BYTES
            .checked_add(ordinal.checked_mul(64).expect("bounded ordinal"))
            .expect("bounded offset");
        if cursor != expected {
            return "BOUNDED_OFFSET_ORDER_FAILURE";
        }
        cursor = cursor.checked_add(64).expect("bounded cursor");
        max_run_entries = max_run_entries.max((ordinal % PACK_SORT_RUN_ENTRIES) + 1);
    }
    if max_run_entries <= PACK_SORT_RUN_ENTRIES {
        "BOUNDED_STREAMING_VALIDATION_ACCEPTED"
    } else {
        "BOUND_EXCEEDED"
    }
}

fn checked_byte_window(capacity: usize) -> Result<usize, &'static str> {
    capacity
        .checked_add(ALLOCATION_HEADER_BYTES)
        .ok_or("ACCOUNTING_ARITHMETIC_OVERFLOW")
}

fn checked_owned(capacity: usize, item_size: usize) -> Result<usize, &'static str> {
    capacity
        .checked_mul(item_size)
        .and_then(|bytes| bytes.checked_add(ALLOCATION_HEADER_BYTES))
        .ok_or("ACCOUNTING_ARITHMETIC_OVERFLOW")
}

fn checked_standalone_validator_receipt() -> Result<usize, &'static str> {
    let object_reader = checked_byte_window(65_536)?;
    let index_sorter = checked_byte_window(3_728_320)?;
    let fixed_validator = checked_byte_window(PACK_VALIDATOR_FIXED_CAPACITY_BYTES)?;
    object_reader
        .checked_mul(2)
        .and_then(|value| value.checked_add(index_sorter))
        .and_then(|value| value.checked_add(object_reader.checked_mul(2)?))
        .and_then(|value| value.checked_add(fixed_validator))
        .ok_or("ACCOUNTING_ARITHMETIC_OVERFLOW")
}

fn checked_writer_validator_descriptor_receipt() -> Result<usize, &'static str> {
    let writer_receipt = checked_byte_window(65_536)?
        .checked_add(checked_byte_window(65_536)?)
        .and_then(|value| value.checked_add(checked_byte_window(131_072).ok()?))
        .and_then(|value| value.checked_add(checked_byte_window(262_144).ok()?))
        .ok_or("ACCOUNTING_ARITHMETIC_OVERFLOW")?;
    writer_receipt
        .checked_add(checked_standalone_validator_receipt()?)
        .and_then(|value| {
            value.checked_add(
                checked_owned(GROUP_PRIOR_PACK_DESCRIPTOR_MAX, CATALOG_DESCRIPTOR_BYTES).ok()?,
            )
        })
        .ok_or("ACCOUNTING_ARITHMETIC_OVERFLOW")
}

fn group_rollover_duplicate_status(
    descriptor_count: usize,
    same_typed_key: bool,
    prior_record: &[u8],
    candidate_record: &[u8],
    live_prior_pack_windows: usize,
    live_prior_pack_fds: usize,
) -> &'static str {
    if descriptor_count > GROUP_PRIOR_PACK_DESCRIPTOR_MAX {
        return "GROUP_DESCRIPTOR_N_PLUS_1_REFUSED_PRE_EFFECT";
    }
    if live_prior_pack_windows != 1 || live_prior_pack_fds != 1 {
        return "GROUP_PRIOR_PACK_PROBE_WIDTH_VIOLATION";
    }
    if !same_typed_key {
        if descriptor_count == GROUP_PRIOR_PACK_DESCRIPTOR_MAX {
            return "GROUP_DISTINCT_AT_PACK_CEILING_REFUSED_PRE_EFFECT";
        }
        return "GROUP_UNIQUE_RECORD_ACCEPTED_ONE_PRIOR_WINDOW_FD";
    }
    if prior_record == candidate_record {
        "GROUP_EXACT_EQUAL_REUSED_NO_SECOND_RECORD"
    } else {
        "GROUP_SAME_KEY_DIFFERENT_BYTES_FAIL_CLOSED"
    }
}

fn validator_bucket_owner_status(
    mode: &str,
    foreground_bytes: usize,
    recovery_gc_bytes: usize,
) -> &'static str {
    match (mode, foreground_bytes, recovery_gc_bytes) {
        ("writer_immediate", WRITER_VALIDATOR_DESCRIPTOR_RECEIPT_BYTES, 0) => {
            "COMPLETE_WRITER_VALIDATOR_DESCRIPTOR_SET_FOREGROUND_OWNED"
        }
        ("standalone", 0, STANDALONE_VALIDATOR_RECEIPT_BYTES) => {
            "COMPLETE_STANDALONE_VALIDATOR_SET_RECOVERY_GC_OWNED"
        }
        (_, foreground, recovery) if foreground != 0 && recovery != 0 => {
            "SPLIT_BUCKET_VALIDATOR_OWNERSHIP_FORBIDDEN"
        }
        _ => "VALIDATOR_BUCKET_OWNER_MISMATCH",
    }
}

struct StreamingCounters {
    payload_staging_bytes: usize,
    changed_source_reads_max: usize,
    checksum_construction_carrier_reads: usize,
    completed_carrier_validation_reads: usize,
    completed_carrier_validation_is_independent: bool,
    second_changed_source_ordering_reads: usize,
    second_selected_old_carrier_ordering_reads: usize,
    pack_rewrite_passes: usize,
    whole_pack_resident_bytes: usize,
    whole_index_resident_bytes: usize,
    max_run_entries: usize,
}

fn simulate_streaming_counters(record_count: usize) -> StreamingCounters {
    let mut max_run_entries = 0usize;
    for ordinal in 0..record_count {
        max_run_entries = max_run_entries.max((ordinal % PACK_SORT_RUN_ENTRIES) + 1);
    }
    StreamingCounters {
        payload_staging_bytes: 0,
        changed_source_reads_max: usize::from(record_count > 0),
        checksum_construction_carrier_reads: 1,
        completed_carrier_validation_reads: 1,
        completed_carrier_validation_is_independent: true,
        second_changed_source_ordering_reads: 0,
        second_selected_old_carrier_ordering_reads: 0,
        pack_rewrite_passes: 0,
        whole_pack_resident_bytes: 0,
        whole_index_resident_bytes: 0,
        max_run_entries,
    }
}

fn identity_invariance_status() -> (&'static str, String) {
    let identities = identity_vectors();
    let by_name = |name: &str| {
        identities
            .iter()
            .find(|vector| vector.name == name)
            .expect("identity vector")
    };
    let objects = vec![
        physical_object(0x05, b"abc"),
        physical_object(0x03, &by_name("file_node_0644_abc").preimage),
        physical_object(0x04, &by_name("symlink_node_file_txt").preimage),
        physical_object(
            0x02,
            &by_name("directory_explicit_0755_nested_file").preimage,
        ),
        physical_object(
            0x02,
            &by_name("directory_implicit_composite_root_1000").preimage,
        ),
        physical_object(0x01, &by_name("version_composite").preimage),
    ];
    let carrier_a = vec![build_pack(&objects)];
    let carrier_b = vec![
        build_pack(&[objects[5].clone(), objects[2].clone(), objects[0].clone()]),
        build_pack(&[objects[4].clone(), objects[1].clone(), objects[3].clone()]),
    ];
    let canonical_keys = |packs: &[PackVector]| {
        let mut keys = packs
            .iter()
            .flat_map(|pack| pack_index_keys(&pack.bytes).expect("pack keys"))
            .collect::<Vec<_>>();
        keys.sort();
        keys
    };
    let keys_a = canonical_keys(&carrier_a);
    let keys_b = canonical_keys(&carrier_b);
    let pack_ids_a = carrier_a
        .iter()
        .map(|pack| array32(&pack.bytes, pack.bytes.len() - 32).expect("pack id"))
        .collect::<Vec<_>>();
    let pack_ids_b = carrier_b
        .iter()
        .map(|pack| array32(&pack.bytes, pack.bytes.len() - 32).expect("pack id"))
        .collect::<Vec<_>>();
    let version_id = by_name("version_composite").digest;
    let status = if keys_a == keys_b && pack_ids_a != pack_ids_b {
        "OBJECT_IDS_AND_VERSION_ID_INVARIANT_PACK_IDS_MAY_DIFFER"
    } else {
        "IDENTITY_INVARIANCE_FAILURE"
    };
    (
        status,
        format!(
            "closure=chunk,file,symlink,nested-tree,root-tree,version-record; closure_keys={}; VersionId={}; carrierA_packids={}; carrierB_packids={}",
            keys_a.len(),
            hex(&version_id),
            pack_ids_a.iter().map(|id| hex(id)).collect::<Vec<_>>().join(","),
            pack_ids_b.iter().map(|id| hex(id)).collect::<Vec<_>>().join(","),
        ),
    )
}

fn crash_cut_status(cut: u8) -> &'static str {
    match cut {
        1 => "OLD_PRIVATE_UNFENCED_RECOVERY_CUSTODY",
        2 => "OLD_PRIVATE_FENCED_RESUME_OR_QUARANTINE",
        3 => "OLD_FINAL_PACK_INSTALLED_ORPHAN",
        4 => "OLD_UNSELECTED_CLOSURE_ORPHAN",
        5 => "OLD_INTENT_ONLY_RECONCILE_SELECTOR",
        6 => "UNKNOWN_OUTCOME_UNKNOWN_RECOVERY_HOLD",
        7 => "NEW_RECONSTRUCT_COMMITTED_OUTCOME",
        8 => "EXACT_RECORDED_RESULT_COMMITTED_NEW_ONLY_RECEIPT",
        _ => "CRASH_CUT_UNMODELED",
    }
}

fn journal_capacity_status(
    existing_items: usize,
    existing_bytes: usize,
    requested_items: usize,
    requested_bytes: usize,
) -> &'static str {
    let fits = existing_items
        .checked_add(requested_items)
        .is_some_and(|items| items <= 1_024)
        && existing_bytes
            .checked_add(requested_bytes)
            .is_some_and(|bytes| bytes <= 4_304_896);
    if fits {
        "JOURNAL_PRECHARGE_ACCEPTED"
    } else {
        "JournalCapacityExhausted_EVIDENCE_PRESERVED_PRE_EFFECT"
    }
}

fn oracle_label(result: Result<&'static str, &'static str>) -> &'static str {
    match result {
        Ok(label) | Err(label) => label,
    }
}

fn model_vectors(
    journal_pair: &[Vec<u8>],
    qrec_bytes: &[u8],
    reason2_private_pack: &[u8],
    mut catalog_models: Vec<ModelVector>,
) -> Vec<ModelVector> {
    let profile = "44".repeat(32);
    let txid = "11".repeat(16);
    let intent = format!("journal/v1/{profile}/{txid}-0000000000000000.frame");
    let outcome_private = format!("journal/v1/{profile}/{txid}-0000000000000001.frame.tmp");
    let uppercase = format!("journal/v1/{profile}/{txid}-000000000000000A.frame");
    let narrow = format!("journal/v1/{profile}/{txid}-000000000000000.frame");
    let unsupported = format!("journal/v1/{profile}/{txid}-0000000000000002.frame");
    let alternative_private = format!("journal/v1/{profile}/{txid}-0000000000000001.frame.tmp.1");
    let profile_mismatch_path = format!(
        "journal/v1/{}/{txid}-0000000000000000.frame",
        "45".repeat(32)
    );
    let transaction_mismatch_path = format!(
        "journal/v1/{profile}/{}-0000000000000001.frame.tmp",
        "12".repeat(16)
    );
    let mut invalid_occupied_journal = journal_pair[0].clone();
    *invalid_occupied_journal
        .last_mut()
        .expect("journal checksum") ^= 1;
    let mut trailing_occupied_journal = journal_pair[0].clone();
    trailing_occupied_journal.push(0);
    let valid_catalog_gate = [
        ("read_old", false),
        ("catalog_compare_fixed_old_tuple_epoch", true),
        ("catalog_lease_install", true),
        ("journal_intent", false),
        ("selector_rename_fence", false),
        ("readback", false),
        ("catalog_lease_validate", true),
        (
            "catalog_lease_clear_select_fixed_old_new_or_authority_unavailable",
            true,
        ),
    ];
    let valid_catalog_inconclusive_gate = [
        ("authenticated_inconclusive_readback", false),
        ("recovery_hold_custody_transfer", false),
        ("catalog_lease_validate", true),
        (
            "catalog_lease_clear_select_fixed_old_new_or_authority_unavailable",
            true,
        ),
    ];
    let valid_read_gate = [
        ("ledger_precharge", false),
        ("read_compare_binding_locator", true),
        ("read_pin_install", true),
        ("provider_read", false),
        ("read_pin_release", true),
    ];
    let valid_reclamation_gate = [
        ("snapshot_pin_install", true),
        ("enumerate", false),
        ("mark", false),
        ("snapshot_generation_epoch_compare_exact_g0_e0", true),
        ("snapshot_pin_release", true),
    ];
    let mut models = vec![
        ModelVector {
            name: "journal_intent_final_path",
            input: intent.clone(),
            expected: "JOURNAL_PATH_ACCEPTED",
            actual: journal_path_status(&intent, &journal_pair[0]),
        },
        ModelVector {
            name: "journal_outcome_private_path",
            input: outcome_private.clone(),
            expected: "JOURNAL_PATH_ACCEPTED",
            actual: journal_path_status(&outcome_private, &journal_pair[1]),
        },
        ModelVector {
            name: "journal_uppercase_sequence",
            input: uppercase.clone(),
            expected: "PATH_SEQUENCE_LOWERHEX",
            actual: journal_path_status(&uppercase, &journal_pair[0]),
        },
        ModelVector {
            name: "journal_sequence_wrong_width",
            input: narrow.clone(),
            expected: "PATH_SEQUENCE_LOWERHEX",
            actual: journal_path_status(&narrow, &journal_pair[0]),
        },
        ModelVector {
            name: "journal_sequence_outside_v1_pair",
            input: unsupported.clone(),
            expected: "JOURNAL_SEQUENCE",
            actual: journal_path_status(&unsupported, &journal_pair[0]),
        },
        ModelVector {
            name: "journal_sequence_name_frame_mismatch",
            input: intent,
            expected: "JOURNAL_SEQUENCE_NAME_MISMATCH",
            actual: journal_path_status(
                &format!("journal/v1/{profile}/{txid}-0000000000000000.frame"),
                &journal_pair[1],
            ),
        },
        ModelVector {
            name: "journal_profile_name_authenticated_frame_mismatch",
            input: profile_mismatch_path.clone(),
            expected: "JOURNAL_PROFILE_NAME_MISMATCH",
            actual: journal_path_status(&profile_mismatch_path, &journal_pair[0]),
        },
        ModelVector {
            name: "journal_transaction_name_authenticated_frame_mismatch",
            input: transaction_mismatch_path.clone(),
            expected: "JOURNAL_TRANSACTION_NAME_MISMATCH",
            actual: journal_path_status(&transaction_mismatch_path, &journal_pair[1]),
        },
        ModelVector {
            name: "journal_sequence_no_wrap",
            input: "checked_next(u64::MAX)".into(),
            expected: "JOURNAL_SEQUENCE_OVERFLOW_REFUSED_PRE_EFFECT",
            actual: if u64::MAX.checked_add(1).is_none() {
                "JOURNAL_SEQUENCE_OVERFLOW_REFUSED_PRE_EFFECT"
            } else {
                "WRAP"
            },
        },
        ModelVector {
            name: "journal_private_occupied_exact_authenticated_bytes_and_eof",
            input: format!(
                "path={outcome_private}; occupied_bytes={}; EOF={}",
                hex(&journal_pair[1]),
                journal_pair[1].len()
            ),
            expected: "JOURNAL_PRIVATE_RESUME",
            actual: journal_collision_status(
                &outcome_private,
                false,
                &journal_pair[1],
                &journal_pair[1],
            ),
        },
        ModelVector {
            name: "journal_final_occupied_exact_authenticated_bytes_and_eof",
            input: format!(
                "path=journal/v1/{profile}/{txid}-0000000000000000.frame; occupied_bytes={}; EOF={}",
                hex(&journal_pair[0]),
                journal_pair[0].len()
            ),
            expected: "JOURNAL_FINAL_IDEMPOTENT_REUSE",
            actual: journal_collision_status(
                &format!("journal/v1/{profile}/{txid}-0000000000000000.frame"),
                true,
                &journal_pair[0],
                &journal_pair[0],
            ),
        },
        ModelVector {
            name: "journal_private_path_cannot_be_claimed_final",
            input: format!(
                "path={outcome_private}; claimed_final=true; occupied_bytes={}; EOF={}",
                hex(&journal_pair[1]),
                journal_pair[1].len()
            ),
            expected: "JOURNAL_PATH_CLASS_MISMATCH",
            actual: journal_collision_status(
                &outcome_private,
                true,
                &journal_pair[1],
                &journal_pair[1],
            ),
        },
        ModelVector {
            name: "journal_final_path_cannot_be_claimed_private",
            input: format!(
                "path=journal/v1/{profile}/{txid}-0000000000000000.frame; claimed_final=false; occupied_bytes={}; EOF={}",
                hex(&journal_pair[0]),
                journal_pair[0].len()
            ),
            expected: "JOURNAL_PATH_CLASS_MISMATCH",
            actual: journal_collision_status(
                &format!("journal/v1/{profile}/{txid}-0000000000000000.frame"),
                false,
                &journal_pair[0],
                &journal_pair[0],
            ),
        },
        ModelVector {
            name: "journal_private_occupied_different_authenticated_frame",
            input: "occupied path has a valid but byte-different authenticated frame".into(),
            expected: "JournalPathCollision",
            actual: journal_collision_status(
                &outcome_private,
                false,
                &journal_pair[1],
                &journal_pair[0],
            ),
        },
        ModelVector {
            name: "journal_final_occupied_invalid_checksum",
            input: "occupied final path has exact length but invalid authentication".into(),
            expected: "JournalPathCollision",
            actual: journal_collision_status(
                &format!("journal/v1/{profile}/{txid}-0000000000000000.frame"),
                true,
                &journal_pair[0],
                &invalid_occupied_journal,
            ),
        },
        ModelVector {
            name: "journal_final_occupied_trailing_bytes_after_frame",
            input: "occupied final path has authenticated prefix plus a byte after declared EOF".into(),
            expected: "JournalPathCollision",
            actual: journal_collision_status(
                &format!("journal/v1/{profile}/{txid}-0000000000000000.frame"),
                true,
                &journal_pair[0],
                &trailing_occupied_journal,
            ),
        },
        ModelVector {
            name: "journal_collision_never_uses_alternate_suffix",
            input: alternative_private.clone(),
            expected: "PATH_SUFFIX",
            actual: journal_path_status(&alternative_private, &journal_pair[1]),
        },
        ModelVector {
            name: "quarantine_private_final_pair",
            input: format!(
                "quarantine/v1/{}/.0000002a.qrec.tmp -> quarantine/v1/{}/0000002a.qrec",
                "55".repeat(16),
                "55".repeat(16)
            ),
            expected: "QUARANTINE_PATH_PAIR_ACCEPTED",
            actual: quarantine_pair_status(
                &format!("quarantine/v1/{}/.0000002a.qrec.tmp", "55".repeat(16)),
                &format!("quarantine/v1/{}/0000002a.qrec", "55".repeat(16)),
            ),
        },
        ModelVector {
            name: "quarantine_uppercase_ordinal",
            input: format!(
                "quarantine/v1/{}/.0000002A.qrec.tmp -> quarantine/v1/{}/0000002A.qrec",
                "55".repeat(16),
                "55".repeat(16)
            ),
            expected: "QUARANTINE_ORDINAL_LOWERHEX",
            actual: quarantine_pair_status(
                &format!("quarantine/v1/{}/.0000002A.qrec.tmp", "55".repeat(16)),
                &format!("quarantine/v1/{}/0000002A.qrec", "55".repeat(16)),
            ),
        },
        ModelVector {
            name: "quarantine_alternative_private_suffix",
            input: format!(
                "quarantine/v1/{}/.0000002a.qrec.tmp.1 -> quarantine/v1/{}/0000002a.qrec",
                "55".repeat(16),
                "55".repeat(16)
            ),
            expected: "QUARANTINE_PRIVATE_GRAMMAR",
            actual: quarantine_pair_status(
                &format!("quarantine/v1/{}/.0000002a.qrec.tmp.1", "55".repeat(16)),
                &format!("quarantine/v1/{}/0000002a.qrec", "55".repeat(16)),
            ),
        },
        ModelVector {
            name: "quarantine_ordinal_relation_mismatch",
            input: format!(
                "quarantine/v1/{}/.0000002a.qrec.tmp -> quarantine/v1/{}/0000002b.qrec",
                "55".repeat(16),
                "55".repeat(16)
            ),
            expected: "QUARANTINE_FINAL_RELATION",
            actual: quarantine_pair_status(
                &format!("quarantine/v1/{}/.0000002a.qrec.tmp", "55".repeat(16)),
                &format!("quarantine/v1/{}/0000002b.qrec", "55".repeat(16)),
            ),
        },
        ModelVector {
            name: "quarantine_group_relation_mismatch",
            input: format!(
                "quarantine/v1/{}/.0000002a.qrec.tmp -> quarantine/v1/{}/0000002a.qrec",
                "55".repeat(16),
                "66".repeat(16)
            ),
            expected: "QUARANTINE_GROUP_RELATION",
            actual: quarantine_pair_status(
                &format!("quarantine/v1/{}/.0000002a.qrec.tmp", "55".repeat(16)),
                &format!("quarantine/v1/{}/0000002a.qrec", "66".repeat(16)),
            ),
        },
        ModelVector {
            name: "quarantine_private_occupied_identical",
            input: "private occupied; authenticated exact 284 bytes+EOF; identical".into(),
            expected: "QUARANTINE_PRIVATE_RESUME",
            actual: quarantine_collision_status(false, true, true),
        },
        ModelVector {
            name: "quarantine_private_occupied_mismatch",
            input: "private occupied; authenticated exact 284 bytes+EOF; different".into(),
            expected: "QuarantinePathCollision",
            actual: quarantine_collision_status(false, true, false),
        },
        ModelVector {
            name: "quarantine_final_occupied_identical",
            input: "final occupied; authenticated exact 284 bytes+EOF; identical".into(),
            expected: "QUARANTINE_FINAL_IDEMPOTENT_REUSE",
            actual: quarantine_collision_status(true, true, true),
        },
        ModelVector {
            name: "quarantine_final_occupied_invalid",
            input: "final occupied; invalid length/checksum/EOF".into(),
            expected: "QuarantinePathCollision",
            actual: quarantine_collision_status(true, false, false),
        },
        ModelVector {
            name: "catalog_switch_valid_gate_partition",
            input: "I/O outside; fixed old tuple/epoch compare, fixed lease install/validate, and final atomic clear-select of only fixed old/new/authority-unavailable under catalog gate".into(),
            expected: "LIFECYCLE_GATE_TRACE_ACCEPTED",
            actual: gate_trace_status(GateClass::CatalogSwitch, &valid_catalog_gate),
        },
        ModelVector {
            name: "catalog_switch_inconclusive_readback_valid_gate_partition",
            input: "authenticated readback=inconclusive outside gate; custody transfer to bounded RecoveryHold outside gate; fixed lease validate plus atomic clear-select fixed authority-unavailable under CatalogSwitch; caller-supplied tuple=none".into(),
            expected: "LIFECYCLE_GATE_TRACE_ACCEPTED",
            actual: gate_trace_status(
                GateClass::CatalogSwitch,
                &valid_catalog_inconclusive_gate,
            ),
        },
        ModelVector {
            name: "catalog_switch_inconclusive_readback_selects_fixed_authority_unavailable",
            input: "authenticated readback=inconclusive; caller-supplied tuple=none; result source=lease-fixed authority-unavailable marker; lease cleared atomically".into(),
            expected: "FIXED_AUTHORITY_UNAVAILABLE_SELECTED_LEASE_CLEARED",
            actual: catalog_lease_clear_select_status("inconclusive", None),
        },
        ModelVector {
            name: "catalog_switch_clear_rejects_caller_supplied_tuple",
            input: "authenticated readback=exact_new; caller-supplied tuple=injected_new; fixed lease tuple remains sole selection authority".into(),
            expected: "CALLER_SUPPLIED_CATALOG_TUPLE_FORBIDDEN",
            actual: catalog_lease_clear_select_status("exact_new", Some("injected_new")),
        },
        ModelVector {
            name: "read_pin_valid_gate_partition",
            input: "precharge/I/O outside; fixed binding+locator comparison and counter install/release under read gate".into(),
            expected: "LIFECYCLE_GATE_TRACE_ACCEPTED",
            actual: gate_trace_status(GateClass::ReadPin, &valid_read_gate),
        },
        ModelVector {
            name: "reclamation_valid_gate_partition",
            input: "fixed snapshot pin plus generation/epoch comparison under the separately typed reclamation gate; enumeration/mark outside".into(),
            expected: "LIFECYCLE_GATE_TRACE_ACCEPTED",
            actual: gate_trace_status(GateClass::Reclamation, &valid_reclamation_gate),
        },
        ModelVector {
            name: "reclamation_gate_rejects_catalog_switch_lease_install",
            input: "attempt CatalogSwitchLeaseV1 install while ProtectedAuthoritySnapshotPinV1 reclamation gate is held".into(),
            expected: "LIFECYCLE_GATE_EFFECT_FORBIDDEN",
            actual: gate_trace_status(
                GateClass::Reclamation,
                &[("catalog_lease_install", true)],
            ),
        },
        ModelVector {
            name: "reclamation_to_catalog_switch_continuous_common_gate_purpose_transition",
            input: "one continuous common short-gate hold: Reclamation performs exact G0/E0 recheck and releases its fixed snapshot pin; without release/reacquire, purpose changes to separately typed CatalogSwitch, which compares the fixed old tuple/epoch and installs its fixed lease; release common gate only after install; snapshot pin grants/shares no lease authority".into(),
            expected: "CONTINUOUS_COMMON_GATE_PURPOSE_TRANSITION_ACCEPTED",
            actual: continuous_gate_purpose_transition_status(false, true, false),
        },
        ModelVector {
            name: "catalog_switch_stale_expected_old",
            input: "gate compare stale before intent".into(),
            expected: "CatalogSwitchConflict_NO_JOURNAL_NO_SELECTOR_EFFECT",
            actual: catalog_switch_admission_status(false, true),
        },
        ModelVector {
            name: "catalog_switch_contender_while_lease_held",
            input: "bounded waiter unavailable before I/O".into(),
            expected: "CatalogSwitchBusy_PRE_IO",
            actual: catalog_switch_admission_status(true, false),
        },
        ModelVector {
            name: "catalog_recovery_selector_exact_old",
            input: "intent exists; selector=authenticated exact old tuple".into(),
            expected: "ROLL_BACK_AND_WRITE_CONCLUSIVE_OUTCOME",
            actual: catalog_recovery_status("authenticated_exact_old", false),
        },
        ModelVector {
            name: "catalog_recovery_selector_exact_new",
            input: "intent exists; selector=authenticated exact new tuple".into(),
            expected: "COMMIT_AND_WRITE_CONCLUSIVE_OUTCOME",
            actual: catalog_recovery_status("authenticated_exact_new", false),
        },
        ModelVector {
            name: "journal_retirement_crash_after_outcome_unlink_fence",
            input: "unlink outcome; directory fence; crash".into(),
            expected: "INTENT_ONLY_RECOVERABLE",
            actual: journal_retirement_status(true, true, false, false),
        },
        ModelVector {
            name: "journal_retirement_complete",
            input: "unlink outcome+fence; unlink intent+fence".into(),
            expected: "ABSENT_RELEASE_CHARGES_AFTER_SECOND_FENCE",
            actual: journal_retirement_status(true, true, true, true),
        },
        ModelVector {
            name: "journal_retirement_intent_unlinked_before_outcome_fence",
            input: "unlink outcome; unlink intent before outcome directory fence".into(),
            expected: "JOURNAL_RETIREMENT_ORDER_OR_FENCE_VIOLATION",
            actual: journal_retirement_status(true, false, true, false),
        },
    ];

    let qrec = parse_qrec(qrec_bytes, Some(288)).expect("valid qrec");
    let private_pack = format!(
        "packs/v1/{}/.tmp/{}-0000002a.pack",
        hex(&qrec.profile),
        hex(&qrec.journal_txid)
    );
    let occupied_final_pack = format!("packs/v1/{}/{}.pack", hex(&qrec.profile), hex(&qrec.pack));
    let mut mismatched_private_pack = reason2_private_pack.to_vec();
    mismatched_private_pack[PACK_HEADER_BYTES + 4] ^= 1;
    models.extend([
        ModelVector {
            name: "reason2_retained_transaction_private_pack_path",
            input: format!(
                "authenticated ProfileHash={} + JournalTxId={} + u32 ordinal={} derive pre-existing path={private_pack}; created_carrier_paths=0",
                hex(&qrec.profile),
                hex(&qrec.journal_txid),
                qrec.object_offset,
            ),
            expected: "Q_REASON2_PRIVATE_PATH_ACCEPTED",
            actual: private_pack_path_status(&private_pack, &qrec),
        },
        ModelVector {
            name: "reason2_qrec_mints_no_second_carrier_path",
            input: "qrec publication records evidence only; existing transaction-private pack remains the sole retained carrier; no copy/rename/new carrier path".into(),
            expected: "Q_REASON2_ZERO_NEW_CARRIER_PATHS",
            actual: if private_pack_path_status(&private_pack, &qrec)
                == "Q_REASON2_PRIVATE_PATH_ACCEPTED"
                && reason2_private_carrier_status(&qrec, Some(reason2_private_pack), 1)
                    == "Q_REASON2_EXISTING_PRIVATE_CARRIER_AUTHENTICATED"
            {
                "Q_REASON2_ZERO_NEW_CARRIER_PATHS"
            } else {
                "Q_REASON2_PATH_DERIVATION_FAILED"
            },
        },
        ModelVector {
            name: "reason2_existing_private_carrier_exact_authentication",
            input: format!(
                "derived_path={private_pack}; encoded_len={}; PackId={}; observed_checksum={}; custody_owners=1",
                qrec.encoded_len,
                hex(&qrec.pack),
                hex(&qrec.observed_checksum),
            ),
            expected: "Q_REASON2_EXISTING_PRIVATE_CARRIER_AUTHENTICATED",
            actual: reason2_private_carrier_status(&qrec, Some(reason2_private_pack), 1),
        },
        ModelVector {
            name: "reason2_existing_private_carrier_missing",
            input: "derived transaction-private path is absent".into(),
            expected: "Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE",
            actual: reason2_private_carrier_status(&qrec, None, 1),
        },
        ModelVector {
            name: "reason2_existing_private_carrier_mismatch",
            input: "derived path contains bytes that fail qrec PackId/checksum authentication".into(),
            expected: "Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE",
            actual: reason2_private_carrier_status(&qrec, Some(&mismatched_private_pack), 1),
        },
        ModelVector {
            name: "reason2_existing_private_carrier_multiply_owned",
            input: "authenticated retained path has two claimed custody owners".into(),
            expected: "Q_REASON2_QUARANTINED_AUTHORITY_UNAVAILABLE",
            actual: reason2_private_carrier_status(&qrec, Some(reason2_private_pack), 2),
        },
        ModelVector {
            name: "reason2_qrec_identifies_occupied_final_pack",
            input: format!(
                "path={occupied_final_pack}; object_kind={}; ObjectId={}; private_encoded_len={}",
                qrec.object_kind,
                hex(&qrec.object),
                qrec.encoded_len,
            ),
            expected: "Q_REASON2_OCCUPIED_FINAL_PACK_IDENTIFIED",
            actual: occupied_final_pack_path_status(&occupied_final_pack, &qrec),
        },
        ModelVector {
            name: "reason2_private_pack_wrong_txid",
            input: format!(
                "packs/v1/{}/.tmp/{}-0000002a.pack",
                hex(&qrec.profile),
                "12".repeat(16)
            ),
            expected: "Q_REASON2_PRIVATE_PATH_RELATION",
            actual: private_pack_path_status(
                &format!(
                    "packs/v1/{}/.tmp/{}-0000002a.pack",
                    hex(&qrec.profile),
                    "12".repeat(16)
                ),
                &qrec,
            ),
        },
        ModelVector {
            name: "reason2_obsolete_incoming_pack_path_rejected",
            input: format!("quarantine/v1/{}/0000002a.incoming.pack", "55".repeat(16)),
            expected: "Q_REASON2_PRIVATE_PATH_RELATION",
            actual: private_pack_path_status(
                &format!("quarantine/v1/{}/0000002a.incoming.pack", "55".repeat(16)),
                &qrec,
            ),
        },
    ]);

    let duplicate = vec![journal_pair[0].clone(), journal_pair[0].clone()];
    let outcome_only = vec![journal_pair[1].clone()];
    let mut profile_mismatch_outcome = journal_pair[1].clone();
    profile_mismatch_outcome[16] ^= 1;
    reseal_journal(&mut profile_mismatch_outcome);
    let profile_mismatch = vec![journal_pair[0].clone(), profile_mismatch_outcome];
    let mut tx_mismatch_outcome = journal_pair[1].clone();
    tx_mismatch_outcome[48] ^= 1;
    reseal_journal(&mut tx_mismatch_outcome);
    let tx_mismatch = vec![journal_pair[0].clone(), tx_mismatch_outcome];
    let mut contradiction_outcome = journal_pair[1].clone();
    write_be_u64(&mut contradiction_outcome, JOURNAL_HEADER_BYTES + 1, 7);
    contradiction_outcome[JOURNAL_HEADER_BYTES + 9..JOURNAL_HEADER_BYTES + 41].fill(0x22);
    reseal_journal(&mut contradiction_outcome);
    let contradiction = vec![journal_pair[0].clone(), contradiction_outcome];
    models.extend([
        ModelVector {
            name: "journal_valid_intent_outcome_pair",
            input: "authenticated sequence-0 intent plus sequence-1 committed outcome".into(),
            expected: "J_TRANSACTION_ACCEPTED",
            actual: oracle_label(validate_journal_transaction(journal_pair)),
        },
        ModelVector {
            name: "journal_intent_only_reconstruction",
            input: "authenticated sequence-0 intent; outcome absent".into(),
            expected: "J_INTENT_ONLY_RECOVERABLE",
            actual: oracle_label(validate_journal_transaction(&journal_pair[..1])),
        },
        ModelVector {
            name: "journal_duplicate_sequence",
            input: "two authenticated sequence-0 frames".into(),
            expected: "J_DUPLICATE_SEQUENCE",
            actual: oracle_label(validate_journal_transaction(&duplicate)),
        },
        ModelVector {
            name: "journal_outcome_without_intent",
            input: "authenticated sequence-1 outcome only".into(),
            expected: "J_OUTCOME_WITHOUT_INTENT",
            actual: oracle_label(validate_journal_transaction(&outcome_only)),
        },
        ModelVector {
            name: "journal_profile_mismatch",
            input: "outcome ProfileId differs from intent; both resealed".into(),
            expected: "J_PROFILE_MISMATCH",
            actual: oracle_label(validate_journal_transaction(&profile_mismatch)),
        },
        ModelVector {
            name: "journal_transaction_mismatch",
            input: "outcome transaction_id differs from intent; both resealed".into(),
            expected: "J_TRANSACTION_MISMATCH",
            actual: oracle_label(validate_journal_transaction(&tx_mismatch)),
        },
        ModelVector {
            name: "journal_contradicting_committed_outcome",
            input: "committed result selects authenticated old tuple; resealed".into(),
            expected: "J_OUTCOME_CONTRADICTION",
            actual: oracle_label(validate_journal_transaction(&contradiction)),
        },
    ]);

    for (name, class, operation) in [
        ("catalog_gate_forbids_io", GateClass::CatalogSwitch, "io"),
        (
            "catalog_gate_forbids_allocation",
            GateClass::CatalogSwitch,
            "allocation",
        ),
        (
            "catalog_gate_forbids_wait",
            GateClass::CatalogSwitch,
            "wait",
        ),
        (
            "catalog_gate_forbids_enumeration",
            GateClass::CatalogSwitch,
            "enumeration",
        ),
        (
            "catalog_gate_forbids_mark",
            GateClass::CatalogSwitch,
            "mark",
        ),
        ("read_gate_forbids_io", GateClass::ReadPin, "io"),
        (
            "read_gate_forbids_allocation",
            GateClass::ReadPin,
            "allocation",
        ),
        ("read_gate_forbids_wait", GateClass::ReadPin, "wait"),
        (
            "read_gate_forbids_enumeration",
            GateClass::ReadPin,
            "enumeration",
        ),
        ("read_gate_forbids_mark", GateClass::ReadPin, "mark"),
        ("reclamation_gate_forbids_io", GateClass::Reclamation, "io"),
        (
            "reclamation_gate_forbids_allocation",
            GateClass::Reclamation,
            "allocation",
        ),
        (
            "reclamation_gate_forbids_wait",
            GateClass::Reclamation,
            "wait",
        ),
        (
            "reclamation_gate_forbids_enumeration",
            GateClass::Reclamation,
            "enumeration",
        ),
        (
            "reclamation_gate_forbids_mark",
            GateClass::Reclamation,
            "mark",
        ),
    ] {
        models.push(ModelVector {
            name,
            input: format!("{operation} under purpose-scoped gate"),
            expected: "LIFECYCLE_GATE_EFFECT_FORBIDDEN",
            actual: gate_trace_status(class, &[(operation, true)]),
        });
    }

    for (name, selector, bootstrap, expected) in [
        (
            "selector_bootstrap_absent",
            "absent",
            true,
            "BOOTSTRAP_ABSENT_IS_EXACT_OLD_ROLLBACK",
        ),
        (
            "selector_nonbootstrap_absent",
            "absent",
            false,
            "NONBOOTSTRAP_ABSENT_QUARANTINE_BLOCK_READINESS",
        ),
        (
            "selector_invalid",
            "invalid",
            false,
            "OutcomeUnknown_QUARANTINE_BLOCK_READINESS",
        ),
        (
            "selector_truncated",
            "truncated",
            false,
            "OutcomeUnknown_QUARANTINE_BLOCK_READINESS",
        ),
        (
            "selector_unrelated",
            "unrelated",
            false,
            "OutcomeUnknown_QUARANTINE_BLOCK_READINESS",
        ),
        (
            "selector_contradictory",
            "contradictory",
            false,
            "OutcomeUnknown_QUARANTINE_BLOCK_READINESS",
        ),
    ] {
        models.push(ModelVector {
            name,
            input: format!(
                "intent-only recovery observes selector={selector}; bootstrap_old={bootstrap}"
            ),
            expected,
            actual: catalog_recovery_status(selector, bootstrap),
        });
    }

    let crash_names = [
        "crash_before_private_pack_file_fence",
        "crash_private_pack_fenced_before_final_install_fences",
        "crash_final_pack_installed_before_catalog_installed",
        "crash_final_catalog_installed_before_intent_fence",
        "crash_intent_fenced_before_selector_rename",
        "crash_selector_rename_entered_directory_fence_inconclusive",
        "crash_selector_directory_fenced_before_outcome_fence",
        "crash_outcome_fenced",
    ];
    let crash_expected = [
        "OLD_PRIVATE_UNFENCED_RECOVERY_CUSTODY",
        "OLD_PRIVATE_FENCED_RESUME_OR_QUARANTINE",
        "OLD_FINAL_PACK_INSTALLED_ORPHAN",
        "OLD_UNSELECTED_CLOSURE_ORPHAN",
        "OLD_INTENT_ONLY_RECONCILE_SELECTOR",
        "UNKNOWN_OUTCOME_UNKNOWN_RECOVERY_HOLD",
        "NEW_RECONSTRUCT_COMMITTED_OUTCOME",
        "EXACT_RECORDED_RESULT_COMMITTED_NEW_ONLY_RECEIPT",
    ];
    for cut in 1..=8 {
        models.push(ModelVector {
            name: crash_names[cut - 1],
            input: format!("last possibly durable admission cut {cut}"),
            expected: crash_expected[cut - 1],
            actual: crash_cut_status(cut as u8),
        });
    }
    models.extend([
        ModelVector {
            name: "quarantine_ambiguity_blocks_readiness",
            input: "reason-4 ambiguity retained under recovery hold".into(),
            expected: "OutcomeUnknown_QUARANTINE_BLOCK_READINESS",
            actual: catalog_recovery_status("contradictory", false),
        },
        ModelVector {
            name: "journal_capacity_refusal_preserves_unresolved_evidence",
            input: "1024 unresolved frame items plus request for reserved pair".into(),
            expected: "JournalCapacityExhausted_EVIDENCE_PRESERVED_PRE_EFFECT",
            actual: journal_capacity_status(1_024, 4_304_896, 2, 8_408),
        },
    ]);

    for (name, value, expected) in [
        (
            "pack_len_cap_n_minus_1",
            MAX_PACK_BYTES - 1,
            "PACK_CAP_ACCEPTED",
        ),
        ("pack_len_cap_n", MAX_PACK_BYTES, "PACK_CAP_ACCEPTED"),
        (
            "pack_len_cap_n_plus_1",
            MAX_PACK_BYTES + 1,
            "PACK_CAP_REFUSED_PREALLOCATION",
        ),
    ] {
        models.push(ModelVector {
            name,
            input: format!("pack_len={value}"),
            expected,
            actual: pack_len_status(value),
        });
    }
    for (name, value, expected) in [
        (
            "record_count_cap_n_minus_1",
            PACK_RECORDS_MAX - 1,
            "PACK_RECORD_CAP_ACCEPTED",
        ),
        (
            "record_count_cap_n",
            PACK_RECORDS_MAX,
            "PACK_RECORD_CAP_ACCEPTED",
        ),
        (
            "record_count_cap_n_plus_1",
            PACK_RECORDS_MAX + 1,
            "PACK_RECORD_CAP_REFUSED_PREALLOCATION",
        ),
    ] {
        models.push(ModelVector {
            name,
            input: format!("record_count={value}"),
            expected,
            actual: record_count_status(value),
        });
    }
    for (name, value, expected) in [
        (
            "index_len_cap_n_minus_1_entry",
            PACK_INDEX_BYTES_MAX - 80,
            "PACK_INDEX_CAP_ACCEPTED",
        ),
        (
            "index_len_cap_n",
            PACK_INDEX_BYTES_MAX,
            "PACK_INDEX_CAP_ACCEPTED",
        ),
        (
            "index_len_cap_n_plus_1_entry",
            PACK_INDEX_BYTES_MAX + 80,
            "PACK_INDEX_CAP_REFUSED_PREALLOCATION",
        ),
    ] {
        models.push(ModelVector {
            name,
            input: format!("index_len={value}"),
            expected,
            actual: index_len_status(value),
        });
    }
    for (name, value, expected) in [
        (
            "index_len_literal_n_minus_1",
            PACK_INDEX_BYTES_MAX - 1,
            "PACK_INDEX_CAP_REFUSED_PREALLOCATION",
        ),
        (
            "index_len_literal_n_plus_1",
            PACK_INDEX_BYTES_MAX + 1,
            "PACK_INDEX_CAP_REFUSED_PREALLOCATION",
        ),
    ] {
        models.push(ModelVector {
            name,
            input: format!("index_len={value}; exact N±1 byte boundary"),
            expected,
            actual: index_len_status(value),
        });
    }
    let descriptor_owned = checked_owned(GROUP_PRIOR_PACK_DESCRIPTOR_MAX, CATALOG_DESCRIPTOR_BYTES)
        .expect("bounded descriptor accounting");
    let fixed_validator_receipt = checked_byte_window(PACK_VALIDATOR_FIXED_CAPACITY_BYTES)
        .expect("bounded validator accounting");
    let standalone_validator_receipt =
        checked_standalone_validator_receipt().expect("bounded validator accounting");
    let writer_validator_descriptor_receipt = checked_writer_validator_descriptor_receipt()
        .expect("bounded writer and validator accounting");
    models.extend([
        ModelVector {
            name: "group_rollover_unique_record_valid",
            input: "descriptor_count=7; distinct typed key; one prior-pack byte window and one prior-pack FD live"
                .into(),
            expected: "GROUP_UNIQUE_RECORD_ACCEPTED_ONE_PRIOR_WINDOW_FD",
            actual: group_rollover_duplicate_status(
                7,
                false,
                b"authenticated-prior-record",
                b"authenticated-new-record",
                1,
                1,
            ),
        },
        ModelVector {
            name: "group_rollover_unique_record_at_last_appendable_boundary",
            input: "descriptor_count=1023; distinct typed key; the 1024th and final representable pack may be opened only after the full precharge; one prior-pack byte window and one prior-pack FD live"
                .into(),
            expected: "GROUP_UNIQUE_RECORD_ACCEPTED_ONE_PRIOR_WINDOW_FD",
            actual: group_rollover_duplicate_status(
                GROUP_PRIOR_PACK_DESCRIPTOR_MAX - 1,
                false,
                b"authenticated-prior-record",
                b"authenticated-new-record",
                1,
                1,
            ),
        },
        ModelVector {
            name: "group_rollover_exact_equal_reuse_at_descriptor_boundary",
            input: format!(
                "descriptor_count={GROUP_PRIOR_PACK_DESCRIPTOR_MAX}; same typed key; authenticated candidate bytes/length/EOF exactly equal prior record; one prior-pack byte window and one prior-pack FD live"
            ),
            expected: "GROUP_EXACT_EQUAL_REUSED_NO_SECOND_RECORD",
            actual: group_rollover_duplicate_status(
                GROUP_PRIOR_PACK_DESCRIPTOR_MAX,
                true,
                b"authenticated-exact-record-and-eof",
                b"authenticated-exact-record-and-eof",
                1,
                1,
            ),
        },
        ModelVector {
            name: "group_rollover_same_key_different_bytes_hostile",
            input: "descriptor_count=1024; same typed key; authenticated candidate differs from prior record before exact EOF; one prior-pack byte window and one prior-pack FD live"
                .into(),
            expected: "GROUP_SAME_KEY_DIFFERENT_BYTES_FAIL_CLOSED",
            actual: group_rollover_duplicate_status(
                GROUP_PRIOR_PACK_DESCRIPTOR_MAX,
                true,
                b"authenticated-prior-record-and-eof",
                b"authenticated-hostile-record-and-eof",
                1,
                1,
            ),
        },
        ModelVector {
            name: "group_rollover_distinct_record_at_pack_ceiling_hostile",
            input: "descriptor_count=1024; distinct typed key after the group already contains the maximum 1024 pack descriptors; refuse before allocation, open, or append"
                .into(),
            expected: "GROUP_DISTINCT_AT_PACK_CEILING_REFUSED_PRE_EFFECT",
            actual: group_rollover_duplicate_status(
                GROUP_PRIOR_PACK_DESCRIPTOR_MAX,
                false,
                b"authenticated-prior-record",
                b"authenticated-new-record",
                1,
                1,
            ),
        },
        ModelVector {
            name: "group_rollover_descriptor_owned_exact_boundary",
            input: format!(
                "owned({GROUP_PRIOR_PACK_DESCRIPTOR_MAX},{CATALOG_DESCRIPTOR_BYTES})={descriptor_owned}; one prior-pack byte window and one prior-pack FD remain the probe maximum"
            ),
            expected: "GROUP_DESCRIPTOR_OWNED_172096",
            actual: if descriptor_owned == GROUP_PRIOR_PACK_DESCRIPTOR_OWNED_BYTES {
                "GROUP_DESCRIPTOR_OWNED_172096"
            } else {
                "GROUP_DESCRIPTOR_ACCOUNTING_MISMATCH"
            },
        },
        ModelVector {
            name: "group_rollover_descriptor_n_plus_1_hostile_pre_effect_refusal",
            input: format!(
                "descriptor_count={}; existing prior-pack probe stays at one byte window and one FD; no extra open, allocation, or append begins",
                GROUP_PRIOR_PACK_DESCRIPTOR_MAX + 1
            ),
            expected: "GROUP_DESCRIPTOR_N_PLUS_1_REFUSED_PRE_EFFECT",
            actual: group_rollover_duplicate_status(
                GROUP_PRIOR_PACK_DESCRIPTOR_MAX + 1,
                false,
                b"authenticated-prior-record",
                b"authenticated-new-record",
                1,
                1,
            ),
        },
        ModelVector {
            name: "pack_validator_fixed_state_minimum_receipt",
            input: format!(
                "byte_window({PACK_VALIDATOR_FIXED_CAPACITY_BYTES})={fixed_validator_receipt}"
            ),
            expected: "PACK_VALIDATOR_FIXED_RECEIPT_4160",
            actual: if fixed_validator_receipt == PACK_VALIDATOR_FIXED_RECEIPT_BYTES {
                "PACK_VALIDATOR_FIXED_RECEIPT_4160"
            } else {
                "VALIDATOR_ACCOUNTING_MISMATCH"
            },
        },
        ModelVector {
            name: "standalone_validator_minimum_recovery_gc_receipt",
            input: format!(
                "2*byte_window(65536)+byte_window(3728320)+2*byte_window(65536)+byte_window(4096)={standalone_validator_receipt}"
            ),
            expected: "STANDALONE_VALIDATOR_RECEIPT_3994944",
            actual: if standalone_validator_receipt == STANDALONE_VALIDATOR_RECEIPT_BYTES {
                "STANDALONE_VALIDATOR_RECEIPT_3994944"
            } else {
                "VALIDATOR_ACCOUNTING_MISMATCH"
            },
        },
        ModelVector {
            name: "standalone_validator_minimum_recovery_gc_slack",
            input: format!(
                "{RECOVERY_GC_BUCKET_MIN_BYTES}-{standalone_validator_receipt}={} recovery/GC bytes",
                RECOVERY_GC_BUCKET_MIN_BYTES
                    .checked_sub(standalone_validator_receipt)
                    .expect("bounded slack")
            ),
            expected: "RECOVERY_GC_SLACK_199360",
            actual: if RECOVERY_GC_BUCKET_MIN_BYTES.checked_sub(standalone_validator_receipt)
                == Some(RECOVERY_GC_BUCKET_SLACK_BYTES)
            {
                "RECOVERY_GC_SLACK_199360"
            } else {
                "VALIDATOR_SLACK_MISMATCH"
            },
        },
        ModelVector {
            name: "writer_validator_descriptors_minimum_foreground_receipt",
            input: format!(
                "byte_window(65536)+byte_window(65536)+byte_window(131072)+byte_window(262144)+{standalone_validator_receipt}+owned(1024,168)={writer_validator_descriptor_receipt}"
            ),
            expected: "WRITER_VALIDATOR_DESCRIPTORS_RECEIPT_4691584",
            actual: if writer_validator_descriptor_receipt
                == WRITER_VALIDATOR_DESCRIPTOR_RECEIPT_BYTES
            {
                "WRITER_VALIDATOR_DESCRIPTORS_RECEIPT_4691584"
            } else {
                "VALIDATOR_ACCOUNTING_MISMATCH"
            },
        },
        ModelVector {
            name: "writer_validator_descriptors_minimum_foreground_slack",
            input: format!(
                "{FOREGROUND_BUCKET_MIN_BYTES}-{writer_validator_descriptor_receipt}={} foreground bytes",
                FOREGROUND_BUCKET_MIN_BYTES
                    .checked_sub(writer_validator_descriptor_receipt)
                    .expect("bounded slack")
            ),
            expected: "FOREGROUND_SLACK_5794176",
            actual: if FOREGROUND_BUCKET_MIN_BYTES
                .checked_sub(writer_validator_descriptor_receipt)
                == Some(FOREGROUND_BUCKET_SLACK_BYTES)
            {
                "FOREGROUND_SLACK_5794176"
            } else {
                "VALIDATOR_SLACK_MISMATCH"
            },
        },
        ModelVector {
            name: "writer_immediate_validator_indivisible_foreground_owner",
            input: format!(
                "mode=writer_immediate; foreground={writer_validator_descriptor_receipt}; recovery_gc=0"
            ),
            expected: "COMPLETE_WRITER_VALIDATOR_DESCRIPTOR_SET_FOREGROUND_OWNED",
            actual: validator_bucket_owner_status(
                "writer_immediate",
                writer_validator_descriptor_receipt,
                0,
            ),
        },
        ModelVector {
            name: "standalone_validator_indivisible_recovery_gc_owner",
            input: format!(
                "mode=standalone; foreground=0; recovery_gc={standalone_validator_receipt}"
            ),
            expected: "COMPLETE_STANDALONE_VALIDATOR_SET_RECOVERY_GC_OWNED",
            actual: validator_bucket_owner_status(
                "standalone",
                0,
                standalone_validator_receipt,
            ),
        },
        ModelVector {
            name: "validator_split_bucket_owner_hostile",
            input: format!(
                "mode=standalone; foreground=1; recovery_gc={} (same complete validator set split across owners)",
                standalone_validator_receipt - 1
            ),
            expected: "SPLIT_BUCKET_VALIDATOR_OWNERSHIP_FORBIDDEN",
            actual: validator_bucket_owner_status(
                "standalone",
                1,
                standalone_validator_receipt - 1,
            ),
        },
    ]);
    let streaming = simulate_streaming_counters(PACK_RECORDS_MAX);
    models.extend([
        ModelVector {
            name: "minimal_pack_at_record_cap",
            input: format!("checked_minimal_pack_len({PACK_RECORDS_MAX})={}", checked_minimal_pack_len(PACK_RECORDS_MAX).expect("bounded")),
            expected: "MINIMAL_PACK_67108752_ACCEPTED",
            actual: if checked_minimal_pack_len(PACK_RECORDS_MAX) == Ok(67_108_752) { "MINIMAL_PACK_67108752_ACCEPTED" } else { "ARITHMETIC_MISMATCH" },
        },
        ModelVector {
            name: "minimal_pack_above_record_cap",
            input: format!("checked_minimal_pack_len({})={}", PACK_RECORDS_MAX + 1, checked_minimal_pack_len(PACK_RECORDS_MAX + 1).expect("bounded")),
            expected: "MINIMAL_PACK_67108896_REFUSED",
            actual: if checked_minimal_pack_len(PACK_RECORDS_MAX + 1) == Ok(67_108_896) && record_count_status(PACK_RECORDS_MAX + 1) == "PACK_RECORD_CAP_REFUSED_PREALLOCATION" { "MINIMAL_PACK_67108896_REFUSED" } else { "ARITHMETIC_MISMATCH" },
        },
        ModelVector {
            name: "pack_checked_arithmetic_overflow",
            input: "checked_minimal_pack_len(usize::MAX)".into(),
            expected: "PACK_ARITHMETIC_OVERFLOW",
            actual: checked_minimal_pack_len(usize::MAX).expect_err("must overflow"),
        },
        ModelVector {
            name: "run_flush_46604",
            input: "46604 fixed 80-byte entries".into(),
            expected: "ONE_INITIAL_RUN",
            actual: if run_count(46_604) == Ok(1) { "ONE_INITIAL_RUN" } else { "RUN_COUNT_MISMATCH" },
        },
        ModelVector {
            name: "run_flush_46605",
            input: "46605 fixed 80-byte entries".into(),
            expected: "TWO_INITIAL_RUNS",
            actual: if run_count(46_605) == Ok(2) { "TWO_INITIAL_RUNS" } else { "RUN_COUNT_MISMATCH" },
        },
        ModelVector {
            name: "ten_run_two_way_first_merge_file_bound",
            input: "10 input runs plus ceil(10/2)=5 output runs".into(),
            expected: "FIFTEEN_FILES_WITHIN_R28_MIN_16",
            actual: if run_count(PACK_RECORDS_MAX) == Ok(10) && 10 + 5 == PACK_SORT_FILES_MAX { "FIFTEEN_FILES_WITHIN_R28_MIN_16" } else { "FILE_BOUND_MISMATCH" },
        },
        ModelVector {
            name: "spill_exact_boundary",
            input: format!("physical spill charge={PACK_SORT_PHYSICAL_SPILL_MAX}"),
            expected: "SPILL_BOUNDARY_ACCEPTED",
            actual: if PACK_SORT_PHYSICAL_SPILL_MAX < 75_000_000 { "SPILL_BOUNDARY_ACCEPTED" } else { "SPILL_REFUSED" },
        },
        ModelVector {
            name: "spill_subcap_refusal",
            input: "physical spill charge=75000001".into(),
            expected: "SPILL_REFUSED_PRE_EFFECT",
            actual: if 75_000_001usize > 75_000_000 { "SPILL_REFUSED_PRE_EFFECT" } else { "SPILL_ACCEPTED" },
        },
        ModelVector {
            name: "spill_subcap_exact_boundary",
            input: "physical spill charge=75000000".into(),
            expected: "SPILL_SUBCAP_EXACT_ACCEPTED",
            actual: if 75_000_000usize <= 75_000_000 { "SPILL_SUBCAP_EXACT_ACCEPTED" } else { "SPILL_REFUSED" },
        },
        ModelVector {
            name: "bounded_offset_order_validation_at_record_cap",
            input: "466032 synthetic 64-byte record locators iterated without a max-size pack allocation".into(),
            expected: "BOUNDED_STREAMING_VALIDATION_ACCEPTED",
            actual: bounded_offset_validation_status(PACK_RECORDS_MAX),
        },
        ModelVector {
            name: "streaming_construction_residency_counters",
            input: format!(
                "payload_staging={}; whole_pack_resident={}; whole_index_resident={}; max_run_entries={}",
                streaming.payload_staging_bytes,
                streaming.whole_pack_resident_bytes,
                streaming.whole_index_resident_bytes,
                streaming.max_run_entries,
            ),
            expected: "BOUNDED_RESIDENCY_COUNTERS_ACCEPTED",
            actual: if streaming.payload_staging_bytes == 0
                && streaming.whole_pack_resident_bytes == 0
                && streaming.whole_index_resident_bytes == 0
                && streaming.max_run_entries == PACK_SORT_RUN_ENTRIES
            {
                "BOUNDED_RESIDENCY_COUNTERS_ACCEPTED"
            } else {
                "COUNTER_MISMATCH"
            },
        },
        ModelVector {
            name: "changed_source_checksum_and_independent_validation_read_counters",
            input: format!(
                "changed_source_reads_max={}; checksum_construction_carrier_reads={}; completed_carrier_validation_reads={}; completed_validation_independent={}; second_changed_source_ordering_reads={}; second_selected_old_carrier_ordering_reads={}; pack_rewrite_passes={}; whole_pack_resident={}; whole_index_resident={}",
                streaming.changed_source_reads_max,
                streaming.checksum_construction_carrier_reads,
                streaming.completed_carrier_validation_reads,
                streaming.completed_carrier_validation_is_independent,
                streaming.second_changed_source_ordering_reads,
                streaming.second_selected_old_carrier_ordering_reads,
                streaming.pack_rewrite_passes,
                streaming.whole_pack_resident_bytes,
                streaming.whole_index_resident_bytes,
            ),
            expected: "EXACT_ONE_CHECKSUM_READ_ONE_INDEPENDENT_VALIDATION_READ_NO_REREAD_REWRITE_RESIDENCY",
            actual: if streaming.changed_source_reads_max == 1
                && streaming.checksum_construction_carrier_reads == 1
                && streaming.completed_carrier_validation_reads == 1
                && streaming.completed_carrier_validation_is_independent
                && streaming.second_changed_source_ordering_reads == 0
                && streaming.second_selected_old_carrier_ordering_reads == 0
                && streaming.pack_rewrite_passes == 0
                && streaming.whole_pack_resident_bytes == 0
                && streaming.whole_index_resident_bytes == 0
            {
                "EXACT_ONE_CHECKSUM_READ_ONE_INDEPENDENT_VALIDATION_READ_NO_REREAD_REWRITE_RESIDENCY"
            } else {
                "PACK_READ_COUNTER_MISMATCH"
            },
        },
    ]);

    let (identity_status, identity_input) = identity_invariance_status();
    models.push(ModelVector {
        name: "canonical_object_closure_two_carrier_orders_and_partitions",
        input: identity_input,
        expected: "OBJECT_IDS_AND_VERSION_ID_INVARIANT_PACK_IDS_MAY_DIFFER",
        actual: identity_status,
    });
    models.append(&mut catalog_models);
    models
}

fn run_assertions(
    identities: &[IdentityVector],
    valid_receipts: &[(&'static str, Vec<u8>, ValidationContext)],
    boundaries: &BTreeMap<&'static str, (Vec<u8>, ValidationContext, &'static str)>,
    hostiles: &[HostileVector],
    structural: &[StructuralHostile],
    occupied: &[OccupiedVector],
    dispositions: &[DispositionVector],
    packs: &[PackVector],
    pack_hostiles: &[PackHostile],
    binaries: &[BinaryVector],
    models: &[ModelVector],
) {
    for vector in identities {
        assert_eq!(digest(&vector.preimage), vector.digest, "{}", vector.name);
    }
    let registry = registry_from_identities(identities);
    for hostile in structural {
        assert_eq!(
            validate_structural_object(
                &hostile.bytes,
                hostile.object_type,
                hostile.implicit_root,
                &registry,
            ),
            Err(hostile.expected),
            "{}",
            hostile.name
        );
    }
    for vector in occupied {
        assert_eq!(
            occupied_compare(
                &vector.claimed_id,
                &vector.expected_bytes,
                &vector.stored_bytes,
                vector.oracle,
                vector.object_type,
                vector.implicit_root,
                &registry,
            ),
            vector.expected,
            "{}",
            vector.name
        );
    }
    for (name, bytes, context) in valid_receipts {
        assert_eq!(validate_receipt(bytes, context), Ok("ACCEPTED"), "{name}");
    }
    for (name, (bytes, context, expected)) in boundaries {
        assert_eq!(validate_receipt(bytes, context), Ok(*expected), "{name}");
    }
    for hostile in hostiles {
        assert_eq!(
            validate_receipt(&hostile.bytes, &hostile.context),
            Err(hostile.expected),
            "{}",
            hostile.name
        );
    }
    for disposition in dispositions {
        let actual = validate_receipt(&disposition.bytes, &disposition.context);
        let expected = if disposition.is_ok {
            Ok(disposition.expected)
        } else {
            Err(disposition.expected)
        };
        assert_eq!(actual, expected, "{}", disposition.name);
    }
    for pack in packs {
        assert_eq!(
            validate_pack(&pack.bytes),
            Ok("PACK_ACCEPTED"),
            "{}",
            pack.name
        );
    }
    for hostile in pack_hostiles {
        assert_eq!(
            validate_pack(&hostile.bytes),
            Err(hostile.expected),
            "{}",
            hostile.name
        );
    }
    for vector in binaries {
        assert_eq!(
            oracle_label(vector.actual),
            vector.expected,
            "{}",
            vector.name
        );
    }
    for vector in models {
        assert_eq!(vector.actual, vector.expected, "{}", vector.name);
    }
    assert_eq!(3_149_824usize, RECEIPT_PROCESSING_MAX);
    assert!(3_149_824usize < 33_554_432);
    assert_eq!(
        PACK_RECORDS_MAX * PACK_INDEX_ENTRY_BYTES,
        PACK_INDEX_BYTES_MAX
    );
    assert_eq!(PACK_SORT_RUN_ENTRIES * PACK_INDEX_ENTRY_BYTES, 3_728_320);
    assert_eq!(PACK_SORT_INITIAL_RUNS_MAX + 5, PACK_SORT_FILES_MAX);
    assert_eq!(PACK_SORT_BYTE_WINDOW, 3_728_320 + 64);
    assert_eq!(
        PACK_SORT_PHYSICAL_SPILL_MAX,
        2 * PACK_INDEX_BYTES_MAX + PACK_SORT_FILES_MAX * (4_095 + 4_096)
    );
    assert!(PACK_SORT_PHYSICAL_SPILL_MAX < 75_000_000);
    assert_eq!(
        JOURNAL_HEADER_BYTES + JOURNAL_PAYLOAD_MAX + JOURNAL_CHECKSUM_BYTES,
        JOURNAL_FRAME_BYTES_MAX
    );
    assert_eq!(QREC_CHECKSUM_AT + 32, QREC_BYTES);
    assert_eq!(CATALOG_DESCRIPTOR_BYTES, 168);
    assert_eq!(checked_byte_window(4_096), Ok(4_160));
    assert_eq!(
        checked_owned(GROUP_PRIOR_PACK_DESCRIPTOR_MAX, CATALOG_DESCRIPTOR_BYTES),
        Ok(GROUP_PRIOR_PACK_DESCRIPTOR_OWNED_BYTES)
    );
    assert_eq!(
        checked_standalone_validator_receipt(),
        Ok(STANDALONE_VALIDATOR_RECEIPT_BYTES)
    );
    assert_eq!(
        RECOVERY_GC_BUCKET_MIN_BYTES - STANDALONE_VALIDATOR_RECEIPT_BYTES,
        RECOVERY_GC_BUCKET_SLACK_BYTES
    );
    assert_eq!(
        checked_writer_validator_descriptor_receipt(),
        Ok(WRITER_VALIDATOR_DESCRIPTOR_RECEIPT_BYTES)
    );
    assert_eq!(
        FOREGROUND_BUCKET_MIN_BYTES - WRITER_VALIDATOR_DESCRIPTOR_RECEIPT_BYTES,
        FOREGROUND_BUCKET_SLACK_BYTES
    );
}

fn coverage_and_auth(bytes: &[u8]) -> ([u8; 32], [u8; 32]) {
    let facts_len = read_u32(bytes, 152).expect("facts") as usize;
    let coverage_at = 156 + facts_len;
    let auth_at = coverage_at + 37;
    (
        array32(bytes, coverage_at).expect("coverage"),
        array32(bytes, auth_at).expect("auth"),
    )
}

fn render() -> String {
    let identities = identity_vectors();
    let minimal_fields = base_receipt();
    let minimal_bytes = encode_receipt(&minimal_fields);
    let mut minimal_context = receipt_context(&minimal_fields);
    minimal_context.last_sequence_final = 0;
    let nontrivial_fields = nontrivial_receipt();
    let nontrivial_bytes = encode_receipt(&nontrivial_fields);
    let mut nontrivial_context = receipt_context(&nontrivial_fields);
    nontrivial_context.last_sequence_final = 99;
    let valid_receipts = vec![
        (
            "receipt_minimal_no_change",
            minimal_bytes.clone(),
            minimal_context,
        ),
        (
            "receipt_nontrivial",
            nontrivial_bytes.clone(),
            nontrivial_context,
        ),
    ];
    let boundaries = boundary_receipts();
    let hostiles = hostile_vectors();
    let structural = structural_hostiles(&identities);
    let occupied = occupied_vectors(&identities);
    let dispositions = disposition_vectors();
    let packs = pack_vectors();
    let pack_hostiles = pack_hostiles(&packs);
    let reason2_private_pack = packs
        .iter()
        .find(|pack| pack.name == "pack_minimal_one_chunk")
        .expect("reason-2 private carrier fixture")
        .bytes
        .clone();
    let (journal_binaries, journal_pair) = journal_vectors();
    let (qrec_binaries, qrec_bytes) = qrec_vectors(&reason2_private_pack);
    let (catalog_binaries, catalog_models) = catalog_vectors();
    let binaries = journal_binaries
        .into_iter()
        .chain(qrec_binaries)
        .chain(catalog_binaries)
        .collect::<Vec<_>>();
    let models = model_vectors(
        &journal_pair,
        &qrec_bytes,
        &reason2_private_pack,
        catalog_models,
    );
    run_assertions(
        &identities,
        &valid_receipts,
        &boundaries,
        &hostiles,
        &structural,
        &occupied,
        &dispositions,
        &packs,
        &pack_hostiles,
        &binaries,
        &models,
    );

    let mut out = String::new();
    writeln!(&mut out, "{GENERATED_BEGIN}").expect("write");
    writeln!(&mut out, "## Generated structural identity vectors\n").expect("write");
    writeln!(
        &mut out,
        "| Vector | Preimage bytes | Exact preimage hex | BLAKE3-256 ID |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---:|---|---|").expect("write");
    for vector in &identities {
        writeln!(
            &mut out,
            "| `{}` | {} | `{}` | `{}` |",
            vector.name,
            vector.preimage.len(),
            hex(&vector.preimage),
            hex(&vector.digest)
        )
        .expect("write");
    }

    writeln!(&mut out, "\n## Generated valid receipt vectors\n").expect("write");
    for (name, bytes, _) in &valid_receipts {
        let (coverage, auth) = coverage_and_auth(bytes);
        writeln!(&mut out, "### `{name}`\n").expect("write");
        writeln!(&mut out, "- Encoded bytes: `{}`", bytes.len()).expect("write");
        writeln!(
            &mut out,
            "- Exact receipt BLAKE3-256: `{}`",
            hex(&digest(bytes))
        )
        .expect("write");
        writeln!(&mut out, "- Coverage digest: `{}`", hex(&coverage)).expect("write");
        writeln!(&mut out, "- Issuer authentication: `{}`", hex(&auth)).expect("write");
        writeln!(&mut out, "- Exact bytes: `{}`\n", hex(bytes)).expect("write");
    }

    writeln!(&mut out, "## Generated accepted law and boundary vectors\n").expect("write");
    writeln!(
        &mut out,
        "| Vector | Encoded bytes | BLAKE3-256 | Expected |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---:|---|---|").expect("write");
    for (name, (bytes, _, expected)) in &boundaries {
        writeln!(
            &mut out,
            "| `{name}` | {} | `{}` | `{expected}` |",
            bytes.len(),
            hex(&digest(bytes))
        )
        .expect("write");
    }

    writeln!(&mut out, "\n## Generated structural hostile vectors\n").expect("write");
    writeln!(
        &mut out,
        "| Vector | Mutated bytes | BLAKE3-256 | Expected |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---:|---|---|").expect("write");
    for hostile in &structural {
        writeln!(
            &mut out,
            "| `{name}` | {} | `{}` | `{expected}` |",
            hostile.bytes.len(),
            hex(&digest(&hostile.bytes)),
            name = hostile.name,
            expected = hostile.expected,
        )
        .expect("write");
    }
    writeln!(&mut out, "\n## Generated occupied-ID comparison vectors\n").expect("write");
    writeln!(
        &mut out,
        "| Vector | Oracle | Claimed ID | Recomputed expected ID | Recomputed stored ID | Expected |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---|---|---|---|---|").expect("write");
    for vector in &occupied {
        writeln!(
            &mut out,
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |",
            vector.name,
            match vector.oracle {
                OccupiedIdOracle::Blake3 => "BLAKE3_RECOMPUTED",
                OccupiedIdOracle::ForcedSameId => "FORCED_COLLISION_AFTER_RECOMPUTE",
            },
            hex(&vector.claimed_id),
            hex(&digest(&vector.expected_bytes)),
            hex(&digest(&vector.stored_bytes)),
            vector.expected_label,
        )
        .expect("write");
    }

    writeln!(&mut out, "\n## Generated receipt hostile vectors\n").expect("write");
    writeln!(
        &mut out,
        "| Vector | Base | Exact mutation | Mutated bytes | BLAKE3-256 | Expected |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---|---|---:|---|---|").expect("write");
    for hostile in &hostiles {
        writeln!(
            &mut out,
            "| `{}` | `{}` | {} | {} | `{}` | `{}` |",
            hostile.name,
            hostile.base,
            hostile.mutation,
            hostile.bytes.len(),
            hex(&digest(&hostile.bytes)),
            hostile.expected
        )
        .expect("write");
    }
    writeln!(
        &mut out,
        "\n## Generated typed pre-effect disposition vectors\n"
    )
    .expect("write");
    writeln!(
        &mut out,
        "| Vector | Base | Exact context/mutation | Bytes | BLAKE3-256 | Expected |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---|---|---:|---|---|").expect("write");
    for disposition in &dispositions {
        writeln!(
            &mut out,
            "| `{}` | `{}` | {} | {} | `{}` | `{}` |",
            disposition.name,
            disposition.base,
            disposition.mutation,
            disposition.bytes.len(),
            hex(&digest(&disposition.bytes)),
            disposition.expected,
        )
        .expect("write");
    }

    writeln!(&mut out, "\n## Generated valid physical-pack vectors\n").expect("write");
    for pack in &packs {
        let physical = pack
            .physical_keys
            .iter()
            .map(|(kind, id)| format!("{kind:02x}:{}", hex(id)))
            .collect::<Vec<_>>()
            .join(", ");
        let index = pack
            .index_keys
            .iter()
            .map(|(kind, id)| format!("{kind:02x}:{}", hex(id)))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(&mut out, "### `{}`\n", pack.name).expect("write");
        writeln!(&mut out, "- Encoded bytes: `{}`", pack.bytes.len()).expect("write");
        writeln!(
            &mut out,
            "- Exact pack BLAKE3-256: `{}`",
            hex(&digest(&pack.bytes))
        )
        .expect("write");
        writeln!(
            &mut out,
            "- PackId / authenticated trailer checksum: `{}`",
            hex(&pack.bytes[pack.bytes.len() - 32..])
        )
        .expect("write");
        writeln!(&mut out, "- Physical discovery order: `{physical}`").expect("write");
        writeln!(&mut out, "- Embedded key order: `{index}`").expect("write");
        writeln!(&mut out, "- Exact bytes: `{}`\n", hex(&pack.bytes)).expect("write");
    }

    writeln!(&mut out, "## Generated physical-pack hostile vectors\n").expect("write");
    writeln!(
        &mut out,
        "| Vector | Base | Exact mutation | Mutated bytes | BLAKE3-256 | Expected |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---|---|---:|---|---|").expect("write");
    for hostile in &pack_hostiles {
        writeln!(
            &mut out,
            "| `{}` | `{}` | {} | {} | `{}` | `{}` |",
            hostile.name,
            hostile.base,
            hostile.mutation,
            hostile.bytes.len(),
            hex(&digest(&hostile.bytes)),
            hostile.expected,
        )
        .expect("write");
    }

    writeln!(
        &mut out,
        "\n## Generated exact journal, quarantine-record, and catalog-descriptor vectors\n"
    )
    .expect("write");
    writeln!(
        &mut out,
        "| Vector | Base | Exact mutation | Bytes | BLAKE3-256 | Expected | Exact accepted bytes |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---|---|---:|---|---|---|").expect("write");
    for vector in &binaries {
        let exact = if vector.render_exact {
            format!("`{}`", hex(&vector.bytes))
        } else {
            "—".into()
        };
        writeln!(
            &mut out,
            "| `{}` | `{}` | {} | {} | `{}` | `{}` | {} |",
            vector.name,
            vector.base,
            vector.mutation,
            vector.bytes.len(),
            hex(&digest(&vector.bytes)),
            vector.expected,
            exact,
        )
        .expect("write");
    }

    writeln!(&mut out, "\n## Generated bounded-pack arithmetic vectors\n").expect("write");
    writeln!(&mut out, "| Law | Exact value | Expected |").expect("write");
    writeln!(&mut out, "|---|---:|---|").expect("write");
    writeln!(
        &mut out,
        "| maximum pack bytes | {} | `PACK_CAP_ACCEPTED` |",
        MAX_PACK_BYTES
    )
    .expect("write");
    writeln!(
        &mut out,
        "| maximum record metadata entries | {} | `PACK_RECORD_CAP_ACCEPTED` |",
        PACK_RECORDS_MAX
    )
    .expect("write");
    writeln!(
        &mut out,
        "| maximum embedded-index bytes (`{} * 80`) | {} | `PACK_INDEX_CAP_ACCEPTED` |",
        PACK_RECORDS_MAX, PACK_INDEX_BYTES_MAX
    )
    .expect("write");
    writeln!(
        &mut out,
        "| fixed metadata entries per initial run | {} | `RUN_ENTRY_CAP_ACCEPTED` |",
        PACK_SORT_RUN_ENTRIES
    )
    .expect("write");
    writeln!(
        &mut out,
        "| initial run byte window (`{} * 80 + 64`) | {} | `A7_BYTE_WINDOW_ACCEPTED` |",
        PACK_SORT_RUN_ENTRIES, PACK_SORT_BYTE_WINDOW
    )
    .expect("write");
    writeln!(
        &mut out,
        "| initial runs / first-pass input+output files | `{}/{}` | `R28_MIN_16_ACCEPTED` |",
        PACK_SORT_INITIAL_RUNS_MAX, PACK_SORT_FILES_MAX
    )
    .expect("write");
    writeln!(
        &mut out,
        "| physical spill maximum | {} | `BELOW_75000000` |",
        PACK_SORT_PHYSICAL_SPILL_MAX
    )
    .expect("write");

    writeln!(
        &mut out,
        "\n## Generated path, collision, lifecycle, crash, catalog, identity, and bounded-resource model vectors\n"
    )
    .expect("write");
    writeln!(
        &mut out,
        "| Vector | Exact input | Input BLAKE3-256 | Expected |"
    )
    .expect("write");
    writeln!(&mut out, "|---|---|---|---|").expect("write");
    for vector in &models {
        writeln!(
            &mut out,
            "| `{}` | `{}` | `{}` | `{}` |",
            vector.name,
            vector.input,
            hex(&digest(vector.input.as_bytes())),
            vector.expected,
        )
        .expect("write");
    }
    writeln!(&mut out, "\n{GENERATED_END}").expect("write");
    out
}

fn check_document(rendered: &str, path: &Path) -> Result<(), String> {
    let document =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let begin = document
        .find(GENERATED_BEGIN)
        .ok_or("generated begin marker missing")?;
    let relative_end = document[begin..]
        .find(GENERATED_END)
        .ok_or("generated end marker missing")?;
    let end = begin + relative_end + GENERATED_END.len();
    let committed = &document[begin..end];
    let expected = rendered.trim_end();
    if committed != expected {
        return Err("generated vector block differs from M6_0_GOLDEN_VECTORS.md".into());
    }
    Ok(())
}

fn write_document(rendered: &str, path: &Path) -> Result<(), String> {
    let document =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let begin = document
        .find(GENERATED_BEGIN)
        .ok_or("generated begin marker missing")?;
    let relative_end = document[begin..]
        .find(GENERATED_END)
        .ok_or("generated end marker missing")?;
    let end = begin + relative_end + GENERATED_END.len();
    let mut updated = String::with_capacity(document.len() + rendered.len());
    updated.push_str(&document[..begin]);
    updated.push_str(rendered.trim_end());
    updated.push_str(&document[end..]);
    fs::write(path, updated).map_err(|error| format!("{}: {error}", path.display()))
}

fn main() {
    let rendered = render();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../M6_0_GOLDEN_VECTORS.md");
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.iter().any(|argument| argument == "--check") {
        if let Err(error) = check_document(&rendered, &path) {
            eprintln!("M6.0 vector check failed: {error}");
            std::process::exit(1);
        }
        println!("M6.0 vector check PASS");
    } else if arguments.iter().any(|argument| argument == "--write") {
        if let Err(error) = write_document(&rendered, &path) {
            eprintln!("M6.0 vector write failed: {error}");
            std::process::exit(1);
        }
        println!("M6.0 vector block regenerated");
    } else {
        print!("{rendered}");
    }
}
