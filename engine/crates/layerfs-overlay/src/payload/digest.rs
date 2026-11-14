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
        // Prototype (write-path probe, `noresume`): both resumable cases need
        // the window to start where the followed bytes end, so a write that
        // starts elsewhere needs no row read.
        if probe_switch("noresume") && write.offset != 0 {
            let open = self.open.borrow();
            let possible = open
                .resumable
                .rows
                .get(&())
                .is_some_and(|(held, _, followed)| {
                    *held == serial && followed.length == write.offset
                });
            if !possible {
                return Ok(());
            }
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

    /// Prototype (write-path probe, `digestpar`): starts the BLAKE3 update
    /// of an appended window on the helper thread while the job writes its
    /// rows and commits. `follow_digests` advances the followed length and
    /// the transaction installs the hasher after COMMIT (or joins at once
    /// where the digest is finished in the job). Called after `resume_digest`.
    pub(crate) fn start_digest(&self, changes: &Changes) -> OverlayResult<()> {
        PENDING.with(|pending| pending.borrow_mut().take());
        if !probe_switch("digestpar") || changes.cell.is_some() {
            return Ok(());
        }
        // A small window hashes inline faster than a helper hand-off wakes
        // (about 1 µs per KiB against a 10 µs round trip).
        let Some(write) = changes
            .write
            .as_ref()
            .filter(|write| write.data.len() >= HELPER_BYTES)
        else {
            return Ok(());
        };
        let key = integer(write.serial)?;
        let hasher = match self.open.borrow().digests.rows.get(&key) {
            Some(followed) if followed.length == write.offset => followed.hasher.clone(),
            _ => return Ok(()),
        };
        let (reply, result) = std::sync::mpsc::channel();
        let check = probe_switch("digestcheck").then(|| (hasher.clone(), write.data.clone()));
        let job = (hasher, write.data.clone(), reply);
        let sent = HELPER
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .send(job)
            .is_ok();
        if sent {
            PENDING.with(|pending| {
                *pending.borrow_mut() = Some(Pending {
                    key,
                    offset: write.offset,
                    length: write.data.len() as u64,
                    result,
                    deferred: false,
                    check,
                })
            });
        }
        Ok(())
    }
    /// Installs a deferred hasher of this job (prototype `digestpar`).
    pub(crate) fn settle_digest(&self) {
        let Some(pending) = PENDING.with(|pending| pending.borrow_mut().take()) else {
            return;
        };
        if !pending.deferred {
            return;
        }
        let _stage = layerfs_telemetry::timer::stages::Timer::start(
            layerfs_telemetry::timer::stages::Stage::OverlayDigest,
        );
        let hasher = pending.receive();
        if let Some(followed) = self.open.borrow_mut().digests.rows.get_mut(&pending.key) {
            followed.hasher = hasher;
        }
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
            // Prototype (write-path probe, `noresume`): an inode this job
            // creates has a fresh serial, so no earlier row or pin exists.
            if probe_switch("noresume") && changes.created == Some(inode.serial) {
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
                let opens = open.custody.rows.get(&key).map_or(0, |refs| refs.opens);
                // The running hasher is followed in place: an append extends it
                // where it is, so a window never clones its state to advance it.
                if let Some(followed) = open.digests.rows.get_mut(&key) {
                    if followed.length == write.offset {
                        let started = PENDING.with(|pending| {
                            let mut pending = pending.borrow_mut();
                            let pending = pending.as_mut().filter(|pending| {
                                pending.key == key
                                    && pending.offset == write.offset
                                    && pending.length == write.data.len() as u64
                            })?;
                            // A file no descriptor holds finishes its digest
                            // in this job: join now.
                            if opens == 0 {
                                Some(Some(pending.receive()))
                            } else {
                                pending.deferred = true;
                                Some(None)
                            }
                        });
                        match started {
                            Some(Some(hasher)) => followed.hasher = hasher,
                            Some(None) => {}
                            None => {
                                let _stage = layerfs_telemetry::timer::stages::Timer::start(
                                    layerfs_telemetry::timer::stages::Stage::OverlayDigest,
                                );
                                followed.hasher.update(&write.data);
                            }
                        }
                        followed.length += write.data.len() as u64;
                    } else {
                        open.digests.remove(key);
                    }
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

/// Prototype (write-path probe): `LAYERFS_PROBE_WRITE`, a comma list.
pub(crate) fn probe_switch(name: &str) -> bool {
    static SWITCHES: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| std::env::var("LAYERFS_PROBE_WRITE").unwrap_or_default());
    SWITCHES.split(',').any(|switch| switch == name)
}
/// The smallest window `digestpar` hands to the helper (prototype).
const HELPER_BYTES: usize = 32 * 1024;
/// A hash started by `start_digest` for one appended window.
struct Pending {
    key: i64,
    offset: u64,
    length: u64,
    result: std::sync::mpsc::Receiver<blake3::Hasher>,
    /// `follow_digests` advanced the length; the hasher waits for COMMIT.
    deferred: bool,
    /// `digestcheck`: the hasher before the window and the window, to
    /// compare the helper's result with an inline update.
    check: Option<(blake3::Hasher, std::sync::Arc<[u8]>)>,
}
impl Pending {
    fn receive(&self) -> blake3::Hasher {
        let hasher = self.result.recv().expect("digest helper");
        if let Some((before, data)) = &self.check {
            use layerfs_telemetry::timer::stages::{add, Stage};
            let mut inline = before.clone();
            inline.update(data);
            let same = inline.finalize() == hasher.finalize();
            add(
                if same {
                    Stage::DigestCheckSame
                } else {
                    Stage::DigestCheckDiffer
                },
                1,
                0,
            );
        }
        hasher
    }
}
thread_local! {
    static PENDING: std::cell::RefCell<Option<Pending>> = const { std::cell::RefCell::new(None) };
}
type HashJob = (
    blake3::Hasher,
    std::sync::Arc<[u8]>,
    std::sync::mpsc::Sender<blake3::Hasher>,
);
/// One process-wide helper thread that extends hashers (prototype).
static HELPER: std::sync::LazyLock<std::sync::Mutex<std::sync::mpsc::Sender<HashJob>>> =
    std::sync::LazyLock::new(|| {
        let (jobs, queue) = std::sync::mpsc::channel::<HashJob>();
        std::thread::Builder::new()
            .name("layerfs-digest".into())
            .spawn(move || {
                for (mut hasher, data, reply) in queue {
                    let _stage = layerfs_telemetry::timer::stages::Timer::start(
                        layerfs_telemetry::timer::stages::Stage::OverlayDigestHelper,
                    );
                    hasher.update(&data);
                    let _ = reply.send(hasher);
                }
            })
            .expect("digest helper thread");
        std::sync::Mutex::new(jobs)
    });
