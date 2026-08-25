#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IntegrityMode {
    #[default]
    Verified,
    TrustedLocalDev,
}

use crate::refs::validate_ref_name;
use crate::scratch::{DiskNamespace, DiskTable};
use crate::{map_sqlite_error, EngineError, EngineResult};
use layerfs_core::content::extent::ExtentNodeV3;
use layerfs_core::content::extent_codec::{decode_file_state, decode_node_with_context};
use layerfs_core::content::rope::{validate_file, FileStateRoot, ObjectRead};
use layerfs_core::inode::{
    visit_inode_table_entries, InodeId, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{
    validate_inode_record_metadata, visit_directory_entries, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_core::namespace_codec::{
    decode_directory_node, decode_directory_state, decode_inode_record, decode_inode_table_node,
    decode_metadata_node, decode_namespace_root, decode_symlink, DirectoryNodeV1, InodeTableNodeV1,
    MetadataNodeV1,
};
use layerfs_core::{
    authenticate_identity, decode_bytes_object, validate_object_from, ObjectId, ObjectKind,
};
use rusqlite::{params, Connection};
use std::cell::Cell;
use std::io::Cursor;
use std::path::Path;

#[derive(Clone, Copy)]
#[repr(u8)]
enum Role {
    Namespace = 1,
    InodeTable = 2,
    InodeRecord = 3,
    DirectoryState = 4,
    DirectoryNode = 5,
    FileState = 6,
    ExtentNode = 7,
    MetadataNode = 8,
    Symlink = 9,
    Payload = 10,
}

pub(crate) struct RetainedUnionObservation {
    pub(crate) verification: VerificationObservation,
    pub(crate) peak_bytes: u64,
}

pub(crate) fn verify_retained_union_observed(
    connection: &Connection,
    store: &Path,
) -> EngineResult<RetainedUnionObservation> {
    let retained = retained_union(connection, store)?;
    Ok(RetainedUnionObservation {
        verification: retained.observation,
        peak_bytes: retained.peak_bytes,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VerificationObservation {
    pub(crate) objects: u64,
    pub(crate) bytes: u64,
    pub(crate) statements: u64,
    pub(crate) fetched_rows: u64,
    pub(crate) authentication_passes: u64,
    pub(crate) role_decode_passes: u64,
    pub(crate) scratch_tables: u64,
    pub(crate) scratch_statements: u64,
    pub(crate) scratch_rows: u64,
    pub(crate) scratch_bytes: u64,
    pub(crate) namespace_graphs: u64,
    pub(crate) retained_roots_validated: u64,
}

pub(crate) fn verify_root(
    connection: &Connection,
    store: &Path,
    root: ObjectId,
) -> EngineResult<VerificationObservation> {
    let store_id = store_id(connection)?;
    let work = DiskTable::create_near_with_store_id(store, "publication-closure", store_id)?;
    let graph = DiskTable::create_near_with_store_id(store, "publication-graph", store_id)?;
    let records = graph.namespace(b"records")?;
    let state = graph.namespace(b"state")?;
    let payload_lengths = graph.namespace(b"payload-lengths")?;
    enqueue(&work, root, Role::Namespace, true)?;
    let mut observation = drain(connection, &work, &payload_lengths)?;
    records.clear()?;
    state.clear()?;
    observation.merge(validate_namespace_graph_disk(
        connection,
        &records,
        &state,
        &payload_lengths,
        root,
    )?)?;
    observation.add_scratch(work.observation()?)?;
    observation.add_scratch(graph.observation()?)?;
    Ok(observation)
}

impl VerificationObservation {
    fn merge(&mut self, source: Self) -> EngineResult<()> {
        self.objects = self
            .objects
            .checked_add(source.objects)
            .ok_or(EngineError::CounterOverflow)?;
        self.bytes = self
            .bytes
            .checked_add(source.bytes)
            .ok_or(EngineError::CounterOverflow)?;
        self.statements = self
            .statements
            .checked_add(source.statements)
            .ok_or(EngineError::CounterOverflow)?;
        self.fetched_rows = self
            .fetched_rows
            .checked_add(source.fetched_rows)
            .ok_or(EngineError::CounterOverflow)?;
        self.authentication_passes = self
            .authentication_passes
            .checked_add(source.authentication_passes)
            .ok_or(EngineError::CounterOverflow)?;
        self.role_decode_passes = self
            .role_decode_passes
            .checked_add(source.role_decode_passes)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_tables = self
            .scratch_tables
            .checked_add(source.scratch_tables)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_statements = self
            .scratch_statements
            .checked_add(source.scratch_statements)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_rows = self
            .scratch_rows
            .checked_add(source.scratch_rows)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_bytes = self
            .scratch_bytes
            .checked_add(source.scratch_bytes)
            .ok_or(EngineError::CounterOverflow)?;
        self.namespace_graphs = self
            .namespace_graphs
            .checked_add(source.namespace_graphs)
            .ok_or(EngineError::CounterOverflow)?;
        self.retained_roots_validated = self
            .retained_roots_validated
            .checked_add(source.retained_roots_validated)
            .ok_or(EngineError::CounterOverflow)?;
        Ok(())
    }

    fn add_scratch(&mut self, source: crate::scratch::ScratchObservation) -> EngineResult<()> {
        self.scratch_tables = self
            .scratch_tables
            .checked_add(source.tables)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_statements = self
            .scratch_statements
            .checked_add(source.statements)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_rows = self
            .scratch_rows
            .checked_add(source.rows)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_bytes = self
            .scratch_bytes
            .checked_add(source.high_water_bytes)
            .ok_or(EngineError::CounterOverflow)?;
        Ok(())
    }
}

fn store_id(connection: &Connection) -> EngineResult<[u8; 32]> {
    connection
        .query_row(
            "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(super::map_sqlite_error)?
        .try_into()
        .map_err(|_| EngineError::InvalidRecord("StoreId"))
}

pub(crate) struct RetainedUnion {
    pub(crate) work: DiskTable,
    pub(crate) peak_bytes: u64,
    pub(crate) observation: VerificationObservation,
}

pub(crate) fn retained_union(connection: &Connection, store: &Path) -> EngineResult<RetainedUnion> {
    let store_id = store_id(connection)?;
    let work = DiskTable::create_near_with_store_id(store, "closure", store_id)?;
    let graph = DiskTable::create_near_with_store_id(store, "namespace-graph", store_id)?;
    let records = graph.namespace(b"records")?;
    let state = graph.namespace(b"state")?;
    let payload_lengths = graph.namespace(b"payload-lengths")?;
    let validated_roots = graph.namespace(b"validated-roots")?;
    // StoreId admission plus the ordered ref scan below are real Store SQL.
    let mut observation = VerificationObservation {
        statements: 2,
        ..VerificationObservation::default()
    };
    let mut peak_bytes = work.storage_bytes()?.saturating_add(graph.storage_bytes()?);
    let mut statement = connection
        .prepare("SELECT name, generation, root_id FROM layerfs_refs ORDER BY name")
        .map_err(map_sqlite_error)?;
    let refs = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(map_sqlite_error)?;
    for row in refs {
        let (name, generation, root) = row.map_err(map_sqlite_error)?;
        validate_ref_name(&name)?;
        if generation < 0 {
            return Err(EngineError::InvalidRecord("ref generation"));
        }
        let root = ObjectId::from_bytes(&root)?;
        let retained = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_retained_roots WHERE root_id = ?1)",
                params![root.as_bytes().as_slice()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        observation.statements = observation
            .statements
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?;
        if !retained {
            return Err(EngineError::MissingRoot(root));
        }
        if claim_root(&validated_roots, root)? {
            observation.retained_roots_validated = observation
                .retained_roots_validated
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            enqueue(&work, root, Role::Namespace, true)?;
            observation.merge(drain(connection, &work, &payload_lengths)?)?;
            records.clear()?;
            state.clear()?;
            observation.merge(validate_namespace_graph_disk(
                connection,
                &records,
                &state,
                &payload_lengths,
                root,
            )?)?;
            peak_bytes =
                peak_bytes.max(work.storage_bytes()?.saturating_add(graph.storage_bytes()?));
        }
    }
    drop(statement);
    observation.statements = observation
        .statements
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    let mut statement = connection
        .prepare("SELECT root_id FROM layerfs_retained_roots ORDER BY root_id")
        .map_err(map_sqlite_error)?;
    let roots = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(map_sqlite_error)?;
    for root in roots {
        let root = ObjectId::from_bytes(&root.map_err(map_sqlite_error)?)?;
        if claim_root(&validated_roots, root)? {
            observation.retained_roots_validated = observation
                .retained_roots_validated
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            enqueue(&work, root, Role::Namespace, true)?;
            observation.merge(drain(connection, &work, &payload_lengths)?)?;
            records.clear()?;
            state.clear()?;
            observation.merge(validate_namespace_graph_disk(
                connection,
                &records,
                &state,
                &payload_lengths,
                root,
            )?)?;
            peak_bytes =
                peak_bytes.max(work.storage_bytes()?.saturating_add(graph.storage_bytes()?));
        }
    }
    observation.add_scratch(work.observation()?)?;
    observation.add_scratch(graph.observation()?)?;
    Ok(RetainedUnion {
        work,
        peak_bytes,
        observation,
    })
}

fn claim_root(validated_roots: &DiskNamespace<'_>, root: ObjectId) -> EngineResult<bool> {
    if validated_roots.get(root.as_bytes())?.is_some() {
        return Ok(false);
    }
    validated_roots.put(root.as_bytes(), &[])?;
    Ok(true)
}

const RECORD_KEY: u8 = 0x40;
const SEEN_KEY: u8 = 0x41;
const REF_KEY: u8 = 0x42;
const QUEUE_KEY: u8 = 0x43;

fn validate_namespace_graph_disk(
    connection: &Connection,
    records: &DiskNamespace<'_>,
    state: &DiskNamespace<'_>,
    payload_lengths: &DiskNamespace<'_>,
    root: ObjectId,
) -> EngineResult<VerificationObservation> {
    let object_store = ConnectionStore::new(connection);
    let namespace = object_store
        .with_authenticated_canonical(root, decode_namespace_root)
        .map_err(EngineError::Core)?;
    let mut scratch_error = None;
    let visited = visit_inode_table_entries(
        &object_store,
        InodeTableRoot(namespace.inode_table_root),
        &mut InodeTableCounters::default(),
        |entries| {
            for (inode, record) in entries {
                if let Err(error) = records.put(&graph_key(RECORD_KEY, *inode), record.as_bytes()) {
                    scratch_error = Some(error);
                    return Err(layerfs_core::CoreError::Io);
                }
            }
            Ok(())
        },
    );
    if let Some(error) = scratch_error {
        return Err(error);
    }
    visited.map_err(EngineError::Core)?;

    enqueue_graph_inode(
        &object_store,
        records,
        state,
        namespace.root_directory_inode,
        true,
    )?;
    while let Some((key, value)) = state.pop_pending()? {
        if key.first() != Some(&QUEUE_KEY) || key.len() != 33 || value.len() != 1 {
            return Err(EngineError::InvalidRecord("namespace graph queue"));
        }
        let inode = InodeId::from_slice(&key[1..])?;
        let record = graph_record(&object_store, records, inode)?;
        let root_inode = value[0] != 0;
        validate_inode_record_metadata(&object_store, record, root_inode)
            .map_err(EngineError::Core)?;
        let mut callback_error = None;
        let validated = match record.kind {
            InodeKind::RegularFile => validate_file(
                &PayloadSummaryStore {
                    store: &object_store,
                    payload_lengths,
                },
                FileStateRoot(record.content_root),
            ),
            InodeKind::Symlink => object_store
                .with_authenticated_canonical(record.content_root, |canonical| {
                    decode_symlink(canonical).map(drop)
                }),
            InodeKind::Directory => visit_directory_entries(
                &object_store,
                DirectoryStateRoot(record.content_root),
                &mut NamespaceCounters::default(),
                |entries| {
                    for (_, child) in entries {
                        if let Err(error) =
                            observe_graph_inode(&object_store, records, state, *child)
                        {
                            callback_error = Some(error);
                            return Err(layerfs_core::CoreError::Io);
                        }
                    }
                    Ok(())
                },
            ),
        };
        if let Some(error) = callback_error {
            return Err(error);
        }
        validated.map_err(EngineError::Core)?;
    }

    records.for_each_key(|key| {
        if key.first() != Some(&RECORD_KEY) || key.len() != 33 {
            return Err(EngineError::InvalidRecord("namespace record key"));
        }
        let inode = InodeId::from_slice(&key[1..])?;
        if state.get(&graph_key(SEEN_KEY, inode))?.is_none() {
            return Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
                "unreachable inode table entry",
            )));
        }
        let record = graph_record(&object_store, records, inode)?;
        let observed = state
            .get(&graph_key(REF_KEY, inode))?
            .map(|bytes| decode_ref_count(&bytes))
            .transpose()?
            .unwrap_or(0);
        let expected = if inode == namespace.root_directory_inode {
            0
        } else {
            record.namespace_ref_count
        };
        if observed != expected {
            return Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
                "namespace ref count",
            )));
        }
        Ok(())
    })?;
    let mut observation = object_store.observation();
    observation.namespace_graphs = 1;
    Ok(observation)
}

