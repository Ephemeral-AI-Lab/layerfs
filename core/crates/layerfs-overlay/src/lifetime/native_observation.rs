//! Atomic native answer and kernel lookup increment of a source-holding request.
use crate::{
    db::integer, inode, BaseSource, InodeKind, NativeDecision, NativeMount, NativeObservation,
    Overlay, OverlayError, OverlayResult, SourceRows, StatementKind,
};
impl Overlay {
    /// The deciding transaction of a source-holding LOOKUP or GETATTR: the
    /// answer and a positive lookup increment are atomic, and no read is
    /// retained because the reply carries attributes only. The callback is
    /// one bounded semantic decision over current rows and previously
    /// acquired immutable facts. It must perform no provider I/O. Its
    /// original value survives even a later transaction-completion failure.
    pub fn observe_native_attributes<T>(
        &self,
        mount: NativeMount,
        source: BaseSource,
        lookup: bool,
        decide: impl FnOnce(SourceRows<'_>, u64) -> OverlayResult<NativeDecision<T>>,
    ) -> NativeObservation<T> {
        let mut decision = None;
        let result = self
            .atomic(|| {
                let (request, protected, state) = self.check_native_source(mount, source)?;
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
                let (inode, value, finished) =
                    match decide(self.source_rows_at(source, state), protected)? {
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
                Ok(())
            })
            .map(|()| None);
        NativeObservation {
            decision,
            result,
            candidate: None,
            open_candidate: None,
            directory_candidate: None,
        }
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
