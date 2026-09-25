//! Per-mount private LFB3 stages. No Workspace mutation occurs before APPLY.
use super::wire;
use crate::adapter::Adapter;
use fuser::{Errno, FileHandle, INodeNo};
use layerfs_workspace::{RangePart, RangeStamp};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_TOTAL: u64 = 8 * 1024 * 1024;
const MAX_STAGES: usize = 32;
const MAX_PARTS: usize = 1024;
const LIFETIME: Duration = Duration::from_secs(30);
const ZERO: [u8; 4096] = [0; 4096];

pub(super) struct Stage {
    token: [u8; 16],
    handle: u64,
    pub(super) stamp: RangeStamp,
    pub(super) start: u64,
    pub(super) end: u64,
    logical: u64,
    literal_length: u64,
    next: u64,
    digest: [u8; 32],
    hash: Sha256,
    pub(super) bytes: Vec<u8>,
    pub(super) parts: Vec<RangePart>,
    expires: Instant,
}

#[derive(Default)]
pub(crate) struct Stages {
    entries: Mutex<Vec<Stage>>,
}

impl Stages {
    pub(crate) fn sweep(&self) {
        let mut stages = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        stages.retain(|stage| stage.expires > Instant::now());
    }

    pub(crate) fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }

    pub(crate) fn release(&self, handle: u64) {
        let mut stages = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        stages.retain(|stage| stage.handle != handle);
    }

    pub(super) fn begin(
        &self,
        adapter: &Adapter,
        ino: INodeNo,
        fh: FileHandle,
        input: &[u8],
    ) -> Result<[u8; 128], Errno> {
        let stamp = RangeStamp {
            inode: wire::u64_at(input, 8),
            incarnation: input[16..48].try_into().unwrap(),
            generation: wire::u64_at(input, 48),
            revision: wire::u64_at(input, 56),
        };
        let start = wire::u64_at(input, 64);
        let end = start + wire::u64_at(input, 72); // checked by wire::validate
        let logical = wire::u64_at(input, 80);
        let literal_length = wire::u64_at(input, 88);
        let mut token = [0; 16];
        File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut token))
            .map_err(|_| Errno::EIO)?;
        // Release takes this lock before closing the Workspace handle.
        let mut stages = self.entries.lock().map_err(|_| Errno::EIO)?;
        stages.retain(|stage| stage.expires > Instant::now());
        let current = adapter
            .workspace
            .projected_range_state(fh.0, crate::replies::serial(ino, adapter.root()))
            .map_err(crate::replies::errno)?;
        if !current.writable || stamp.inode != current.stamp.inode {
            return Err(Errno::EBADF);
        }
        if current.stamp != stamp {
            return Err(Errno::ESTALE);
        }
        if end > current.length {
            return Err(Errno::EINVAL);
        }
        if current
            .length
            .checked_sub(end - start)
            .and_then(|remaining| remaining.checked_add(logical))
            .is_none_or(|length| length > 4 * 1024 * 1024 * 1024)
        {
            return Err(Errno::ENOSPC);
        }
        if stages.iter().any(|stage| stage.handle == fh.0) {
            return Err(Errno::EBUSY);
        }
        if stages.len() == MAX_STAGES
            || stages.iter().any(|stage| stage.token == token)
            || stages
                .iter()
                .map(|stage| stage.logical)
                .sum::<u64>()
                .checked_add(logical)
                .is_none_or(|total| total > MAX_TOTAL)
        {
            return Err(Errno::ENOSPC);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(literal_length as usize)
            .map_err(|_| Errno::ENOSPC)?;
        stages.try_reserve(1).map_err(|_| Errno::ENOSPC)?;
        stages.push(Stage {
            token,
            handle: fh.0,
            stamp,
            start,
            end,
            logical,
            literal_length,
            next: 0,
            digest: input[96..128].try_into().unwrap(),
            hash: Sha256::new(),
            bytes,
            parts: Vec::new(),
            expires: Instant::now() + LIFETIME,
        });
        let mut reply = [0; 128];
        reply[..4].copy_from_slice(b"LFB3");
        reply[4..6].copy_from_slice(&3u16.to_le_bytes());
        reply[8..24].copy_from_slice(&token);
        Ok(reply)
    }

    pub(super) fn data(&self, fh: FileHandle, input: &[u8]) -> Result<(), Errno> {
        let token = &input[8..24];
        let offset = wire::u64_at(input, 24);
        let logical = wire::u64_at(input, 32);
        let literal = wire::u32_at(input, 40) as usize;
        let mut stages = self.entries.lock().map_err(|_| Errno::EIO)?;
        stages.retain(|stage| stage.expires > Instant::now());
        let stage = stages
            .iter_mut()
            .find(|stage| stage.handle == fh.0 && stage.token.as_slice() == token)
            .ok_or(Errno::EBADF)?;
        if offset != stage.next
            || offset
                .checked_add(logical)
                .is_none_or(|end| end > stage.logical)
            || (literal > 0
                && (stage.bytes.len() as u64)
                    .checked_add(literal as u64)
                    .is_none_or(|end| end > stage.literal_length))
        {
            return Err(Errno::EINVAL);
        }
        let joins_last = matches!(
            (literal > 0, stage.parts.last()),
            (true, Some(RangePart::Bytes(_))) | (false, Some(RangePart::Zero(_)))
        );
        if !joins_last {
            if stage.parts.len() == MAX_PARTS {
                return Err(Errno::ENOSPC);
            }
            stage.parts.try_reserve(1).map_err(|_| Errno::ENOSPC)?;
        }
        if literal > 0 {
            stage.hash.update(&input[128..128 + literal]);
            stage.bytes.extend_from_slice(&input[128..128 + literal]);
            match stage.parts.last_mut() {
                Some(RangePart::Bytes(length)) => *length += logical,
                _ => stage.parts.push(RangePart::Bytes(logical)),
            }
        } else {
            let mut left = logical;
            while left > 0 {
                let take = left.min(ZERO.len() as u64) as usize;
                stage.hash.update(&ZERO[..take]);
                left -= take as u64;
            }
            match stage.parts.last_mut() {
                Some(RangePart::Zero(length)) => *length += logical,
                _ => stage.parts.push(RangePart::Zero(logical)),
            }
        }
        stage.next += logical;
        Ok(())
    }

    pub(super) fn abort(&self, fh: FileHandle, input: &[u8]) -> Result<(), Errno> {
        let mut stages = self.entries.lock().map_err(|_| Errno::EIO)?;
        stages.retain(|stage| stage.expires > Instant::now());
        let index = stages
            .iter()
            .position(|stage| stage.handle == fh.0 && stage.token.as_slice() == &input[8..24])
            .ok_or(Errno::EBADF)?;
        stages.remove(index);
        Ok(())
    }

    pub(super) fn take_for_apply(
        &self,
        adapter: &Adapter,
        ino: INodeNo,
        fh: FileHandle,
        input: &[u8],
    ) -> Result<Stage, Errno> {
        let mut stages = self.entries.lock().map_err(|_| Errno::EIO)?;
        stages.retain(|stage| stage.expires > Instant::now());
        let index = stages
            .iter()
            .position(|stage| stage.handle == fh.0 && stage.token.as_slice() == &input[8..24])
            .ok_or(Errno::EBADF)?;
        let stage = stages.remove(index);
        drop(stages);
        if stage.next != stage.logical
            || stage.bytes.len() as u64 != stage.literal_length
            || stage.hash.clone().finalize()[..] != stage.digest[..]
            || stage
                .parts
                .iter()
                .map(|part| match part {
                    RangePart::Bytes(length) | RangePart::Zero(length) => *length,
                })
                .sum::<u64>()
                != stage.logical
        {
            return Err(Errno::EINVAL);
        }
        let current = adapter
            .workspace
            .projected_range_state(fh.0, crate::replies::serial(ino, adapter.root()))
            .map_err(crate::replies::errno)?;
        if !current.writable || current.stamp.inode != stage.stamp.inode {
            return Err(Errno::EBADF);
        }
        if current.stamp != stage.stamp {
            return Err(Errno::ESTALE);
        }
        if stage.start > stage.end || stage.end > current.length {
            return Err(Errno::EINVAL);
        }
        Ok(stage)
    }
}