fn graph_key(prefix: u8, inode: InodeId) -> [u8; 33] {
    let mut key = [0; 33];
    key[0] = prefix;
    key[1..].copy_from_slice(inode.as_bytes());
    key
}

fn graph_record(
    store: &ConnectionStore<'_>,
    records: &DiskNamespace<'_>,
    inode: InodeId,
) -> EngineResult<layerfs_core::inode::InodeRecordV1> {
    let id = records
        .get(&graph_key(RECORD_KEY, inode))?
        .ok_or(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
            "directory references missing inode",
        )))?;
    let id = ObjectId::from_bytes(&id)?;
    store
        .with_authenticated_canonical(id, decode_inode_record)
        .map_err(EngineError::Core)
}

fn enqueue_graph_inode(
    store: &ConnectionStore<'_>,
    records: &DiskNamespace<'_>,
    state: &DiskNamespace<'_>,
    inode: InodeId,
    root: bool,
) -> EngineResult<()> {
    let record = graph_record(store, records, inode)?;
    if root && record.kind != InodeKind::Directory {
        return Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
            "root inode kind",
        )));
    }
    state.put(&graph_key(SEEN_KEY, inode), &[])?;
    state.enqueue_once(&graph_key(QUEUE_KEY, inode), &[u8::from(root)])
}

