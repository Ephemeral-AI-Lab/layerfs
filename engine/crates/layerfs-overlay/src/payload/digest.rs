//! Whole-content digests of files written as appends (#364).
//!
//! A regular file this Workspace creates is followed while every byte it
//! gains is an append at its current end: the BLAKE3 state of its bytes so
//! far is kept in the open state, about 2 KiB and no byte. Anything else
//! that could change its bytes ends the following for good: a write at
//! another offset, a cell change, a size that is not the followed length,
//! a second link or a removal. When the file has no open descriptor, the
//! digest is written to `fs_digest`, pinned to the content sequence of the
//! row that holds those bytes. A later job that changes no byte (a mode or
//! time change at the same size) moves the pin to its row; every other row
//! write leaves it behind, so a digest is only read for the row it describes.
use crate::{
    db::integer, CapturedReader, Changes, InodeKind, Overlay, OverlayResult, StatementKind,
    WorkspaceState,
};

/// A captured file's whole-content digest and the length it describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapturedDigest {
    pub digest: [u8; 32],
    pub length: u64,
}

/// The followed bytes of one file: their BLAKE3 state and their length.
#[derive(Clone)]
pub(crate) struct Digesting {
    hasher: blake3::Hasher,
    length: u64,
}

const PUT: &str = "INSERT INTO fs_digest(inode,sequence,length,digest) VALUES(?1,?2,?3,?4)
    ON CONFLICT(inode) DO UPDATE SET sequence=excluded.sequence,length=excluded.length,
    digest=excluded.digest";
const LATEST: &str = "SELECT sequence,size FROM fs_inode WHERE inode=?1 AND generation>0
    ORDER BY generation DESC LIMIT 1";
const EMPTY: &str = "SELECT 1 FROM fs_digest WHERE inode=?1 AND sequence=?2 AND length=0";
const PINNED: &str = "SELECT 1 FROM fs_digest WHERE inode=?1 AND sequence=?2 AND length=?3";
const REPIN: &str = "UPDATE fs_digest SET sequence=?2 WHERE inode=?1";
const CAPTURED: &str = "SELECT d.digest,d.length FROM fs_inode i JOIN fs_digest d
    ON d.inode=i.inode AND d.sequence=i.sequence AND d.length=i.size
    WHERE i.inode=?1 AND i.generation=?2";

