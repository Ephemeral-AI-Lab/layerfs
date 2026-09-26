//! Receive one prepared stream into a charged row spool, then hand it to C1.
//!
//! The workspace declares the exact rows of an update and sends them as an
//! ordered body; this is where they are read. Each row is validated, given the
//! canonical work it needs - the role check on a saved file's roots, the portable
//! patch of a maintained directory, the first metadata of a declared one - and
//! then written into a [`RowSpool`] the service owns. The spool is what C1 reads,
//! so nothing between the transport and C1 holds a row per changed name: the
//! resident cost of receiving an update follows the widest row in it.
//!
//! **The stream is validated once, and the spool is bounded by the declaration.**
//! `read_prepared_stream` checks the row grammar, the ordering, the role counts
//! and the exact byte total as the body arrives; the spool charges every slot and
//! payload against the capacity its update declared, and `seal` refuses a spool
//! that did not receive every row. The relations that need the whole update - the
//! one the caller states by binding a serial it never declared, and the one a
//! declaration states by naming a directory row - are checked here, after the
//! body is complete, over the spool rather than over a resident copy of it.

use super::metadata::{build_metadata, patch_portable};
use super::validation::validate_inode_role;
use crate::service::{error::content, read::content::id};
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::rows::{RowSource, RowSpool};
use layerfs_content::filesystem::{DirectoryUpdate, FilesystemRead, InodeUpdate, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentResult, FilesystemObjects};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{io::Read, path::PathBuf, time::Instant};

/// Bytes beyond the declared body one receive spool may charge: its slot table.
const SPOOL_SLOT_SLACK: u64 = 1024 * 1024;

/// One received update: the spool its rows live in.
pub(crate) struct Received {
    /// The charged spool C1 reads.
    pub(crate) rows: RowSpool,
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
    changes: &PreparedChanges,
    body: &mut dyn Read,
    deadline: Instant,
) -> Result<Received, Failure> {
    let bytes = changes.stream_bytes()?;
    let path = spool_path();
    let mut spool = RowSpool::create(
        path,
        usize::try_from(changes.totals.directories).map_err(|_| Code::Capacity)?,
        usize::try_from(changes.totals.identities).map_err(|_| Code::Capacity)?,
        usize::try_from(changes.totals.fresh).map_err(|_| Code::Capacity)?,
        bytes.checked_add(SPOOL_SLOT_SLACK).ok_or(Code::Capacity)?,
    )
    .map_err(content)?;
    let mut sink = SpoolSink {
        objects,
        fs,
        provider,
        scope,
        spool: &mut spool,
        deadline,
        identities: 0,
        fresh: 0,
    };
    let received = read_prepared_stream(&changes.totals, changes.root_serial, body, &mut sink);
    let declared = changes.totals;
    if let Err(error) = received {
        let _ = spool.cleanup();
        return Err(error);
    }
    let _ = declared;
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
    identities: u64,
    fresh: u64,
}

impl PreparedRowSink for SpoolSink<'_, '_> {
    fn directory(
        &mut self,
        parent: u64,
        changes: Vec<(Vec<u8>, Option<u64>)>,
    ) -> Result<(), Failure> {
        let changes = changes
            .into_iter()
            .map(|(name, serial)| Ok((PathName::from_bytes(&name).map_err(content)?, serial)))
            .collect::<Result<Vec<_>, Failure>>()?;
        self.spool
            .push_directory(&DirectoryUpdate { parent, changes })
            .map_err(content)
    }

    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure> {
        if Instant::now() >= self.deadline {
            return Err(Code::Deadline.into());
        }
        self.identities += 1;
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
                    self.fresh += 1;
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
                if self.spool.directory_for(serial).map_err(content)?.is_none() {
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
                self.fresh += 1;
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
    let mut wave: Vec<(u64, bool)> = Vec::with_capacity(batch.max(1));
    let mut rows = spool.directories()?;
    loop {
        wave.clear();
        while wave.len() < batch.max(1) {
            let Some(row) = rows.next_row()? else {
                break;
            };
            wave.push((row.parent, spool.is_new(row.parent)?));
            for (_, binding) in &row.changes {
                if let Some(serial) = binding {
                    if wave.len() == batch.max(1) {
                        break;
                    }
                    wave.push((*serial, spool.is_new(*serial)?));
                }
            }
        }
        if wave.is_empty() {
            break;
        }
        check_wave(fs, &wave)?;
    }
    drop(rows);
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