fn observe_graph_inode(
    store: &ConnectionStore<'_>,
    records: &DiskNamespace<'_>,
    state: &DiskNamespace<'_>,
    inode: InodeId,
) -> EngineResult<()> {
    let count_key = graph_key(REF_KEY, inode);
    let count = state
        .get(&count_key)?
        .map(|bytes| decode_ref_count(&bytes))
        .transpose()?
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    state.put(&count_key, &count.to_be_bytes())?;
    let record = graph_record(store, records, inode)?;
    if state.get(&graph_key(SEEN_KEY, inode))?.is_some() {
        if record.kind == InodeKind::Directory {
            return Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
                "directory has multiple parents",
            )));
        }
        return Ok(());
    }
    enqueue_graph_inode(store, records, state, inode, false)
}

fn decode_ref_count(bytes: &[u8]) -> EngineResult<u64> {
    Ok(u64::from_be_bytes(bytes.try_into().map_err(|_| {
        EngineError::InvalidRecord("namespace reference count")
    })?))
}

struct ConnectionStore<'a> {
    connection: &'a Connection,
    observation: Cell<VerificationObservation>,
}

struct PayloadSummaryStore<'a, 'connection, 'scratch> {
    store: &'a ConnectionStore<'connection>,
    payload_lengths: &'a DiskNamespace<'scratch>,
}

