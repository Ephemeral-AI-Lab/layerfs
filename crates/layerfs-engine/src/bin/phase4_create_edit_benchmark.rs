//! WP4-M private candidate campaign.
//!
//! This executable is intentionally the only profile selector.  It owns the
//! candidate-only SQLite schema and never opens the production v1 engine.

use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use blake3::Hasher;
use layerfs_core::cdc::FastCdc;
use layerfs_core::content::persistence as file_codec;
use layerfs_core::cow::persistence as dir_codec;
use layerfs_core::delta::codec as delta_codec;
use layerfs_core::object::{DirectoryEntry, Object, ObjectKind, ObjectReference};
use layerfs_core::{
    chunk_id, decode_object, encode_object as encode_canonical_object, CanonicalName, CoreError,
    CoreResult, ObjectId,
};
use rusqlite::{params, Connection, OptionalExtension};

const SOURCE_100: u64 = 100 * 1024 * 1024;
const SOURCE_512: u64 = 512 * 1024 * 1024;
const DIRECTORY_ENTRIES: usize = 100_000;
const RECEIPT_BYTES: usize = 216;
const MAX_DEPTH: usize = 256;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug)]
struct Candidate {
    name: &'static str,
    k: usize,
    f: usize,
    directory_page: usize,
}

const FILE_CANDIDATES: [Candidate; 3] = [
    Candidate {
        name: "K64-F64",
        k: 64,
        f: 64,
        directory_page: 256 * 1024,
    },
    Candidate {
        name: "K59-F101",
        k: 59,
        f: 101,
        directory_page: 256 * 1024,
    },
    Candidate {
        name: "K256-F256",
        k: 256,
        f: 256,
        directory_page: 256 * 1024,
    },
];

const DIR_CANDIDATES: [Candidate; 3] = [
    Candidate {
        name: "DIR64K",
        k: 64,
        f: 64,
        directory_page: 64 * 1024,
    },
    Candidate {
        name: "DIR256K",
        k: 64,
        f: 64,
        directory_page: 256 * 1024,
    },
    Candidate {
        name: "DIR1M",
        k: 64,
        f: 64,
        directory_page: 1024 * 1024,
    },
];

#[derive(Clone, Copy, Debug, Default)]
struct Metrics {
    sql_statements: u64,
    sql_rows: u64,
    blob_opens: u64,
    transactions: u64,
    commits: u64,
    objects_created: u64,
    objects_reused: u64,
    objects_authenticated: u64,
    canonical_bytes_authenticated: u64,
    canonical_bytes_written: u64,
    mapping_bytes_rewritten: u64,
    closure_occurrences: u64,
    chunks: u64,
    references: u64,
    pages: u64,
    branches: u64,
    suffix_references: u64,
    q_high_water: u64,
    w_bytes: u64,
    d_bytes: u64,
}

