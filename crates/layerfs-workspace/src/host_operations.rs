//! Authenticated transport dispatch into the same host authority used by SDK
//! mutations. Installed operations retain an exact bounded replay result.
use crate::host_overlay::HostOverlay;
use crate::overlay::{ReplayAdmission, ReplayWindow};
use crate::overlay_budget::{Charge, Operation as Admission};
use layerfs_fuse::host_wire::{self as wire, Operation};
use layerfs_fuse::{PortError, PortResult};
use layerfs_layerstack_store::{Result, StoreError};
use std::sync::{Arc, Mutex};

pub(crate) struct HostOperations {
    pub(crate) host: Arc<HostOverlay>,
    replay: Mutex<Option<([u8; 16], ReplayWindow)>>,
}
impl HostOperations {
    pub(crate) fn new(host: Arc<HostOverlay>) -> Self {
        Self {
            host,
            replay: Mutex::new(None),
        }
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
}
