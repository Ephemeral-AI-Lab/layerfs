//! Exact open and read-processing owners; no descriptor pins a generation chain.
use crate::{
    db::integer, inode, sql, BaseSource, FileRead, Inode, InodeKind, LeaseKind, OpenFile, Overlay,
    OverlayError, OverlayResult, Route, StatementKind,
};

impl Overlay {
    /// Mints one owner identity of this engine, unique across its namespaces.
    /// A job that rolls back does not return its identities.
    pub(crate) fn mint_owner(&self) -> OverlayResult<u64> {
        let next = self.next_owner.get();
        // Identities are stored as SQLite signed integers.
        let after = next
            .checked_add(1)
            .filter(|after| *after <= i64::MAX as u64)
            .ok_or(OverlayError::Invalid("owner identity exhausted"))?;
        self.next_owner.set(after);
        Ok(next)
    }
    pub(crate) fn file_ref(
        &self,
        ns: i64,
        serial: i64,
        kind: LeaseKind,
        add: bool,
    ) -> OverlayResult<()> {
        let (increment, decrement) = match kind {
            LeaseKind::Open | LeaseKind::FileHandle => (sql::FILE_OPENS_ADD, sql::FILE_OPENS_DROP),
            LeaseKind::Lookup | LeaseKind::LookupOwner => {
                (sql::FILE_LOOKUPS_ADD, sql::FILE_LOOKUPS_DROP)
            }
            LeaseKind::FileReader => (sql::FILE_READERS_ADD, sql::FILE_READERS_DROP),
            _ => return Ok(()),
        };
        if add {
            self.execute(StatementKind::Lease, increment, &[&ns, &serial], 16)?;
        } else {
            let changed = self.execute(StatementKind::Lease, decrement, &[&ns, &serial], 16)?;
            if changed != 1 {
                return Err(OverlayError::Stale);
            }
            if self.file_refs(ns, serial)? == 0 {
                self.enqueue(ns, crate::maintenance::ORPHAN, serial, -1)?;
                self.wake_orphan(ns, serial)?;
            }
        }
        Ok(())
    }
    pub(crate) fn file_refs(&self, ns: i64, serial: i64) -> OverlayResult<u64> {
        Ok(self
            .query(
                StatementKind::Lease,
                crate::sql::FILE_REFS,
                &[&ns, &serial],
                16,
                |r| crate::db::unsigned(r, 0),
            )?
            .pop()
            .unwrap_or(0))
    }
    /// Opens a current regular inode. `base` is an authenticated immutable fact
    /// supplied outside SQL under the still-owned source; local rows decide.
    /// Request IDs permit observation after lost replies, never automatic replay.
    pub fn open_file(
        &self,
        source: BaseSource,
        request: u64,
        base: &Inode,
        writable: bool,
    ) -> OverlayResult<OpenFile> {
        inode::check(base)?;
        let request = integer(request)?;
        if request == 0 {
            return Err(OverlayError::Invalid("zero open request"));
        }
        self.atomic(|| {
            let state = self.source_state(source)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            let current = self
                .inode_at(source.route, base.serial, state.active, state.installed)?
                .unwrap_or_else(|| base.clone());
            if current.kind != InodeKind::File || current.nlink == 0 {
                return Err(OverlayError::Missing);
            }
            self.retain_file(source.route, request, current.serial, writable)
        })
    }
    /// The enclosing owner transaction has already validated current metadata.
    pub(crate) fn retain_file(
        &self,
        route: Route,
        request: i64,
        serial: u64,
        writable: bool,
    ) -> OverlayResult<OpenFile> {
        let owner = self.mint_owner()?;
        let serial = integer(serial)?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO file_handle VALUES(?1,?2,?3,?4,?5)",
            &[&route.ns, &request, &integer(owner)?, &serial, &writable],
            33,
        )?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO lease VALUES(?1,7,?2,?3)",
            &[&route.ns, &integer(owner)?, &serial],
            24,
        )?;
        self.file_ref(route.ns, serial, LeaseKind::FileHandle, true)?;
        Ok(OpenFile {
            route,
            owner,
            serial: serial as u64,
            writable,
        })
    }

    /// Original open custody, queried by its caller-known request identity.
    pub fn retained_file(&self, route: Route, request: u64) -> OverlayResult<Option<OpenFile>> {
        self.state(route)?;
        self.query(
            StatementKind::Lease,
            "SELECT owner,serial,writable FROM file_handle WHERE ns=?1 AND request=?2",
            &[&route.ns, &integer(request)?],
            16,
            |r| {
                Ok(OpenFile {
                    route,
                    owner: crate::db::unsigned(r, 0)?,
                    serial: crate::db::unsigned(r, 1)?,
                    writable: r.get(2)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    /// Validates actual descriptor custody; read-only descriptors cannot write.
    pub fn check_file(&self, file: OpenFile, write: bool) -> OverlayResult<()> {
        self.state(file.route)?;
        let found = self
            .query(
                StatementKind::Lease,
                "SELECT writable FROM file_handle
            WHERE ns=?1 AND owner=?2 AND serial=?3",
                &[
                    &file.route.ns,
                    &integer(file.owner)?,
                    &integer(file.serial)?,
                ],
                24,
                |r| r.get::<_, bool>(0),
            )?
            .pop();
        if found != Some(file.writable) {
            return Err(OverlayError::Stale);
        }
        if write && !file.writable {
            return Err(OverlayError::Invalid("read-only descriptor"));
        }
        Ok(())
    }
    /// Exact close after this descriptor's native/request continuations finish.
    /// Read-processing windows keep their own independent references.
    pub fn close_file(&self, file: OpenFile) -> OverlayResult<()> {
        self.atomic_cleanup(|| self.close_file_inner(file))
    }
    pub(crate) fn close_file_inner(&self, file: OpenFile) -> OverlayResult<()> {
        self.check_file(file, false)?;
        self.execute(
            StatementKind::Lease,
            "DELETE FROM file_handle WHERE ns=?1 AND owner=?2 AND serial=?3",
            &[
                &file.route.ns,
                &integer(file.owner)?,
                &integer(file.serial)?,
            ],
            24,
        )?;
        self.execute(
            StatementKind::Lease,
            "DELETE FROM lease WHERE ns=?1 AND kind=7 AND owner=?2 AND resource=?3",
            &[
                &file.route.ns,
                &integer(file.owner)?,
                &integer(file.serial)?,
            ],
            24,
        )?;
        self.file_ref(
            file.route.ns,
            integer(file.serial)?,
            LeaseKind::FileHandle,
            false,
        )?;
        self.queue_closed(file.route)
    }

    /// Acquires an independently owned read window under a current source.
    /// Its engine-owned source domain cannot collide with caller source IDs.
    pub fn acquire_file_read(
        &self,
        source: BaseSource,
        file: OpenFile,
        request: u64,
    ) -> OverlayResult<FileRead> {
        let request = integer(request)?;
        if request == 0 {
            return Err(OverlayError::Invalid("zero read request"));
        }
        self.atomic(|| {
            let state = self.source_state(source)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            if file.route != source.route {
                return Err(OverlayError::Stale);
            }
            self.check_file(file, false)?;
            self.retain_serial_read(source, file.serial, request)
        })
    }
    pub(crate) fn retain_serial_read(
        &self,
        source: BaseSource,
        serial: u64,
        request: i64,
    ) -> OverlayResult<FileRead> {
        let owner = self.mint_owner()?;
        let serial = integer(serial)?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO file_read VALUES(?1,?2,?3,?4)",
            &[&source.route.ns, &request, &integer(owner)?, &serial],
            32,
        )?;
        self.execute(
            StatementKind::Lease,
            sql::BASE_SOURCE_INSERT,
            &[
                &source.route.ns,
                &integer(owner)?,
                &source.root.as_slice(),
                &1_i64,
            ],
            56,
        )?;
        self.execute(
            StatementKind::Workspace,
            sql::BASE_SOURCE_INCREMENT,
            &[&source.route.ns],
            8,
        )?;
        self.execute(
            StatementKind::Lease,
            "INSERT INTO lease VALUES(?1,5,?2,?3)",
            &[&source.route.ns, &integer(owner)?, &serial],
            24,
        )?;
        self.file_ref(source.route.ns, serial, LeaseKind::FileReader, true)?;
        Ok(FileRead {
            source: BaseSource {
                owner,
                class: 1,
                ..source
            },
            serial: serial as u64,
        })
    }
    pub fn retained_file_read(
        &self,
        route: Route,
        request: u64,
    ) -> OverlayResult<Option<FileRead>> {
        let state = self.state(route)?;
        self.query(
            StatementKind::Lease,
            "SELECT r.owner,r.serial,s.base_root FROM file_read r JOIN base_source s
            ON s.ns=r.ns AND s.kind=1 AND s.owner=r.owner WHERE r.ns=?1 AND r.request=?2",
            &[&route.ns, &integer(request)?],
            16,
            |r| {
                let root: Vec<u8> = r.get(2)?;
                Ok(FileRead {
                    source: BaseSource {
                        route,
                        owner: crate::db::unsigned(r, 0)?,
                        class: 1,
                        root: root.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?,
                        installed: state.installed,
                    },
                    serial: crate::db::unsigned(r, 1)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub(crate) fn check_file_read(&self, read: FileRead) -> OverlayResult<()> {
        self.file_read_request(read).map(|_| ())
    }
    fn file_read_request(&self, read: FileRead) -> OverlayResult<i64> {
        self.source_state(read.source)?;
        self.query(
            StatementKind::Lease,
            "SELECT request FROM file_read WHERE ns=?1 AND owner=?2 AND serial=?3",
            &[
                &read.source.route.ns,
                &integer(read.source.owner)?,
                &integer(read.serial)?,
            ],
            24,
            |r| r.get(0),
        )?
        .pop()
        .ok_or(OverlayError::Stale)
    }
    /// Releases only this read window after the last consumer/base demand is fenced.
    pub fn release_file_read(&self, read: FileRead) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            let native = self.file_read_request(read)? < 0;
            let ns = read.source.route.ns;
            let owner = integer(read.source.owner)?;
            let serial = integer(read.serial)?;
            if native {
                self.execute(
                    StatementKind::Lease,
                    "DELETE FROM native_read WHERE ns=?1 AND owner=?2",
                    &[&ns, &owner],
                    16,
                )?;
            }
            self.execute(
                StatementKind::Lease,
                "DELETE FROM file_read WHERE ns=?1 AND owner=?2",
                &[&ns, &owner],
                16,
            )?;
            self.execute(
                StatementKind::Lease,
                "DELETE FROM lease WHERE ns=?1 AND kind=5 AND owner=?2 AND resource=?3",
                &[&ns, &owner, &serial],
                24,
            )?;
            self.file_ref(ns, serial, LeaseKind::FileReader, false)?;
            self.release_source_inner(read.source)
        })
    }
}
