//! Fixed captured input remains independently owned across install and failure.
use crate::{
    db::{integer, unsigned},
    Capture, CapturedReader, LeaseKind, Overlay, OverlayError, OverlayResult, Route, StatementKind,
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
            let state = self.state(capture.route)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            let owner = self.mint_owner(capture.route)?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO captured_reader VALUES(?1,?2,?3,?4,?5,?6,?7)",
                &[
                    &capture.route.ns,
                    &request,
                    &integer(owner)?,
                    &capture.generation.0,
                    &capture.revision,
                    &capture.base_root.as_slice(),
                    &state.installed,
                ],
                80,
            )?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO lease VALUES(?1,6,?2,?3)",
                &[&capture.route.ns, &integer(owner)?, &capture.generation.0],
                24,
            )?;
            Ok(CapturedReader {
                capture,
                owner,
                installed: state.installed,
            })
        })
    }
    /// Coherent original custody; absence is not permission to repeat acquisition.
    pub fn retained_captured_reader(
        &self,
        route: Route,
        request: u64,
    ) -> OverlayResult<Option<CapturedReader>> {
        self.state(route)?;
        self.query(StatementKind::Lease,"SELECT owner,gen,revision,base_root,installed FROM captured_reader WHERE ns=?1 AND request=?2",
            &[&route.ns,&integer(request)?],16,|r| {
                let root:Vec<u8>=r.get(3)?;
                Ok(CapturedReader {owner:unsigned(r,0)?,installed:r.get(4)?,capture:Capture {
                    route,generation:crate::Generation(r.get(1)?),revision:r.get(2)?,
                    base_root:root.try_into().map_err(|_|rusqlite::Error::InvalidQuery)?}})
            }).map(|mut rows|rows.pop())
    }
    pub(crate) fn check_captured_reader(&self, reader: CapturedReader) -> OverlayResult<()> {
        self.state(reader.capture.route)?;
        let c = reader.capture;
        if self.query(StatementKind::Lease,"SELECT 1 FROM captured_reader WHERE ns=?1 AND owner=?2 AND gen=?3 AND revision=?4 AND base_root=?5 AND installed=?6",
            &[&c.route.ns,&integer(reader.owner)?,&c.generation.0,&c.revision,&c.base_root.as_slice(),&reader.installed],72,|_|Ok(()))?.is_empty() {return Err(OverlayError::Stale)}
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
            &[
                &reader.capture.route.ns,
                &reader.capture.generation.0,
                &integer(after)?,
            ],
            24,
            crate::inode::decode,
        )
    }
    pub fn reader_dentries(
        &self,
        reader: CapturedReader,
        after: Option<(u64, &[u8])>,
    ) -> OverlayResult<Vec<crate::Dentry>> {
        self.check_captured_reader(reader)?;
        let (parent, name) = after.unwrap_or((0, &[]));
        if name.len() > 255 {
            return Err(OverlayError::Invalid("captured name cursor"));
        }
        self.query(
            StatementKind::Capture,
            crate::sql::DENTRY_CAPTURE,
            &[
                &reader.capture.route.ns,
                &reader.capture.generation.0,
                &integer(parent)?,
                &name,
            ],
            24 + name.len() as u64,
            |r| {
                Ok(crate::Dentry {
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
            self.execute(
                StatementKind::Lease,
                "DELETE FROM captured_reader WHERE ns=?1 AND owner=?2",
                &[&c.route.ns, &integer(reader.owner)?],
                16,
            )?;
            self.execute(
                StatementKind::Lease,
                "DELETE FROM lease WHERE ns=?1 AND kind=?2 AND owner=?3 AND resource=?4",
                &[
                    &c.route.ns,
                    &(LeaseKind::CapturedReader as i64),
                    &integer(reader.owner)?,
                    &c.generation.0,
                ],
                32,
            )?;
            self.wake_generation(c.route.ns, c.generation.0 as u64)?;
            self.queue_closed(c.route)
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
            reader.capture.route.ns,
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
