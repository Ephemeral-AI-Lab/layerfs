//! WP4-M private candidate campaign.
//!
//! This executable is intentionally the only profile selector.  It owns the
//! candidate-only SQLite schema and never opens the production v1 engine.

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use blake3::Hasher;
use layerfs_core::cdc::FastCdc;
use layerfs_core::content::persistence as file_codec;
use layerfs_core::cow::persistence as dir_codec;
use layerfs_core::cow::{RootHandle, TreeNode};
use layerfs_core::delta::codec as delta_codec;
use layerfs_core::object::{DirectoryEntry, Object, ObjectKind, ObjectReference};
use layerfs_core::validation::ValidatedSnapshotReceiptV1;
use layerfs_core::{
    chunk_id, decode_object, encode_object as encode_canonical_object, CanonicalName, CoreError,
    CoreResult, ObjectId,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_100: u64 = 100 * 1024 * 1024;
const SOURCE_512: u64 = 512 * 1024 * 1024;
const RETAINED_CDC_100: u64 = 5_284;
const RETAINED_CDC_512: u64 = 27_162;
const RETAINED_RAW_100: &str = "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7";
const RETAINED_RAW_512: &str = "84f895c546504bd80a343c7c7300b26cc010dad27c7c897efc6f37fc2821efc2";
const RETAINED_CDC_SEQUENCE_100: &str =
    "5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994";
const RETAINED_CDC_SEQUENCE_512: &str =
    "8b9c305cc4e128acbbe16d6aea4d000f3a483604c7b5f914d953bcccd7225d0b";
const RETAINED_SEED: u64 = 0x4c41594552534653;
const DIRECTORY_ENTRIES: usize = 100_000;
const MAX_DEPTH: usize = 256;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;
type StoreMetaRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
type VisibleHeadRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
type VisibleHead = (u64, ObjectId, ObjectId, Vec<u8>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublishFault {
    BeforeCommit,
    AfterCommitBeforeAck,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Reconciliation {
    NotAttempted,
    RequestedVisible,
    PriorVisible,
    DifferentHead,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FailureProvenance {
    first: Option<CoreError>,
    cleanup_first: Option<CoreError>,
    reconciliation: Reconciliation,
    dominant: Option<CoreError>,
}

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
    suffix_bytes: u64,
    suffix_objects: u64,
    q_single_canonical_max: u64,
    source_bytes_read: u64,
    source_cdc_bytes_read: u64,
    canonical_stage_source_bytes_read: u64,
    raw_bytes_hashed: u64,
    w_bytes: u64,
    d_bytes: u64,
}

#[derive(Clone, Debug, Default)]
struct PhaseTimes {
    source_cdc_ns: u128,
    canonical_cas_mapping_stage_ns: u128,
    precommit_closure_validation_ns: u128,
    sqlite_commit_durability_ns: u128,
    durable_capture_total_ns: u128,
    fresh_reopen_head_ns: u128,
    fresh_full_scrub_ns: u128,
    reconstruction_ns: u128,
    range_verification_ns: u128,
    complete_lifecycle_total_ns: u128,
}

#[derive(Clone, Debug)]
struct RangeMeasurement {
    label: &'static str,
    range: std::ops::Range<u64>,
    wall_ns: u128,
    returned_bytes: usize,
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

fn observe_closure(
    hasher: &mut Hasher,
    role: &[u8],
    id: ObjectId,
    canonical: &[u8],
) -> CoreResult<()> {
    hasher.update(
        &u64::try_from(role.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(role);
    hasher.update(id.as_bytes());
    hasher.update(
        &u64::try_from(canonical.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    hasher.update(canonical);
    Ok(())
}

fn combined_closure_digest(transition: [u8; 32], content: [u8; 32]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"layerfs/wp4m/ordered-closure/v1\0");
    hasher.update(&transition);
    hasher.update(&content);
    *hasher.finalize().as_bytes()
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

struct Store {
    path: PathBuf,
    authority_path: PathBuf,
    profile: [u8; 32],
    store_instance_id: [u8; 16],
    validation_authority_id: [u8; 32],
    validation_key: [u8; 32],
    integrity_epoch: u64,
    connection: Connection,
}

fn authority_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".authority");
    PathBuf::from(value)
}

fn new_validation_key(path: &Path, profile: [u8; 32]) -> CoreResult<[u8; 32]> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CoreError::LengthOverflow)?
        .as_nanos();
    let mut hasher = Hasher::new();
    hasher.update(b"layerfs/wp4m/validation-key/v2\0");
    hasher.update(&profile);
    hasher.update(&now.to_be_bytes());
    hasher.update(path.as_os_str().as_encoded_bytes());
    Ok(*hasher.finalize().as_bytes())
}

fn read_authority(path: &Path) -> AnyResult<[u8; 32]> {
    fs::read(path)?
        .try_into()
        .map_err(|_| CoreError::ValidationAuthorityUnavailable.into())
}

fn create_authority(path: &Path, profile: [u8; 32]) -> AnyResult<[u8; 32]> {
    let key = new_validation_key(path, profile)?;
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return read_authority(path);
        }
        Err(error) => return Err(error.into()),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&key)?;
    file.sync_all()?;
    Ok(key)
}

impl Store {
    fn open(path: &Path, candidate: Candidate) -> AnyResult<Self> {
        let connection = Connection::open(path)?;
        let authority_path = authority_path(path);
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
                store_instance_id BLOB NOT NULL,
                validation_authority_id BLOB NOT NULL,
                validation_key BLOB NOT NULL,
                integrity_epoch BLOB NOT NULL,
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
        let existing: Option<StoreMetaRow> = connection
            .query_row(
                "SELECT profile_id, store_instance_id, validation_authority_id, integrity_epoch
                 FROM wp4m_meta WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let (store_instance_id, validation_authority_id, integrity_epoch, validation_key) =
            match existing {
                Some((value, instance, authority, epoch)) => {
                    if value.as_slice() != profile.as_slice()
                        || instance.len() != 16
                        || authority.len() != 32
                        || epoch.len() != 8
                    {
                        return Err(CoreError::PublicationConflict.into());
                    }
                    let validation_key = read_authority(&authority_path)?;
                    (
                        instance
                            .try_into()
                            .map_err(|_| CoreError::InvalidValidationReceipt)?,
                        authority
                            .try_into()
                            .map_err(|_| CoreError::InvalidValidationReceipt)?,
                        u64::from_be_bytes(
                            epoch
                                .try_into()
                                .map_err(|_| CoreError::InvalidValidationReceipt)?,
                        ),
                        validation_key,
                    )
                }
                None => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| CoreError::LengthOverflow)?
                        .as_nanos();
                    let mut instance_hasher = Hasher::new();
                    instance_hasher.update(b"layerfs/wp4m/store-instance/v1\0");
                    instance_hasher.update(&profile);
                    instance_hasher.update(&now.to_be_bytes());
                    instance_hasher.update(path.as_os_str().as_encoded_bytes());
                    let digest = instance_hasher.finalize();
                    let store_instance_id: [u8; 16] = digest.as_bytes()[..16]
                        .try_into()
                        .map_err(|_| CoreError::InvalidValidationReceipt)?;
                    let validation_key = create_authority(&authority_path, profile)?;
                    let validation_authority_id =
                        ValidatedSnapshotReceiptV1::validation_authority_id(
                            store_instance_id,
                            &validation_key,
                        );
                    connection.execute(
                    "INSERT INTO wp4m_meta (id, profile_id, store_instance_id, validation_authority_id,
                         validation_key, integrity_epoch, schema_version, journal_mode, synchronous,
                         temp_store, mmap_size)
                    VALUES (1, ?1, ?2, ?3, ?4, ?5, 5, 'delete', 2, 1, 0)",
                    params![
                        profile.as_slice(),
                        store_instance_id.as_slice(),
                        validation_authority_id.as_slice(),
                        [0_u8; 32].as_slice(),
                        1_u64.to_be_bytes().as_slice()
                    ],
                )?;
                    (
                        store_instance_id,
                        validation_authority_id,
                        1,
                        validation_key,
                    )
                }
            };
        if validation_authority_id
            != ValidatedSnapshotReceiptV1::validation_authority_id(
                store_instance_id,
                &validation_key,
            )
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        Ok(Self {
            path: path.to_path_buf(),
            authority_path,
            profile,
            store_instance_id,
            validation_authority_id,
            validation_key,
            integrity_epoch,
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
        metrics.q_single_canonical_max = metrics
            .q_single_canonical_max
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
        metrics.q_single_canonical_max = metrics
            .q_single_canonical_max
            .max(u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?);
        let object = layerfs_core::validate_identity(&bytes, id)?;
        Ok((object, bytes))
    }

    fn current_head(&self) -> AnyResult<Option<VisibleHead>> {
        let row: Option<VisibleHeadRow> = self
            .connection
            .query_row(
                "SELECT generation, child, transition, validation_receipt FROM wp4m_visible_head WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((generation, child, transition, validation_receipt)) = row else {
            return Ok(None);
        };
        if generation.len() != 8 || child.len() != 32 || transition.len() != 32 {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        let generation = u64::from_be_bytes(
            generation
                .try_into()
                .map_err(|_| CoreError::InvalidValidationReceipt)?,
        );
        let child = ObjectId::from_bytes(&child)?;
        let transition = ObjectId::from_bytes(&transition)?;
        let receipt = ValidatedSnapshotReceiptV1::decode(
            &validation_receipt,
            &self.validation_key,
            ObjectId::from_bytes(&self.profile)?,
            self.validation_authority_id,
        )?;
        if receipt.store_instance_id != self.store_instance_id
            || receipt.integrity_epoch != self.integrity_epoch
            || receipt.head_generation != generation
            || receipt.child_root_id != child
            || receipt.transition_id != transition
        {
            return Err(CoreError::InvalidValidationReceipt.into());
        }
        Ok(Some((generation, child, transition, validation_receipt)))
    }

    fn publication_key(
        &self,
        prior: Option<&VisibleHead>,
        requested: &VisibleHead,
    ) -> CoreResult<[u8; 32]> {
        let mut hasher = Hasher::new();
        hasher.update(b"layerfs/publication-idempotency/v1\0");
        hasher.update(&self.store_instance_id);
        match prior {
            Some((generation, child, transition, receipt)) => {
                hasher.update(&[1]);
                hasher.update(&generation.to_be_bytes());
                hasher.update(child.as_bytes());
                hasher.update(transition.as_bytes());
                if receipt.len() != 216 {
                    return Err(CoreError::InvalidValidationReceipt);
                }
                hasher.update(receipt);
            }
            None => {
                hasher.update(&[0]);
            }
        }
        hasher.update(&requested.0.to_be_bytes());
        hasher.update(requested.1.as_bytes());
        hasher.update(requested.2.as_bytes());
        if requested.3.len() != 216 {
            return Err(CoreError::InvalidValidationReceipt);
        }
        hasher.update(&requested.3);
        Ok(*hasher.finalize().as_bytes())
    }

    fn reconcile_publication(
        &self,
        prior: Option<&VisibleHead>,
        requested: &VisibleHead,
        request_key: [u8; 32],
    ) -> Reconciliation {
        let Ok(authoritative) = self.current_head() else {
            return Reconciliation::Ambiguous;
        };
        if authoritative.as_ref() == Some(requested)
            && self
                .publication_key(prior, requested)
                .is_ok_and(|key| key == request_key)
        {
            Reconciliation::RequestedVisible
        } else if authoritative.as_ref() == prior {
            Reconciliation::PriorVisible
        } else {
            Reconciliation::DifferentHead
        }
    }

    fn publish(
        &mut self,
        expected_parent: Option<ObjectId>,
        child: ObjectId,
        transition: ObjectId,
        metrics: &mut Metrics,
    ) -> AnyResult<()> {
        self.publish_with_fault(expected_parent, child, transition, None, metrics)
            .map(|_| ())
    }

    fn publish_with_fault(
        &mut self,
        expected_parent: Option<ObjectId>,
        child: ObjectId,
        transition: ObjectId,
        fault: Option<PublishFault>,
        metrics: &mut Metrics,
    ) -> AnyResult<FailureProvenance> {
        let current = self.current_head()?;
        if current.as_ref().map(|head| head.1) != expected_parent {
            return Err(CoreError::PublicationConflict.into());
        }
        let generation = current
            .as_ref()
            .map_or(0, |head| head.0)
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let receipt_bytes = ValidatedSnapshotReceiptV1 {
            store_instance_id: self.store_instance_id,
            validation_authority_id: self.validation_authority_id,
            integrity_epoch: self.integrity_epoch,
            head_generation: generation,
            child_root_id: child,
            transition_id: transition,
            mapping_profile_id: ObjectId::from_bytes(&self.profile)?,
        }
        .encode(&self.validation_key)?;
        let requested = (generation, child, transition, receipt_bytes.to_vec());
        let request_key = self.publication_key(current.as_ref(), &requested)?;
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
        if fault == Some(PublishFault::BeforeCommit) {
            let cleanup_first = self
                .connection
                .execute_batch("ROLLBACK")
                .err()
                .map(|_| CoreError::Io);
            return Ok(FailureProvenance {
                first: Some(CoreError::Io),
                cleanup_first,
                reconciliation: Reconciliation::NotAttempted,
                dominant: Some(CoreError::Io),
            });
        }
        let next_commits = metrics
            .commits
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        self.connection.execute_batch("COMMIT")?;
        metrics.commits = next_commits;
        if fault == Some(PublishFault::AfterCommitBeforeAck) {
            let reconciliation =
                self.reconcile_publication(current.as_ref(), &requested, request_key);
            return Ok(FailureProvenance {
                first: Some(CoreError::Io),
                cleanup_first: None,
                dominant: match reconciliation {
                    Reconciliation::RequestedVisible => None,
                    Reconciliation::PriorVisible => Some(CoreError::Io),
                    Reconciliation::DifferentHead => Some(CoreError::PublicationConflict),
                    Reconciliation::Ambiguous => Some(CoreError::AmbiguousDurability),
                    Reconciliation::NotAttempted => Some(CoreError::AmbiguousDurability),
                },
                reconciliation,
            });
        }
        Ok(FailureProvenance {
            first: None,
            cleanup_first: None,
            reconciliation: Reconciliation::RequestedVisible,
            dominant: None,
        })
    }

    fn physical_bytes(&self) -> (Option<u64>, Option<u64>, Option<u64>) {
        let db = fs::metadata(&self.path).ok().map(|metadata| metadata.len());
        let mut journal = self.path.as_os_str().to_os_string();
        journal.push("-journal");
        (
            db,
            fs::metadata(PathBuf::from(journal))
                .ok()
                .map(|metadata| metadata.len()),
            fs::metadata(&self.authority_path)
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
    level_totals: Vec<u64>,
    total_raw: u64,
    references: u64,
}

impl FileBuilder {
    fn new(candidate: Candidate) -> Self {
        Self {
            candidate,
            leaf: Vec::with_capacity(candidate.k),
            levels: Vec::new(),
            level_totals: Vec::new(),
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
        add_len(&mut metrics.source_bytes_read, bytes.len())?;
        add_len(&mut metrics.canonical_stage_source_bytes_read, bytes.len())?;
        add_len(&mut metrics.raw_bytes_hashed, bytes.len())?;
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
        self.level_totals[level] = 0;
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
            self.level_totals.push(0);
        }
        let cumulative_end = self.level_totals[level]
            .checked_add(child.cumulative_end)
            .ok_or(CoreError::LengthOverflow)?;
        self.levels[level].push(file_codec::FileChild {
            object_id: child.object_id,
            cumulative_end,
        });
        self.level_totals[level] = cumulative_end;
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
        let mut level = self
            .levels
            .iter()
            .enumerate()
            .find_map(|(index, children)| (!children.is_empty()).then_some(index))
            .unwrap_or_default();
        let mut children = if self.levels.is_empty() {
            Vec::new()
        } else {
            std::mem::take(&mut self.levels[level])
        };
        while level > 0 && children.len() == 1 {
            let (_, bytes) = store.get(children[0].object_id, metrics)?;
            let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
            let (branch_level, branch_children) = file_codec::parse_file_children(&payload, true)?;
            if usize::from(branch_level) != level {
                return Err(CoreError::NonCanonicalOrdering.into());
            }
            children = branch_children;
            level = level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?;
        }
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
        let leaf_total = self.leaf.iter().try_fold(0_u64, |total, reference| {
            total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)
        })?;
        self.push_node_with_store(
            store,
            0,
            file_codec::FileChild {
                object_id: id,
                cumulative_end: leaf_total,
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

fn namespace_file_root(
    store: &mut Store,
    file_root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let name = CanonicalName::from_bytes(b"file")?;
    let canonical = encode_canonical_object(&Object::directory(vec![DirectoryEntry::new(
        name,
        ObjectReference::new(ObjectKind::Bytes, file_root),
    )])?)?;
    let id = ObjectId::for_bytes(&canonical);
    store.put(id, &canonical, metrics)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok(id)
}

fn resolve_namespace_file_root(
    store: &Store,
    root: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let (object, _) = store.get(root, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if entries.len() != 1
        || entries[0].name().as_bytes() != b"file"
        || entries[0].reference().kind() != ObjectKind::Bytes
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let file_root = entries[0].reference().id();
    let (_, bytes) = store.get(file_root, metrics)?;
    file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    Ok(file_root)
}

fn namespace_entry_id(
    store: &Store,
    root: ObjectId,
    name: &[u8],
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let (object, _) = store.get(root, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    entries
        .iter()
        .find(|entry| entry.name().as_bytes() == name)
        .map(|entry| entry.reference().id())
        .ok_or_else(|| CoreError::WrongLogicalRole.into())
}

fn source_path(root: &Path, size: u64) -> PathBuf {
    root.join(if size == SOURCE_100 {
        "S1-100.source"
    } else {
        "S1-512.source"
    })
}

fn fill_source(path: &Path, size: u64, seed: u64) -> AnyResult<()> {
    let mut file = File::create(path)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut written = 0_u64;
    while written < size {
        fill_retained_buffer(
            &mut buffer,
            written,
            if seed == 0x51 { "S1-100" } else { "S1-512" },
        );
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

fn fill_retained_buffer(buffer: &mut [u8], offset: u64, salt: &str) {
    let salt_hash = salt
        .bytes()
        .fold(0_u64, |value, byte| value.rotate_left(5) ^ u64::from(byte));
    let mut state = RETAINED_SEED ^ salt_hash ^ offset;
    for (index, byte) in buffer.iter_mut().enumerate() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let position = offset.wrapping_add(index as u64);
        *byte = if (position / 8192) % 23 == 0 {
            (salt_hash as u8).wrapping_add((position / 8192) as u8)
        } else {
            (state >> 24) as u8
        };
    }
}

fn prepare_sources(root: &Path) -> AnyResult<()> {
    prepare_sources_for(root, &[SOURCE_100, SOURCE_512])
}

fn prepare_sources_for(root: &Path, sizes: &[u64]) -> AnyResult<()> {
    fs::create_dir_all(root)?;
    let mut manifest = String::from(
        "{\"format\":1,\"fixture_origin\":\"phase2-deterministic-retained-generator\",\"fixtures\":[",
    );
    for &size in sizes {
        let path = source_path(root, size);
        if fs::metadata(&path).ok().map(|metadata| metadata.len()) != Some(size) {
            return Err(format!(
                "retained fixture {} is missing; generate/copy it outside the campaign first",
                path.display()
            )
            .into());
        }
        let expected = if size == SOURCE_100 {
            RETAINED_CDC_100
        } else {
            RETAINED_CDC_512
        };
        let expected_raw = if size == SOURCE_100 {
            RETAINED_RAW_100
        } else {
            RETAINED_RAW_512
        };
        let expected_sequence = if size == SOURCE_100 {
            RETAINED_CDC_SEQUENCE_100
        } else {
            RETAINED_CDC_SEQUENCE_512
        };
        let (actual_length, source_fingerprint) = source_hash(&path)?;
        let (chunks, sequence_fingerprint) = source_cdc_sequence(&path)?;
        if actual_length != size {
            return Err(CoreError::LengthMismatch {
                expected: size,
                actual: actual_length,
            }
            .into());
        }
        if chunks != expected {
            return Err(format!(
                "retained fixture {} has {chunks} CDC chunks, expected {expected}",
                path.display()
            )
            .into());
        }
        if source_fingerprint != expected_raw || sequence_fingerprint != expected_sequence {
            return Err(format!(
                "retained fixture {} fingerprint mismatch: raw={} sequence={}",
                path.display(),
                source_fingerprint,
                sequence_fingerprint
            )
            .into());
        }
        if !manifest.ends_with('[') {
            manifest.push(',');
        }
        manifest.push_str(&format!(
            "{{\"name\":\"{}\",\"size_bytes\":{},\"raw_fingerprint\":\"{}\",\"cdc_references\":{},\"cdc_sequence_fingerprint\":\"{}\"}}",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("unknown"),
            size,
            source_fingerprint,
            chunks,
            sequence_fingerprint
        ));
    }
    manifest.push_str("]}\n");
    let manifest_path = root.join("wp4m-retained-fixture-manifest.json");
    let retained_manifest = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "retained fixture manifest {} is missing; custody it outside the campaign: {error}",
            manifest_path.display()
        )
    })?;
    if retained_manifest != manifest {
        return Err(format!(
            "retained fixture manifest {} does not match the frozen raw/CDC fingerprints",
            manifest_path.display()
        )
        .into());
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

fn source_cdc_sequence(path: &Path) -> AnyResult<(u64, String)> {
    let mut sequence_hasher = Hasher::new();
    let mut count = 0_u64;
    FastCdc::new().scan(File::open(path)?, |chunk| {
        sequence_hasher.update(
            &u32::try_from(chunk.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        sequence_hasher.update(chunk_id(chunk).as_bytes());
        count = count.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    Ok((count, sequence_hasher.finalize().to_hex().to_string()))
}

fn timed_source_cdc(path: &Path, metrics: &mut Metrics) -> AnyResult<()> {
    FastCdc::new().scan(File::open(path)?, |chunk| {
        add_len(&mut metrics.source_bytes_read, chunk.len())?;
        add_len(&mut metrics.source_cdc_bytes_read, chunk.len())?;
        Ok(())
    })?;
    Ok(())
}

fn source_edit_point(source: &Path, operation: &str) -> AnyResult<(u64, u64, usize)> {
    let mut references = 0_u64;
    FastCdc::new().scan(File::open(source)?, |_| {
        references = references.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    if references == 0 {
        return Err(CoreError::MissingObject.into());
    }
    let target = if operation.contains("early") {
        0
    } else {
        references / 2
    };
    let mut ordinal = 0_u64;
    let mut offset = 0_u64;
    let mut point = None;
    FastCdc::new().scan(File::open(source)?, |chunk| {
        if ordinal == target {
            point = Some((offset, chunk.len()));
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)
            .map_err(|_| CoreError::Io)?;
        offset = offset
            .checked_add(u64::try_from(chunk.len()).map_err(|_| CoreError::Io)?)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    let (byte_offset, length) = point.ok_or(CoreError::MissingObject)?;
    Ok((references, byte_offset, length))
}

fn boundary_probe(
    label: &'static str,
    boundary: u64,
    length: u64,
) -> Option<(&'static str, std::ops::Range<u64>)> {
    (boundary > 0 && boundary < length).then(|| {
        (
            label,
            boundary.saturating_sub(1)..boundary.saturating_add(1).min(length),
        )
    })
}

fn expected_range_probes(
    source: &Path,
    operation: &str,
    source_length: u64,
    candidate: Candidate,
) -> AnyResult<Vec<(&'static str, std::ops::Range<u64>)>> {
    let is_plus_one = operation.starts_with("plus1-");
    let (reference_count, _, replacement_length) = source_edit_point(
        source,
        if operation == "full" {
            "same-middle"
        } else {
            operation
        },
    )?;
    let position = if operation.contains("early") {
        0
    } else {
        reference_count / 2
    };
    let replacement = if operation == "same-middle" {
        vec![0x5a; replacement_length]
    } else {
        vec![0xa5]
    };
    let final_length = source_length
        .checked_add(u64::from(is_plus_one))
        .ok_or(CoreError::LengthOverflow)?;
    let mut probes = vec![("zero", 0..0), ("first-byte", 0..final_length.min(1))];
    let mut output_offset = 0_u64;
    let mut output_references = 0_u64;
    let mut cross_chunk = false;
    let mut leaf_boundary = false;
    let mut branch_boundary = false;
    let mut inserted = false;
    FastCdc::new().scan(File::open(source)?, |chunk| {
        let insert_now = is_plus_one && !inserted && output_references == position;
        let replace_now = operation == "same-middle" && output_references == position;
        let mut emit = |segment: &[u8]| -> CoreResult<()> {
            if !cross_chunk {
                if let Some(probe) = boundary_probe("cross-chunk", output_offset, final_length) {
                    probes.push(probe);
                    cross_chunk = true;
                }
            }
            output_offset = output_offset
                .checked_add(u64::try_from(segment.len()).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            output_references = output_references
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            if !leaf_boundary
                && output_references
                    == u64::try_from(candidate.k).map_err(|_| CoreError::LengthOverflow)?
            {
                if let Some(probe) = boundary_probe("leaf-boundary", output_offset, final_length) {
                    probes.push(probe);
                    leaf_boundary = true;
                }
            }
            let branch_at = u64::try_from(candidate.k)
                .map_err(|_| CoreError::LengthOverflow)?
                .checked_mul(u64::try_from(candidate.f).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            if !branch_boundary && output_references == branch_at {
                if let Some(probe) = boundary_probe("branch-boundary", output_offset, final_length)
                {
                    probes.push(probe);
                    branch_boundary = true;
                }
            }
            Ok(())
        };
        if insert_now {
            emit(&replacement)?;
            inserted = true;
        }
        if replace_now {
            emit(&replacement)?;
        } else {
            emit(chunk)?;
        }
        Ok(())
    })?;
    if is_plus_one && !inserted {
        let mut emit = |segment: &[u8]| -> CoreResult<()> {
            output_offset = output_offset
                .checked_add(u64::try_from(segment.len()).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?;
            output_references = output_references
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            Ok(())
        };
        emit(&replacement)?;
    }
    probes.push(("last-byte", final_length.saturating_sub(1)..final_length));
    probes.push(("eof", final_length..final_length));
    Ok(probes)
}

fn append_expected_segment(
    start: &mut u64,
    bytes: &[u8],
    probes: &[std::ops::Range<u64>],
    outputs: &mut [Vec<u8>],
    hasher: &mut Hasher,
    sequence_hasher: &mut Hasher,
) -> CoreResult<()> {
    let end = start
        .checked_add(u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    hasher.update(bytes);
    sequence_hasher.update(
        &u32::try_from(bytes.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    sequence_hasher.update(chunk_id(bytes).as_bytes());
    for (index, probe) in probes.iter().enumerate() {
        let overlap_start = (*start).max(probe.start);
        let overlap_end = end.min(probe.end);
        if overlap_start < overlap_end {
            let from =
                usize::try_from(overlap_start - *start).map_err(|_| CoreError::LengthOverflow)?;
            let to =
                usize::try_from(overlap_end - *start).map_err(|_| CoreError::LengthOverflow)?;
            outputs[index].extend_from_slice(&bytes[from..to]);
        }
    }
    *start = end;
    Ok(())
}

fn expected_file_observations(
    source: &Path,
    operation: &str,
    source_length: u64,
    candidate: Candidate,
) -> AnyResult<(
    u64,
    String,
    String,
    Vec<Vec<u8>>,
    Vec<(&'static str, std::ops::Range<u64>)>,
)> {
    let is_plus_one = operation.starts_with("plus1-");
    let (reference_count, _, replacement_length) = source_edit_point(
        source,
        if operation == "full" {
            "same-middle"
        } else {
            operation
        },
    )?;
    let position = if operation.contains("early") {
        0
    } else {
        reference_count / 2
    };
    let probes = expected_range_probes(source, operation, source_length, candidate)?;
    let probe_ranges = probes
        .iter()
        .map(|(_, range)| range.clone())
        .collect::<Vec<_>>();
    let mut outputs = probe_ranges.iter().map(|_| Vec::new()).collect::<Vec<_>>();
    let mut hasher = Hasher::new();
    let mut sequence_hasher = Hasher::new();
    let mut output_offset = 0_u64;
    let mut ordinal = 0_u64;
    let mut inserted = false;
    let replacement = if operation == "same-middle" {
        vec![0x5a; replacement_length]
    } else {
        vec![0xa5]
    };
    FastCdc::new().scan(File::open(source)?, |chunk| {
        if is_plus_one && !inserted && ordinal == position {
            append_expected_segment(
                &mut output_offset,
                &replacement,
                &probe_ranges,
                &mut outputs,
                &mut hasher,
                &mut sequence_hasher,
            )?;
            inserted = true;
        }
        let emitted = if operation == "same-middle" && ordinal == position {
            replacement.as_slice()
        } else {
            chunk
        };
        append_expected_segment(
            &mut output_offset,
            emitted,
            &probe_ranges,
            &mut outputs,
            &mut hasher,
            &mut sequence_hasher,
        )?;
        ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        Ok(())
    })?;
    if is_plus_one && !inserted {
        append_expected_segment(
            &mut output_offset,
            &replacement,
            &probe_ranges,
            &mut outputs,
            &mut hasher,
            &mut sequence_hasher,
        )?;
    }
    let expected_references = ordinal
        .checked_add(u64::from(is_plus_one))
        .ok_or(CoreError::LengthOverflow)?;
    Ok((
        expected_references,
        hasher.finalize().to_hex().to_string(),
        sequence_hasher.finalize().to_hex().to_string(),
        outputs,
        probes,
    ))
}

fn make_reference(
    store: &mut Store,
    bytes: &[u8],
    metrics: &mut Metrics,
) -> AnyResult<file_codec::FileReference> {
    add_len(&mut metrics.raw_bytes_hashed, bytes.len())?;
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
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    publish_transition_with_operations(store, parent, child, &[], metrics)
}

fn publish_transition_with_operations(
    store: &mut Store,
    parent: Option<ObjectId>,
    child: ObjectId,
    operations: &[delta_codec::TransitionOperation],
    metrics: &mut Metrics,
) -> AnyResult<ObjectId> {
    let pages = if operations.is_empty() {
        Vec::new()
    } else {
        vec![put_mapping(
            store,
            delta_codec::encode_delta_page(operations)?,
            metrics,
        )?]
    };
    let entry_count = u32::try_from(operations.len()).map_err(|_| CoreError::LengthOverflow)?;
    let inner = match parent {
        Some(parent) => delta_codec::encode_change(parent, child, entry_count, &pages)?,
        None => delta_codec::encode_genesis(child)?,
    };
    let transition = put_mapping(store, inner, metrics)?;
    Ok(transition)
}

fn verify_transition(
    store: &Store,
    transition: ObjectId,
    expected_parent: Option<ObjectId>,
    expected_child: ObjectId,
    expected_operations: Option<&[delta_codec::TransitionOperation]>,
    metrics: &mut Metrics,
) -> AnyResult<[u8; 32]> {
    let mut closure_hasher = Hasher::new();
    let (_, bytes) = store.get(transition, metrics)?;
    observe_closure(&mut closure_hasher, b"transition", transition, &bytes)?;
    let decoded = delta_codec::decode_mapping_transition(&bytes)?;
    if decoded.parent != expected_parent || decoded.child != expected_child {
        return Err(CoreError::PublicationConflict.into());
    }
    let mut operations = Vec::new();
    for page in &decoded.pages {
        let (_, bytes) = store.get(*page, metrics)?;
        observe_closure(&mut closure_hasher, b"transition-page", *page, &bytes)?;
        let page_operations = delta_codec::decode_mapping_delta_page(&bytes)?;
        if page_operations.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        operations.extend(page_operations);
    }
    if u32::try_from(operations.len()).map_err(|_| CoreError::LengthOverflow)?
        != decoded.entry_count
    {
        return Err(CoreError::LengthMismatch {
            expected: u64::from(decoded.entry_count),
            actual: u64::try_from(operations.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    if expected_operations.is_some_and(|expected| operations.as_slice() != expected) {
        return Err(CoreError::PublicationConflict.into());
    }
    if expected_parent.is_none() && (!operations.is_empty() || !decoded.pages.is_empty()) {
        return Err(CoreError::PublicationConflict.into());
    }
    if let Some(parent) = expected_parent {
        replay_shadow_transition(
            store,
            &decoded,
            &operations,
            parent,
            expected_child,
            metrics,
        )?;
    }
    let (object, bytes) = store.get(expected_child, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"transition-child",
        expected_child,
        &bytes,
    )?;
    if !matches!(object, Object::Directory(_)) {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(*closure_hasher.finalize().as_bytes())
}

fn shadow_node(id: ObjectId) -> CoreResult<TreeNode> {
    let name = CanonicalName::from_bytes(format!("id-{id}").as_bytes())?;
    TreeNode::directory([(name, TreeNode::empty_directory())])
}

fn shadow_root(store: &Store, id: ObjectId, metrics: &mut Metrics) -> AnyResult<RootHandle> {
    let (_, bytes) = store.get(id, metrics)?;
    let Object::Directory(entries) = decode_object(&bytes)? else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    let children = entries
        .into_iter()
        .map(|entry| Ok((entry.name().clone(), shadow_node(entry.reference().id())?)))
        .collect::<AnyResult<Vec<_>>>()?;
    Ok(RootHandle::from_entries(children)?)
}

fn replay_shadow_transition(
    store: &Store,
    transition: &delta_codec::DecodedTransition,
    operations: &[delta_codec::TransitionOperation],
    parent_id: ObjectId,
    child_id: ObjectId,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    let [delta_codec::TransitionOperation::Replace {
        path,
        before,
        after,
    }] = operations
    else {
        return Err(CoreError::DeltaConflict.into());
    };
    let expected_tag = match path.as_slice() {
        b"file" => file_codec::FILE_ROOT_TAG,
        b"t" => file_codec::DIR_INDEX_TAG,
        _ => return Err(CoreError::DeltaConflict.into()),
    };
    let (_, after_bytes) = store.get(*after, metrics)?;
    file_codec::decode_mapping(&after_bytes, expected_tag)?;
    let after_node = shadow_node(*after)?;
    let parent = shadow_root(store, parent_id, metrics)?;
    let parent_tree_id = parent.node().identity();
    let before_tree_id = parent
        .lookup_required(&layerfs_core::CanonicalPath::from_bytes(path)?)?
        .identity();
    let after_tree_id = after_node.identity();
    let mut durable_ids = HashMap::from([
        (parent_tree_id, parent_id),
        (before_tree_id, *before),
        (after_tree_id, *after),
    ]);
    let replay = delta_codec::replay_durable_transition(
        transition,
        operations,
        &parent,
        parent_id,
        |id| {
            let (_, bytes) = store.get(id, metrics).map_err(|error| {
                match error.downcast_ref::<CoreError>() {
                    Some(error) => *error,
                    None => CoreError::Io,
                }
            })?;
            file_codec::decode_mapping(&bytes, expected_tag)?;
            shadow_node(id)
        },
        |node| {
            if let Some(id) = durable_ids.get(&node.identity()) {
                return Ok(*id);
            }
            if node.entries().is_some() {
                durable_ids.insert(node.identity(), child_id);
                return Ok(child_id);
            }
            Err(CoreError::MissingObject)
        },
    )?;
    let replayed = replay.apply(&parent)?;
    if replayed != shadow_root(store, child_id, metrics)? {
        return Err(CoreError::DeltaChildMismatch {
            expected: child_id,
            actual: child_id,
        }
        .into());
    }
    Ok(())
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
    let file_root = builder.finish(store, metrics)?;
    let root = namespace_file_root(store, file_root, metrics)?;
    let transition = publish_transition(store, None, root, metrics)?;
    Ok((root, transition))
}

fn rewrite_same_node_by_offset(
    store: &mut Store,
    id: ObjectId,
    level: u8,
    node_start: u64,
    target: u64,
    replacement: file_codec::FileReference,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, u64, bool)> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    let (_, bytes) = store.get(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let mut refs = file_codec::parse_file_leaf(&payload)?;
        let mut offset = node_start;
        let mut changed = false;
        for reference in &mut refs {
            let end = offset
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
            if !changed && reference.raw_length != 0 && target >= offset && target < end {
                *reference = replacement;
                changed = true;
            }
            offset = end;
        }
        let total = offset
            .checked_sub(node_start)
            .ok_or(CoreError::LengthOverflow)?;
        if !changed {
            return Ok((id, total, false));
        }
        let (new_id, canonical) = canonical_bytes(file_codec::encode_file_leaf(&refs)?)?;
        store.put(new_id, &canonical, metrics)?;
        add(&mut metrics.pages, 1)?;
        add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
        return Ok((new_id, total, true));
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let (branch_level, mut children) = file_codec::parse_file_children(&payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    let total = children.last().map_or(0, |child| child.cumulative_end);
    let mut previous = 0_u64;
    let mut changed = false;
    for child in &mut children {
        let child_start = node_start
            .checked_add(previous)
            .ok_or(CoreError::LengthOverflow)?;
        let child_end = node_start
            .checked_add(child.cumulative_end)
            .ok_or(CoreError::LengthOverflow)?;
        if !changed && target >= child_start && target < child_end {
            let (new_id, _, did_change) = rewrite_same_node_by_offset(
                store,
                child.object_id,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                child_start,
                target,
                replacement,
                metrics,
            )?;
            if did_change {
                child.object_id = new_id;
                changed = true;
            }
        }
        previous = child.cumulative_end;
    }
    if !changed {
        return Ok((id, total, false));
    }
    let (new_id, canonical) = canonical_bytes(file_codec::encode_file_branch(level, &children)?)?;
    store.put(new_id, &canonical, metrics)?;
    add(&mut metrics.branches, 1)?;
    add_len(&mut metrics.mapping_bytes_rewritten, canonical.len())?;
    Ok((new_id, total, true))
}

fn rewrite_same_root_by_offset(
    store: &mut Store,
    root: ObjectId,
    target: u64,
    replacement: file_codec::FileReference,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, bool)> {
    let (_, bytes) = store.get(root, metrics)?;
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
    let (total_raw, reference_count, level, mut children) = file_codec::parse_file_root(&payload)?;
    let mut previous = 0_u64;
    let mut changed = false;
    for child in &mut children {
        let child_start = previous;
        let child_end = child.cumulative_end;
        if !changed && target >= child_start && target < child_end {
            let (new_id, new_total, did_change) = rewrite_same_node_by_offset(
                store,
                child.object_id,
                level,
                child_start,
                target,
                replacement,
                metrics,
            )?;
            if did_change {
                if new_total != child_end.saturating_sub(child_start) {
                    return Err(CoreError::LengthMismatch {
                        expected: child_end.saturating_sub(child_start),
                        actual: new_total,
                    }
                    .into());
                }
                child.object_id = new_id;
                changed = true;
            }
        }
        previous = child_end;
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
    source: &Path,
    candidate: Candidate,
    operation: &str,
    metrics: &mut Metrics,
) -> AnyResult<(ObjectId, ObjectId)> {
    let (_, parent, _, _) = store.current_head()?.ok_or(CoreError::MissingObject)?;
    let file_parent = resolve_namespace_file_root(store, parent, metrics)?;
    if operation == "same-middle" {
        let (_, byte_offset, length) = source_edit_point(source, operation)?;
        let replacement = vec![0x5a; length];
        store.begin(metrics)?;
        let replacement = make_reference(store, &replacement, metrics)?;
        let (file_root, changed) =
            rewrite_same_root_by_offset(store, file_parent, byte_offset, replacement, metrics)?;
        if !changed {
            return Err(CoreError::MissingObject.into());
        }
        let root = namespace_file_root(store, file_root, metrics)?;
        let operation = delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: file_parent,
            after: file_root,
        };
        let transition =
            publish_transition_with_operations(store, Some(parent), root, &[operation], metrics)?;
        return Ok((root, transition));
    }
    let (_, file_root_bytes) = store.get(file_parent, metrics)?;
    let file_root_payload =
        file_codec::decode_mapping(&file_root_bytes, file_codec::FILE_ROOT_TAG)?;
    let (_, reference_count, _, _) = file_codec::parse_file_root(&file_root_payload)?;
    let position = if operation.contains("early") {
        0
    } else {
        reference_count / 2
    };
    let replacement = vec![0xa5];
    store.begin(metrics)?;
    let inserted = make_reference(store, &replacement, metrics)?;
    let mut builder = FileBuilder::new(candidate);
    let mut ordinal = 0_u64;
    let mut inserted_done = false;
    let mut suffix_bytes = 0_u64;
    let mut suffix_objects = 0_u64;
    let mut active = Vec::new();
    walk_file_root_references(
        store,
        file_parent,
        file_codec::FileMappingProfile::new(candidate.k, candidate.f),
        &mut active,
        &mut |store, reference, metrics| {
            if !inserted_done && ordinal == position {
                builder.push_reference(store, inserted, metrics)?;
                inserted_done = true;
            }
            let before_objects = metrics
                .objects_created
                .checked_add(metrics.objects_reused)
                .ok_or(CoreError::LengthOverflow)?;
            builder.push_reference(store, reference, metrics)?;
            if ordinal >= position {
                suffix_bytes = suffix_bytes
                    .checked_add(u64::from(reference.raw_length))
                    .ok_or(CoreError::LengthOverflow)?;
                let after_objects = metrics
                    .objects_created
                    .checked_add(metrics.objects_reused)
                    .ok_or(CoreError::LengthOverflow)?;
                suffix_objects = suffix_objects
                    .checked_add(after_objects.saturating_sub(before_objects))
                    .ok_or(CoreError::LengthOverflow)?;
            }
            ordinal = ordinal.checked_add(1).ok_or(CoreError::LengthOverflow)?;
            Ok(())
        },
        metrics,
    )?;
    if !inserted_done {
        builder.push_reference(store, inserted, metrics)?;
    }
    if ordinal != reference_count {
        return Err(CoreError::LengthMismatch {
            expected: reference_count,
            actual: ordinal,
        }
        .into());
    }
    metrics.suffix_references = reference_count
        .checked_sub(position)
        .ok_or(CoreError::LengthOverflow)?;
    metrics.suffix_bytes = suffix_bytes;
    metrics.suffix_objects = suffix_objects;
    let file_root = builder.finish(store, metrics)?;
    let root = namespace_file_root(store, file_root, metrics)?;
    let operation = delta_codec::TransitionOperation::Replace {
        path: b"file".to_vec(),
        before: file_parent,
        after: file_root,
    };
    let transition =
        publish_transition_with_operations(store, Some(parent), root, &[operation], metrics)?;
    Ok((root, transition))
}

#[allow(clippy::too_many_arguments)]
fn walk_file_root_references<F>(
    store: &mut Store,
    id: ObjectId,
    profile: file_codec::FileMappingProfile,
    active: &mut Vec<ObjectId>,
    callback: &mut F,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)>
where
    F: FnMut(&mut Store, file_codec::FileReference, &mut Metrics) -> AnyResult<()>,
{
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let result = (|| {
        let (_, bytes) = store.get(id, metrics)?;
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
        let (expected_length, expected_references, level, children) =
            file_codec::parse_file_root(&payload)?;
        if level != file_codec::expected_file_level(expected_references, profile)? {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        if expected_references == 0 {
            if expected_length != 0 || level != 0 || !children.is_empty() {
                return Err(CoreError::NonCanonicalPagePartition.into());
            }
            return Ok((0, 0));
        }
        file_codec::validate_file_children(&children, profile, true)?;
        let child_count = children.len();
        let mut length = 0_u64;
        let mut references = 0_u64;
        let mut previous_end = 0_u64;
        for (index, child) in children.into_iter().enumerate() {
            let (child_length, child_references) = walk_file_references(
                store,
                child.object_id,
                level,
                index + 1 == child_count,
                profile,
                active,
                callback,
                metrics,
            )?;
            let actual_end = previous_end
                .checked_add(child_length)
                .ok_or(CoreError::LengthOverflow)?;
            if child.cumulative_end != actual_end {
                return Err(CoreError::LengthMismatch {
                    expected: child.cumulative_end,
                    actual: actual_end,
                }
                .into());
            }
            length = actual_end;
            references = references
                .checked_add(child_references)
                .ok_or(CoreError::LengthOverflow)?;
            previous_end = child.cumulative_end;
        }
        file_codec::validate_file_root_summary(
            expected_length,
            expected_references,
            length,
            references,
        )?;
        Ok((length, references))
    })();
    active.pop();
    result
}

#[allow(clippy::too_many_arguments)]
fn walk_file_references<F>(
    store: &mut Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    active: &mut Vec<ObjectId>,
    callback: &mut F,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)>
where
    F: FnMut(&mut Store, file_codec::FileReference, &mut Metrics) -> AnyResult<()>,
{
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let (_, bytes) = store.get(id, metrics)?;
    let result = if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let references = file_codec::parse_file_leaf(&payload)?;
        file_codec::validate_file_leaf(&references, profile, final_node)?;
        let mut length = 0_u64;
        for reference in references {
            callback(store, reference, metrics)?;
            length = length
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
        }
        (
            length,
            u64::try_from(payload.len().saturating_sub(4) / file_codec::FILE_REF_BYTES)
                .map_err(|_| CoreError::LengthOverflow)?,
        )
    } else {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
        let (branch_level, children) = file_codec::parse_file_children(&payload, true)?;
        if branch_level != level {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        file_codec::validate_file_children(&children, profile, final_node)?;
        let child_count = children.len();
        let mut length = 0_u64;
        let mut references = 0_u64;
        let mut previous_end = 0_u64;
        for (index, child) in children.into_iter().enumerate() {
            let (child_length, child_references) = walk_file_references(
                store,
                child.object_id,
                level
                    .checked_sub(1)
                    .ok_or(CoreError::MappingDepthExceeded)?,
                final_node && index + 1 == child_count,
                profile,
                active,
                callback,
                metrics,
            )?;
            let actual_end = previous_end
                .checked_add(child_length)
                .ok_or(CoreError::LengthOverflow)?;
            if child.cumulative_end != actual_end {
                return Err(CoreError::LengthMismatch {
                    expected: child.cumulative_end,
                    actual: actual_end,
                }
                .into());
            }
            length = actual_end;
            references = references
                .checked_add(child_references)
                .ok_or(CoreError::LengthOverflow)?;
            previous_end = child.cumulative_end;
        }
        (length, references)
    };
    active.pop();
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn stream_file(
    store: &Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    profile: file_codec::FileMappingProfile,
    active: &mut Vec<ObjectId>,
    hasher: &mut Hasher,
    closure_hasher: &mut Hasher,
    sequence_hasher: &mut Hasher,
    length: &mut u64,
    reference_count: &mut u64,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if usize::from(level) > MAX_DEPTH {
        return Err(CoreError::MappingDepthExceeded.into());
    }
    if active.contains(&id) {
        return Err(CoreError::MappingCycle.into());
    }
    active.push(id);
    let (object, bytes) = store.get(id, metrics)?;
    observe_closure(closure_hasher, b"file-mapping", id, &bytes)?;
    add(&mut metrics.closure_occurrences, 1)?;
    let payload = match level {
        0 => file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?,
        _ => file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?,
    };
    let _ = object;
    if level == 0 {
        let references = file_codec::parse_file_leaf(&payload)?;
        file_codec::validate_file_leaf(&references, profile, final_node)?;
        *reference_count = (*reference_count)
            .checked_add(u64::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?)
            .ok_or(CoreError::LengthOverflow)?;
        for reference in references {
            sequence_hasher.update(&reference.raw_length.to_be_bytes());
            sequence_hasher.update(reference.raw_id.as_bytes());
            let (chunk, canonical) = store.get(reference.object_id, metrics)?;
            add(&mut metrics.closure_occurrences, 1)?;
            observe_closure(
                closure_hasher,
                b"file-chunk",
                reference.object_id,
                &canonical,
            )?;
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
        active.pop();
        return Ok(());
    }
    let (branch_level, children) = file_codec::parse_file_children(&payload, true)?;
    if branch_level != level {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    file_codec::validate_file_children(&children, profile, final_node)?;
    let child_count = children.len();
    let mut previous_end = 0_u64;
    for (index, child) in children.into_iter().enumerate() {
        let child_length_before = *length;
        let child_references_before = *reference_count;
        stream_file(
            store,
            child.object_id,
            level
                .checked_sub(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            final_node && index + 1 == child_count,
            profile,
            active,
            hasher,
            closure_hasher,
            sequence_hasher,
            length,
            reference_count,
            metrics,
        )?;
        let child_length = (*length)
            .checked_sub(child_length_before)
            .ok_or(CoreError::LengthOverflow)?;
        let child_references = (*reference_count)
            .checked_sub(child_references_before)
            .ok_or(CoreError::LengthOverflow)?;
        let actual_end = previous_end
            .checked_add(child_length)
            .ok_or(CoreError::LengthOverflow)?;
        if child.cumulative_end != actual_end {
            return Err(CoreError::LengthMismatch {
                expected: child.cumulative_end,
                actual: actual_end,
            }
            .into());
        }
        if child_references == 0 && child_length != 0 {
            return Err(CoreError::LengthMismatch {
                expected: 0,
                actual: child_length,
            }
            .into());
        }
        previous_end = child.cumulative_end;
    }
    active.pop();
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
    let requested = usize::try_from(
        range
            .end
            .checked_sub(range.start)
            .ok_or(CoreError::LengthOverflow)?,
    )
    .map_err(|_| CoreError::LengthOverflow)?;
    let mut output = Vec::with_capacity(requested);
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
    let mut previous = 0_u64;
    for child in children {
        let child_start = node_start
            .checked_add(previous)
            .ok_or(CoreError::LengthOverflow)?;
        let child_end = node_start
            .checked_add(child.cumulative_end)
            .ok_or(CoreError::LengthOverflow)?;
        previous = child.cumulative_end;
        if child_end > range.start && child_start < range.end {
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
    candidate: Candidate,
    expected_fingerprint: Option<&str>,
    expected_sequence: Option<&str>,
    metrics: &mut Metrics,
) -> AnyResult<([u8; 32], u64, u64)> {
    let mut closure_hasher = Hasher::new();
    let (namespace, namespace_bytes) = store.get(root, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"namespace-root",
        root,
        &namespace_bytes,
    )?;
    let Object::Directory(entries) = namespace else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if entries.len() != 1
        || entries[0].name().as_bytes() != b"file"
        || entries[0].reference().kind() != ObjectKind::Bytes
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let file_root = entries[0].reference().id();
    let (_, root_bytes) = store.get(file_root, metrics)?;
    observe_closure(&mut closure_hasher, b"file-root", file_root, &root_bytes)?;
    let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let (expected_length, expected_references, level, root_children) =
        file_codec::parse_file_root(&payload)?;
    let expected_level = level;
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if expected_level != file_codec::expected_file_level(expected_references, profile)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    if expected_references == 0 {
        if !root_children.is_empty() || level != 0 || expected_length != 0 {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
    } else {
        file_codec::validate_file_children(&root_children, profile, true)?;
    }
    let mut hasher = Hasher::new();
    let mut sequence_hasher = Hasher::new();
    let mut length = 0_u64;
    let root_payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let (_, _, root_level, children) = file_codec::parse_file_root(&root_payload)?;
    let mut active = Vec::new();
    let child_count = children.len();
    let mut reference_count = 0_u64;
    let mut previous_end = 0_u64;
    for (index, child) in children.into_iter().enumerate() {
        let child_length_before = length;
        let child_references_before = reference_count;
        stream_file(
            store,
            child.object_id,
            root_level,
            index + 1 == child_count,
            profile,
            &mut active,
            &mut hasher,
            &mut closure_hasher,
            &mut sequence_hasher,
            &mut length,
            &mut reference_count,
            metrics,
        )?;
        let child_length = length
            .checked_sub(child_length_before)
            .ok_or(CoreError::LengthOverflow)?;
        let child_references = reference_count
            .checked_sub(child_references_before)
            .ok_or(CoreError::LengthOverflow)?;
        let actual_end = previous_end
            .checked_add(child_length)
            .ok_or(CoreError::LengthOverflow)?;
        if child.cumulative_end != actual_end || (child_references == 0 && child_length != 0) {
            return Err(CoreError::LengthMismatch {
                expected: child.cumulative_end,
                actual: actual_end,
            }
            .into());
        }
        previous_end = child.cumulative_end;
    }
    let reconstructed_fingerprint = hasher.finalize().to_hex().to_string();
    let reconstructed_sequence = sequence_hasher.finalize().to_hex().to_string();
    file_codec::validate_file_root_summary(
        expected_length,
        expected_references,
        length,
        reference_count,
    )?;
    if length != expected_length
        || level != root_level
        || expected_fingerprint.is_some_and(|fingerprint| fingerprint != reconstructed_fingerprint)
        || expected_sequence.is_some_and(|sequence| sequence != reconstructed_sequence)
    {
        return Err(CoreError::LengthMismatch {
            expected: expected_length,
            actual: length,
        }
        .into());
    }
    Ok((
        *closure_hasher.finalize().as_bytes(),
        reference_count,
        length,
    ))
}

fn scrub_file(
    store: &mut Store,
    root: ObjectId,
    candidate: Candidate,
    metrics: &mut Metrics,
) -> AnyResult<(u64, u64)> {
    let file_root = namespace_entry_id(store, root, b"file", metrics)?;
    let (_, root_bytes) = store.get(file_root, metrics)?;
    let payload = file_codec::decode_mapping(&root_bytes, file_codec::FILE_ROOT_TAG)?;
    let (expected_length, expected_references, level, _) = file_codec::parse_file_root(&payload)?;
    let profile = file_codec::FileMappingProfile::new(candidate.k, candidate.f);
    if level != file_codec::expected_file_level(expected_references, profile)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let mut active = Vec::new();
    let mut callback = |store: &mut Store,
                        reference: file_codec::FileReference,
                        metrics: &mut Metrics|
     -> AnyResult<()> {
        let (object, _) = store.get(reference.object_id, metrics)?;
        let Object::Bytes(raw) = object else {
            return Err(CoreError::WrongLogicalRole.into());
        };
        if u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)? != reference.raw_length
            || chunk_id(&raw) != reference.raw_id
        {
            return Err(CoreError::ChunkIdentityMismatch.into());
        }
        Ok(())
    };
    let (length, references) = walk_file_root_references(
        store,
        file_root,
        profile,
        &mut active,
        &mut callback,
        metrics,
    )?;
    file_codec::validate_file_root_summary(
        expected_length,
        expected_references,
        length,
        references,
    )?;
    Ok((length, references))
}

fn verify_ranges(
    store: &Store,
    file_root: ObjectId,
    probes: &[(&'static str, std::ops::Range<u64>)],
    expected: &[Vec<u8>],
    metrics: &mut Metrics,
) -> AnyResult<Vec<RangeMeasurement>> {
    if probes.len() != expected.len() {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(probes.len()).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(expected.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    let mut measurements = Vec::with_capacity(probes.len());
    for ((label, range), expected) in probes.iter().zip(expected) {
        let started = Instant::now();
        let actual = read_file_range(store, file_root, range.clone(), metrics)?;
        let wall_ns = started.elapsed().as_nanos();
        if &actual != expected {
            return Err(CoreError::PublicationConflict.into());
        }
        measurements.push(RangeMeasurement {
            label: *label,
            range: range.clone(),
            wall_ns,
            returned_bytes: actual.len(),
        });
    }
    Ok(measurements)
}

fn empty_file_root(store: &mut Store, metrics: &mut Metrics) -> AnyResult<ObjectId> {
    let inner = file_codec::encode_file_root(0, 0, 0, 0, &[])?;
    put_mapping(store, inner, metrics)
}

fn directory_name(number: usize) -> AnyResult<CanonicalName> {
    CanonicalName::from_bytes(format!("{number:08}-{}", "x".repeat(246)).as_bytes())
        .map_err(Into::into)
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

fn greedy_directory_entries(
    first: usize,
    last_number: usize,
    child: ObjectId,
    candidate: Candidate,
) -> AnyResult<Vec<DirectoryEntry>> {
    let end = last_number
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    if first >= end {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let mut entries = Vec::new();
    let mut encoded_size = 9_usize.checked_add(4).ok_or(CoreError::LengthOverflow)?;
    for number in first..end {
        let name = directory_name(number)?;
        let entry_size = 4_usize
            .checked_add(name.as_bytes().len())
            .and_then(|value| value.checked_add(1 + 32))
            .ok_or(CoreError::LengthOverflow)?;
        let next_size = encoded_size
            .checked_add(entry_size)
            .ok_or(CoreError::LengthOverflow)?;
        if !entries.is_empty() && next_size > candidate.directory_page {
            break;
        }
        entries.push(DirectoryEntry::new(
            name,
            ObjectReference::new(ObjectKind::Bytes, child),
        ));
        encoded_size = next_size;
    }
    if entries.is_empty() || encoded_size > candidate.directory_page {
        return Err(CoreError::NonCanonicalPagePartition.into());
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
    let mut pages = Vec::new();
    let mut start = 1_usize;
    let total = if leading {
        DIRECTORY_ENTRIES
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?
    } else {
        DIRECTORY_ENTRIES
    };
    let last_number = if leading { total - 1 } else { total };
    while start <= total {
        let first_number = if leading {
            start.saturating_sub(1)
        } else {
            start
        };
        let page_child = if replacement
            && start <= DIRECTORY_ENTRIES / 2
            && start.checked_add(1).ok_or(CoreError::LengthOverflow)? > DIRECTORY_ENTRIES / 2
        {
            replacement_child.ok_or(CoreError::MissingObject)?
        } else {
            child
        };
        let entries = greedy_directory_entries(first_number, last_number, page_child, candidate)?;
        let count = entries.len();
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
    let transition = publish_transition(store, None, root, metrics)?;
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
    if entries.len() != 2
        || entries[0].name().as_bytes() != b"m"
        || entries[1].name().as_bytes() != b"t"
        || entries
            .iter()
            .any(|entry| entry.reference().kind() != ObjectKind::Bytes)
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let (_, metadata_bytes) = store.get(entries[0].reference().id(), metrics)?;
    let metadata = file_codec::decode_mapping(&metadata_bytes, file_codec::DIR_METADATA_TAG)?;
    if metadata.len() != 4 {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    let index = entries[1].reference().id();
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
    let (_, parent_bytes) = store.get(parent, metrics)?;
    let Object::Directory(parent_entries) = decode_object(&parent_bytes)? else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    let before_index = parent_entries
        .iter()
        .find(|entry| entry.name().as_bytes() == b"t")
        .ok_or(CoreError::WrongLogicalRole)?
        .reference()
        .id();
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
        let (page_index, local) = old_pages
            .iter()
            .enumerate()
            .find_map(|(index, page)| {
                let count = usize::try_from(page.count).ok()?;
                let end = seen.checked_add(count)?;
                let result = (target >= seen && target < end).then_some((index, target - seen));
                seen = end;
                result
            })
            .ok_or(CoreError::NonCanonicalPagePartition)?;
        pages = old_pages.clone();
        let page = &old_pages[page_index];
        let mut entries = directory_page_entries(store, page.object_id, metrics)?;
        if local >= entries.len() {
            return Err(CoreError::NonCanonicalPagePartition.into());
        }
        entries[local] = DirectoryEntry::new(
            entries[local].name().clone(),
            ObjectReference::new(ObjectKind::Bytes, child),
        );
        let (id, _) = page_object(store, &entries, metrics)?;
        pages[page_index] = dir_codec::DirectoryPageRef {
            count: page.count,
            first_name: entries[0].name().as_bytes().to_vec(),
            object_id: id,
        };
    } else {
        let total = DIRECTORY_ENTRIES
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let mut start = 0_usize;
        while start < total {
            let entries = greedy_directory_entries(
                start,
                total.checked_sub(1).ok_or(CoreError::LengthOverflow)?,
                child,
                candidate,
            )?;
            let count = entries.len();
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
    let operation_record = delta_codec::TransitionOperation::Replace {
        path: b"t".to_vec(),
        before: before_index,
        after: index,
    };
    let transition = publish_transition_with_operations(
        store,
        Some(parent),
        root,
        &[operation_record],
        metrics,
    )?;
    Ok((root, transition))
}

fn verify_directory(
    store: &Store,
    root: ObjectId,
    candidate: Candidate,
    expected_entries: u64,
    expected_replacement: Option<(u64, ObjectId)>,
    metrics: &mut Metrics,
) -> AnyResult<[u8; 32]> {
    let mut closure_hasher = Hasher::new();
    let (object, root_bytes) = store.get(root, metrics)?;
    observe_closure(&mut closure_hasher, b"directory-root", root, &root_bytes)?;
    let Object::Directory(wrapper) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if wrapper.len() != 2
        || wrapper[0].name().as_bytes() != b"m"
        || wrapper[1].name().as_bytes() != b"t"
        || wrapper
            .iter()
            .any(|entry| entry.reference().kind() != ObjectKind::Bytes)
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let metadata_id = wrapper[0].reference().id();
    let (metadata_object, metadata_bytes) = store.get(metadata_id, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"directory-metadata",
        metadata_id,
        &metadata_bytes,
    )?;
    if !matches!(metadata_object, Object::Bytes(_))
        || file_codec::decode_mapping(&metadata_bytes, file_codec::DIR_METADATA_TAG)?.len() != 4
    {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let index_id = wrapper[1].reference().id();
    let (_, index_bytes) = store.get(index_id, metrics)?;
    observe_closure(
        &mut closure_hasher,
        b"directory-index",
        index_id,
        &index_bytes,
    )?;
    let pages = dir_codec::parse_directory_index(&file_codec::decode_mapping(
        &index_bytes,
        file_codec::DIR_INDEX_TAG,
    )?)?;
    let mut loaded_pages = Vec::with_capacity(pages.len());
    for page in &pages {
        let (page_object, page_bytes) = store.get(page.object_id, metrics)?;
        observe_closure(
            &mut closure_hasher,
            b"directory-page",
            page.object_id,
            &page_bytes,
        )?;
        let Object::Directory(entries) = page_object else {
            return Err(CoreError::WrongLogicalRole.into());
        };
        loaded_pages.push((page.clone(), entries));
    }
    let partition = loaded_pages
        .iter()
        .map(|(page, entries)| (entries.as_slice(), page))
        .collect::<Vec<_>>();
    dir_codec::validate_directory_partition(
        u32::try_from(expected_entries).map_err(|_| CoreError::LengthOverflow)?,
        &partition,
        candidate.directory_page,
    )?;
    let mut total = 0_u64;
    let mut replacement_seen = false;
    let mut expected_number = if expected_entries == DIRECTORY_ENTRIES as u64 + 1 {
        0
    } else {
        1
    };
    let mut previous_last: Option<Vec<u8>> = None;
    for (page, entries) in loaded_pages {
        let page_start = expected_number;
        let page_child = entries
            .first()
            .ok_or(CoreError::NonCanonicalPagePartition)?
            .reference()
            .id();
        let last_directory_number = if expected_entries == DIRECTORY_ENTRIES as u64 + 1 {
            expected_entries
                .checked_sub(1)
                .ok_or(CoreError::LengthOverflow)?
        } else {
            expected_entries
        };
        let greedy = greedy_directory_entries(
            page_start,
            usize::try_from(last_directory_number).map_err(|_| CoreError::LengthOverflow)?,
            page_child,
            candidate,
        )?;
        if entries.len() != usize::try_from(page.count).map_err(|_| CoreError::LengthOverflow)?
            || entries.first().map(|entry| entry.name().as_bytes())
                != Some(page.first_name.as_slice())
            || greedy.len() != entries.len()
        {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        for entry in &entries {
            if entry.name().as_bytes() != directory_name(expected_number)?.as_bytes() {
                return Err(CoreError::NonCanonicalOrdering.into());
            }
            if previous_last
                .as_ref()
                .is_some_and(|last| last.as_slice() >= entry.name().as_bytes())
            {
                return Err(CoreError::NonCanonicalPagePartition.into());
            }
            previous_last = Some(entry.name().as_bytes().to_vec());
            expected_number = expected_number
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
        }
        for (index, entry) in entries.into_iter().enumerate() {
            let entry_number = page_start
                .checked_add(index)
                .ok_or(CoreError::LengthOverflow)?;
            let child_id = entry.reference().id();
            if let Some((expected_number, expected_id)) = expected_replacement {
                if u64::try_from(entry_number).map_err(|_| CoreError::LengthOverflow)?
                    == expected_number
                {
                    if child_id != expected_id {
                        return Err(CoreError::ChunkIdentityMismatch.into());
                    }
                    replacement_seen = true;
                } else if child_id == expected_id {
                    return Err(CoreError::NonCanonicalOrdering.into());
                }
            }
            let (child, child_bytes) = store.get(child_id, metrics)?;
            observe_closure(
                &mut closure_hasher,
                b"directory-target",
                child_id,
                &child_bytes,
            )?;
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
    if expected_replacement.is_some() && !replacement_seen {
        return Err(CoreError::NonCanonicalOrdering.into());
    }
    Ok(*closure_hasher.finalize().as_bytes())
}

fn apparent_store_bytes(store: &Store) -> (Option<u64>, Option<u64>, Option<u64>) {
    store.physical_bytes()
}

fn remove_sqlite_image(path: &Path) -> AnyResult<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    let mut journal = path.as_os_str().to_os_string();
    journal.push("-journal");
    let journal = PathBuf::from(journal);
    if journal.exists() {
        fs::remove_file(journal)?;
    }
    let authority = authority_path(path);
    if authority.exists() {
        fs::remove_file(authority)?;
    }
    Ok(())
}

fn clone_sqlite_image(source: &Path, destination: &Path) -> AnyResult<()> {
    remove_sqlite_image(destination)?;
    fs::copy(source, destination)?;
    let mut source_journal = source.as_os_str().to_os_string();
    source_journal.push("-journal");
    let source_journal = PathBuf::from(source_journal);
    if source_journal.exists() {
        let mut destination_journal = destination.as_os_str().to_os_string();
        destination_journal.push("-journal");
        fs::copy(source_journal, PathBuf::from(destination_journal))?;
    }
    fs::copy(authority_path(source), authority_path(destination))?;
    Ok(())
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

fn mib_per_second(bytes: u64, wall_ns: u128) -> String {
    if wall_ns == 0 {
        return "Unavailable".to_string();
    }
    format!(
        "{:.6}",
        (bytes as f64 / (1024.0 * 1024.0)) / (wall_ns as f64 / 1_000_000_000.0)
    )
}

#[allow(clippy::too_many_arguments)]
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
    expected_references: Option<u64>,
    expected_sequence: Option<&str>,
    actual_references: u64,
    closure_digest: [u8; 32],
    metrics: Metrics,
    physical_bytes: (Option<u64>, Option<u64>, Option<u64>),
    phases: &PhaseTimes,
    range_measurements: &[RangeMeasurement],
    error: Option<&str>,
) -> AnyResult<String> {
    let profile = profile_hex(candidate)?;
    let status = if error.is_some() { "FAIL" } else { "PASS" };
    let error_json = error.map_or_else(
        || "null".to_string(),
        |value| format!("\"{}\"", value.replace('"', "'")),
    );
    let optional_u64_json = |value: Option<u64>| {
        value.map_or_else(|| "\"Unavailable\"".to_string(), |value| value.to_string())
    };
    let (database_bytes, journal_bytes, authority_bytes) = physical_bytes;
    let expected_references_json = expected_references
        .map_or_else(|| "\"Unavailable\"".to_string(), |value| value.to_string());
    let expected_sequence_json = expected_sequence.map_or_else(
        || "\"Unavailable\"".to_string(),
        |value| format!("\"{value}\""),
    );
    let closure_digest = closure_digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let durable_sum = phases
        .source_cdc_ns
        .checked_add(phases.canonical_cas_mapping_stage_ns)
        .and_then(|value| value.checked_add(phases.precommit_closure_validation_ns))
        .and_then(|value| value.checked_add(phases.sqlite_commit_durability_ns));
    let lifecycle_sum = durable_sum
        .and_then(|value| value.checked_add(phases.fresh_reopen_head_ns))
        .and_then(|value| value.checked_add(phases.fresh_full_scrub_ns))
        .and_then(|value| value.checked_add(phases.reconstruction_ns))
        .and_then(|value| value.checked_add(phases.range_verification_ns));
    let ranges_json = range_measurements
        .iter()
        .map(|measurement| {
            format!(
                "{{\"label\":\"{}\",\"start\":{},\"end\":{},\"wall_ns\":{},\"returned_bytes\":{},\"throughput_mib_s\":{}}}",
                measurement.label,
                measurement.range.start,
                measurement.range.end,
                measurement.wall_ns,
                measurement.returned_bytes,
                mib_per_second(
                    u64::try_from(measurement.returned_bytes).unwrap_or(0),
                    measurement.wall_ns
                )
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        "{{\"qualification\":false,\"purpose\":\"release_current_implementation_baseline\",\"throughput_measurement_admissible\":{eligible},\"status\":\"{status}\",\"candidate\":\"{}\",\"profile_id\":\"{profile}\",\"size_bytes\":{size},\"operation\":\"{operation}\",\"iteration\":{iteration},\"warmup\":{warmup},\"fixture\":\"{}\",\"fixture_manifest\":\"wp4m-retained-fixture-manifest.json\",\"source_fingerprint\":\"{source_fingerprint}\",\"expected_cdc_references\":{expected_references_json},\"expected_cdc_sequence_fingerprint\":{expected_sequence_json},\"actual_cdc_references\":{actual_references},\"ordered_closure_digest\":\"{closure_digest}\",\"root_id\":\"{}\",\"transition_id\":\"{}\",\"capture_publish_wall_ns\":{capture_ns},\"sqlite_qualification_wall_ns\":{verification_ns},\"elapsed_wall_ns\":{},\"source_cdc_wall_ns\":{source_cdc_ns},\"canonical_cas_mapping_stage_wall_ns\":{canonical_stage_ns},\"precommit_closure_validation_wall_ns\":{precommit_ns},\"sqlite_commit_durability_wall_ns\":{commit_ns},\"durable_capture_total_wall_ns\":{durable_ns},\"fresh_reopen_head_wall_ns\":{reopen_ns},\"fresh_full_scrub_wall_ns\":{scrub_ns},\"reconstruction_wall_ns\":{reconstruction_ns},\"range_verification_wall_ns\":{range_ns},\"complete_lifecycle_total_wall_ns\":{lifecycle_ns},\"durable_phase_sum_ns\":{durable_sum_ns},\"durable_phase_sum_matches\":{durable_matches},\"lifecycle_phase_sum_ns\":{lifecycle_sum_ns},\"lifecycle_phase_sum_matches\":{lifecycle_matches},\"source_cdc_nested_in_mapping_stage\":false,\"precommit_includes_reconstruction\":true,\"source_bytes_read\":{source_bytes_read},\"source_cdc_bytes_read\":{source_cdc_bytes_read},\"canonical_stage_source_bytes_read\":{canonical_stage_source_bytes_read},\"raw_bytes_hashed\":{raw_bytes_hashed},\"capture_mib_s\":\"{capture_mib_s}\",\"complete_lifecycle_mib_s\":\"{complete_mib_s}\",\"scrub_authentication_mib_s\":\"Unavailable\",\"reconstruction_mib_s\":\"{reconstruction_mib_s}\",\"range_measurements\":[{ranges_json}],\"cpu_ns\":\"Unavailable\",\"rss_bytes\":\"Unavailable\",\"allocated_store_delta_bytes\":\"Unavailable\",\"physical_db_bytes\":{},\"physical_journal_bytes\":{},\"physical_authority_sidecar_bytes\":{},\"physical_allocation_bytes\":\"Unavailable\",\"q_high_water\":\"Unavailable\",\"q_single_canonical_max\":{},\"w_bytes\":{},\"d_bytes\":{},\"sql_statements\":{},\"sql_rows\":{},\"blob_opens\":{},\"transactions\":{},\"commits\":{},\"objects_created\":{},\"objects_reused\":{},\"objects_authenticated\":{},\"canonical_bytes_authenticated\":{},\"canonical_bytes_written\":{},\"mapping_bytes_rewritten\":{},\"closure_occurrences\":{},\"chunks\":{},\"references\":{},\"pages\":{},\"branches\":{},\"suffix_references\":{},\"suffix_bytes\":{},\"suffix_objects\":{},\"receipt_provenance\":\"first=Unavailable;cleanup_first=Unavailable;reconciliation=observed-fresh-reopen;dominant=Unavailable\",\"error\":{error_json}}}",
        candidate.name,
        if size == SOURCE_100 { "S1-100" } else { "S1-512" },
        hex_id(root),
        hex_id(transition),
        started.elapsed().as_nanos(),
        optional_u64_json(database_bytes),
        optional_u64_json(journal_bytes),
        optional_u64_json(authority_bytes),
        metrics.q_single_canonical_max,
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
        metrics.suffix_bytes,
        metrics.suffix_objects,
        eligible = !warmup,
        source_cdc_ns = phases.source_cdc_ns,
        canonical_stage_ns = phases.canonical_cas_mapping_stage_ns,
        precommit_ns = phases.precommit_closure_validation_ns,
        commit_ns = phases.sqlite_commit_durability_ns,
        durable_ns = phases.durable_capture_total_ns,
        reopen_ns = phases.fresh_reopen_head_ns,
        scrub_ns = phases.fresh_full_scrub_ns,
        reconstruction_ns = phases.reconstruction_ns,
        range_ns = phases.range_verification_ns,
        lifecycle_ns = phases.complete_lifecycle_total_ns,
        durable_sum_ns = durable_sum.unwrap_or(0),
        durable_matches = durable_sum == Some(phases.durable_capture_total_ns),
        lifecycle_sum_ns = lifecycle_sum.unwrap_or(0),
        lifecycle_matches = lifecycle_sum == Some(phases.complete_lifecycle_total_ns),
        source_bytes_read = metrics.source_bytes_read,
        source_cdc_bytes_read = metrics.source_cdc_bytes_read,
        canonical_stage_source_bytes_read = metrics.canonical_stage_source_bytes_read,
        raw_bytes_hashed = metrics.raw_bytes_hashed,
        capture_mib_s = mib_per_second(size, phases.durable_capture_total_ns),
        complete_mib_s = mib_per_second(size, phases.complete_lifecycle_total_ns),
        reconstruction_mib_s = mib_per_second(size, phases.reconstruction_ns),
        ranges_json = ranges_json,
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

fn require_optimized_benchmark() -> AnyResult<()> {
    if cfg!(debug_assertions) {
        return Err("throughput/campaign rows require an optimized --release build (debug_assertions=false)".into());
    }
    Ok(())
}

fn run_row(
    root: &Path,
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
    warmup: bool,
) -> AnyResult<String> {
    require_optimized_benchmark()?;
    prepare_sources_for(root, &[size])?;
    let source = source_path(root, size);
    let (source_length, source_fingerprint) = source_hash(&source)?;
    let expected_observations = (!operation.starts_with("dir-"))
        .then(|| expected_file_observations(&source, operation, source_length, candidate))
        .transpose()?;
    let expected_reference_count = expected_observations
        .as_ref()
        .map(|(count, _, _, _, _)| *count);
    let expected_fingerprint = expected_observations
        .as_ref()
        .map(|(_, fingerprint, _, _, _)| fingerprint.as_str());
    let expected_sequence = expected_observations
        .as_ref()
        .map(|(_, _, sequence, _, _)| sequence.as_str());
    let expected_ranges = expected_observations
        .as_ref()
        .map(|(_, _, _, ranges, _)| ranges.as_slice());
    let expected_probes = expected_observations
        .as_ref()
        .map(|(_, _, _, _, probes)| probes.as_slice());
    let expected_dir_replacement = if operation == "dir-replace" {
        Some((
            u64::try_from(DIRECTORY_ENTRIES / 2 + 1).map_err(|_| CoreError::LengthOverflow)?,
            canonical_bytes(file_codec::encode_file_root(1, 0, 0, 0, &[])?)?.0,
        ))
    } else {
        None
    };
    let db_path = root.join(format!(
        "db-{}-{size}-{operation}-{iteration}.sqlite",
        candidate.name
    ));
    remove_sqlite_image(&db_path)?;
    let mut metrics = Metrics::default();
    let started = Instant::now();
    let mut store = Store::open(&db_path, candidate)?;
    let mut phases = PhaseTimes::default();
    let mut range_measurements = Vec::new();
    let durable_capture_start = Instant::now();
    let qualification_start = Instant::now();
    let mut durable_cursor = durable_capture_start;
    let (root_id, transition_id) = if operation == "full" {
        timed_source_cdc(&source, &mut metrics)?;
        let source_end = Instant::now();
        phases.source_cdc_ns = source_end.duration_since(durable_cursor).as_nanos();
        let stage_started = source_end;
        let (root_id, transition_id) = build_file(&mut store, &source, candidate, &mut metrics)?;
        let stage_end = Instant::now();
        phases.canonical_cas_mapping_stage_ns = stage_end.duration_since(stage_started).as_nanos();
        durable_cursor = stage_end;
        (root_id, transition_id)
    } else if operation == "same-middle"
        || operation == "plus1-early"
        || operation == "plus1-middle"
    {
        drop(store);
        let base_path = root.join(format!(
            "base-{}-{size}-{operation}-{iteration}.sqlite",
            candidate.name
        ));
        remove_sqlite_image(&base_path)?;
        let mut base_store = Store::open(&base_path, candidate)?;
        let (base_root, base_transition) =
            build_file(&mut base_store, &source, candidate, &mut metrics)?;
        base_store.publish(None, base_root, base_transition, &mut metrics)?;
        drop(base_store);
        clone_sqlite_image(&base_path, &db_path)?;
        store = Store::open(&db_path, candidate)?;
        metrics = Metrics::default();
        edit_file(&mut store, &source, candidate, operation, &mut metrics)?
    } else if operation == "dir-create" {
        build_directory(&mut store, candidate, false, false, &mut metrics)?
    } else if operation == "dir-replace" || operation == "dir-leading" {
        drop(store);
        let base_path = root.join(format!(
            "base-{}-{size}-{operation}-{iteration}.sqlite",
            candidate.name
        ));
        remove_sqlite_image(&base_path)?;
        let mut base_store = Store::open(&base_path, candidate)?;
        let (base_root, base_transition) =
            build_directory(&mut base_store, candidate, false, false, &mut metrics)?;
        base_store.publish(None, base_root, base_transition, &mut metrics)?;
        drop(base_store);
        clone_sqlite_image(&base_path, &db_path)?;
        store = Store::open(&db_path, candidate)?;
        metrics = Metrics::default();
        edit_directory(&mut store, candidate, operation, &mut metrics)?
    } else {
        return Err(format!("unknown operation {operation}").into());
    };
    let expected_parent = if operation == "full" || operation == "dir-create" {
        None
    } else {
        store.current_head()?.map(|head| head.1)
    };
    let expected_operations = if let Some(parent) = expected_parent {
        let (path, before, after) = if operation.starts_with("dir-") {
            (
                b"t".to_vec(),
                namespace_entry_id(&store, parent, b"t", &mut metrics)?,
                namespace_entry_id(&store, root_id, b"t", &mut metrics)?,
            )
        } else {
            (
                b"file".to_vec(),
                namespace_entry_id(&store, parent, b"file", &mut metrics)?,
                namespace_entry_id(&store, root_id, b"file", &mut metrics)?,
            )
        };
        Some(vec![delta_codec::TransitionOperation::Replace {
            path,
            before,
            after,
        }])
    } else {
        None
    };
    let precommit_started = if operation == "full" {
        durable_cursor
    } else {
        qualification_start
    };
    let transition_digest = verify_transition(
        &store,
        transition_id,
        expected_parent,
        root_id,
        expected_operations.as_deref(),
        &mut metrics,
    )?;
    let (content_digest, actual_references) = if operation.starts_with("dir-") {
        let digest = verify_directory(
            &store,
            root_id,
            candidate,
            u64::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                .map_err(|_| CoreError::LengthOverflow)?,
            expected_dir_replacement,
            &mut metrics,
        )?;
        (digest, 0)
    } else {
        let (digest, references, _) = verify_file(
            &store,
            root_id,
            candidate,
            expected_fingerprint,
            expected_sequence,
            &mut metrics,
        )?;
        if expected_reference_count != Some(references) {
            return Err(CoreError::LengthMismatch {
                expected: expected_reference_count.unwrap_or(0),
                actual: references,
            }
            .into());
        }
        (digest, references)
    };
    let precommit_end = Instant::now();
    phases.precommit_closure_validation_ns =
        precommit_end.duration_since(precommit_started).as_nanos();
    let closure_digest = combined_closure_digest(transition_digest, content_digest);
    let commit_started = precommit_end;
    store.publish(expected_parent, root_id, transition_id, &mut metrics)?;
    let commit_end = Instant::now();
    phases.sqlite_commit_durability_ns = commit_end.duration_since(commit_started).as_nanos();
    phases.durable_capture_total_ns = commit_end.duration_since(durable_capture_start).as_nanos();
    let capture_ns = if operation == "full" {
        phases.durable_capture_total_ns
    } else {
        qualification_start.elapsed().as_nanos()
    };
    let capture_end_bytes = apparent_store_bytes(&store);
    let reopen_started = commit_end;
    drop(store);
    let mut store = Store::open(&db_path, candidate)?;
    let head = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    if head.1 != root_id || head.2 != transition_id {
        return Err(CoreError::PublicationConflict.into());
    }
    let reopen_end = Instant::now();
    phases.fresh_reopen_head_ns = reopen_end.duration_since(reopen_started).as_nanos();
    let scrub_started = reopen_end;
    let fresh_transition_digest = verify_transition(
        &store,
        transition_id,
        expected_parent,
        root_id,
        expected_operations.as_deref(),
        &mut metrics,
    )?;
    let fresh_references = if operation.starts_with("dir-") {
        let digest = verify_directory(
            &store,
            root_id,
            candidate,
            u64::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                .map_err(|_| CoreError::LengthOverflow)?,
            expected_dir_replacement,
            &mut metrics,
        )?;
        let _ = digest;
        0
    } else {
        let mut scrub_store = store;
        let (_, references) = scrub_file(&mut scrub_store, root_id, candidate, &mut metrics)?;
        store = scrub_store;
        references
    };
    let scrub_end = Instant::now();
    phases.fresh_full_scrub_ns = scrub_end.duration_since(scrub_started).as_nanos();
    let reconstruction_started = scrub_end;
    let (fresh_content_digest, reconstructed_references) = if operation.starts_with("dir-") {
        let digest = verify_directory(
            &store,
            root_id,
            candidate,
            u64::try_from(DIRECTORY_ENTRIES + usize::from(operation == "dir-leading"))
                .map_err(|_| CoreError::LengthOverflow)?,
            expected_dir_replacement,
            &mut metrics,
        )?;
        (digest, 0)
    } else {
        let (digest, references, _) = verify_file(
            &store,
            root_id,
            candidate,
            expected_fingerprint,
            expected_sequence,
            &mut metrics,
        )?;
        (digest, references)
    };
    let reconstruction_end = Instant::now();
    phases.reconstruction_ns = reconstruction_end
        .duration_since(reconstruction_started)
        .as_nanos();
    if !operation.starts_with("dir-") {
        let file_root = namespace_entry_id(&store, root_id, b"file", &mut metrics)?;
        range_measurements = verify_ranges(
            &store,
            file_root,
            expected_probes.ok_or(CoreError::MissingObject)?,
            expected_ranges.ok_or(CoreError::MissingObject)?,
            &mut metrics,
        )?;
    }
    if reconstructed_references != fresh_references {
        return Err(CoreError::PublicationConflict.into());
    }
    if fresh_references != actual_references
        || combined_closure_digest(fresh_transition_digest, fresh_content_digest) != closure_digest
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let lifecycle_end = Instant::now();
    if !operation.starts_with("dir-") {
        phases.range_verification_ns = lifecycle_end.duration_since(reconstruction_end).as_nanos();
    }
    phases.complete_lifecycle_total_ns = lifecycle_end
        .duration_since(durable_capture_start)
        .as_nanos();
    drop(store);
    let qualification_ns = if operation == "full" {
        phases.complete_lifecycle_total_ns
    } else {
        qualification_start.elapsed().as_nanos()
    };
    let output = row_json(
        candidate,
        size,
        operation,
        iteration,
        warmup,
        &source_fingerprint,
        started,
        capture_ns,
        qualification_ns,
        root_id,
        transition_id,
        expected_reference_count,
        expected_sequence,
        fresh_references,
        closure_digest,
        metrics,
        capture_end_bytes,
        &phases,
        &range_measurements,
        None,
    )?;
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
    let (root_id, transition_id) = build_file(&mut store, &source, candidate, &mut metrics)?;
    let (_, expected_fingerprint, expected_sequence, _expected_ranges, _) =
        expected_file_observations(&source, "full", 256 * 1024, candidate)?;
    let _ = verify_transition(&store, transition_id, None, root_id, None, &mut metrics)?;
    let _ = verify_file(
        &store,
        root_id,
        candidate,
        Some(&expected_fingerprint),
        Some(&expected_sequence),
        &mut metrics,
    )?;
    store.publish(None, root_id, transition_id, &mut metrics)?;
    drop(store);
    let store = Store::open(&db, candidate)?;
    let _ = verify_file(
        &store,
        root_id,
        candidate,
        Some(&expected_fingerprint),
        Some(&expected_sequence),
        &mut metrics,
    )?;
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

fn json_string_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("\"{key}\":\"");
    let start = line.find(&marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('\"')?;
    Some(&rest[..end])
}

fn json_u128_field(line: &str, key: &str) -> Option<u128> {
    let marker = format!("\"{key}\":");
    let start = line.find(&marker)? + marker.len();
    let rest = &line[start..];
    rest.split([',', '}']).next()?.parse().ok()
}

fn json_usize_field(line: &str, key: &str) -> Option<usize> {
    json_u128_field(line, key)?.try_into().ok()
}

fn decimal_seconds_to_ns(value: &str) -> Option<u128> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let seconds = whole.parse::<u128>().ok()?;
    let mut nanoseconds = 0_u128;
    for byte in fraction.as_bytes().iter().take(9) {
        nanoseconds = nanoseconds
            .checked_mul(10)?
            .checked_add(u128::from(byte.saturating_sub(b'0')))?;
    }
    for _ in fraction.len().min(9)..9 {
        nanoseconds = nanoseconds.checked_mul(10)?;
    }
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_add(nanoseconds))
}

fn external_resource_metrics(stderr: &str) -> (Option<u128>, Option<u64>) {
    let mut user_ns = None;
    let mut sys_ns = None;
    let mut rss_bytes = None;
    for line in stderr.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if let Some(index) = tokens.iter().position(|token| *token == "user") {
            if index > 0 {
                user_ns = decimal_seconds_to_ns(tokens[index - 1]);
            }
        }
        if let Some(index) = tokens.iter().position(|token| *token == "sys") {
            if index > 0 {
                sys_ns = decimal_seconds_to_ns(tokens[index - 1]);
            }
        }
        if line.contains("maximum resident set size") {
            rss_bytes = tokens
                .iter()
                .rev()
                .find_map(|token| token.parse::<u64>().ok());
        }
    }
    (
        user_ns.and_then(|user| sys_ns.and_then(|sys| user.checked_add(sys))),
        rss_bytes,
    )
}

fn add_external_resource_metrics(stdout: &str, stderr: &str) -> String {
    let (cpu_ns, rss_bytes) = external_resource_metrics(stderr);
    let mut line = stdout.trim_end().to_string();
    if let Some(cpu_ns) = cpu_ns {
        line = line.replace(
            "\"cpu_ns\":\"Unavailable\"",
            &format!("\"cpu_ns\":{cpu_ns}"),
        );
    }
    if let Some(rss_bytes) = rss_bytes {
        line = line.replace(
            "\"rss_bytes\":\"Unavailable\"",
            &format!("\"rss_bytes\":{rss_bytes}"),
        );
    }
    line.push('\n');
    line
}

#[allow(clippy::too_many_arguments)]
fn invoke_campaign_row(
    root: &Path,
    candidate: Candidate,
    size: u64,
    operation: &str,
    iteration: usize,
    warmup: bool,
    output: &mut File,
    failures: &mut File,
    commands: &mut File,
) -> AnyResult<()> {
    let executable = env::current_exe()?;
    let mut command = if Path::new("/usr/bin/time").is_file() {
        let mut command = std::process::Command::new("/usr/bin/time");
        command.arg("-l").arg(&executable);
        command
    } else {
        std::process::Command::new(&executable)
    };
    let args = vec![
        "--row".to_string(),
        root.to_str().ok_or("non-UTF8 campaign root")?.to_string(),
        candidate.name.to_string(),
        size.to_string(),
        operation.to_string(),
        iteration.to_string(),
        warmup.to_string(),
    ];
    command.args(&args);
    writeln!(
        commands,
        "{:?} {:?}",
        command.get_program(),
        command.get_args().collect::<Vec<_>>()
    )?;
    let result = command.output()?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr).replace('\n', " ");
        writeln!(
            failures,
            "{{\"candidate\":\"{}\",\"size_bytes\":{},\"operation\":\"{}\",\"iteration\":{},\"stderr\":\"{}\"}}",
            candidate.name,
            size,
            operation,
            iteration,
            stderr.replace('"', "'")
        )?;
        return Err(format!("row failed {}: {stderr}", candidate.name).into());
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    output.write_all(add_external_resource_metrics(&stdout, &stderr).as_bytes())?;
    Ok(())
}

fn median(values: &[u128]) -> Option<u128> {
    let mut values = values.to_vec();
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

fn write_campaign_summary(root: &Path, jsonl: &Path, invocations: usize) -> AnyResult<()> {
    let raw = fs::read_to_string(jsonl)?;
    let mut warmup = 0_usize;
    let mut measured = 0_usize;
    let mut failures = 0_usize;
    let mut protected_metrics_available = true;
    let mut groups: BTreeMap<String, Vec<(usize, u128)>> = BTreeMap::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let is_warmup = line.contains("\"warmup\":true");
        if is_warmup {
            warmup = warmup.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        } else {
            measured = measured.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
        if !line.contains("\"status\":\"PASS\"") {
            failures = failures.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
        if json_string_field(line, "cpu_ns").is_some()
            || json_string_field(line, "rss_bytes").is_some()
            || json_string_field(line, "allocated_store_delta_bytes").is_some()
        {
            protected_metrics_available = false;
        }
        let Some(candidate) = json_string_field(line, "candidate") else {
            continue;
        };
        let Some(size) = json_u128_field(line, "size_bytes") else {
            continue;
        };
        let Some(operation) = json_string_field(line, "operation") else {
            continue;
        };
        let Some(qualification) = json_u128_field(line, "sqlite_qualification_wall_ns") else {
            continue;
        };
        let iteration = json_usize_field(line, "iteration").unwrap_or(0);
        groups
            .entry(format!("{candidate}|{size}|{operation}"))
            .or_default()
            .push((iteration, qualification));
    }
    let mut rows = String::new();
    let mut first = true;
    for (key, values) in &groups {
        let mut samples: Vec<u128> = values.iter().map(|(_, value)| *value).collect();
        samples.sort_unstable();
        let min = samples.first().copied().unwrap_or(0);
        let max = samples.last().copied().unwrap_or(0);
        let spread = max.saturating_sub(min);
        if !first {
            rows.push(',');
        }
        first = false;
        rows.push_str(&format!(
            "{{\"group\":\"{key}\",\"samples\":{},\"median_sqlite_qualification_wall_ns\":{},\"min_ns\":{min},\"max_ns\":{max},\"spread_ns\":{spread}}}",
            samples.len(),
            median(&samples).unwrap_or(0)
        ));
    }
    let gate = if invocations != 198 || warmup != 33 || measured != 165 || failures != 0 {
        "FAIL"
    } else {
        "INCONCLUSIVE"
    };
    let summary_path = root.join("wp4m-profile-selection-summary.json");
    let mut summary = File::create(&summary_path)?;
    writeln!(
        summary,
        "{{\"format\":1,\"invocations\":{invocations},\"warmup\":{warmup},\"measured\":{measured},\"row_failures\":{failures},\"protected_metrics_available\":{protected_metrics_available},\"internal_500ms_diagnostic\":\"Unavailable\",\"sql_sensitivity\":\"INCONCLUSIVE-no-low-SQL-control\",\"admissibility\":\"{gate}\",\"reason\":\"protected CPU/RSS/allocated deltas are not all present in row evidence\",\"rows\":[{rows}]}}"
    )?;
    summary.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use layerfs_core::cas::InMemoryCas;
    use layerfs_core::content::{ChunkReference, LogicalFile};

    fn test_path(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "layerfs-wp4m-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn candidate_sqlite_matches_memory_range_and_reopens() {
        let raw = b"candidate-shadow-memory-parity";
        let source = test_path("parity-source");
        let database = test_path("parity-db.sqlite");
        fs::write(&source, raw).expect("source");

        let mut cas = InMemoryCas::new();
        let (chunk, _) = cas.put_chunk(raw).expect("memory chunk");
        let logical =
            LogicalFile::from_chunks(&cas, vec![ChunkReference::new(chunk, raw.len() as u64)])
                .expect("memory file");
        let memory = logical.read_range(&cas, 4..24).expect("memory range");

        let candidate = FILE_CANDIDATES[0];
        let mut metrics = Metrics::default();
        let (root, transition) = {
            let mut store = Store::open(&database, candidate).expect("candidate open");
            let (root, transition) =
                build_file(&mut store, &source, candidate, &mut metrics).expect("candidate build");
            let file_root =
                namespace_entry_id(&store, root, b"file", &mut metrics).expect("file root");
            let sqlite =
                read_file_range(&store, file_root, 4..24, &mut metrics).expect("sqlite range");
            assert_eq!(sqlite, memory.bytes());
            store
                .publish(None, root, transition, &mut metrics)
                .expect("candidate publish");
            (root, transition)
        };

        let reopened = Store::open(&database, candidate).expect("candidate reopen");
        assert_eq!(
            reopened
                .current_head()
                .expect("head")
                .map(|head| (head.1, head.2)),
            Some((root, transition))
        );
        drop(reopened);
        fs::remove_file(source).expect("source cleanup");
        remove_sqlite_image(&database).expect("database cleanup");
    }

    #[test]
    fn publication_faults_record_reconciliation_and_require_private_authority() {
        let source = test_path("fault-source");
        fs::write(&source, b"publication-fault-test").expect("source");
        let candidate = FILE_CANDIDATES[0];

        let before_database = test_path("fault-before.sqlite");
        {
            let mut store = Store::open(&before_database, candidate).expect("open");
            let mut metrics = Metrics::default();
            let result = build_file(&mut store, &source, candidate, &mut metrics).expect("build");
            let provenance = store
                .publish_with_fault(
                    None,
                    result.0,
                    result.1,
                    Some(PublishFault::BeforeCommit),
                    &mut metrics,
                )
                .expect("fault provenance");
            assert_eq!(provenance.first, Some(CoreError::Io));
            assert_eq!(provenance.reconciliation, Reconciliation::NotAttempted);
        }
        let reopened = Store::open(&before_database, candidate).expect("reopen");
        assert!(reopened.current_head().expect("head").is_none());
        drop(reopened);
        remove_sqlite_image(&before_database).expect("before cleanup");

        let after_database = test_path("fault-after.sqlite");
        {
            let mut store = Store::open(&after_database, candidate).expect("open");
            let mut metrics = Metrics::default();
            let result = build_file(&mut store, &source, candidate, &mut metrics).expect("build");
            let provenance = store
                .publish_with_fault(
                    None,
                    result.0,
                    result.1,
                    Some(PublishFault::AfterCommitBeforeAck),
                    &mut metrics,
                )
                .expect("fault provenance");
            assert_eq!(provenance.first, Some(CoreError::Io));
            assert_eq!(provenance.reconciliation, Reconciliation::RequestedVisible);
            assert_eq!(provenance.dominant, None);
        }
        let authority = authority_path(&after_database);
        fs::remove_file(&authority).expect("authority removal");
        assert!(Store::open(&after_database, candidate).is_err());
        remove_sqlite_image(&after_database).expect("after cleanup");
        fs::remove_file(source).expect("source cleanup");
    }
}

fn run_campaign(root: &Path) -> AnyResult<()> {
    require_optimized_benchmark()?;
    prepare_sources(root)?;
    let jsonl = root.join("wp4m-profile-selection.jsonl");
    let mut output = File::create(&jsonl)?;
    let mut failures = File::create(root.join("wp4m-profile-selection-failures.jsonl"))?;
    let mut commands = File::create(root.join("wp4m-profile-selection-commands.txt"))?;
    let mut invocations = 0_usize;
    for iteration in 0..6 {
        let warmup = iteration == 0;
        let mut candidates = FILE_CANDIDATES.to_vec();
        let candidate_count = candidates.len();
        candidates.rotate_left(iteration % candidate_count);
        for candidate in candidates {
            for size in [SOURCE_100, SOURCE_512] {
                for operation in ["full", "same-middle", "plus1-early", "plus1-middle"] {
                    invoke_campaign_row(
                        root,
                        candidate,
                        size,
                        operation,
                        iteration,
                        warmup,
                        &mut output,
                        &mut failures,
                        &mut commands,
                    )?;
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
                invoke_campaign_row(
                    root,
                    candidate,
                    SOURCE_100,
                    operation,
                    iteration,
                    warmup,
                    &mut output,
                    &mut failures,
                    &mut commands,
                )?;
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
    failures.sync_all()?;
    commands.sync_all()?;
    write_campaign_summary(root, &jsonl, invocations)?;
    println!(
        "campaign COMPLETE invocations={invocations} jsonl={} summary={}",
        jsonl.display(),
        root.join("wp4m-profile-selection-summary.json").display()
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
