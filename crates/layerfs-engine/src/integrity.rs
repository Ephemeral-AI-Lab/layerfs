#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IntegrityMode {
    #[default]
    Verified,
    TrustedLocalDev,
}

use crate::refs::validate_ref_name;
use crate::scratch::DiskTable;
use crate::{map_sqlite_error, EngineError, EngineResult};
use layerfs_core::content::extent::ExtentNodeV3;
use layerfs_core::content::extent_codec::{decode_file_state, decode_node_with_context};
use layerfs_core::content::rope::ObjectRead;
use layerfs_core::inode::{
    visit_inode_table_entries, InodeId, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::validate_inode_record;
use layerfs_core::namespace_codec::{
    decode_directory_node, decode_directory_state, decode_inode_record, decode_inode_table_node,
    decode_metadata_node, decode_namespace_root, decode_symlink, DirectoryNodeV1, InodeTableNodeV1,
    MetadataNodeV1,
};
use layerfs_core::{decode_bytes_object, validate_identity, ObjectId, ObjectKind};
use rusqlite::{params, Connection, OptionalExtension};
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

pub(crate) fn verify_retained_union(connection: &Connection, store: &Path) -> EngineResult<()> {
    verify_retained_union_observed(connection, store).map(drop)
}

pub(crate) fn verify_retained_union_observed(
    connection: &Connection,
    store: &Path,
) -> EngineResult<u64> {
    let retained = retained_union(connection, store)?;
    Ok(retained.peak_bytes)
}

pub(crate) fn verify_root(
    connection: &Connection,
    store: &Path,
    root: ObjectId,
) -> EngineResult<()> {
    let work = DiskTable::create_near(store, "publication-closure")?;
    enqueue(&work, root, Role::Namespace, true)?;
    drain(connection, &work)?;
    validate_namespace_graph_disk(connection, store, root).map(drop)
}

pub(crate) struct RetainedUnion {
    pub(crate) work: DiskTable,
    pub(crate) peak_bytes: u64,
}

pub(crate) fn retained_union(connection: &Connection, store: &Path) -> EngineResult<RetainedUnion> {
    let work = DiskTable::create_near(store, "closure")?;
    let mut peak_bytes = work.storage_bytes()?;
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
        if !retained {
            return Err(EngineError::MissingRoot(root));
        }
        enqueue(&work, root, Role::Namespace, true)?;
        drain(connection, &work)?;
        let graph = validate_namespace_graph_disk(connection, store, root)?;
        peak_bytes = peak_bytes.max(work.storage_bytes()?.saturating_add(graph));
    }
    drop(statement);
    let mut statement = connection
        .prepare("SELECT root_id FROM layerfs_retained_roots ORDER BY root_id")
        .map_err(map_sqlite_error)?;
    let roots = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(0))
        .map_err(map_sqlite_error)?;
    for root in roots {
        let root = ObjectId::from_bytes(&root.map_err(map_sqlite_error)?)?;
        enqueue(&work, root, Role::Namespace, true)?;
        drain(connection, &work)?;
        let graph = validate_namespace_graph_disk(connection, store, root)?;
        peak_bytes = peak_bytes.max(work.storage_bytes()?.saturating_add(graph));
    }
    Ok(RetainedUnion { work, peak_bytes })
}

const RECORD_KEY: u8 = 0x40;
const SEEN_KEY: u8 = 0x41;
const REF_KEY: u8 = 0x42;
const QUEUE_KEY: u8 = 0x43;

fn validate_namespace_graph_disk(
    connection: &Connection,
    store: &Path,
    root: ObjectId,
) -> EngineResult<u64> {
    let object_store = ConnectionStore(connection);
    let namespace = decode_namespace_root(&authenticated(&object_store, root)?)?;
    let records = DiskTable::create_near(store, "namespace-records")?;
    let state = DiskTable::create_near(store, "namespace-state")?;
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
        &records,
        &state,
        namespace.root_directory_inode,
        true,
    )?;
    while let Some((key, value)) = state.pop_pending()? {
        if key.first() != Some(&QUEUE_KEY) || key.len() != 33 || value.len() != 1 {
            return Err(EngineError::InvalidRecord("namespace graph queue"));
        }
        let inode = InodeId::from_slice(&key[1..])?;
        let record = graph_record(&object_store, &records, inode)?;
        let root_inode = value[0] != 0;
        let mut callback_error = None;
        let validated = validate_inode_record(&object_store, record, root_inode, |child| {
            if let Err(error) = observe_graph_inode(&object_store, &records, &state, child) {
                callback_error = Some(error);
                return Err(layerfs_core::CoreError::Io);
            }
            Ok(())
        });
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
        let record = graph_record(&object_store, &records, inode)?;
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
    records
        .storage_bytes()?
        .checked_add(state.storage_bytes()?)
        .ok_or(EngineError::CounterOverflow)
}

fn authenticated<S: ObjectRead>(store: &S, id: ObjectId) -> EngineResult<Vec<u8>> {
    let bytes = store.get(id).map_err(EngineError::Core)?;
    validate_identity(&bytes, id).map_err(|cause| EngineError::MalformedObject { id, cause })?;
    Ok(bytes)
}

fn graph_key(prefix: u8, inode: InodeId) -> [u8; 33] {
    let mut key = [0; 33];
    key[0] = prefix;
    key[1..].copy_from_slice(inode.as_bytes());
    key
}

fn graph_record(
    store: &ConnectionStore<'_>,
    records: &DiskTable,
    inode: InodeId,
) -> EngineResult<layerfs_core::inode::InodeRecordV1> {
    let id = records
        .get(&graph_key(RECORD_KEY, inode))?
        .ok_or(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
            "directory references missing inode",
        )))?;
    let id = ObjectId::from_bytes(&id)?;
    decode_inode_record(&authenticated(store, id)?).map_err(EngineError::Core)
}

fn enqueue_graph_inode(
    store: &ConnectionStore<'_>,
    records: &DiskTable,
    state: &DiskTable,
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
    records: &DiskTable,
    state: &DiskTable,
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

struct ConnectionStore<'a>(&'a Connection);

impl ObjectRead for ConnectionStore<'_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, layerfs_core::CoreError> {
        self.0
            .query_row(
                "SELECT canonical_bytes FROM layerfs_objects WHERE object_id = ?1",
                params![id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| layerfs_core::CoreError::Io)?
            .ok_or(layerfs_core::CoreError::MissingObject)
    }
}

fn drain(connection: &Connection, work: &DiskTable) -> EngineResult<()> {
    while let Some((key, _)) = work.pop_pending()? {
        let (id, role, root) = decode_key(&key)?;
        let row = connection.query_row("SELECT kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?1", params![id.as_bytes().as_slice()], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, Vec<u8>>(2)?))).optional().map_err(map_sqlite_error)?.ok_or(EngineError::MissingObject(id))?;
        let kind = ObjectKind::try_from(
            u8::try_from(row.0).map_err(|_| EngineError::InvalidRecord("closure object kind"))?,
        )?;
        if kind != ObjectKind::Bytes || usize::try_from(row.1).ok() != Some(row.2.len()) {
            return Err(EngineError::InvalidRecord("closure object metadata"));
        }
        validate_identity(&row.2, id)
            .map_err(|cause| EngineError::MalformedObject { id, cause })?;
        visit(work, role, root, &row.2)?;
    }
    Ok(())
}

fn visit(work: &DiskTable, role: Role, root: bool, canonical: &[u8]) -> EngineResult<()> {
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
            decode_bytes_object(canonical)?;
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