impl ObjectRead for PayloadSummaryStore<'_, '_, '_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, layerfs_core::CoreError> {
        self.store.fetch(id)
    }

    fn with_authenticated_canonical<T, F>(
        &self,
        id: ObjectId,
        callback: F,
    ) -> Result<T, layerfs_core::CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, layerfs_core::CoreError>,
    {
        self.store.with_authenticated_canonical(id, callback)
    }

    fn get_authenticated_payload_lengths_batch<F>(
        &self,
        ids: &[ObjectId],
        mut callback: F,
    ) -> Result<(), layerfs_core::CoreError>
    where
        F: FnMut(ObjectId, u32) -> Result<(), layerfs_core::CoreError>,
    {
        let keys = ids
            .iter()
            .map(|id| id.as_bytes().as_slice())
            .collect::<Vec<_>>();
        self.payload_lengths
            .get_ordered_batch(&keys, |ordinal, bytes| {
                let id = ids[ordinal];
                let bytes =
                    bytes.ok_or(EngineError::Core(layerfs_core::CoreError::MissingObject))?;
                let length = u32::from_be_bytes(bytes.try_into().map_err(|_| {
                    EngineError::Core(layerfs_core::CoreError::InvalidRecord("payload summary"))
                })?);
                callback(id, length).map_err(EngineError::Core)
            })
            .map_err(|error| match error {
                EngineError::Core(error) => error,
                EngineError::CounterOverflow => layerfs_core::CoreError::LengthOverflow,
                EngineError::InvalidRecord(name) => layerfs_core::CoreError::InvalidRecord(name),
                _ => layerfs_core::CoreError::Io,
            })
    }
}

