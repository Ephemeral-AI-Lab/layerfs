//! Fixed captured input remains independently owned across install and failure.
use crate::{
    db::{integer, unsigned},
    Capture, CapturedReader, Overlay, OverlayError, OverlayResult, StatementKind,
};
impl Overlay {
    /// Acquire before construction/read passes. The exact captured root/floor
    /// are stored, and a lost result can be observed by its request identity.
    pub fn acquire_captured_reader(
        &self,
        capture: Capture,
        request: u64,
    ) -> OverlayResult<CapturedReader> {
        let request = integer(request)?;
        if request == 0 {
            return Err(OverlayError::Invalid("zero captured request"));
        }
        self.atomic(|| {
            self.checked_capture(capture)?;
            let state = self.state()?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            let owner = self.mint_owner()?;
            let reader = CapturedReader {
                capture,
                owner,
                installed: state.installed,
            };
            self.open.borrow_mut().captured.insert(
                owner,
                request,
                reader,
                "duplicate captured request",
            )?;
            self.captured_reference(capture.generation.0, true)?;
            Ok(reader)
        })
    }
    /// Coherent original custody; absence is not permission to repeat acquisition.
    pub fn retained_captured_reader(&self, request: u64) -> OverlayResult<Option<CapturedReader>> {
        self.state()?;
        Ok(self.open.borrow().captured.retained(integer(request)?))
    }
    pub(crate) fn check_captured_reader(&self, reader: CapturedReader) -> OverlayResult<()> {
        self.state()?;
        if self
            .open
            .borrow()
            .captured
            .get(reader.owner)
            .map(|(held, _)| held)
            != Some(reader)
        {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    fn captured_reference(&self, generation: i64, add: bool) -> OverlayResult<()> {
        let mut open = self.open.borrow_mut();
        let count = open
            .captured_generations
            .rows
            .get(&generation)
            .copied()
            .unwrap_or(0);
        let next = if add {
            count.checked_add(1)
        } else {
            count.checked_sub(1)
        }
        .ok_or(OverlayError::Stale)?;
        if next == 0 {
            open.captured_generations.remove(generation);
        } else {
            open.captured_generations.put(generation, next);
        }
        Ok(())
    }
    /// Fixed sealed metadata pages remain valid after install/definite failure.
    pub fn reader_inodes(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> OverlayResult<Vec<crate::Inode>> {
        self.check_captured_reader(reader)?;
        self.query(
            StatementKind::Capture,
            crate::sql::INODE_CAPTURE,
            &[&reader.capture.generation.0, &integer(after)?],
            24,
            crate::inode::decode,
        )
    }
    /// Latest local namespace inode in the reader's exact sealed range. This
    /// excludes the current orphan domain and never substitutes current state.
    pub fn reader_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> OverlayResult<Option<crate::Inode>> {
        self.check_captured_reader(reader)?;
        if serial == 0 {
            return Err(OverlayError::Invalid("zero captured inode serial"));
        }
        self.query(
            StatementKind::Capture,
            crate::sql::INODE_LOOKUP,
            &[
                &integer(serial)?,
                &reader.capture.generation.0,
                &reader.installed,
            ],
            32,
            crate::inode::decode,
        )
        .map(|mut rows| rows.pop())
    }
    pub fn reader_directory_entries(
        &self,
        reader: CapturedReader,
        after: Option<(u64, &[u8])>,
    ) -> OverlayResult<Vec<crate::DirectoryEntry>> {
        self.check_captured_reader(reader)?;
        let (parent, name) = after.unwrap_or((0, &[]));
        if name.len() > 255 {
            return Err(OverlayError::Invalid("captured name cursor"));
        }
        self.query(
            StatementKind::Capture,
            crate::sql::DIRECTORY_ENTRY_CAPTURE,
            &[&reader.capture.generation.0, &integer(parent)?, &name],
            24 + name.len() as u64,
            |r| {
                Ok(crate::DirectoryEntry {
                    parent: unsigned(r, 0)?,
                    name: r.get(1)?,
                    serial: r
                        .get::<_, Option<i64>>(2)?
                        .map(|v| u64::try_from(v).map_err(|_| rusqlite::Error::InvalidQuery))
                        .transpose()?,
                    inherited: r.get(3)?,
                })
            },
        )
    }
    /// Exact release after all captured consumers and external calls finish.
    pub fn release_captured_reader(&self, reader: CapturedReader) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.check_captured_reader(reader)?;
            let c = reader.capture;
            self.open
                .borrow_mut()
                .captured
                .remove(reader.owner, |held| *held == reader)?;
            self.captured_reference(c.generation.0, false)?;
            self.wake_generation(c.generation.0 as u64)?;
            Ok(())
        })
    }
    /// One immutable captured byte window, independent of current installed floor.
    pub fn read_captured(
        &self,
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> OverlayResult<Option<crate::LocalRead>> {
        self.check_captured_reader(reader)?;
        let mut plan = self.read_layers(
            serial,
            reader.capture.generation.0,
            reader.installed,
            offset,
            length,
        )?;
        if let Some(ref mut plan) = plan {
            plan.base_root = Some(reader.capture.base_root)
        }
        Ok(plan)
    }
}
