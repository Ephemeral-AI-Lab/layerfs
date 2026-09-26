//! The Commit's EditFile body stream: packed descriptors, then replacement bytes.
use super::source::ReplacementSource;
use crate::{backing::metadata_index::vector, WorkspaceError};
use layerfs_bridge::contract::{Edit, Source};
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

/// Packs one lowering's edit list into the wire descriptor block: 24 bytes per
/// edit — `start`, `end`, `replacement`, each big-endian u64 — in declared edit
/// order. The block is bounded by the per-operation edit budget (96 KiB at the
/// cap), so packing it never allocates against the file's size.
pub(crate) fn descriptor_block(edits: &[Edit]) -> Result<Vec<u8>, WorkspaceError> {
    let mut block = vector(edits.len() * 24)?;
    for edit in edits {
        block.extend_from_slice(&edit.start.to_be_bytes());
        block.extend_from_slice(&edit.end.to_be_bytes());
        block.extend_from_slice(&edit.replacement.to_be_bytes());
    }
    Ok(block)
}

/// Serves the EditFile body stream: the packed descriptor block first, then the
/// replacement bytes of the frozen sequence through the existing bounded
/// [`ReplacementSource`]. The adapter owns no file-sized buffer — the block is
/// the bounded descriptor list and everything after it streams.
pub(crate) struct EditUpload<'a> {
    descriptors: Vec<u8>,
    at: usize,
    source: &'a mut ReplacementSource,
}

impl<'a> EditUpload<'a> {
    pub(crate) fn new(
        edits: &[Edit],
        source: &'a mut ReplacementSource,
    ) -> Result<Self, WorkspaceError> {
        Ok(Self {
            descriptors: descriptor_block(edits)?,
            at: 0,
            source,
        })
    }
}

impl Source for EditUpload<'_> {
    fn read(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.at < self.descriptors.len() {
            let count = (self.descriptors.len() - self.at).min(out.len());
            out[..count].copy_from_slice(&self.descriptors[self.at..self.at + count]);
            self.at += count;
            return Ok(count);
        }
        Source::read(self.source, out, deadline, cancel)
    }
}