impl<'a> ConnectionStore<'a> {
    fn new(connection: &'a Connection) -> Self {
        Self {
            connection,
            observation: Cell::new(VerificationObservation::default()),
        }
    }

    fn observation(&self) -> VerificationObservation {
        self.observation.get()
    }

    fn with_record<T>(
        &self,
        id: ObjectId,
        role_decode: bool,
        callback: impl FnOnce(ObjectKind, &[u8]) -> Result<T, layerfs_core::CoreError>,
    ) -> Result<T, layerfs_core::CoreError> {
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?1",
            )
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let mut rows = statement
            .query(params![id.as_bytes().as_slice()])
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let row = rows
            .next()
            .map_err(|_| layerfs_core::CoreError::Io)?
            .ok_or(layerfs_core::CoreError::MissingObject)?;
        let kind = row
            .get::<_, i64>(0)
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let length = row
            .get::<_, i64>(1)
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let bytes = match row.get_ref(2).map_err(|_| layerfs_core::CoreError::Io)? {
            rusqlite::types::ValueRef::Blob(bytes) => bytes,
            _ => return Err(layerfs_core::CoreError::InvalidRecord("object bytes")),
        };
        let expected_kind = ObjectKind::try_from(
            u8::try_from(kind)
                .map_err(|_| layerfs_core::CoreError::InvalidRecord("object kind"))?,
        )?;
        let expected_length = u64::try_from(length)
            .map_err(|_| layerfs_core::CoreError::InvalidRecord("object length"))?;
        let object = authenticate_identity(bytes, id)?;
        if object.kind != expected_kind || bytes.len() as u64 != expected_length {
            return Err(layerfs_core::CoreError::LengthMismatch {
                expected: expected_length,
                actual: bytes.len() as u64,
            });
        }
        if !role_decode && validate_object_from(Cursor::new(bytes))? != object {
            return Err(layerfs_core::CoreError::InvalidRecord("object summary"));
        }
        let mut observation = self.observation.get();
        observation.statements = observation
            .statements
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        observation.fetched_rows = observation
            .fetched_rows
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        observation.authentication_passes = observation
            .authentication_passes
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        observation.objects = observation
            .objects
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        observation.bytes = observation
            .bytes
            .checked_add(expected_length)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        self.observation.set(observation);
        let value = callback(expected_kind, bytes)?;
        if role_decode {
            self.note_role_decode()?;
        }
        Ok(value)
    }

    fn fetch(&self, id: ObjectId) -> Result<Vec<u8>, layerfs_core::CoreError> {
        self.with_record(id, false, |_, bytes| Ok(bytes.to_vec()))
    }

    fn note_role_decode(&self) -> Result<(), layerfs_core::CoreError> {
        let mut observation = self.observation.get();
        observation.role_decode_passes = observation
            .role_decode_passes
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        self.observation.set(observation);
        Ok(())
    }
}

impl ObjectRead for ConnectionStore<'_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, layerfs_core::CoreError> {
        self.fetch(id)
    }

    fn with_authenticated_canonical<T, F>(
        &self,
        id: ObjectId,
        callback: F,
    ) -> Result<T, layerfs_core::CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, layerfs_core::CoreError>,
    {
        self.with_record(id, true, |_, bytes| callback(bytes))
    }
}

fn drain(
    connection: &Connection,
    work: &DiskTable,
    payload_lengths: &DiskNamespace<'_>,
) -> EngineResult<VerificationObservation> {
    let object_store = ConnectionStore::new(connection);
    while let Some((key, _)) = work.pop_pending()? {
        let (id, role, root) = decode_key(&key)?;
        let mut visit_error = None;
        let decoded = object_store.with_record(id, true, |kind, canonical| {
            if kind != ObjectKind::Bytes {
                return Err(layerfs_core::CoreError::InvalidRecord(
                    "closure object metadata",
                ));
            }
            if let Err(error) = visit(work, payload_lengths, id, role, root, canonical) {
                visit_error = Some(error);
                return Err(layerfs_core::CoreError::Io);
            }
            Ok(())
        });
        if let Some(error) = visit_error {
            return Err(error);
        }
        decoded.map_err(|cause| match cause {
            layerfs_core::CoreError::MissingObject => EngineError::MissingObject(id),
            cause => EngineError::MalformedObject { id, cause },
        })?;
    }
    Ok(object_store.observation())
}

