//! One charged record of actual canonical bytes returned by an ordinary read.
use crate::{backing::budget::Charge, *};
use layerfs_bridge::contract::{Root, Source};
use std::{mem::size_of, time::Instant};

pub(crate) struct ReadOrigin {
    inode: u64,
    root: Root,
    offset: u64,
    bytes: Vec<u8>,
    _charge: Charge,
}

impl Workspace {
    pub(crate) fn retain_read_origin(&self, inode: u64, origin: Option<(Root, u64)>, bytes: &[u8]) {
        let Ok(mut held) = self.inner.read_origin.lock() else {
            return;
        };
        *held = None;
        let Some((root, offset)) = origin else { return };
        if bytes.is_empty() || bytes.len() > MAX_READ_BYTES || root == [0; 32] {
            return;
        }
        let Ok(mut charge) = self
            .host
            .budget
            .reserve(size_of::<ReadOrigin>() + bytes.len())
        else {
            return;
        };
        let mut copy = Vec::new();
        if copy.try_reserve_exact(bytes.len()).is_err()
            || charge
                .resize(size_of::<ReadOrigin>() + copy.capacity())
                .is_err()
        {
            return;
        }
        copy.extend_from_slice(bytes);
        *held = Some(ReadOrigin {
            inode,
            root,
            offset,
            bytes: copy,
            _charge: charge,
        });
    }

    pub(crate) fn read_origin_bytes(
        &self,
        inode: u64,
        root: Root,
        bytes: &[u8],
    ) -> Result<Option<u64>, WorkspaceError> {
        let held = self
            .inner
            .read_origin
            .lock()
            .map_err(|_| WorkspaceError::Io)?;
        Ok(held
            .as_ref()
            .filter(|record| record.inode == inode && record.root == root && record.bytes == bytes)
            .map(|record| record.offset))
    }

    pub(crate) fn read_origin_payload(
        &self,
        inode: u64,
        root: Root,
        payload: &OwnedPayload,
        deadline: Instant,
    ) -> Result<Option<u64>, WorkspaceError> {
        let held = self
            .inner
            .read_origin
            .lock()
            .map_err(|_| WorkspaceError::Io)?;
        let Some(record) = held.as_ref().filter(|record| {
            record.inode == inode
                && record.root == root
                && record.bytes.len() as u64 == payload.len()
        }) else {
            return Ok(None);
        };
        let _scratch = self.host.budget.reserve(8192)?;
        let mut buffer = [0; 8192];
        let mut source = payload.reader(0..payload.len())?;
        for expected in record.bytes.chunks(buffer.len()) {
            let mut done = 0;
            while done < expected.len() {
                let read = Source::read(
                    &mut source,
                    &mut buffer[done..expected.len()],
                    deadline,
                    &self.inner.stopping,
                )?;
                if read == 0 {
                    return Err(WorkspaceError::Io);
                }
                done += read;
            }
            if expected != &buffer[..expected.len()] {
                return Ok(None);
            }
        }
        Ok(Some(record.offset))
    }
}