impl Overlay {
    /// Before a job writes its rows: a write to an unfollowed file whose
    /// latest row still holds exactly the bytes last digested follows it
    /// again. Two cases need no unbounded kept state: the one-slot last
    /// file finished without a descriptor (a chunked SDK write), and an
    /// empty file still the row it was created as (create, then write).
    pub(crate) fn resume_digest(&self, changes: &Changes) -> OverlayResult<()> {
        let Some(write) = changes.write.as_ref() else {
            return Ok(());
        };
        let serial = integer(write.serial)?;
        if self.open.borrow().digests.rows.contains_key(&serial) {
            return Ok(());
        }
        let latest = self
            .query(StatementKind::Inode, LATEST, &[&serial], 16, |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?
            .pop();
        let Some((sequence, size)) = latest else {
            return Ok(());
        };
        let slot = self.open.borrow().resumable.rows.get(&()).cloned();
        let resumed = match slot {
            Some((held, at, followed))
                if held == serial
                    && at == sequence
                    && i64::try_from(followed.length) == Ok(size)
                    && followed.length == write.offset =>
            {
                Some(followed)
            }
            _ if size == 0 && write.offset == 0 => {
                let empty = self.query(
                    StatementKind::Inode,
                    EMPTY,
                    &[&serial, &sequence],
                    8,
                    |_| Ok(()),
                )?;
                (!empty.is_empty()).then(|| Digesting {
                    hasher: blake3::Hasher::new(),
                    length: 0,
                })
            }
            _ => None,
        };
        if let Some(followed) = resumed {
            self.open.borrow_mut().digests.put(serial, followed);
        }
        Ok(())
    }

    /// Before a job writes its rows: the files whose digest still describes
    /// their latest row and that this job changes without touching a byte
    /// (no write, no cell, the same size: a mode or time change). Their
    /// digest moves to the job's row afterwards.
    pub(crate) fn carried_digests(&self, changes: &Changes) -> OverlayResult<Vec<i64>> {
        if changes.write.is_some() || changes.cell.is_some() {
            return Ok(Vec::new());
        }
        let mut carried = Vec::new();
        for inode in &changes.inodes {
            if inode.kind != InodeKind::File || inode.nlink != 1 {
                continue;
            }
            let serial = integer(inode.serial)?;
            if self.open.borrow().digests.rows.contains_key(&serial) {
                continue;
            }
            let latest = self
                .query(StatementKind::Inode, LATEST, &[&serial], 16, |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?
                .pop();
            let Some((sequence, size)) = latest else {
                continue;
            };
            if Ok(size) != i64::try_from(inode.size) {
                continue;
            }
            let pinned = self.query(
                StatementKind::Inode,
                PINNED,
                &[&serial, &sequence, &size],
                24,
                |_| Ok(()),
            )?;
            if !pinned.is_empty() {
                carried.push(serial);
            }
        }
        Ok(carried)
    }

    /// Follows the files one compound job changes, after its rows and bytes
    /// are written in the same transaction; the open state is journaled
    /// with it. `state` is the row the job read: its rows carry the next
    /// revision as their content sequence.
    pub(crate) fn follow_digests(
        &self,
        state: &WorkspaceState,
        changes: &Changes,
        carried: &[i64],
    ) -> OverlayResult<()> {
        {
            let mut open = self.open.borrow_mut();
            if let Some(serial) = changes.created {
                let fresh = changes.inodes.iter().any(|inode| {
                    inode.serial == serial && inode.kind == InodeKind::File && inode.size == 0
                });
                if fresh {
                    open.digests.put(
                        integer(serial)?,
                        Digesting {
                            hasher: blake3::Hasher::new(),
                            length: 0,
                        },
                    );
                }
            }
            if let Some((serial, _)) = &changes.cell {
                open.digests.remove(integer(*serial)?);
            }
            if let Some(write) = &changes.write {
                let key = integer(write.serial)?;
                match open.digests.rows.get(&key) {
                    Some(followed) if followed.length == write.offset => {
                        let mut next = followed.clone();
                        next.hasher.update(&write.data);
                        next.length += write.data.len() as u64;
                        open.digests.put(key, next);
                    }
                    Some(_) => open.digests.remove(key),
                    None => {}
                }
            }
            for inode in &changes.inodes {
                let key = integer(inode.serial)?;
                let kept = open.digests.rows.get(&key).is_some_and(|followed| {
                    inode.kind == InodeKind::File
                        && inode.nlink == 1
                        && inode.size == followed.length
                });
                if !kept {
                    open.digests.remove(key);
                }
            }
        }
        let sequence = state
            .revision
            .checked_add(1)
            .ok_or(crate::OverlayError::Invalid("revision exhausted"))?;
        for serial in carried {
            self.execute(StatementKind::Inode, REPIN, &[serial, &sequence], 16)?;
            let mut open = self.open.borrow_mut();
            if let Some((held, _, followed)) = open.resumable.rows.get(&()).cloned() {
                if held == *serial {
                    open.resumable.put((), (held, sequence, followed));
                }
            }
        }
        // A file no descriptor holds is complete now.
        for inode in &changes.inodes {
            let key = integer(inode.serial)?;
            if self.file_opens(key) == 0 {
                self.finish_digest(key, sequence)?;
            }
        }
        Ok(())
    }

    /// Open descriptors of one serial.
    fn file_opens(&self, serial: i64) -> u64 {
        self.open
            .borrow()
            .custody
            .rows
            .get(&serial)
            .map_or(0, |refs| refs.opens)
    }

    /// The last descriptor of `serial` closed: its followed digest, if any,
    /// describes its latest row.
    pub(crate) fn digest_closed(&self, serial: i64) -> OverlayResult<()> {
        if !self.open.borrow().digests.rows.contains_key(&serial) {
            return Ok(());
        }
        let latest = self
            .query(StatementKind::Inode, LATEST, &[&serial], 16, |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?
            .pop();
        match latest {
            Some((sequence, size)) if sequence > 0 && Some(size) == self.followed(serial) => {
                self.finish_digest(serial, sequence)
            }
            _ => {
                self.open.borrow_mut().digests.remove(serial);
                Ok(())
            }
        }
    }

    fn followed(&self, serial: i64) -> Option<i64> {
        let open = self.open.borrow();
        let length = open.digests.rows.get(&serial)?.length;
        i64::try_from(length).ok()
    }

    /// Writes the followed digest of `serial` for the row with content
    /// sequence `sequence`, and stops following it.
    fn finish_digest(&self, serial: i64, sequence: i64) -> OverlayResult<()> {
        let Some(followed) = self.open.borrow().digests.rows.get(&serial).cloned() else {
            return Ok(());
        };
        {
            let mut open = self.open.borrow_mut();
            open.digests.remove(serial);
            open.resumable.put((), (serial, sequence, followed.clone()));
        }
        let digest = followed.hasher.finalize();
        self.execute(
            StatementKind::Inode,
            PUT,
            &[
                &serial,
                &sequence,
                &integer(followed.length)?,
                &digest.as_bytes().as_slice(),
            ],
            64,
        )?;
        Ok(())
    }

    /// The whole-content digest of each serial's captured row, when its
    /// bytes were written as appends by descriptors all closed before any
    /// later change and none is open now: BLAKE3 of exactly the captured
    /// bytes, with the row's length. At most one captured page of
    /// serials per call.
    pub fn captured_digests(
        &self,
        reader: CapturedReader,
        serials: &[u64],
    ) -> OverlayResult<Vec<Option<CapturedDigest>>> {
        self.check_captured_reader(reader)?;
        if serials.len() > crate::PAGE_ROWS {
            return Err(crate::OverlayError::Invalid("captured digest window"));
        }
        let generation = reader.capture.generation.0;
        serials
            .iter()
            .map(|serial| {
                // A file open now may still change: no digest describes it.
                if self.file_opens(integer(*serial)?) != 0 {
                    return Ok(None);
                }
                let rows = self.query(
                    StatementKind::Capture,
                    CAPTURED,
                    &[&integer(*serial)?, &generation],
                    16,
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
                )?;
                Ok(rows.into_iter().next().and_then(|(digest, length)| {
                    Some(CapturedDigest {
                        digest: <[u8; 32]>::try_from(digest).ok()?,
                        length: u64::try_from(length).ok()?,
                    })
                }))
            })
            .collect()
    }
}
