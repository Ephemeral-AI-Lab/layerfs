//! Receive one prepared stream into a charged row spool, then hand it to C1.
//!
//! The workspace declares the exact rows of an update and sends them as an
//! ordered body; this is where they are read. Each row is validated, given the
//! canonical work it needs - the role check on a saved file's roots, the portable
//! patch of a maintained directory, the first metadata of a declared one - and
//! then written into a [`RowSpool`] the service owns. The spool is what C1 reads,
//! so nothing between the transport and C1 holds a row per changed name: the
//! receive state holds one current name, and subject checking uses admitted waves.
//!
//! **The stream is validated once, and the spool is bounded by the declaration.**
//! `read_prepared_bindings` checks the row grammar, ordering and role counts
//! and the exact byte total as the body arrives; the spool charges every slot and
//! payload against the capacity its update declared, and `seal` refuses a spool
//! that did not receive every row. The relations that need the whole update - the
//! one the caller states by binding a serial it never declared, and the one a
//! declaration states by naming a directory row - are checked here, after the
//! body is complete, over the spool rather than over a resident copy of it.

use super::filesystem::PreparedUpdate;
use super::metadata::{build_metadata, patch_portable};
use super::validation::validate_inode_role;
use crate::service::{error::content, read::content::id};
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::rows::{BindingRows, RowSource, RowSpool, SpoolDeclaration};
use layerfs_content::filesystem::{FilesystemRead, FilesystemResources, InodeUpdate, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentResult, FilesystemObjects};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{path::PathBuf, time::Instant};

/// Bytes beyond the declared body one receive spool may charge: its slot table.
const SPOOL_SLOT_SLACK: u64 = 1024 * 1024;

/// One received update: the spool its rows live in.
pub(crate) struct Received {
    /// The charged spool C1 reads.
    pub(crate) rows: RowSpool,
}

/// Pure predictable receive admission, shared with the pre-scratch/Save caller.
pub(crate) fn planned_spool_admission(
    changes: &PreparedChanges,
) -> Result<(SpoolDeclaration, u64), Failure> {
    changes.check()?;
    let limit = FilesystemResources::default().ordering_bytes / 1024;
    if [
        changes.totals.directories,
        changes.totals.names,
        changes.totals.identities,
        changes.totals.fresh,
    ]
    .into_iter()
    .any(|count| count > limit)
    {
        return Err(Code::Capacity.into());
    }
    let declaration = SpoolDeclaration {
        directories: usize::try_from(changes.totals.directories).map_err(|_| Code::Capacity)?,
        inodes: usize::try_from(changes.totals.identities).map_err(|_| Code::Capacity)?,
        fresh: usize::try_from(changes.totals.fresh).map_err(|_| Code::Capacity)?,
        bindings: changes.totals.names,
        wire_name_bytes: changes.totals.name_bytes,
    };
    let capacity = changes
        .stream_bytes()?
        .checked_add(SPOOL_SLOT_SLACK)
        .ok_or(Code::Capacity)?;
    if declaration.required_bytes_upper().map_err(content)? > capacity {
        return Err(Code::Capacity.into());
    }
    Ok((declaration, capacity))
}

/// Reads one prepared stream into a charged spool.
///
/// The spool is created from the request's own declared totals, so a stream that
/// lies about its row counts is refused by the spool rather than by a later
/// re-count, and the file it owns is removed when the update is done.
pub(crate) fn receive<'a>(
    objects: &mut FilesystemObjects<'a>,
    fs: &mut FilesystemRead<'a>,
    provider: &dyn AuthenticatedObjects,
    scope: &TimingScope<'_, Active>,
    update: PreparedUpdate<'_>,
    deadline: Instant,
) -> Result<Received, Failure> {
    // This preparation owns the source issuer already bound before native/Save
    // effects. Consuming it gives the receive spool that exact authority.
    let mut spool =
        RowSpool::create_prepared(spool_path(), update.preparation.ok_or(Code::Ownership)?)
            .map_err(content)?;
    let received = {
        let mut sink = SpoolSink {
            objects,
            fs,
            provider,
            scope,
            spool: &mut spool,
            deadline,
        };
        read_prepared_bindings(
            &update.changes.totals,
            update.changes.root_serial,
            update.body,
            &mut sink,
        )
    };
    if let Err(error) = received {
        let _ = spool.cleanup();
        return Err(error);
    }
    if let Err(error) = spool.seal() {
        let _ = spool.cleanup();
        return Err(content(error));
    }
    Ok(Received { rows: spool })
}

