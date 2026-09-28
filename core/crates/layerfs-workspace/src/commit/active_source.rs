//! Charged, fixed-window pack-local replacement reads from a captured selection.
use crate::{
    backing::{
        active::{ActivePackReader, Extent, ExtentKind},
        budget::{Budget, Charge},
    },
    WorkspaceError,
};
use std::{mem::size_of, sync::Arc, time::Instant};

// Both limits apply to required replacement references, not file length or
// unrelated records. Only one decoded pack page remains in the reader.
const REFS: usize = 256;
const BYTES: usize = 32 * 1024;

#[derive(Clone, Copy)]
struct Reference {
    extent: usize,
    offset: u64,
    length: usize,
    into: usize,
}

pub(super) struct Prefetch {
    refs: Vec<Reference>,
    order: Vec<usize>,
    data: Vec<u8>,
    next: usize,
    windows: u64,
    references: u64,
    distinct_packs: u64,
    fill_ns: u64,
    _charge: Charge,
}

impl Prefetch {
    pub(super) fn new(budget: &Arc<Budget>) -> Result<Self, WorkspaceError> {
        let mut charge =
            budget.reserve(BYTES + REFS * (size_of::<Reference>() + size_of::<usize>()))?;
        let refs = Vec::with_capacity(REFS);
        let order = Vec::with_capacity(REFS);
        let data = vec![0; BYTES];
        charge.resize(
            refs.capacity() * size_of::<Reference>()
                + order.capacity() * size_of::<usize>()
                + data.capacity(),
        )?;
        Ok(Self {
            refs,
            order,
            data,
            next: 0,
            windows: 0,
            references: 0,
            distinct_packs: 0,
            fill_ns: 0,
            _charge: charge,
        })
    }

    pub(super) fn counts(&self) -> (u64, u64, u64, u64) {
        (
            self.windows,
            self.references,
            self.distinct_packs,
            self.fill_ns,
        )
    }

    fn covers(&self, at: usize, offset: u64) -> bool {
        self.refs.get(self.next).is_some_and(|reference| {
            reference.extent == at
                && offset >= reference.offset
                && offset - reference.offset < reference.length as u64
        })
    }

    fn fill(
        &mut self,
        serial: u64,
        extents: &[Extent],
        at: usize,
        offset: u64,
        reader: &mut ActivePackReader,
    ) -> Result<(), WorkspaceError> {
        let began = Instant::now();
        self.refs.clear();
        self.order.clear();
        self.next = 0;
        let mut used = 0;
        for (index, extent) in extents.iter().enumerate().skip(at) {
            if extent.kind != ExtentKind::Packed {
                continue;
            }
            let start = if index == at { offset } else { 0 };
            let length = usize::try_from(extent.end - extent.start - start)
                .map_err(|_| WorkspaceError::Capacity)?;
            if length == 0 || length > BYTES {
                return Err(WorkspaceError::Io);
            }
            if self.refs.len() == REFS || used + length > BYTES {
                break;
            }
            self.refs.push(Reference {
                extent: index,
                offset: start,
                length,
                into: used,
            });
            used += length;
        }
        if !self.covers(at, offset) {
            return Err(WorkspaceError::Io);
        }
        self.windows += 1;
        self.references += self.refs.len() as u64;
        self.order.extend(0..self.refs.len());
        // Only required references are visited. A logical pack is loaded once
        // per window by the existing single-page, identity-checking reader.
        self.order
            .sort_unstable_by_key(|&index| extents[self.refs[index].extent].logical_page);
        let mut previous = None;
        for &index in &self.order {
            let reference = self.refs[index];
            let extent = extents[reference.extent];
            if previous != Some(extent.logical_page) {
                self.distinct_packs += 1;
                previous = Some(extent.logical_page);
            }
            reader.read(
                serial,
                extent,
                extent.start + reference.offset,
                &mut self.data[reference.into..reference.into + reference.length],
            )?;
        }
        self.fill_ns += began.elapsed().as_nanos() as u64;
        Ok(())
    }

    pub(super) fn read(
        &mut self,
        serial: u64,
        extents: &[Extent],
        at: usize,
        offset: u64,
        output: &mut [u8],
        reader: &mut ActivePackReader,
    ) -> Result<(), WorkspaceError> {
        if !self.covers(at, offset) {
            self.fill(serial, extents, at, offset, reader)?;
        }
        let reference = self.refs[self.next];
        let within = usize::try_from(offset - reference.offset).map_err(|_| WorkspaceError::Io)?;
        if within + output.len() > reference.length {
            return Err(WorkspaceError::Io);
        }
        let start = reference.into + within;
        output.copy_from_slice(&self.data[start..start + output.len()]);
        if within + output.len() == reference.length {
            self.next += 1;
        }
        Ok(())
    }
}