fn visit(
    work: &DiskTable,
    payload_lengths: &DiskNamespace<'_>,
    id: ObjectId,
    role: Role,
    root: bool,
    canonical: &[u8],
) -> EngineResult<()> {
    match role {
        Role::Namespace => {
            let value = decode_namespace_root(canonical)?;
            enqueue(work, value.inode_table_root, Role::InodeTable, true)?;
        }
        Role::InodeTable => match decode_inode_table_node(canonical)? {
            InodeTableNodeV1::Leaf(entries) => {
                for (_, record) in entries {
                    enqueue(work, record, Role::InodeRecord, true)?;
                }
            }
            InodeTableNodeV1::Branch { children, .. } => {
                for (_, child) in children {
                    enqueue(work, child, Role::InodeTable, false)?;
                }
            }
        },
        Role::InodeRecord => {
            let value = decode_inode_record(canonical)?;
            enqueue(work, value.metadata_root, Role::MetadataNode, true)?;
            enqueue(
                work,
                value.content_root,
                match value.kind {
                    InodeKind::RegularFile => Role::FileState,
                    InodeKind::Directory => Role::DirectoryState,
                    InodeKind::Symlink => Role::Symlink,
                },
                true,
            )?;
        }
        Role::DirectoryState => {
            let value = decode_directory_state(canonical)?;
            enqueue(work, value.mapping_root, Role::DirectoryNode, true)?;
        }
        Role::DirectoryNode => match decode_directory_node(canonical)? {
            DirectoryNodeV1::Leaf { .. } => {}
            DirectoryNodeV1::Branch { children, .. } => {
                for (_, child) in children {
                    enqueue(work, child, Role::DirectoryNode, false)?;
                }
            }
        },
        Role::FileState => {
            let value = decode_file_state(canonical)?;
            enqueue(work, value.mapping_root, Role::ExtentNode, true)?;
        }
        Role::ExtentNode => match decode_node_with_context(canonical, root)? {
            ExtentNodeV3::Leaf { extents, .. } => {
                for extent in extents {
                    enqueue(work, extent.payload_object_id, Role::Payload, true)?;
                }
            }
            ExtentNodeV3::Branch { children, .. } => {
                for child in children {
                    enqueue(work, child.child_object_id, Role::ExtentNode, false)?;
                }
            }
        },
        Role::MetadataNode => match decode_metadata_node(canonical)? {
            MetadataNodeV1::Leaf { entries, .. } => {
                for entry in entries {
                    enqueue(work, entry.value_file_root, Role::FileState, true)?;
                }
            }
            MetadataNodeV1::Branch { children, .. } => {
                for (_, child) in children {
                    enqueue(work, child, Role::MetadataNode, false)?;
                }
            }
        },
        Role::Symlink => {
            decode_symlink(canonical)?;
        }
        Role::Payload => {
            let payload = decode_bytes_object(canonical)?;
            if payload.len() > layerfs_core::cdc::MAXIMUM_CHUNK_BYTES {
                return Err(EngineError::Core(
                    layerfs_core::CoreError::ChunkLengthMismatch,
                ));
            }
            let length = u32::try_from(payload.len())
                .map_err(|_| EngineError::CounterOverflow)?
                .to_be_bytes();
            payload_lengths.put(id.as_bytes(), &length)?;
        }
    }
    Ok(())
}

fn enqueue(work: &DiskTable, id: ObjectId, role: Role, root: bool) -> EngineResult<()> {
    let mut key = [0_u8; 34];
    key[..32].copy_from_slice(id.as_bytes());
    key[32] = role as u8;
    key[33] = u8::from(root);
    work.enqueue_once(&key, &[])
}
fn decode_key(bytes: &[u8]) -> EngineResult<(ObjectId, Role, bool)> {
    if bytes.len() != 34 {
        return Err(EngineError::InvalidRecord("closure key"));
    }
    let role = match bytes[32] {
        1 => Role::Namespace,
        2 => Role::InodeTable,
        3 => Role::InodeRecord,
        4 => Role::DirectoryState,
        5 => Role::DirectoryNode,
        6 => Role::FileState,
        7 => Role::ExtentNode,
        8 => Role::MetadataNode,
        9 => Role::Symlink,
        10 => Role::Payload,
        _ => return Err(EngineError::InvalidRecord("closure role")),
    };
    Ok((ObjectId::from_bytes(&bytes[..32])?, role, bytes[33] != 0))
}
