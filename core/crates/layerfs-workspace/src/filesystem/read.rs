use crate::*;
use layerfs_bridge::contract::{Inspect, Operation, Response};
use std::{io::Cursor, time::Instant};

impl Workspace {
    pub fn open(&self, serial: u64, scope: ReferenceScope) -> Result<HandleId, WorkspaceError> {
        self.open_handle(serial, false, scope)
    }
    pub fn read(
        &self,
        handle: HandleId,
        offset: u64,
        size: usize,
        deadline: Instant,
    ) -> Result<ReadReply, WorkspaceError> {
        if size > MAX_READ_BYTES {
            return Err(WorkspaceError::Capacity);
        }
        let deadline = Self::callback_deadline(deadline);
        let (attr, content) = {
            let state = self.state()?;
            self.available(&state)?;
            let handle = state.handle(handle, false)?;
            let node = state.node(handle.serial)?;
            (node.attr, node.content)
        };
        let length = if offset >= attr.size {
            0
        } else {
            (attr.size - offset).min(size as u64) as usize
        };
        let operation = self.begin(length > 0, deadline)?;
        let charge = self.host.budget.reserve(length)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| WorkspaceError::Capacity)?;
        bytes.resize(length, 0);
        if length > 0 {
            let mut output = Cursor::new(bytes.as_mut_slice());
            let response = self.call(
                Operation::ReadFile {
                    root: content,
                    start: offset,
                    end: offset + length as u64,
                },
                length as u64,
                &mut output,
                deadline,
            )?;
            if response
                != (Response::Read {
                    length: length as u64,
                })
                || output.position() != length as u64
            {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        Ok(ReadReply {
            bytes,
            _charge: charge,
            _operation: operation,
        })
    }
    pub fn readlink(&self, serial: u64, deadline: Instant) -> Result<ReadReply, WorkspaceError> {
        let deadline = Self::callback_deadline(deadline);
        let operation = self.begin(true, deadline)?;
        let (path, size) = {
            let state = self.state()?;
            let node = state.node(serial)?;
            if node.attr.kind != NodeKind::Symlink {
                return Err(WorkspaceError::WrongKind);
            }
            (node.path().to_vec(), node.attr.size)
        };
        let response = self.call(
            Operation::Inspect {
                root: self.inner.base,
                query: Inspect::Readlink { path },
            },
            0,
            &mut std::io::sink(),
            deadline,
        )?;
        let Response::Link(bytes) = response else {
            return Err(WorkspaceError::InvalidInput);
        };
        if bytes.len() > 4096 || bytes.len() as u64 != size || bytes.contains(&0) {
            return Err(WorkspaceError::InvalidInput);
        }
        let charge = self.host.budget.reserve(bytes.capacity())?;
        Ok(ReadReply {
            bytes,
            _charge: charge,
            _operation: operation,
        })
    }
    pub fn flush(&self, handle: HandleId) -> Result<(), WorkspaceError> {
        self.state()?.handle(handle, false)?;
        Ok(())
    }
    pub fn release(&self, handle: HandleId) -> Result<(), WorkspaceError> {
        self.release_handle(handle, false)
    }
}