fn add(value: &mut u64, amount: u64) -> CoreResult<()> {
    *value = value.checked_add(amount).ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn add_len(value: &mut u64, amount: usize) -> CoreResult<()> {
    add(
        value,
        u64::try_from(amount).map_err(|_| CoreError::LengthOverflow)?,
    )
}

fn profile_id(candidate: Candidate) -> CoreResult<[u8; 32]> {
    let mut bytes = Vec::with_capacity(8 + 4 * 4);
    bytes.extend_from_slice(b"layerfs/mapping-profile/wp4m/v1\0");
    bytes.extend_from_slice(
        &u32::try_from(candidate.k)
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(
        &u32::try_from(candidate.f)
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(
        &u32::try_from(candidate.directory_page)
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&(8 * 1024 * 1024_u32).to_be_bytes());
    Ok(*blake3::hash(&bytes).as_bytes())
}

fn receipt(
    profile: [u8; 32],
    generation: u64,
    child: ObjectId,
    transition: ObjectId,
) -> [u8; RECEIPT_BYTES] {
    let mut output = [0_u8; RECEIPT_BYTES];
    output[..32].copy_from_slice(&profile);
    output[32..40].copy_from_slice(&generation.to_be_bytes());
    output[40..72].copy_from_slice(child.as_bytes());
    output[72..104].copy_from_slice(transition.as_bytes());
    let mut hash = Hasher::new();
    hash.update(b"layerfs/validation-receipt/wp4m/v1\0");
    hash.update(&profile);
    hash.update(&generation.to_be_bytes());
    hash.update(child.as_bytes());
    hash.update(transition.as_bytes());
    output[104..136].copy_from_slice(hash.finalize().as_bytes());
    output
}

struct Store {
    path: PathBuf,
    profile: [u8; 32],
    connection: Connection,
}

impl Store {
    fn open(path: &Path, candidate: Candidate) -> AnyResult<Self> {
        let connection = Connection::open(path)?;
        connection.query_row("PRAGMA journal_mode=DELETE", [], |row| {
            row.get::<_, String>(0)
        })?;
        connection.execute_batch(
            "PRAGMA synchronous=FULL; PRAGMA temp_store=FILE; PRAGMA mmap_size=0;",
        )?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS wp4m_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                profile_id BLOB NOT NULL,
                schema_version INTEGER NOT NULL,
                journal_mode TEXT NOT NULL,
                synchronous INTEGER NOT NULL,
                temp_store INTEGER NOT NULL,
                mmap_size INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS wp4m_objects (
                object_id BLOB PRIMARY KEY,
                kind INTEGER NOT NULL,
                canonical_length BLOB NOT NULL,
                canonical_bytes BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS wp4m_visible_head (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                generation BLOB NOT NULL,
                child BLOB NOT NULL,
                transition BLOB NOT NULL,
                validation_receipt BLOB NOT NULL
            );",
        )?;
        let profile = profile_id(candidate)?;
        let existing: Option<Vec<u8>> = connection
            .query_row("SELECT profile_id FROM wp4m_meta WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;
        match existing {
            Some(value) if value.as_slice() != profile.as_slice() => {
                return Err(CoreError::PublicationConflict.into())
            }
            None => {
                connection.execute(
                    "INSERT INTO wp4m_meta (id, profile_id, schema_version, journal_mode, synchronous, temp_store, mmap_size)
                     VALUES (1, ?1, 2, 'delete', 2, 1, 0)",
                    params![profile.as_slice()],
                )?;
            }
            Some(_) => {}
        }
        Ok(Self {
            path: path.to_path_buf(),
            profile,
            connection,
        })
    }

    fn begin(&mut self, metrics: &mut Metrics) -> AnyResult<()> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        add(&mut metrics.transactions, 1)?;
        Ok(())
    }

    fn put(&mut self, id: ObjectId, canonical: &[u8], metrics: &mut Metrics) -> AnyResult<()> {
        layerfs_core::validate_identity(canonical, id)?;
        add(&mut metrics.objects_authenticated, 1)?;
        add_len(&mut metrics.canonical_bytes_authenticated, canonical.len())?;
        add_len(&mut metrics.w_bytes, canonical.len())?;
        metrics.q_high_water = metrics
            .q_high_water
            .max(u64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?);
        let existing: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT canonical_bytes FROM wp4m_objects WHERE object_id = ?1",
                params![id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        add(&mut metrics.sql_statements, 1)?;
        if let Some(existing) = existing {
            layerfs_core::validate_identity(&existing, id)?;
            if existing != canonical {
                return Err(CoreError::IdentityMismatch.into());
            }
            add(&mut metrics.objects_reused, 1)?;
            return Ok(());
        }
        let object = decode_object(canonical)?;
        self.connection.execute(
            "INSERT INTO wp4m_objects (object_id, kind, canonical_length, canonical_bytes)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                id.as_bytes().as_slice(),
                object.kind() as u8,
                i64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?,
                canonical
            ],
        )?;
        add(&mut metrics.sql_statements, 1)?;
        add(&mut metrics.objects_created, 1)?;
        add_len(&mut metrics.canonical_bytes_written, canonical.len())?;
        Ok(())
    }

    fn get(&self, id: ObjectId, metrics: &mut Metrics) -> AnyResult<(Object, Vec<u8>)> {
        let bytes: Vec<u8> = self.connection.query_row(
            "SELECT canonical_bytes FROM wp4m_objects WHERE object_id = ?1",
            params![id.as_bytes().as_slice()],
            |row| row.get(0),
        )?;
        add(&mut metrics.sql_statements, 1)?;
        add(&mut metrics.sql_rows, 1)?;
        add(&mut metrics.blob_opens, 1)?;
        add(&mut metrics.objects_authenticated, 1)?;
        add_len(&mut metrics.canonical_bytes_authenticated, bytes.len())?;
        add_len(&mut metrics.d_bytes, bytes.len())?;
        metrics.q_high_water = metrics
            .q_high_water
            .max(u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?);
        let object = layerfs_core::validate_identity(&bytes, id)?;
        Ok((object, bytes))
    }

    fn current_head(&self) -> AnyResult<Option<(u64, ObjectId, ObjectId, Vec<u8>)>> {
        self.connection
            .query_row(
                "SELECT generation, child, transition, validation_receipt FROM wp4m_visible_head WHERE id = 1",
                [],
                |row| {
                    let generation: Vec<u8> = row.get(0)?;
                    let child: Vec<u8> = row.get(1)?;
                    let transition: Vec<u8> = row.get(2)?;
                    let validation_receipt: Vec<u8> = row.get(3)?;
                    Ok((generation, child, transition, validation_receipt))
                },
            )
            .optional()?
            .map(|(generation, child, transition, validation_receipt)| {
                let generation = u64::from_be_bytes(generation.try_into().map_err(|_| CoreError::InvalidValidationReceipt)?);
                let child = ObjectId::from_bytes(&child)?;
                let transition = ObjectId::from_bytes(&transition)?;
                if validation_receipt.len() != RECEIPT_BYTES {
                    return Err(CoreError::InvalidValidationReceipt.into());
                }
                if validation_receipt != receipt(self.profile, generation, child, transition) {
                    return Err(CoreError::InvalidValidationReceipt.into());
                }
                Ok((generation, child, transition, validation_receipt))
            })
            .transpose()
    }

    fn publish(
        &mut self,
        child: ObjectId,
        transition: ObjectId,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        let generation = self
            .current_head()?
            .map_or(0, |head| head.0)
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let receipt_bytes = receipt(self.profile, generation, child, transition);
        self.connection.execute(
            "INSERT INTO wp4m_visible_head (id, generation, child, transition, validation_receipt)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET generation=excluded.generation, child=excluded.child,
                 transition=excluded.transition, validation_receipt=excluded.validation_receipt",
            params![
                generation.to_be_bytes().as_slice(),
                child.as_bytes().as_slice(),
                transition.as_bytes().as_slice(),
                receipt_bytes.as_slice()
            ],
        )?;
        add(&mut metrics.sql_statements, 1)?;
        self.connection.execute_batch("COMMIT")?;
        add(&mut metrics.commits, 1)?;
        Ok(())
    }

    fn physical_bytes(&self) -> (Option<u64>, Option<u64>) {
        let db = fs::metadata(&self.path).ok().map(|metadata| metadata.len());
        let mut journal = self.path.as_os_str().to_os_string();
        journal.push("-journal");
        (
            db,
            fs::metadata(PathBuf::from(journal))
                .ok()
                .map(|metadata| metadata.len()),
        )
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.connection.execute_batch("ROLLBACK");
    }
}

struct FileBuilder {
    candidate: Candidate,
    leaf: Vec<file_codec::FileReference>,
    levels: Vec<Vec<file_codec::FileChild>>,
    total_raw: u64,
    references: u64,
}

impl FileBuilder {
    fn new(candidate: Candidate) -> Self {
        Self {
            candidate,
            leaf: Vec::with_capacity(candidate.k),
            levels: Vec::new(),
            total_raw: 0,
            references: 0,
        }
    }

    fn push_bytes(
        &mut self,
        store: &mut Store,
        bytes: &[u8],
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        let raw_length = u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
        let raw_id = chunk_id(bytes);
        let canonical = encode_canonical_object(&Object::bytes(bytes.to_vec())?)?;
        let object_id = ObjectId::for_bytes(&canonical);
        store.put(object_id, &canonical, metrics)?;
        add(&mut metrics.chunks, 1)?;
        self.push_reference(
            store,
            file_codec::FileReference {
                raw_id,
                raw_length,
                object_id,
            },
            metrics,
        )
    }

    fn push_reference(
        &mut self,
        store: &mut Store,
        reference: file_codec::FileReference,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        self.total_raw = self
            .total_raw
            .checked_add(u64::from(reference.raw_length))
            .ok_or(CoreError::LengthOverflow)?;
        self.references = self
            .references
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        add(&mut metrics.references, 1)?;
        self.leaf.push(reference);
        if self.leaf.len() == self.candidate.k {
            self.flush_leaf_with_store(store, metrics)?;
        }
        Ok(())
    }

    fn flush_level_with_store(
        &mut self,
        store: &mut Store,
        level: usize,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        let children = std::mem::take(&mut self.levels[level]);
        let branch_level = u8::try_from(level + 1).map_err(|_| CoreError::MappingDepthExceeded)?;
        let inner = file_codec::encode_file_branch(branch_level, &children)?;
        let canonical = encode_canonical_object(&Object::bytes(inner)?)?;
        let id = ObjectId::for_bytes(&canonical);
        store.put(id, &canonical, metrics)?;
        add(&mut metrics.branches, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
        let end = children.last().map_or(0, |child| child.cumulative_end);
        self.push_node_with_store(
            store,
            level + 1,
            file_codec::FileChild {
                object_id: id,
                cumulative_end: end,
            },
            metrics,
        )
    }

    fn push_node_with_store(
        &mut self,
        store: &mut Store,
        level: usize,
        child: file_codec::FileChild,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        while self.levels.len() <= level {
            self.levels.push(Vec::with_capacity(self.candidate.f));
        }
        self.levels[level].push(child);
        if self.levels[level].len() == self.candidate.f {
            self.flush_level_with_store(store, level, metrics)?;
        }
        Ok(())
    }

    fn finish(mut self, store: &mut Store, metrics: &mut Metrics) -> AnyResult<ObjectId> {
        self.flush_leaf_with_store(store, metrics)?;
        loop {
            let nonempty: Vec<usize> = self
                .levels
                .iter()
                .enumerate()
                .filter_map(|(index, children)| (!children.is_empty()).then_some(index))
                .collect();
            if nonempty.len() <= 1 {
                break;
            }
            let level = nonempty[0];
            self.flush_level_with_store(store, level, metrics)?;
        }
        let level = match self
            .levels
            .iter()
            .enumerate()
            .find_map(|(index, children)| (!children.is_empty()).then_some(index))
        {
            Some(level) => level,
            None => 0,
        };
        let children = if self.levels.is_empty() {
            Vec::new()
        } else {
            std::mem::take(&mut self.levels[level])
        };
        let inner = file_codec::encode_file_root(
            0,
            self.total_raw,
            self.references,
            u8::try_from(level).map_err(|_| CoreError::MappingDepthExceeded)?,
            &children,
        )?;
        let canonical = encode_canonical_object(&Object::bytes(inner)?)?;
        let id = ObjectId::for_bytes(&canonical);
        store.put(id, &canonical, metrics)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
        Ok(id)
    }

    fn flush_leaf_with_store(&mut self, store: &mut Store, metrics: &mut Metrics) -> AnyResult<()> {
        if self.leaf.is_empty() {
            return Ok(());
        }
        let inner = file_codec::encode_file_leaf(&self.leaf)?;
        let canonical = encode_canonical_object(&Object::bytes(inner)?)?;
        let id = ObjectId::for_bytes(&canonical);
        store.put(id, &canonical, metrics)?;
        add(&mut metrics.pages, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
        self.push_node_with_store(
            store,
            0,
            file_codec::FileChild {
                object_id: id,
                cumulative_end: self.total_raw,
            },
            metrics,
        )?;
        self.leaf.clear();
        Ok(())
    }
}

fn canonical_bytes(inner: Vec<u8>) -> AnyResult<(ObjectId, Vec<u8>)> {
    let canonical = encode_canonical_object(&Object::bytes(inner)?)?;
    let id = ObjectId::for_bytes(&canonical);
    Ok((id, canonical))
}

fn put_mapping(store: &mut Store, inner: Vec<u8>, metrics: &mut Metrics) -> AnyResult<ObjectId> {
    let (id, canonical) = canonical_bytes(inner)?;
    store.put(id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok(id)
}

fn source_path(root: &Path, size: u64) -> PathBuf {
    root.join(if size == SOURCE_100 {
        "source-100m-v2.bin"
    } else {
        "source-512m-v2.bin"
    })
}

fn fill_source(path: &Path, size: u64, seed: u64) -> AnyResult<()> {
    let mut file = File::create(path)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut written = 0_u64;
    while written < size {
        for (index, byte) in buffer.iter_mut().enumerate() {
            let value = written
                .checked_add(u64::try_from(index).map_err(|_| CoreError::LengthOverflow)?)
                .and_then(|value| value.checked_add(seed))
                .ok_or(CoreError::LengthOverflow)?;
            let mut mixed = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
            mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            *byte = (mixed ^ (mixed >> 31)) as u8;
        }
        let remaining = size.checked_sub(written).ok_or(CoreError::LengthOverflow)?;
        let take = usize::try_from(
            remaining.min(u64::try_from(buffer.len()).map_err(|_| CoreError::LengthOverflow)?),
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        file.write_all(&buffer[..take])?;
        written = written
            .checked_add(u64::try_from(take).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    file.sync_all()?;
    Ok(())
}

fn prepare_sources(root: &Path) -> AnyResult<()> {
    fs::create_dir_all(root)?;
    for (size, seed) in [(SOURCE_100, 0x51_u64), (SOURCE_512, 0xa7_u64)] {
        let path = source_path(root, size);
        let current = fs::metadata(&path).ok().map(|metadata| metadata.len());
        if current != Some(size) {
            fill_source(&path, size, seed)?;
        }
    }
    Ok(())
}

fn source_hash(path: &Path) -> AnyResult<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = [0_u8; 1024 * 1024];
    let mut length = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length = length
            .checked_add(u64::try_from(read).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok((length, hasher.finalize().to_hex().to_string()))
}

fn make_reference(
    store: &mut Store,
    bytes: &[u8],
    metrics: &mut Metrics,
) -> AnyResult<file_codec::FileReference> {
    let raw_length = u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
    let raw_id = chunk_id(bytes);
    let canonical = encode_canonical_object(&Object::bytes(bytes.to_vec())?)?;
    let object_id = ObjectId::for_bytes(&canonical);
    store.put(object_id, &canonical, metrics)?;
    add(&mut metrics.chunks, 1)?;
    Ok(file_codec::FileReference {
        raw_id,
        raw_length,
        object_id,
    })
}

fn publish_transition(
    store: &mut Store,
    parent: Option<ObjectId>,
    child: ObjectId,
    entry_count: u32,
    pages: &[ObjectId],
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let inner = match parent {
        Some(parent) => delta_codec::encode_change(parent, child, entry_count, pages)?,
        None => delta_codec::encode_genesis(child)?,
    };
    let transition = put_mapping(store, inner, metrics)?;
    store.publish(child, transition, metrics)?;
    Ok(transition)
}

fn build_file(
    store: &mut Store,
    source: &Path,
    candidate: Candidate,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    store.begin(metrics)?;
    let mut builder = FileBuilder::new(candidate);
    FastCdc::new().scan(File::open(source)?, |chunk| {
        builder
            .push_bytes(store, chunk, metrics)
            .map_err(|_| CoreError::Io)
    })?;
    let root = builder.finish(store, metrics)?;
    let transition = publish_transition(store, None, root, 0, &[], metrics)?;
    Ok((root, transition))
}

fn collect_file_refs(
    store: &Store,
    root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<Vec<file_codec::FileReference>> {
    let (_, root_bytes) = store.get(root, metrics)?;
    let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let (_, _, level, children) = file_codec::parse_file_root(&payload)?;
    let mut output = Vec::new();
    for child in children {
        collect_file_node(store, child.object_id, level, &mut output, metrics)?;
    }
    Ok(output)
}

fn collect_file_node(
    store: &Store,
    id: ObjectId,
    level: u8,
    output: &mut Vec<file_codec::FileReference>,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    let (_, bytes) = store.get(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        output.extend(file_codec::parse_file_leaf(&payload)?);
        return Ok(());
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let (branch_level, children) = file_codec::parse_file_children(&payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    for child in children {
        collect_file_node(
            store,
            child.object_id,
            level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            output,
            metrics,
        )?;
    }
    Ok(())
}

fn count_file_node(
    store: &Store,
    id: ObjectId,
    level: u8,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)> {
    let (_, bytes) = store.get(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let refs = file_codec::parse_file_leaf(&payload)?;
        let total = refs.iter().try_fold(0_u64, |total, reference| {
            total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)
        })?;
        return Ok((
            u64::try_from(refs.len()).map_err(|_| CoreError::LengthOverflow)?,
            total,
        ));
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let (branch_level, children) = file_codec::parse_file_children(&payload, true)?;
    if branch_level != level || usize::from(level) > MAX_DEPTH {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    let mut count = 0_u64;
    let mut total = 0_u64;
    for child in children {
        let (child_count, child_total) = count_file_node(
            store,
            child.object_id,
            level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            metrics,
        )?;
        count = count
            .checked_add(child_count)
            .ok_or(CoreError::LengthOverflow)?;
        total = total
            .checked_add(child_total)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok((count, total))
}

fn rewrite_same_node(
    store: &mut Store,
    id: ObjectId,
    level: u8,
    target: u64,
    replacement: file_codec::FileReference,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, u64, u64, bool)> {
    let (_, bytes) = store.get(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let mut refs = file_codec::parse_file_leaf(&payload)?;
        let count = u64::try_from(refs.len()).map_err(|_| CoreError::LengthOverflow)?;
        if target >= count {
            let total = refs.iter().try_fold(0_u64, |total, reference| {
                total
                    .checked_add(u64::from(reference.raw_length))
                    .ok_or(CoreError::LengthOverflow)
            })?;
            return Ok((id, count, total, false));
        }
        refs[usize::try_from(target).map_err(|_| CoreError::LengthOverflow)?] = replacement;
        let total = refs.iter().try_fold(0_u64, |total, reference| {
            total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)
        })?;
        let (new_id, canonical) = canonical_bytes(file_codec::encode_file_leaf(&refs)?)?;
        store.put(new_id, &canonical, metrics)?;
        add(&mut metrics.pages, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
        return Ok((new_id, count, total, true));
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let (branch_level, original_children) = file_codec::parse_file_children(&payload, true)?;
    if branch_level != level || usize::from(level) > MAX_DEPTH {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    let mut children = original_children;
    let mut ordinal = 0_u64;
    let mut changed = false;
    for child in &mut children {
        let (child_count, _) = count_file_node(
            store,
            child.object_id,
            level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            metrics,
        )?;
        if !changed
            && target
                < ordinal
                    .checked_add(child_count)
                    .ok_or(CoreError::LengthOverflow)?
        {
            let (new_id, _, new_total, did_change) = rewrite_same_node(
                store,
                child.object_id,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                target
                    .checked_sub(ordinal)
                    .ok_or(CoreError::LengthOverflow)?,
                replacement,
                metrics,
            )?;
            if did_change {
                child.object_id = new_id;
                let _ = new_total;
                changed = true;
            }
        }
        ordinal = ordinal
            .checked_add(child_count)
            .ok_or(CoreError::LengthOverflow)?;
    }
    let (count, total) = count_file_children(
        store,
        &children,
        level
            .checked_sub(1)
            .ok_or(CoreError::MappingDepthExceeded)?,
        metrics,
    )?;
    if !changed {
        return Ok((id, count, total, false));
    }
    let (new_id, canonical) = canonical_bytes(file_codec::encode_file_branch(level, &children)?)?;
    store.put(new_id, &canonical, metrics)?;
    add(&mut metrics.branches, 1)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((new_id, count, total, true))
}

fn count_file_children(
    store: &Store,
    children: &[file_codec::FileChild],
    child_level: u8,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)> {
    let mut count = 0_u64;
    let mut total = 0_u64;
    for child in children {
        let (child_count, child_total) =
            count_file_node(store, child.object_id, child_level, metrics)?;
        count = count
            .checked_add(child_count)
            .ok_or(CoreError::LengthOverflow)?;
        total = total
            .checked_add(child_total)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok((count, total))
}

fn rewrite_same_root(
    store: &mut Store,
    root: ObjectId,
    target: u64,
    replacement: file_codec::FileReference,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, bool)> {
    let (_, bytes) = store.get(root, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let (total_raw, reference_count, level, original_children) =
        file_codec::parse_file_root(&payload)?;
    let mut children = original_children;
    let mut ordinal = 0_u64;
    let mut changed = false;
    for child in &mut children {
        let (child_count, child_total) = count_file_node(store, child.object_id, level, metrics)?;
        if !changed
            && target
                < ordinal
                    .checked_add(child_count)
                    .ok_or(CoreError::LengthOverflow)?
        {
            let (new_id, _, new_total, did_change) = rewrite_same_node(
                store,
                child.object_id,
                level,
                target
                    .checked_sub(ordinal)
                    .ok_or(CoreError::LengthOverflow)?,
                replacement,
                metrics,
            )?;
            if did_change {
                if new_total != child_total {
                    return Err(CoreError::LengthMismatch {
                        expected: child_total,
                        actual: new_total,
                    }
                    .into());
                }
                child.object_id = new_id;
                changed = true;
            }
        }
        ordinal = ordinal
            .checked_add(child_count)
            .ok_or(CoreError::LengthOverflow)?;
    }
    if !changed {
        return Ok((root, false));
    }
    let (new_id, canonical) = canonical_bytes(file_codec::encode_file_root(
        0,
        total_raw,
        reference_count,
        level,
        &children,
    )?)?;
    store.put(new_id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((new_id, true))
}

fn edit_file(
    store: &mut Store,
    _source: &Path,
    candidate: Candidate,
    operation: &str,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    let (_, parent, _, _) = store.current_head()?.ok_or(CoreError::MissingObject)?;
    let refs = collect_file_refs(store, parent, metrics)?;
    let position = if operation.contains("early") {
        0
    } else {
        refs.len() / 2
    };
    let replacement = if operation == "same-middle" {
        let length = match refs.get(position) {
            Some(reference) => {
                usize::try_from(reference.raw_length).map_err(|_| CoreError::LengthOverflow)?
            }
            None => 1,
        };
        vec![0x5a; length]
    } else {
        vec![0xa5]
    };
    if operation == "same-middle" {
        store.begin(metrics)?;
        let replacement = make_reference(store, &replacement, metrics)?;
        add(
            &mut metrics.references,
            u64::try_from(refs.len()).map_err(|_| CoreError::LengthOverflow)?,
        )?;
        if position >= refs.len() {
            return Err(CoreError::InvalidRange {
                start: position as u64,
                end: position as u64,
                length: refs.len() as u64,
            }
            .into());
        }
        let (root, changed) = rewrite_same_root(
            store,
            parent,
            u64::try_from(position).map_err(|_| CoreError::LengthOverflow)?,
            replacement,
            metrics,
        )?;
        if !changed {
            return Err(CoreError::MissingObject.into());
        }
        let transition = publish_transition(store, Some(parent), root, 0, &[], metrics)?;
        return Ok((root, transition));
    }
    store.begin(metrics)?;
    let inserted = make_reference(store, &replacement, metrics)?;
    let mut edited =
        Vec::with_capacity(refs.len().checked_add(1).ok_or(CoreError::LengthOverflow)?);
    edited.extend_from_slice(&refs[..position.min(refs.len())]);
    edited.push(inserted);
    edited.extend_from_slice(&refs[position.min(refs.len())..]);
    metrics.suffix_references = u64::try_from(refs.len().saturating_sub(position))
        .map_err(|_| CoreError::LengthOverflow)?;
    let mut builder = FileBuilder::new(candidate);
    for reference in edited {
        builder.push_reference(store, reference, metrics)?;
    }
    let root = builder.finish(store, metrics)?;
    let transition = publish_transition(store, Some(parent), root, 0, &[], metrics)?;
    Ok((root, transition))
}

fn stream_file(
    store: &Store,
    id: ObjectId,
    level: u8,
    hasher: &mut Hasher,
    length: &mut u64,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    let (object, bytes) = store.get(id, metrics)?;
    add(&mut metrics.closure_occurrences, 1)?;
    let payload = match level {
        0 => file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?,
        _ => file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?,
    };
    let _ = object;
    if level == 0 {
        for reference in file_codec::parse_file_leaf(&payload)? {
            let (chunk, canonical) = store.get(reference.object_id, metrics)?;
            add(&mut metrics.closure_occurrences, 1)?;
            let Object::Bytes(raw) = chunk else {
                return Err(CoreError::WrongLogicalRole.into());
            };
            if u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                != reference.raw_length
            {
                return Err(CoreError::ChunkLengthMismatch.into());
            }
            if chunk_id(&raw) != reference.raw_id {
                return Err(CoreError::ChunkIdentityMismatch.into());
            }
            let _ = canonical;
            hasher.update(&raw);
            *length = length
                .checked_add(u64::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
        }
        return Ok(());
    }
    let (branch_level, children) = file_codec::parse_file_children(&payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    for child in children {
        stream_file(
            store,
            child.object_id,
            level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            hasher,
            length,
            metrics,
        )?;
    }
    Ok(())
}

fn read_file_range(
    store: &Store,
    root: ObjectId,
    range: std::ops::Range<u64>,
    metrics: &mut Metrics,
) -> AnyResult<Vec<u8>> {
    if range.start > range.end {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: 0,
        }
        .into());
    }
    let requested = usize::try_from(
        range
            .end
            .checked_sub(range.start)
            .ok_or(CoreError::LengthOverflow)?,
    )
    .map_err(|_| CoreError::LengthOverflow)?;
    let mut output = Vec::with_capacity(requested);
    let (_, bytes) = store.get(root, metrics)?;
    add(&mut metrics.closure_occurrences, 1)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let (total, _, level, children) = file_codec::parse_file_root(&payload)?;
    if range.end > total {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: total,
        }
        .into());
    }
    let mut previous = 0_u64;
    for child in children {
        let child_start = previous;
        previous = child.cumulative_end;
        if child.cumulative_end <= range.start || child_start >= range.end {
            continue;
        }
        route_file_range(
            store,
            child.object_id,
            level,
            child_start,
            &range,
            &mut output,
            metrics,
        )?;
    }
    Ok(output)
}

fn route_file_range(
    store: &Store,
    id: ObjectId,
    level: u8,
    node_start: u64,
    range: &std::ops::Range<u64>,
    output: &mut Vec<u8>,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    let (_, bytes) = store.get(id, metrics)?;
    add(&mut metrics.closure_occurrences, 1)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let refs = file_codec::parse_file_leaf(&payload)?;
        let mut offset = node_start;
        for reference in refs {
            let end = offset
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
            if end > range.start && offset < range.end && reference.raw_length != 0 {
                let (object, _) = store.get(reference.object_id, metrics)?;
                add(&mut metrics.closure_occurrences, 1)?;
                let Object::Bytes(raw) = object else {
                    return Err(CoreError::WrongLogicalRole.into());
                };
                if chunk_id(&raw) != reference.raw_id
                    || u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?
                        != reference.raw_length
                {
                    return Err(CoreError::ChunkIdentityMismatch.into());
                }
                let start = usize::try_from(range.start.saturating_sub(offset))
                    .map_err(|_| CoreError::LengthOverflow)?;
                let finish = usize::try_from(range.end.min(end) - offset)
                    .map_err(|_| CoreError::LengthOverflow)?;
                output.extend_from_slice(&raw[start..finish]);
            }
            offset = end;
        }
        return Ok(());
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let (branch_level, children) = file_codec::parse_file_children(&payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    let mut previous = node_start;
    for child in children {
        let child_start = previous;
        previous = child.cumulative_end;
        if child.cumulative_end > range.start && child_start < range.end {
            route_file_range(
                store,
                child.object_id,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                child_start,
                range,
                output,
                metrics,
            )?;
        }
    }
    Ok(())
}

fn verify_file(
    store: &Store,
    root: ObjectId,
    source: Option<&Path>,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    let (_, root_bytes) = store.get(root, metrics)?;
    let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let (expected_length, _, level, _) = file_codec::parse_file_root(&payload)?;
    let mut hasher = Hasher::new();
    let mut length = 0_u64;
    let source_fingerprint = source
        .map(source_hash)
        .transpose()?
        .map(|(_, fingerprint)| fingerprint);
    let (_, root_object) = store.get(root, metrics)?;
    let root_payload = file_codec::decode_mapping(&root_object, file_codec::FILE_ROOT_TAG)?;
    let (_, _, root_level, children) = file_codec::parse_file_root(&root_payload)?;
    for child in children {
        stream_file(
            store,
            child.object_id,
            root_level,
            &mut hasher,
            &mut length,
            metrics,
        )?;
    }
    let reconstructed_fingerprint = hasher.finalize().to_hex().to_string();
    if length != expected_length
        || level != root_level
        || source_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| fingerprint != reconstructed_fingerprint)
    {
        return Err(CoreError::LengthMismatch {
            expected: expected_length,
            actual: length,
        }
        .into());
    }
    let probes = [
        0_u64..0_u64,
        0..1,
        expected_length.saturating_sub(1)..expected_length,
        4095..4097,
        expected_length..expected_length,
    ];
    for range in probes {
        let _ = read_file_range(store, root, range, metrics)?;
    }
    Ok(())
}

fn empty_file_root(store: &mut Store, metrics: &mut Metrics) -> AnyResult<ObjectId> {
    let inner = file_codec::encode_file_root(0, 0, 0, 0, &[])?;
    put_mapping(store, inner, metrics)
}

fn directory_name(number: usize) -> AnyResult<CanonicalName> {
    CanonicalName::from_bytes(format!("{number:08}-{}", "x".repeat(246)).as_bytes())
        .map_err(Into::into)
}

fn page_capacity(candidate: Candidate) -> usize {
    candidate
        .directory_page
        .saturating_sub(13)
        .checked_div(292)
        .unwrap_or(1)
        .max(1)
}

fn page_object(
    store: &mut Store,
    entries: &[DirectoryEntry],
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, Vec<u8>)> {
    let canonical = dir_codec::encode_directory_page(entries)?;
    let id = ObjectId::for_bytes(&canonical);
    store.put(id, &canonical, metrics)?;
    add(&mut metrics.pages, 1)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((id, canonical))
}

fn directory_entries(
    first: usize,
    count: usize,
    child: ObjectId,
) -> AnyResult<Vec<DirectoryEntry>> {
    let mut entries = Vec::with_capacity(count);
    for number in first..first.checked_add(count).ok_or(CoreError::LengthOverflow)? {
        entries.push(DirectoryEntry::new(
            directory_name(number)?,
            ObjectReference::new(ObjectKind::Bytes, child),
        ));
    }
    Ok(entries)
}

fn build_directory(
    store: &mut Store,
    candidate: Candidate,
    leading: bool,
    replacement: bool,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    store.begin(metrics)?;
    let child = empty_file_root(store, metrics)?;
    let replacement_child = if replacement {
        let inner = file_codec::encode_file_root(1, 0, 0, 0, &[])?;
        Some(put_mapping(store, inner, metrics)?)
    } else {
        None
    };
    let capacity = page_capacity(candidate);
    let mut pages = Vec::new();
    let mut start = 1_usize;
    let total = if leading {
        DIRECTORY_ENTRIES
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?
    } else {
        DIRECTORY_ENTRIES
    };
    while start <= total {
        let count = capacity.min(
            total
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?
                .saturating_sub(start),
        );
        let first_number = if leading {
            start.saturating_sub(1)
        } else {
            start
        };
        let entries = directory_entries(
            first_number,
            count,
            if replacement
                && start <= DIRECTORY_ENTRIES / 2
                && start.checked_add(count).ok_or(CoreError::LengthOverflow)?
                    > DIRECTORY_ENTRIES / 2
            {
                replacement_child.ok_or(CoreError::MissingObject)?
            } else {
                child
            },
        )?;
        let (id, _) = page_object(store, &entries, metrics)?;
        pages.push(dir_codec::DirectoryPageRef {
            count: u32::try_from(count).map_err(|_| CoreError::LengthOverflow)?,
            first_name: entries[0].name().as_bytes().to_vec(),
            object_id: id,
        });
        start = start.checked_add(count).ok_or(CoreError::LengthOverflow)?;
    }
    let metadata = put_mapping(store, dir_codec::encode_directory_metadata(0)?, metrics)?;
    let index = put_mapping(
        store,
        dir_codec::encode_directory_index(
            u32::try_from(total).map_err(|_| CoreError::LengthOverflow)?,
            &pages,
        )?,
        metrics,
    )?;
    let wrapper = dir_codec::encode_directory_wrapper(metadata, index)?;
    let root = ObjectId::for_bytes(&wrapper);
    store.put(root, &wrapper, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, wrapper.len())?;
    let transition = publish_transition(
        store,
        None,
        root,
        u32::try_from(total).map_err(|_| CoreError::LengthOverflow)?,
        &pages.iter().map(|page| page.object_id).collect::<Vec<_>>(),
        metrics,
    )?;
    Ok((root, transition))
}

fn directory_parts(
    store: &Store,
    root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<Vec<dir_codec::DirectoryPageRef>> {
    let (object, _) = store.get(root, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    let mut index = None;
    for entry in entries {
        if entry.name().as_bytes() == b"t" {
            index = Some(entry.reference().id());
        }
    }
    let index = index.ok_or(CoreError::MissingObject)?;
    let (_, bytes) = store.get(index, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::DIR_INDEX_TAG)?;
    Ok(dir_codec::parse_directory_index(&payload)?)
}

fn directory_page_entries(
    store: &Store,
    id: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<Vec<DirectoryEntry>> {
    let (object, _) = store.get(id, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    Ok(entries)
}

fn edit_directory(
    store: &mut Store,
    candidate: Candidate,
    operation: &str,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    let (_, parent, _, _) = store.current_head()?.ok_or(CoreError::MissingObject)?;
    let old_pages = directory_parts(store, parent, metrics)?;
    store.begin(metrics)?;
    let child = if operation == "dir-replace" {
        put_mapping(
            store,
            file_codec::encode_file_root(1, 0, 0, 0, &[])?,
            metrics,
        )?
    } else {
        empty_file_root(store, metrics)?
    };
    let mut pages = Vec::new();
    if operation == "dir-replace" {
        let target = DIRECTORY_ENTRIES / 2;
        let mut seen = 0_usize;
        for page in old_pages {
            let mut entries = directory_page_entries(store, page.object_id, metrics)?;
            if target >= seen
                && target
                    < seen
                        .checked_add(entries.len())
                        .ok_or(CoreError::LengthOverflow)?
            {
                let local = target.checked_sub(seen).ok_or(CoreError::LengthOverflow)?;
                entries[local] = DirectoryEntry::new(
                    entries[local].name().clone(),
                    ObjectReference::new(ObjectKind::Bytes, child),
                );
                let (id, _) = page_object(store, &entries, metrics)?;
                pages.push(dir_codec::DirectoryPageRef {
                    count: page.count,
                    first_name: entries[0].name().as_bytes().to_vec(),
                    object_id: id,
                });
            } else {
                pages.push(page);
            }
            seen = seen
                .checked_add(entries.len())
                .ok_or(CoreError::LengthOverflow)?;
        }
    } else {
        let capacity = page_capacity(candidate);
        let total = DIRECTORY_ENTRIES
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let mut start = 0_usize;
        while start < total {
            let count = capacity.min(total.checked_sub(start).ok_or(CoreError::LengthOverflow)?);
            let entries = directory_entries(start, count, child)?;
            let (id, _) = page_object(store, &entries, metrics)?;
            pages.push(dir_codec::DirectoryPageRef {
                count: u32::try_from(count).map_err(|_| CoreError::LengthOverflow)?,
                first_name: entries[0].name().as_bytes().to_vec(),
                object_id: id,
            });
            start = start.checked_add(count).ok_or(CoreError::LengthOverflow)?;
        }
    }
    let metadata = put_mapping(store, dir_codec::encode_directory_metadata(0)?, metrics)?;
    let index = put_mapping(
        store,
        dir_codec::encode_directory_index(
            u32::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                .map_err(|_| CoreError::LengthOverflow)?,
            &pages,
        )?,
        metrics,
    )?;
    let wrapper = dir_codec::encode_directory_wrapper(metadata, index)?;
    let root = ObjectId::for_bytes(&wrapper);
    store.put(root, &wrapper, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, wrapper.len())?;
    let transition = publish_transition(
        store,
        Some(parent),
        root,
        u32::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
            .map_err(|_| CoreError::LengthOverflow)?,
        &pages.iter().map(|page| page.object_id).collect::<Vec<_>>(),
        metrics,
    )?;
    Ok((root, transition))
}

fn verify_directory(
    store: &Store,
    root: ObjectId,
    expected_entries: u64,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    let pages = directory_parts(store, root, metrics)?;
    let mut total = 0_u64;
    for page in pages {
        let entries = directory_page_entries(store, page.object_id, metrics)?;
        if entries.len() != usize::try_from(page.count).map_err(|_| CoreError::LengthOverflow)?
            || entries.first().map(|entry| entry.name().as_bytes())
                != Some(page.first_name.as_slice())
        {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        for entry in entries {
            let (child, _) = store.get(entry.reference().id(), metrics)?;
            if child.kind() != ObjectKind::Bytes {
                return Err(CoreError::WrongLogicalRole.into());
            }
            add(&mut metrics.closure_occurrences, 1)?;
            total = total.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
    }
    if total != expected_entries {
        return Err(CoreError::LengthMismatch {
            expected: expected_entries,
            actual: total,
        }
        .into());
    }
    Ok(())
}

fn apparent_store_bytes(store: &Store) -> u64 {
    let (db, journal) = store.physical_bytes();
    db.unwrap_or(0).saturating_add(journal.unwrap_or(0))
}

fn hex_id(id: ObjectId) -> String {
    id.to_string()
}

fn profile_hex(candidate: Candidate) -> AnyResult<String> {
    Ok(profile_id(candidate)?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn row_json(
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
    warmup: bool,
    source_fingerprint: &str,
    started: Instant,
    capture_ns: u128,
    verification_ns: u128,
    root: ObjectId,
    transition: ObjectId,
    metrics: Metrics,
    store_bytes: u64,
    error: Option<&str>,
) -> AnyResult<String> {
    let profile = profile_hex(candidate)?;
    let status = if error.is_some() { "FAIL" } else { "PASS" };
    let error_json = error.map_or_else(
        || "null".to_string(),
        |value| format!("\"{}\"", value.replace('"', "'")),
    );
    Ok(format!(
        "{{\"qualification\":false,\"purpose\":\"profile_selection\",\"status\":\"{status}\",\"candidate\":\"{}\",\"profile_id\":\"{profile}\",\"size_bytes\":{size},\"operation\":\"{operation}\",\"iteration\":{iteration},\"warmup\":{warmup},\"source_fingerprint\":\"{source_fingerprint}\",\"root_id\":\"{}\",\"transition_id\":\"{}\",\"capture_publish_wall_ns\":{capture_ns},\"sqlite_qualification_wall_ns\":{verification_ns},\"elapsed_wall_ns\":{},\"cpu_ns\":\"Unavailable\",\"rss_bytes\":\"Unavailable\",\"allocated_store_delta_bytes\":\"Unavailable\",\"apparent_db_plus_journal_bytes\":{store_bytes},\"q_high_water\":{},\"w_bytes\":{},\"d_bytes\":{},\"sql_statements\":{},\"sql_rows\":{},\"blob_opens\":{},\"transactions\":{},\"commits\":{},\"objects_created\":{},\"objects_reused\":{},\"objects_authenticated\":{},\"canonical_bytes_authenticated\":{},\"canonical_bytes_written\":{},\"mapping_bytes_rewritten\":{},\"closure_occurrences\":{},\"chunks\":{},\"references\":{},\"pages\":{},\"branches\":{},\"suffix_references\":{},\"error\":{error_json}}}",
        candidate.name,
        hex_id(root),
        hex_id(transition),
        started.elapsed().as_nanos(),
        metrics.q_high_water,
        metrics.w_bytes,
        metrics.d_bytes,
        metrics.sql_statements,
        metrics.sql_rows,
        metrics.blob_opens,
        metrics.transactions,
        metrics.commits,
        metrics.objects_created,
        metrics.objects_reused,
        metrics.objects_authenticated,
        metrics.canonical_bytes_authenticated,
        metrics.canonical_bytes_written,
        metrics.mapping_bytes_rewritten,
        metrics.closure_occurrences,
        metrics.chunks,
        metrics.references,
        metrics.pages,
        metrics.branches,
        metrics.suffix_references,
    ))
}

fn candidate_by_name(name: &str) -> AnyResult<Candidate> {
    FILE_CANDIDATES
        .iter()
        .chain(DIR_CANDIDATES.iter())
        .find(|candidate| candidate.name == name)
        .copied()
        .ok_or_else(|| format!("unknown candidate {name}").into())
}

fn run_row(
    root: &Path,
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
    warmup: bool,
) -> AnyResult<String> {
    prepare_sources(root)?;
    let source = source_path(root, size);
    let (_, source_fingerprint) = source_hash(&source)?;
    let db_path = root.join(format!(
        "db-{}-{size}-{operation}-{iteration}.sqlite",
        candidate.name
    ));
    if db_path.exists() {
        fs::remove_file(&db_path)?;
    }
    let mut metrics = Metrics::default();
    let started = Instant::now();
    let mut store = Store::open(&db_path, candidate)?;
    let (root_id, transition_id, capture_ns) = if operation == "full" {
        let capture_start = Instant::now();
        let (root_id, transition_id) = build_file(&mut store, &source, candidate, &mut metrics)?;
        (root_id, transition_id, capture_start.elapsed().as_nanos())
    } else if operation == "same-middle"
        || operation == "plus1-early"
        || operation == "plus1-middle"
    {
        let _ = build_file(&mut store, &source, candidate, &mut metrics)?;
        drop(store);
        store = Store::open(&db_path, candidate)?;
        metrics = Metrics::default();
        let capture_start = Instant::now();
        let (root_id, transition_id) =
            edit_file(&mut store, &source, candidate, operation, &mut metrics)?;
        (root_id, transition_id, capture_start.elapsed().as_nanos())
    } else if operation == "dir-create" {
        let capture_start = Instant::now();
        let (root_id, transition_id) =
            build_directory(&mut store, candidate, false, false, &mut metrics)?;
        (root_id, transition_id, capture_start.elapsed().as_nanos())
    } else if operation == "dir-replace" || operation == "dir-leading" {
        let _ = build_directory(&mut store, candidate, false, false, &mut metrics)?;
        drop(store);
        store = Store::open(&db_path, candidate)?;
        metrics = Metrics::default();
        let capture_start = Instant::now();
        let (root_id, transition_id) =
            edit_directory(&mut store, candidate, operation, &mut metrics)?;
        (root_id, transition_id, capture_start.elapsed().as_nanos())
    } else {
        return Err(format!("unknown operation {operation}").into());
    };
    let capture_end_bytes = apparent_store_bytes(&store);
    drop(store);
    let verification_start = Instant::now();
    let store = Store::open(&db_path, candidate)?;
    let head = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    if head.1 != root_id || head.2 != transition_id {
        return Err(CoreError::PublicationConflict.into());
    }
    if operation.starts_with("dir-") {
        verify_directory(
            &store,
            root_id,
            u64::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                .map_err(|_| CoreError::LengthOverflow)?,
            &mut metrics,
        )?;
    } else {
        verify_file(
            &store,
            root_id,
            (operation == "full").then_some(source.as_path()),
            &mut metrics,
        )?;
    }
    let verification_ns = verification_start.elapsed().as_nanos();
    let output = row_json(
        candidate,
        size,
        operation,
        iteration,
        warmup,
        &source_fingerprint,
        started,
        capture_ns,
        verification_ns,
        root_id,
        transition_id,
        metrics,
        capture_end_bytes,
        None,
    )?;
    drop(store);
    Ok(output)
}

fn self_test(root: &Path) -> AnyResult<()> {
    fs::create_dir_all(root)?;
    let source = root.join("self-test.bin");
    fill_source(&source, 256 * 1024, 0x11)?;
    let candidate = FILE_CANDIDATES[0];
    let db = root.join("self-test.sqlite");
    if db.exists() {
        fs::remove_file(&db)?;
    }
    let mut metrics = Metrics::default();
    let mut store = Store::open(&db, candidate)?;
    let (root_id, _) = build_file(&mut store, &source, candidate, &mut metrics)?;
    drop(store);
    let store = Store::open(&db, candidate)?;
    verify_file(&store, root_id, Some(&source), &mut metrics)?;
    let mut malformed = vec![0_u8; 11];
    malformed[..8].copy_from_slice(b"LFS4MAP\0");
    malformed[8..10].copy_from_slice(&2_u16.to_be_bytes());
    if file_codec::decode_mapping(
        &encode_canonical_object(&Object::bytes(malformed)?)?,
        file_codec::FILE_ROOT_TAG,
    )
    .is_ok()
    {
        return Err("malformed mapping accepted".into());
    }
    store.connection.execute(
        "UPDATE wp4m_visible_head SET validation_receipt = zeroblob(215) WHERE id = 1",
        [],
    )?;
    if !matches!(
        store.current_head(),
        Err(error) if error.downcast_ref::<CoreError>() == Some(&CoreError::InvalidValidationReceipt)
    ) {
        return Err("invalid receipt accepted".into());
    }
    println!(
        "self-test PASS root={root_id} objects={} auth_bytes={}",
        metrics.objects_created, metrics.canonical_bytes_authenticated
    );
    Ok(())
}

fn run_campaign(root: &Path) -> AnyResult<()> {
    prepare_sources(root)?;
    let jsonl = root.join("wp4m-profile-selection.jsonl");
    let mut output = File::create(&jsonl)?;
    let mut invocations = 0_usize;
    for iteration in 0..6 {
        let warmup = iteration == 0;
        let mut candidates = FILE_CANDIDATES.to_vec();
        let candidate_count = candidates.len();
        candidates.rotate_left(iteration % candidate_count);
        for candidate in candidates {
            for size in [SOURCE_100, SOURCE_512] {
                for operation in ["full", "same-middle", "plus1-early", "plus1-middle"] {
                    let executable = env::current_exe()?;
                    let result = std::process::Command::new(executable)
                        .arg("--row")
                        .arg(root)
                        .arg(candidate.name)
                        .arg(size.to_string())
                        .arg(operation)
                        .arg(iteration.to_string())
                        .arg(warmup.to_string())
                        .output()?;
                    if !result.status.success() {
                        return Err(format!(
                            "row failed {}: {}",
                            candidate.name,
                            String::from_utf8_lossy(&result.stderr)
                        )
                        .into());
                    }
                    output.write_all(&result.stdout)?;
                    invocations = invocations
                        .checked_add(1)
                        .ok_or(CoreError::LengthOverflow)?;
                }
            }
        }
        let mut directories = DIR_CANDIDATES.to_vec();
        let directory_count = directories.len();
        directories.rotate_left(iteration % directory_count);
        for candidate in directories {
            for operation in ["dir-create", "dir-replace", "dir-leading"] {
                let executable = env::current_exe()?;
                let result = std::process::Command::new(executable)
                    .arg("--row")
                    .arg(root)
                    .arg(candidate.name)
                    .arg(SOURCE_100.to_string())
                    .arg(operation)
                    .arg(iteration.to_string())
                    .arg(warmup.to_string())
                    .output()?;
                if !result.status.success() {
                    return Err(format!(
                        "directory row failed {}: {}",
                        candidate.name,
                        String::from_utf8_lossy(&result.stderr)
                    )
                    .into());
                }
                output.write_all(&result.stdout)?;
                invocations = invocations
                    .checked_add(1)
                    .ok_or(CoreError::LengthOverflow)?;
            }
        }
    }
    output.sync_all()?;
    if invocations != 198 {
        return Err(format!("campaign invocation count {invocations}, expected 198").into());
    }
    println!(
        "campaign PASS invocations={invocations} jsonl={}",
        jsonl.display()
    );
    Ok(())
}

fn main() -> AnyResult<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--self-test") => self_test(Path::new(args.get(2).ok_or("missing self-test root")?)),
        Some("--campaign") => run_campaign(Path::new(args.get(2).ok_or("missing campaign root")?)),
        Some("--row") => {
            let root = Path::new(args.get(2).ok_or("missing row root")?);
            let candidate = candidate_by_name(args.get(3).ok_or("missing candidate")?)?;
            let size = args.get(4).ok_or("missing size")?.parse::<u64>()?;
            let operation = args.get(5).ok_or("missing operation")?;
            let iteration = args.get(6).ok_or("missing iteration")?.parse::<usize>()?;
            let warmup = args.get(7).ok_or("missing warmup")?.parse::<bool>()?;
            println!("{}", run_row(root, candidate, size, operation, iteration, warmup)?);
            Ok(())
        }
        _ => Err("usage: --self-test ROOT | --campaign ROOT | --row ROOT CANDIDATE SIZE OP ITERATION WARMUP".into()),
    }
}
