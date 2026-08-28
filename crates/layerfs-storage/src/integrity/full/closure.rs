//! Legacy Full-store retained-closure verification.

use super::object::{merge_failed_observation, ConnectionStore, VerificationObservation};
use crate::scratch::{DiskNamespace, DiskTable};
use crate::{EngineError, EngineResult};
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
use layerfs_core::{decode_bytes_object, CoreError, ObjectId, ObjectKind};
use rusqlite::Connection;
use std::{cell::Cell, path::Path};

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Role {
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

struct PayloadSummaryStore<'a, 'connection, 'scratch> {
    store: &'a ConnectionStore<'connection>,
    payload_lengths: &'a DiskNamespace<'scratch>,
}

impl ObjectRead for PayloadSummaryStore<'_, '_, '_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        self.store.fetch(id)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> Result<T, CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, CoreError>,
    {
        self.store.with_authenticated_canonical(id, callback)
    }

    fn get_authenticated_payload_lengths_batch<F>(
        &self,
        ids: &[ObjectId],
        mut callback: F,
    ) -> Result<(), CoreError>
    where
        F: FnMut(ObjectId, u32) -> Result<(), CoreError>,
    {
        let keys = ids
            .iter()
            .map(|id| id.as_bytes().as_slice())
            .collect::<Vec<_>>();
        self.payload_lengths
            .get_ordered_batch(&keys, |ordinal, bytes| {
                let id = ids[ordinal];
                let bytes = bytes.ok_or(EngineError::Core(CoreError::MissingObject))?;
                let length =
                    u32::from_be_bytes(bytes.try_into().map_err(|_| {
                        EngineError::Core(CoreError::InvalidRecord("payload summary"))
                    })?);
                callback(id, length).map_err(EngineError::Core)
            })
            .map_err(|error| match error {
                EngineError::Core(error) => error,
                EngineError::CounterOverflow => CoreError::LengthOverflow,
                EngineError::InvalidRecord(name) => CoreError::InvalidRecord(name),
                _ => CoreError::Io,
            })
    }
}

pub(crate) fn verify_root(
    connection: &Connection,
    store: &Path,
    store_id: [u8; 32],
    root: ObjectId,
    statements: &Cell<u64>,
    failed: &Cell<VerificationObservation>,
) -> EngineResult<VerificationObservation> {
    let work = DiskTable::create_near_with_store_id(store, "publication-closure", store_id)?;
    let graph = match DiskTable::create_near_with_store_id(store, "publication-graph", store_id) {
        Ok(graph) => graph,
        Err(error) => {
            let mut observation = VerificationObservation::default();
            if let Ok(scratch) = work.finish() {
                let _ = observation.add_scratch(scratch);
            }
            merge_failed_observation(failed, observation);
            return Err(error);
        }
    };
    let result = (|| {
        let records = graph.namespace(b"records")?;
        let state = graph.namespace(b"state")?;
        let payload_lengths = graph.namespace(b"payload-lengths")?;
        enqueue(&work, root, Role::Namespace, true)?;
        let mut observation = drain(connection, &work, &payload_lengths, statements, failed)?;
        records.clear()?;
        state.clear()?;
        observation.merge(validate_namespace_graph_disk(
            connection,
            &records,
            &state,
            &payload_lengths,
            root,
            statements,
            failed,
        )?)?;
        Ok(observation)
    })();
    match result {
        Ok(mut observation) => {
            observation.add_scratch(work.finish()?)?;
            observation.add_scratch(graph.finish()?)?;
            observation.statements = statements.get();
            Ok(observation)
        }
        Err(error) => {
            let mut observation = VerificationObservation::default();
            if let Ok(scratch) = work.finish() {
                let _ = observation.add_scratch(scratch);
            }
            if let Ok(scratch) = graph.finish() {
                let _ = observation.add_scratch(scratch);
            }
            merge_failed_observation(failed, observation);
            Err(error)
        }
    }
}

const RECORD_KEY: u8 = 0x40;
const SEEN_KEY: u8 = 0x41;
const REF_KEY: u8 = 0x42;
const QUEUE_KEY: u8 = 0x43;

pub(super) fn validate_namespace_graph_disk(
    connection: &Connection,
    records: &DiskNamespace<'_>,
    state: &DiskNamespace<'_>,
    payload_lengths: &DiskNamespace<'_>,
    root: ObjectId,
    statements: &Cell<u64>,
    failed: &Cell<VerificationObservation>,
) -> EngineResult<VerificationObservation> {
    let object_store = ConnectionStore::new(connection, statements, failed);
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
                    return Err(CoreError::Io);
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
                            return Err(CoreError::Io);
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
            return Err(EngineError::Core(CoreError::InvalidRecord(
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
            return Err(EngineError::Core(CoreError::InvalidRecord(
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
        .ok_or(EngineError::Core(CoreError::InvalidRecord(
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
        return Err(EngineError::Core(CoreError::InvalidRecord(
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
            return Err(EngineError::Core(CoreError::InvalidRecord(
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

pub(super) fn drain(
    connection: &Connection,
    work: &DiskTable,
    payload_lengths: &DiskNamespace<'_>,
    statements: &Cell<u64>,
    failed: &Cell<VerificationObservation>,
) -> EngineResult<VerificationObservation> {
    let object_store = ConnectionStore::new(connection, statements, failed);
    while let Some((key, _)) = work.pop_pending()? {
        let (id, role, root) = decode_key(&key)?;
        let mut visit_error = None;
        let decoded = object_store.with_record(id, true, |kind, canonical| {
            if kind != ObjectKind::Bytes {
                return Err(CoreError::InvalidRecord("closure object metadata"));
            }
            if let Err(error) = visit(work, payload_lengths, id, role, root, canonical) {
                visit_error = Some(error);
                return Err(CoreError::Io);
            }
            Ok(())
        });
        if let Some(error) = visit_error {
            return Err(error);
        }
        decoded.map_err(|cause| match cause {
            CoreError::MissingObject => EngineError::MissingObject(id),
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
                return Err(EngineError::Core(CoreError::ChunkLengthMismatch));
            }
            let length = u32::try_from(payload.len())
                .map_err(|_| EngineError::CounterOverflow)?
                .to_be_bytes();
            payload_lengths.put(id.as_bytes(), &length)?;
        }
    }
    Ok(())
}

pub(super) fn enqueue(work: &DiskTable, id: ObjectId, role: Role, root: bool) -> EngineResult<()> {
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