/// One row's canonical work, then its slot in the spool.
struct SpoolSink<'a, 'b> {
    objects: &'a mut FilesystemObjects<'b>,
    fs: &'a mut FilesystemRead<'b>,
    provider: &'a dyn AuthenticatedObjects,
    scope: &'a TimingScope<'a, Active>,
    spool: &'a mut RowSpool,
    deadline: Instant,
}

impl SpoolSink<'_, '_> {
    fn check_deadline(&self) -> Result<(), Failure> {
        if Instant::now() >= self.deadline {
            Err(Code::Deadline.into())
        } else {
            Ok(())
        }
    }
}

impl PreparedBindingSink for SpoolSink<'_, '_> {
    fn begin_directory(&mut self, parent: u64, bindings: u32) -> Result<(), Failure> {
        self.check_deadline()?;
        self.spool
            .begin_directory(parent, bindings)
            .map_err(content)
    }

    fn binding(&mut self, name: &[u8], child: Option<u64>) -> Result<(), Failure> {
        self.check_deadline()?;
        let name = PathName::from_bytes(name).map_err(content)?;
        self.spool.push_binding(&name, child).map_err(content)
    }

    fn end_directory(&mut self, row: PreparedDirectoryCompletion) -> Result<(), Failure> {
        self.check_deadline()?;
        self.spool
            .end_directory(row.parent, row.bindings, row.wire_name_bytes)
            .map_err(content)
    }

    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure> {
        self.check_deadline()?;
        match row {
            PreparedIdentity::Rooted {
                serial,
                kind,
                content: content_root,
                metadata: metadata_root,
                fresh,
            } => {
                let kind = InodeKind::from_code(kind).map_err(content)?;
                validate_inode_role(
                    self.provider,
                    kind,
                    id(&content_root),
                    id(&metadata_root),
                    self.scope,
                )?;
                let references = if fresh {
                    // C1 derives the count from the bindings it retains.
                    0
                } else {
                    let old = self.fs.lookup_inodes(&[serial]).map_err(content)?[0]
                        .ok_or(Code::InvalidInput)?;
                    if old.kind != kind
                        || (kind == InodeKind::Directory && old.content_root != id(&content_root))
                    {
                        return Err(Code::InvalidInput.into());
                    }
                    self.provider
                        .read_canonical(id(&content_root))
                        .map_err(content)?;
                    self.provider
                        .read_canonical(id(&metadata_root))
                        .map_err(content)?;
                    old.namespace_ref_count
                };
                self.spool
                    .push_inode(&InodeUpdate {
                        serial,
                        value: InodeValue {
                            kind,
                            namespace_ref_count: references,
                            content_root: id(&content_root),
                            metadata_root: id(&metadata_root),
                        },
                    })
                    .map_err(content)?;
                if fresh {
                    self.spool.push_serial(serial).map_err(content)?;
                }
                Ok(())
            }
            PreparedIdentity::DirectoryPatch {
                serial,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => {
                let old = self.fs.lookup_inodes(&[serial]).map_err(content)?[0]
                    .ok_or(Code::InvalidInput)?;
                if old.kind != InodeKind::Directory {
                    return Err(Code::InvalidInput.into());
                }
                let metadata_root = patch_portable(
                    self.objects,
                    old.metadata_root,
                    InodeKind::Directory,
                    PortableMetadata {
                        mode,
                        mtime_seconds,
                        mtime_nanoseconds,
                    },
                )
                .map_err(content)?;
                self.spool
                    .push_inode(&InodeUpdate {
                        serial,
                        value: InodeValue {
                            metadata_root,
                            ..old
                        },
                    })
                    .map_err(content)
            }
            PreparedIdentity::DirectoryDeclaration {
                serial,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => {
                // Every declaration owns a binding record, empty ones included:
                // the prepared profile reads a name solely from that record, so a
                // declared serial with no directory row would describe an inode
                // nothing can reach. The directory section precedes this one, so
                // the record is already in the spool.
                if !self.spool.completed_directory(serial).map_err(content)? {
                    return Err(Code::InvalidInput.into());
                }
                let metadata_root = build_metadata(
                    self.objects,
                    InodeKind::Directory,
                    PortableMetadata {
                        mode,
                        mtime_seconds,
                        mtime_nanoseconds,
                    },
                )?;
                self.spool
                    .push_inode(&InodeUpdate {
                        serial,
                        value: InodeValue {
                            kind: InodeKind::Directory,
                            namespace_ref_count: 0,
                            // Every declaration has a binding record, including
                            // empty ones. C1 replaces this placeholder with its
                            // newly built directory root.
                            content_root: metadata_root,
                            metadata_root,
                        },
                    })
                    .map_err(content)?;
                self.spool.push_serial(serial).map_err(content)
            }
        }
    }
}

/// True when one serial the base does not hold is one the caller declared.
///
/// Every parent, binding and typed value of an update is demanded from the base
/// in waves bounded by the operation's own declared read batch, and each demand
/// is answered by the spool's own declaration run. This is the allocator
/// precondition the prepared profile has always stated - a serial is either
/// already canonical or created by this caller - checked without a resident list
/// of the serials that state it.
pub(crate) fn check_subjects(
    fs: &mut FilesystemRead<'_>,
    spool: &RowSpool,
    batch: usize,
) -> ContentResult<()> {
    let batch = batch.max(1);
    let mut wave: Vec<(u64, bool)> = Vec::with_capacity(batch);
    let mut headers = spool.directory_headers()?;
    while let Some(header) = headers.next_header()? {
        wave.push((header.parent(), spool.is_new(header.parent())?));
        if wave.len() == batch {
            check_wave(fs, &wave)?;
            wave.clear();
        }
        let mut bindings = spool.bindings(&header)?;
        while let Some((_, child)) = bindings.next_binding()? {
            if let Some(serial) = child {
                wave.push((serial, spool.is_new(serial)?));
                if wave.len() == batch {
                    check_wave(fs, &wave)?;
                    wave.clear();
                }
            }
        }
        if !bindings.finish()?.matches(&header) {
            return Err(layerfs_content::ContentError::InvalidRecord(
                "prepared binding completion",
            ));
        }
    }
    if !wave.is_empty() {
        check_wave(fs, &wave)?;
    }
    drop(headers);
    let mut inodes = spool.inodes()?;
    loop {
        wave.clear();
        while wave.len() < batch.max(1) {
            let Some(row) = inodes.next_row()? else {
                break;
            };
            wave.push((row.serial, spool.is_new(row.serial)?));
        }
        if wave.is_empty() {
            break;
        }
        check_wave(fs, &wave)?;
    }
    Ok(())
}

/// One wave of demands: a serial the base holds is canonical, and one it does
/// not hold must be the caller's own declaration.
fn check_wave(fs: &mut FilesystemRead<'_>, wave: &[(u64, bool)]) -> ContentResult<()> {
    let serials = wave.iter().map(|(serial, _)| *serial).collect::<Vec<_>>();
    let found = fs.lookup_inodes(&serials)?;
    for ((_, declared), record) in wave.iter().zip(found) {
        if record.is_none() != *declared {
            return Err(layerfs_content::ContentError::InvalidRecord(
                "prepared serial",
            ));
        }
    }
    Ok(())
}

/// A fresh path for one receive spool.
fn spool_path() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("layerfs-prepared-{}-{unique}", std::process::id()))
}
