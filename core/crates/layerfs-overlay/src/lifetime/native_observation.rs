//! Atomic native answer, kernel lookup increment and independent read ownership.
use crate::{
    db::{integer, unsigned},
    inode, BaseSource, FileRead, InodeKind, NativeDecision, NativeMount, NativeObservation,
    Overlay, OverlayError, OverlayResult, SourceRows, StatementKind,
};
enum NativeOpen {
    File(bool),
    Directory,
}
impl Overlay {
    /// The callback is one bounded semantic decision over current rows and
    /// previously acquired immutable facts. It must perform no provider I/O.
    /// Its original value survives even a later transaction-completion failure.
    pub fn observe_native<T>(
        &self,
        mount: NativeMount,
        source: BaseSource,
        lookup: bool,
        decide: impl FnOnce(SourceRows<'_>, u64) -> OverlayResult<NativeDecision<T>>,
    ) -> NativeObservation<T> {
        self.observe_native_inner(mount, source, lookup, None, decide)
    }
    /// Like a native stat observation, but retains a regular OpenFile in the
    /// deciding transaction. A removed file stays openable: the source's
    /// protecting kernel reference already retains its inode and content.
    pub fn observe_native_open<T>(
        &self,
        mount: NativeMount,
        source: BaseSource,
        writable: bool,
        decide: impl FnOnce(SourceRows<'_>, u64) -> OverlayResult<NativeDecision<T>>,
    ) -> NativeObservation<T> {
        self.observe_native_inner(
            mount,
            source,
            false,
            Some(NativeOpen::File(writable)),
            decide,
        )
    }
    /// Current directory decision and its descriptor/read ownership are atomic.
    pub fn observe_native_directory<T>(
        &self,
        mount: NativeMount,
        source: BaseSource,
        decide: impl FnOnce(SourceRows<'_>, u64) -> OverlayResult<NativeDecision<T>>,
    ) -> NativeObservation<T> {
        self.observe_native_inner(mount, source, false, Some(NativeOpen::Directory), decide)
    }
    fn observe_native_inner<T>(
        &self,
        mount: NativeMount,
        source: BaseSource,
        lookup: bool,
        open: Option<NativeOpen>,
        decide: impl FnOnce(SourceRows<'_>, u64) -> OverlayResult<NativeDecision<T>>,
    ) -> NativeObservation<T> {
        let mut decision = None;
        let mut acquired = None;
        let mut open_candidate = None;
        let mut directory_candidate = None;
        let result = self
            .atomic(|| {
                let (request, protected) = self.check_native_source(mount, source)?;
                let exists = self.query(
                    StatementKind::Lease,
                    "SELECT 1 FROM native_read WHERE ns=?1 AND mount=?2 AND request=?3",
                    &[&mount.route.ns, &integer(mount.owner)?, &request.as_slice()],
                    24,
                    |_| Ok(()),
                )?;
                if !exists.is_empty() {
                    return Err(OverlayError::Stale);
                }
                let (inode, value, finished) = match decide(self.source_rows(source)?, protected)? {
                    NativeDecision::Needs(value) => (None, value, false),
                    NativeDecision::Finished { inode, value } => (inode, value, true),
                };
                decision = Some(value);
                if !finished {
                    return Ok(());
                }
                self.execute(
                    StatementKind::Lease,
                    "UPDATE native_source SET decided=1 WHERE ns=?1 AND owner=?2",
                    &[&mount.route.ns, &integer(source.owner)?],
                    16,
                )?;
                let Some(inode) = inode else { return Ok(()) };
                inode::check(&inode)?;
                if lookup {
                    if inode.nlink == 0 && inode.serial != mount.root {
                        return Err(OverlayError::Missing);
                    }
                    self.add_native_lookup(mount, inode.serial)?;
                    if inode.kind == InodeKind::Directory {
                        self.set_native_parent(mount, inode.serial, protected)?;
                    }
                } else if inode.serial != protected {
                    return Err(OverlayError::Invalid("native observation serial"));
                }
                if let Some(NativeOpen::File(writable)) = open {
                    if inode.kind != InodeKind::File {
                        return Err(OverlayError::Missing);
                    }
                    let file = self.retain_file(
                        mount.route,
                        -integer(source.owner)?,
                        inode.serial,
                        writable,
                    )?;
                    open_candidate = Some(file);
                    self.execute(
                        StatementKind::Lease,
                        "INSERT INTO native_file VALUES(?1,?2,?3,?4)",
                        &[
                            &mount.route.ns,
                            &integer(mount.owner)?,
                            &request.as_slice(),
                            &integer(file.owner)?,
                        ],
                        32,
                    )?;
                }
                if matches!(open, Some(NativeOpen::Directory)) {
                    if inode.kind != InodeKind::Directory
                        || (inode.nlink == 0 && inode.serial != mount.root)
                    {
                        return Err(OverlayError::Missing);
                    }
                    directory_candidate =
                        Some(self.retain_native_directory(mount, &request, inode.serial)?);
                }
                // Negative keys reserve an internal request domain without colliding
                // with public positive FileRead request IDs. The owner is still the
                // existing independently minted FileRead/source/lease identity.
                let read =
                    self.retain_serial_read(source, inode.serial, -integer(source.owner)?)?;
                acquired = Some(read);
                self.execute(
                    StatementKind::Lease,
                    "INSERT INTO native_read VALUES(?1,?2,?3,?4)",
                    &[
                        &mount.route.ns,
                        &integer(mount.owner)?,
                        &request.as_slice(),
                        &integer(read.source.owner)?,
                    ],
                    32,
                )?;
                Ok(())
            })
            .map(|()| acquired);
        NativeObservation {
            decision,
            result,
            candidate: acquired,
            open_candidate,
            directory_candidate,
        }
    }
    /// Read-only observation of the original independent owner after loss.
    /// Finding it permits neither resending a reply nor replaying a lookup.
    pub fn retained_native_read(
        &self,
        mount: NativeMount,
        request: u64,
    ) -> OverlayResult<Option<FileRead>> {
        self.check_native_attached(mount)?;
        let state = self.state(mount.route)?;
        self.query(StatementKind::Lease,
            "SELECT r.owner,r.serial,b.base_root FROM native_read n JOIN file_read r ON r.ns=n.ns AND r.owner=n.owner JOIN base_source b ON b.ns=r.ns AND b.kind=1 AND b.owner=r.owner WHERE n.ns=?1 AND n.mount=?2 AND n.request=?3",
            &[&mount.route.ns, &integer(mount.owner)?, &request.to_be_bytes().as_slice()], 24,
            |r| Ok(FileRead { source: BaseSource { route: mount.route, owner: unsigned(r, 0)?, class: 1,
                root: r.get::<_, Vec<u8>>(2)?.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?, installed: state.installed }, serial: unsigned(r, 1)? }))
            .map(|mut rows| rows.pop())
    }
    /// Actual indexed query plans for the native ownership points and bounded
    /// revoked lookup cursor; pair with diagnostics for work/complexity claims.
    pub fn explain_native(&self, mount: NativeMount) -> OverlayResult<Vec<String>> {
        self.state(mount.route)?;
        let mut plans = Vec::new();
        for (name, sql) in [
            ("lookup-point", "SELECT owner,nlookup,implicit FROM native_lookup WHERE ns=?1 AND mount=?2 AND serial=?3"),
            ("source-owner", "SELECT request,serial FROM native_source WHERE ns=?1 AND mount=?2 AND owner=?3"),
            ("retire-window", "SELECT serial,owner FROM native_lookup WHERE ns=?1 AND mount=?2 AND serial>?3 ORDER BY serial LIMIT 64"),
        ] {
            plans.extend(self.query(StatementKind::Explain, &format!("EXPLAIN QUERY PLAN {sql}"),
                &[&mount.route.ns, &integer(mount.owner)?, &0_i64], 24,
                |r| Ok(format!("{name}: {}", r.get::<_, String>(3)?)))?);
        }
        for (name, sql) in [
            ("retire-files", crate::maintenance::NATIVE_FILE_WINDOW),
            (
                "retire-directories",
                crate::maintenance::NATIVE_DIRECTORY_WINDOW,
            ),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {sql}"),
                &[&mount.route.ns, &integer(mount.owner)?],
                16,
                |r| Ok(format!("{name}: {}", r.get::<_, String>(3)?)),
            )?);
        }
        Ok(plans)
    }
}
