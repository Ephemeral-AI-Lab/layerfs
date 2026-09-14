//! Authenticated transport dispatch into the same host authority used by SDK
//! mutations. Installed operations retain an exact bounded replay result.
use crate::host_overlay::HostOverlay;
use crate::overlay::{ReplayAdmission, ReplayWindow};
use crate::overlay_budget::{Charge, Operation as Admission, Permit};
use crate::snapshot::Snapshot;
use layerfs_fuse::host_wire::{self as wire, Operation};
use layerfs_fuse::{PortError, PortResult};
use layerfs_layerstack_store::{Result, StoreError};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReadLease {
    pub(crate) id: u64,
    pub(crate) node: layerfs_fuse::NodeId,
    pub(crate) size: u64,
}
struct LeasedFile {
    tree: crate::overlay_ranges::Tree,
    reader: layerfs_layerstack_store::SnapshotReader,
    _slot: ReadLeaseSlot,
}
struct ReadLeaseSlot {
    _permit: Permit,
    count: Arc<AtomicUsize>,
}
impl Drop for ReadLeaseSlot {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(crate) struct ReadLeaseReservation {
    id: u64,
    node: layerfs_fuse::NodeId,
    slot: ReadLeaseSlot,
}

pub(crate) struct HostOperations {
    pub(crate) host: Arc<HostOverlay>,
    replay: Mutex<Option<([u8; 16], ReplayWindow)>>,
    leases: Mutex<BTreeMap<u64, Arc<LeasedFile>>>,
    next_lease: AtomicU64,
    lease_count: Arc<AtomicUsize>,
}
impl HostOperations {
    pub(crate) fn new(host: Arc<HostOverlay>) -> Self {
        Self {
            host,
            replay: Mutex::new(None),
            leases: Mutex::new(BTreeMap::new()),
            next_lease: AtomicU64::new(1),
            lease_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Reserve the response reader before installing an SDK mutation. Failed
    /// preparation drops this ticket; activation cannot encounter a new quota.
    pub(crate) fn reserve_read_lease(
        &self,
        node: layerfs_fuse::NodeId,
    ) -> Result<ReadLeaseReservation> {
        if node.0 == 0 {
            return Err(StoreError::InvalidInput("leased inode identity"));
        }
        let permit = self.host.budget.enter(
            Admission::Request,
            Charge {
                memory_bytes: 512,
                ..Charge::default()
            },
        )?;
        let _held = self
            .leases
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot lease lock"))?;
        if self.lease_count.load(Ordering::Acquire) >= self.host.policy.overlay.max_readers {
            return Err(StoreError::StoreBusy);
        }
        let id = self
            .next_lease
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| StoreError::InvalidInput("snapshot lease exhausted"))?;
        self.lease_count.fetch_add(1, Ordering::AcqRel);
        Ok(ReadLeaseReservation {
            id,
            node,
            slot: ReadLeaseSlot {
                _permit: permit,
                count: self.lease_count.clone(),
            },
        })
    }

    /// Activate against the exact SDK candidate. Every fallible check happens
    /// before consuming the reservation; on an I/O error the caller still owns
    /// both the ticket and snapshot for retry of its unresolved coherence scope.
    pub(crate) fn install_read_lease(
        &self,
        reservation: &mut Option<ReadLeaseReservation>,
        snapshot: &Snapshot,
    ) -> Result<ReadLease> {
        let pending = reservation
            .as_ref()
            .ok_or(StoreError::InvalidInput("read lease already activated"))?;
        if !Arc::ptr_eq(&pending.slot.count, &self.lease_count) {
            return Err(StoreError::InvalidInput("foreign read lease reservation"));
        }
        if !self.host.index.owns(&snapshot.root.index) {
            return Err(StoreError::InvalidInput("foreign snapshot lease"));
        }
        let (record, root) = snapshot
            .inode_record(pending.node)?
            .ok_or(StoreError::NotFound("leased inode"))?;
        if record.attr.kind != layerfs_fuse::Kind::File {
            return Err(StoreError::InvalidInput("leased regular file"));
        }
        let tree = self.host.ranges.restore(root)?;
        if tree.len() != record.attr.size {
            return Err(StoreError::Integrity("leased file length"));
        }
        let mut leases = self
            .leases
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot lease lock"))?;
        let pending = reservation.take().unwrap();
        let result = ReadLease {
            id: pending.id,
            node: pending.node,
            size: record.attr.size,
        };
        leases.insert(
            pending.id,
            Arc::new(LeasedFile {
                tree,
                reader: self.host.reader.clone(),
                _slot: pending.slot,
            }),
        );
        Ok(result)
    }

    /// General owned-file readers use the same admission and activation path.
    /// This retains only the file tree, not unrelated Workspace state.
    pub(crate) fn retain_read_lease(
        &self,
        node: layerfs_fuse::NodeId,
        snapshot: Snapshot,
    ) -> Result<ReadLease> {
        let mut pending = Some(self.reserve_read_lease(node)?);
        self.install_read_lease(&mut pending, &snapshot)
    }

    pub(crate) fn release_read_lease(&self, id: u64) -> Result<()> {
        let previous = self
            .leases
            .lock()
            .map_err(|_| StoreError::Integrity("snapshot lease lock"))?
            .remove(&id);
        // In-flight reads retain their Arc after the registry entry disappears.
        drop(previous);
        Ok(())
    }
    pub(crate) fn request(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        let request = wire::decode_request(bytes).map_err(|_| PortError::Invalid)?;
        // BackingServer already owns/admitted the borrowed frame and response
        // transfer. This permit bounds per-Workspace outstanding requests.
        let _request = self
            .host
            .budget
            .enter(Admission::Request, Charge::default())
            .map_err(|_| PortError::Busy)?;
        let mut replay = self.replay.lock().map_err(|_| PortError::Io)?;
        if replay.is_none() {
            let policy = self.host.policy.overlay;
            *replay = Some((
                request.session,
                ReplayWindow::new(
                    request.session,
                    policy.max_replay_entries,
                    policy.max_replay_bytes as usize,
                )
                .map_err(crate::projection::storage_port_error)?,
            ));
        }
        let (session, window) = replay.as_mut().unwrap();
        if *session != request.session {
            return Ok(wire::unknown(request.sequence));
        }
        window
            .acknowledge(request.session, request.acknowledged)
            .map_err(crate::projection::storage_port_error)?;
        if request.sequence == 0 {
            drop(replay);
            return self.response(request.operation, 0);
        }
        match window
            .admit(
                request.session,
                request.sequence,
                wire::replay_digest(bytes).map_err(|_| PortError::Invalid)?,
                request.operation.response_bound(),
            )
            .map_err(crate::projection::storage_port_error)?
        {
            ReplayAdmission::Completed(bytes) => return Ok(bytes.to_vec()),
            ReplayAdmission::Unknown => return Ok(wire::unknown(request.sequence)),
            ReplayAdmission::New => {}
        }
        // The replay lock excludes duplicate dispatch only. It is never acquired
        // by Commit construction/publication or host SDK operations.
        let response = self.response(request.operation, request.sequence)?;
        window
            .finish(request.sequence, Arc::from(response.as_slice()))
            .map_err(crate::projection::storage_port_error)?;
        Ok(response)
    }
    fn response(&self, operation: Operation<'_>, sequence: u64) -> PortResult<Vec<u8>> {
        let result = self
            .dispatch(operation)
            .map_err(crate::projection::storage_port_error);
        let payload = match &result {
            Ok(bytes) => Ok(bytes.as_slice()),
            Err(error) => Err(*error),
        };
        wire::reply(sequence, payload).map_err(|_| PortError::Io)
    }
    fn dispatch(&self, operation: Operation<'_>) -> Result<Vec<u8>> {
        let host = &self.host;
        let unit = |result: Result<()>| result.map(|()| Vec::new());
        match operation {
            Operation::Lookup { kernel: true, .. }
            | Operation::Create { kernel: true, .. }
            | Operation::Mkdir { kernel: true, .. }
            | Operation::Symlink { kernel: true, .. }
            | Operation::Link { kernel: true, .. } => {
                host.kernel_entry(operation).map(wire::attr_out)
            }
            Operation::Lookup { parent, name, .. } => host.lookup(parent, name).map(wire::attr_out),
            Operation::Attr(node) => host.attr(node).map(wire::attr_out),
            Operation::Readlink(node) => host.readlink(node),
            Operation::Directory {
                node,
                after,
                kernel,
            } => {
                let page = host.directory_cookies(node, after, kernel)?;
                Ok(wire::page_out(&page)?)
            }
            Operation::Create {
                parent,
                name,
                mode,
                open,
                ..
            } => (if open {
                host.create_file_open(parent, name, mode)
            } else {
                host.create_file(parent, name, mode)
            })
            .map(wire::attr_out),
            Operation::Mkdir {
                parent, name, mode, ..
            } => host.mkdir(parent, name, mode).map(wire::attr_out),
            Operation::Symlink {
                parent,
                name,
                target,
                ..
            } => host.symlink(parent, name, target).map(wire::attr_out),
            Operation::Link {
                node, parent, name, ..
            } => host.link(node, parent, name).map(wire::attr_out),
            Operation::Unlink {
                parent,
                name,
                directory,
            } => unit(host.unlink(parent, name, directory)),
            Operation::Rename {
                parent,
                name,
                new_parent,
                new_name,
                no_replace,
            } => unit(host.rename(parent, name, new_parent, new_name, no_replace)),
            Operation::Pin {
                node,
                directory,
                truncate,
                ..
            } => unit(if directory {
                if truncate {
                    return Err(StoreError::InvalidInput("directory truncate"));
                }
                host.pin_directory(node)
            } else {
                host.pin(node, truncate)
            }),
            Operation::Unpin {
                node, directory, ..
            } => {
                if (host.attr(node)?.kind == layerfs_workspace_core::Kind::Directory) != directory {
                    return Err(StoreError::InvalidInput("unpin kind"));
                }
                unit(host.unpin(node))
            }
            Operation::Forget { node, count } => unit(host.kernel_forget(node, count)),
            Operation::ForgetBatch { entries } => {
                unit(host.kernel_forget_batch(&wire::forget_batch_in(entries)?))
            }
            Operation::Read { node, offset, size } => {
                let mut bytes = vec![0; size as usize];
                let count = host.read_into(node, offset, &mut bytes)?;
                bytes.truncate(count);
                Ok(bytes)
            }
            Operation::ReadLease {
                lease,
                offset,
                size,
            } => {
                let retained = self
                    .leases
                    .lock()
                    .map_err(|_| StoreError::Integrity("snapshot lease lock"))?
                    .get(&lease)
                    .cloned()
                    .ok_or(StoreError::NotFound("snapshot lease"))?;
                let mut bytes = vec![0; size as usize];
                let _read = host.budget.enter(Admission::Read, Charge::default())?;
                let count = Snapshot::read_tree(
                    &host.ranges,
                    &retained.tree,
                    &retained.reader,
                    offset,
                    &mut bytes,
                )?;
                bytes.truncate(count);
                Ok(bytes)
            }
            Operation::ReleaseLease { lease } => unit(self.release_read_lease(lease)),
            Operation::Write {
                node,
                offset,
                bytes,
            } => Ok((host.write(node, offset, bytes)? as u64)
                .to_be_bytes()
                .to_vec()),
            Operation::Truncate { node, size } => unit(host.truncate(node, size)),
            Operation::Chmod { node, mode } => unit(host.chmod(node, mode)),
            Operation::Mtime {
                node,
                seconds,
                nanos,
            } => unit(host.set_mtime(node, seconds, nanos)),
            Operation::Fsync(node) => {
                if let Some(node) = node {
                    host.attr(node)?;
                }
                host.payload.sync_data()?;
                host.index.sync_data()?;
                Ok(Vec::new())
            }
            Operation::Detach => unit(host.detach_kernel()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_overlay::tests::Fixture;
    use layerfs_fuse::{host_wire::Reply, NodeId};
    use layerfs_workspace_core::{ResourcePolicy, ROOT};

    fn frame(sequence: u64, acknowledged: u64, operation: Operation<'_>) -> Vec<u8> {
        wire::encode_request(wire::Request {
            session: [1; 16],
            sequence,
            acknowledged,
            operation,
        })
        .unwrap()
    }
    fn success(bytes: &[u8], sequence: u64) -> &[u8] {
        match wire::decode_reply(bytes, sequence).unwrap() {
            Reply::Known(Ok(bytes)) => bytes,
            _ => panic!("expected known successful operation"),
        }
    }

    #[test]
    fn lost_replies_bind_exact_requests_and_keep_kernel_owned_orphans_readable() {
        let fixture = Fixture::new(|_| {}, ResourcePolicy::default());
        let server = HostOperations::new(fixture.host.clone());
        let create = frame(
            1,
            0,
            Operation::Create {
                parent: ROOT,
                name: b"file",
                mode: 0o600,
                open: false,
                kernel: true,
            },
        );
        let first = server.request(&create).unwrap();
        let node = wire::attr_in(success(&first, 1)).unwrap().node;
        let installed = fixture.host.snapshot().unwrap().root.sequence;
        assert_eq!(server.request(&create).unwrap(), first, "lost reply replay");
        assert_eq!(fixture.host.snapshot().unwrap().root.sequence, installed);
        let changed = frame(
            1,
            0,
            Operation::Create {
                parent: ROOT,
                name: b"different",
                mode: 0o600,
                open: false,
                kernel: true,
            },
        );
        assert_eq!(server.request(&changed), Err(PortError::Invalid));
        assert!(fixture.host.lookup(ROOT, b"different").is_err());

        let write = frame(
            2,
            0,
            Operation::Write {
                node,
                offset: 0,
                bytes: b"data",
            },
        );
        let written = server.request(&write).unwrap();
        assert_eq!(success(&written, 2), 4u64.to_be_bytes());
        let captured = fixture.host.snapshot().unwrap();
        let acknowledged_retry = frame(
            2,
            1,
            Operation::Write {
                node,
                offset: 0,
                bytes: b"data",
            },
        );
        assert_eq!(server.request(&acknowledged_retry).unwrap(), written);
        assert_eq!(
            fixture.host.snapshot().unwrap().root.sequence,
            captured.root.sequence
        );
        let read = frame(
            0,
            2,
            Operation::Read {
                node,
                offset: 0,
                size: 32,
            },
        );
        assert_eq!(success(&server.request(&read).unwrap(), 0), b"data");
        success(
            &server
                .request(&frame(
                    3,
                    2,
                    Operation::Unlink {
                        parent: ROOT,
                        name: b"file",
                        directory: false,
                    },
                ))
                .unwrap(),
            3,
        );
        assert_eq!(fixture.host.attr(node).unwrap().links, 0);
        let forget = wire::forget_batch_out(&[(node, 1)]).unwrap();
        success(
            &server
                .request(&frame(4, 3, Operation::ForgetBatch { entries: &forget }))
                .unwrap(),
            4,
        );
        assert!(fixture.host.attr(node).is_err());
        let mut retained = [0; 4];
        fixture
            .host
            .read_snapshot(&captured, node, 0, &mut retained)
            .unwrap();
        assert_eq!(&retained, b"data");

        let replacement = frame(
            5,
            4,
            Operation::Create {
                parent: ROOT,
                name: b"file",
                mode: 0o600,
                open: false,
                kernel: false,
            },
        );
        let reply = server.request(&replacement).unwrap();
        let new_node = wire::attr_in(success(&reply, 5)).unwrap().node;
        assert_ne!(new_node, node);
        assert_eq!(
            server.request(&create),
            Err(PortError::Invalid),
            "settled requests are stale"
        );
        assert_eq!(fixture.host.lookup(ROOT, b"file").unwrap().node, new_node);
        let exists = frame(
            6,
            5,
            Operation::Create {
                parent: ROOT,
                name: b"file",
                mode: 0o600,
                open: false,
                kernel: false,
            },
        );
        let failure = server.request(&exists).unwrap();
        assert!(matches!(
            wire::decode_reply(&failure, 6).unwrap(),
            Reply::Known(Err(PortError::Exists))
        ));
        assert_eq!(
            server.request(&exists).unwrap(),
            failure,
            "known failure is replayed too"
        );
        let unknown = wire::encode_request(wire::Request {
            session: [2; 16],
            sequence: 1,
            acknowledged: 0,
            operation: Operation::Forget {
                node: NodeId(999),
                count: 1,
            },
        })
        .unwrap();
        assert!(matches!(
            wire::decode_reply(&server.request(&unknown).unwrap(), 1).unwrap(),
            Reply::Unknown
        ));
        println!("host transport exact replay, known failure, stale/unknown session, kernel orphan+retained snapshot: PASS");
    }

    #[test]
    fn sdk_read_leases_hold_exact_bytes_and_capacity_until_last_reader_releases() {
        let mut policy = ResourcePolicy::default();
        policy.overlay.max_readers = 2;
        let fixture = Fixture::new(|_| {}, policy);
        let host = &fixture.host;
        let node = host.create_file(ROOT, b"file", 0o600).unwrap().node;
        host.write(node, 0, b"before").unwrap();
        let unrelated = host.create_file(ROOT, b"unrelated", 0o600).unwrap().node;
        host.write(unrelated, 0, &vec![b'U'; 64 * 1024]).unwrap();
        let server = HostOperations::new(host.clone());
        let first = server
            .retain_read_lease(node, host.snapshot().unwrap())
            .unwrap();
        assert_eq!((first.node, first.size), (node, 6));
        host.unlink(ROOT, b"unrelated", false).unwrap();
        for _ in 0..4000 {
            if !host.maintain().unwrap() {
                break;
            }
        }
        assert_eq!(
            host.payload.stats().unwrap().chargeable_bytes,
            6,
            "a file read lease must not retain unrelated Workspace payload"
        );
        host.write(node, 0, b"later!").unwrap();
        let second = server
            .retain_read_lease(node, host.snapshot().unwrap())
            .unwrap();
        assert!(server
            .retain_read_lease(node, host.snapshot().unwrap())
            .is_err());
        let read = frame(
            0,
            0,
            Operation::ReadLease {
                lease: first.id,
                offset: 0,
                size: 32,
            },
        );
        assert_eq!(success(&server.request(&read).unwrap(), 0), b"before");
        let reading = server
            .leases
            .lock()
            .unwrap()
            .get(&first.id)
            .unwrap()
            .clone();
        server.release_read_lease(first.id).unwrap();
        assert!(
            server
                .retain_read_lease(node, host.snapshot().unwrap())
                .is_err(),
            "in-flight reader retains its lease slot"
        );
        let mut bytes = [0; 6];
        Snapshot::read_tree(&host.ranges, &reading.tree, &reading.reader, 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"before");
        assert!(matches!(
            wire::decode_reply(&server.request(&read).unwrap(), 0).unwrap(),
            wire::Reply::Known(Err(PortError::NotFound))
        ));
        drop(reading);
        let third = server
            .retain_read_lease(node, host.snapshot().unwrap())
            .unwrap();
        assert!(third.id > second.id);
        host.unlink(ROOT, b"file", false).unwrap();
        let last = frame(
            0,
            0,
            Operation::ReadLease {
                lease: second.id,
                offset: 0,
                size: 6,
            },
        );
        assert_eq!(success(&server.request(&last).unwrap(), 0), b"later!");
        let release = frame(1, 0, Operation::ReleaseLease { lease: second.id });
        let released = server.request(&release).unwrap();
        assert_eq!(success(&released, 1), b"");
        assert_eq!(server.request(&release).unwrap(), released);
        server.release_read_lease(third.id).unwrap();
        assert_eq!(server.lease_count.load(Ordering::Acquire), 0);
        let foreign = Fixture::new(|_| {}, ResourcePolicy::default());
        let foreign_node = foreign
            .host
            .create_file(ROOT, b"foreign", 0o600)
            .unwrap()
            .node;
        assert!(server
            .retain_read_lease(foreign_node, foreign.host.snapshot().unwrap())
            .is_err());
        println!("SDK immutable lease reads survive live overwrite/unlink; full lease capacity stays readable; released in-flight ownership remains charged; foreign host rejected: PASS");
    }

    #[test]
    fn sdk_reader_is_reserved_before_mutation_and_activates_exact_installed_candidate() {
        let mut policy = ResourcePolicy::default();
        policy.overlay.max_readers = 1;
        let fixture = Fixture::new(|_| {}, policy);
        let host = &fixture.host;
        let node = host.create_file(ROOT, b"file", 0o600).unwrap().node;
        host.write(node, 0, b"before").unwrap();
        let server = HostOperations::new(host.clone());
        let mut reserved = Some(server.reserve_read_lease(node).unwrap());
        let before = host.snapshot().unwrap();
        assert!(server.reserve_read_lease(node).is_err());
        assert_eq!(host.snapshot().unwrap().root.sequence, before.root.sequence);
        let other_server = HostOperations::new(host.clone());
        assert!(other_server
            .install_read_lease(&mut reserved, &before)
            .is_err());
        assert!(
            reserved.is_some(),
            "failed validation must preserve the admitted ticket"
        );
        let installed = host
            .edit_many_snapshot(
                node,
                &[(
                    0,
                    6,
                    crate::WorkspaceFileReplacement::Inline(b"SDK".to_vec()),
                )],
            )
            .unwrap();
        host.write(node, 0, b"new").unwrap();
        let lease = server
            .install_read_lease(&mut reserved, &installed)
            .unwrap();
        assert!(reserved.is_none());
        let request = frame(
            0,
            0,
            Operation::ReadLease {
                lease: lease.id,
                offset: 0,
                size: 64,
            },
        );
        assert_eq!(success(&server.request(&request).unwrap(), 0), b"SDK");
        let mut live = [0; 3];
        host.read_into(node, 0, &mut live).unwrap();
        assert_eq!(&live, b"new");
        server.release_read_lease(lease.id).unwrap();
        assert_eq!(server.lease_count.load(Ordering::Acquire), 0);
        println!("SDK pre-reservation rejects overcapacity before mutation; activation owns exact SDKcandidate despite laterlivewrite and preserves failed tickets: PASS");
    }
}
