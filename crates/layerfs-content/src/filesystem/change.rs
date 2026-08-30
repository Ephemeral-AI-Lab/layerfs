use super::{InodeMutation, LogicalCounters};
use crate::file::rope::build;
use crate::filesystem;
use crate::object::access::ObjectStore;
use crate::tree::inode::{InodeId, InodeKind, InodeRecordV1};
use crate::tree::metadata::{
    build_metadata_tree, replace_metadata_entry, MetadataCounters, MetadataEntryV1, MetadataKey,
    PortableMetadataV1,
};
use crate::{CanonicalPath, CoreError, CoreResult, ObjectId};
use std::io::{Cursor, Read};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentChange {
    Write {
        path: String,
        bytes: Vec<u8>,
        mode: u32,
    },
    Splice {
        path: String,
        start: u64,
        delete_len: u64,
        replacement: Vec<u8>,
    },
    Mkdir {
        path: String,
        mode: u32,
    },
    Symlink {
        path: String,
        target: Vec<u8>,
    },
    HardLink {
        source: String,
        target: String,
    },
    Rename {
        source: String,
        target: String,
    },
    Remove {
        path: String,
    },
    SetMode {
        path: String,
        mode: u32,
    },
    SetMtime {
        path: String,
        seconds: i64,
        nanoseconds: u32,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ApplyCounters {
    pub cdc_bytes_scanned: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppliedRoot {
    pub root_id: ObjectId,
    pub counters: ApplyCounters,
}

pub fn apply_changes<S: ObjectStore>(
    store: &mut S,
    base_root: ObjectId,
    changes: &[ContentChange],
    seed: [u8; 32],
) -> CoreResult<AppliedRoot> {
    let mut root = base_root;
    let mut cdc_bytes_scanned = 0_u64;
    for change in changes {
        let candidate = apply_change(store, root, change, seed)?;
        cdc_bytes_scanned = cdc_bytes_scanned
            .checked_add(candidate.counters().rope.cdc_bytes_scanned)
            .ok_or(CoreError::LengthOverflow)?;
        root = candidate.root();
    }
    Ok(AppliedRoot {
        root_id: root,
        counters: ApplyCounters { cdc_bytes_scanned },
    })
}

pub fn write_file<S: ObjectStore, R: Read>(
    store: &mut S,
    root: ObjectId,
    path: &CanonicalPath,
    bytes: R,
    mode: u32,
    seed: [u8; 32],
) -> CoreResult<filesystem::CandidateRoot> {
    let inode = allocated_inode(seed, path.as_str());
    let candidate = filesystem::replace_file(store, root, path, bytes, |store| {
        Ok((inode, metadata(store, InodeKind::RegularFile, mode)?))
    })?;
    set_mode(store, candidate.root(), path, mode)
}

fn apply_change<S: ObjectStore>(
    store: &mut S,
    root: ObjectId,
    change: &ContentChange,
    seed: [u8; 32],
) -> CoreResult<filesystem::CandidateRoot> {
    Ok(match change {
        ContentChange::Write {
            path: value,
            bytes,
            mode,
        } => {
            let path = path(value)?;
            let inode = allocated_inode(seed, value);
            let candidate =
                filesystem::replace_file(store, root, &path, Cursor::new(bytes), |store| {
                    Ok((inode, metadata(store, InodeKind::RegularFile, *mode)?))
                })?;
            set_mode(store, candidate.root(), &path, *mode)?
        }
        ContentChange::Splice {
            path: value,
            start,
            delete_len,
            replacement,
        } => filesystem::replace_range(
            store,
            root,
            &path(value)?,
            *start,
            *delete_len,
            Cursor::new(replacement),
        )?,
        ContentChange::Mkdir { path: value, mode } => {
            let metadata = metadata(store, InodeKind::Directory, *mode)?;
            filesystem::create_directory(
                store,
                root,
                &path(value)?,
                allocated_inode(seed, value),
                metadata,
            )?
        }
        ContentChange::Symlink {
            path: value,
            target,
        } => {
            let metadata = metadata(store, InodeKind::Symlink, 0o777)?;
            filesystem::create_symlink(
                store,
                root,
                &path(value)?,
                allocated_inode(seed, value),
                target.clone(),
                metadata,
            )?
        }
        ContentChange::HardLink { source, target } => {
            filesystem::hard_link(store, root, &path(source)?, &path(target)?)?
        }
        ContentChange::Rename { source, target } => {
            let mut counters = LogicalCounters::default();
            let (left, _) = filesystem::resolve_parent(store, root, &path(source)?, &mut counters)?;
            let (right, _) =
                filesystem::resolve_parent(store, root, &path(target)?, &mut counters)?;
            filesystem::rename(
                store,
                root,
                &path(source)?,
                &path(target)?,
                left.record.metadata_root,
                right.record.metadata_root,
            )?
        }
        ContentChange::Remove { path: value } => {
            filesystem::remove_path(store, root, &path(value)?)?
        }
        ContentChange::SetMode { path: value, mode } => {
            set_mode(store, root, &path(value)?, *mode)?
        }
        ContentChange::SetMtime {
            path: value,
            seconds,
            nanoseconds,
        } => set_mtime(store, root, &path(value)?, *seconds, *nanoseconds)?,
    })
}

pub fn set_mtime<S: ObjectStore>(
    store: &mut S,
    root: ObjectId,
    path: &CanonicalPath,
    seconds: i64,
    nanoseconds: u32,
) -> CoreResult<filesystem::CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let resolved = filesystem::resolve(store, root, path, &mut counters)?;
    let portable = PortableMetadataV1 {
        permission_mode: 0,
        mtime_seconds: seconds,
        mtime_nanoseconds: nanoseconds,
    };
    let (mtime_root, _) = build(store, Cursor::new(portable.mtime_bytes()?))?;
    let metadata_root = replace_metadata_entry(
        store,
        resolved.record.metadata_root,
        MetadataEntryV1 {
            key: MetadataKey::new("portable".to_owned(), b"mtime".to_vec())?,
            value_file_root: mtime_root.0,
        },
        &mut MetadataCounters::default(),
    )?;
    filesystem::apply_inode_mutations(
        store,
        root,
        [InodeMutation::Upsert {
            inode: resolved.inode,
            record: InodeRecordV1 {
                metadata_root,
                ..resolved.record
            },
        }],
    )
}

pub fn set_mode<S: ObjectStore>(
    store: &mut S,
    root: ObjectId,
    path: &CanonicalPath,
    mode: u32,
) -> CoreResult<filesystem::CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let resolved = filesystem::resolve(store, root, path, &mut counters)?;
    let portable = PortableMetadataV1 {
        permission_mode: mode
            & if resolved.record.kind == InodeKind::Directory {
                0o1777
            } else {
                0o777
            },
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
    };
    let (mode_root, _) = build(
        store,
        Cursor::new(portable.mode_bytes(resolved.record.kind)?),
    )?;
    let metadata_root = replace_metadata_entry(
        store,
        resolved.record.metadata_root,
        MetadataEntryV1 {
            key: MetadataKey::new("portable".to_owned(), b"mode".to_vec())?,
            value_file_root: mode_root.0,
        },
        &mut MetadataCounters::default(),
    )?;
    filesystem::apply_inode_mutations(
        store,
        root,
        [InodeMutation::Upsert {
            inode: resolved.inode,
            record: InodeRecordV1 {
                metadata_root,
                ..resolved.record
            },
        }],
    )
}

pub(super) fn metadata<S: ObjectStore>(
    store: &mut S,
    kind: InodeKind,
    mode: u32,
) -> CoreResult<ObjectId> {
    let portable = PortableMetadataV1 {
        permission_mode: mode
            & if kind == InodeKind::Directory {
                0o1777
            } else {
                0o777
            },
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
    };
    let (mode, _) = build(store, Cursor::new(portable.mode_bytes(kind)?))?;
    let (mtime, _) = build(store, Cursor::new(portable.mtime_bytes()?))?;
    build_metadata_tree(
        store,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".to_owned(), b"mode".to_vec())?,
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".to_owned(), b"mtime".to_vec())?,
                value_file_root: mtime.0,
            },
        ],
    )
}

fn path(value: &str) -> CoreResult<CanonicalPath> {
    let path = CanonicalPath::new(value.strip_prefix('/').unwrap_or(value))?;
    if path.is_root() {
        Err(CoreError::RootMutation)
    } else {
        Ok(path)
    }
}

fn allocated_inode(seed: [u8; 32], path: &str) -> InodeId {
    let mut bytes = Vec::with_capacity(seed.len() + path.len());
    bytes.extend_from_slice(&seed);
    bytes.extend_from_slice(path.as_bytes());
    InodeId::allocate(ObjectId::for_bytes(&bytes).to_bytes(), 0)
}
