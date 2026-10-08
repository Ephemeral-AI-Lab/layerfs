//! Bounded cookie allocation and publication of only accepted reply entries.
use crate::{
    db::{integer, unsigned},
    NativeCookie, NativeCookiePlan, NativeDirectoryRead, Overlay, OverlayError, OverlayResult,
    StatementKind, PAGE_ROWS,
};
impl Overlay {
    /// One bounded proposal for an original directory-read request. Existing
    /// name mappings are reused; new IDs reserve a disjoint allocator range.
    /// Reservation creates no valid offset and changes no enumeration cursor.
    pub fn prepare_native_cookies(
        &self,
        read: &NativeDirectoryRead,
        names: &[Vec<u8>],
    ) -> OverlayResult<NativeCookiePlan> {
        if names.len() > PAGE_ROWS
            || names.windows(2).any(|p| p[0] >= p[1])
            || names.iter().any(|name| {
                name.is_empty()
                    || name.len() > 255
                    || name.as_slice() == b"."
                    || name.as_slice() == b".."
                    || name.contains(&0)
                    || name.contains(&b'/')
                    || read
                        .cursor
                        .after_name()
                        .is_some_and(|after| name.as_slice() <= after)
            })
        {
            return Err(OverlayError::Invalid("native cookie names"));
        }
        self.atomic(|| {
            self.check_native_directory_read(read)?;
            let ns = read.source.route.ns;
            let owner = integer(read.source.owner)?;
            let directory = integer(read.directory.owner)?;
            let fresh = self.query(StatementKind::Lease,
                "SELECT 1 FROM native_directory_read WHERE ns=?1 AND owner=?2 AND first_cookie IS NULL",
                &[&ns, &owner], 16, |_| Ok(()))?;
            if fresh.is_empty() { return Err(OverlayError::Stale); }
            let first = self.query(StatementKind::Lease,
                "SELECT next_cookie FROM native_directory WHERE ns=?1 AND owner=?2",
                &[&ns, &directory], 16, |r| unsigned(r, 0))?.pop().ok_or(OverlayError::Stale)?;
            let mut next = first;
            let mut entries = Vec::with_capacity(names.len());
            for name in names {
                let existing = self.query(StatementKind::Lease,
                    "SELECT cookie FROM native_cookie WHERE ns=?1 AND owner=?2 AND name=?3 ORDER BY cookie LIMIT 1",
                    &[&ns, &directory, &name.as_slice()], 16+name.len() as u64, |r| unsigned(r, 0))?.pop();
                let cookie = match existing {
                    Some(cookie) => cookie,
                    None => { let cookie=next; next=next.checked_add(1).ok_or(OverlayError::Invalid("directory cookie exhausted"))?; integer(next)?; cookie }
                };
                entries.push(NativeCookie { name: name.clone(), cookie, existing: existing.is_some() });
            }
            if next!=first {
                self.execute(StatementKind::Lease, "UPDATE native_directory SET next_cookie=?3 WHERE ns=?1 AND owner=?2",
                    &[&ns, &directory, &integer(next)?], 24)?;
            }
            self.execute(StatementKind::Lease,
                "UPDATE native_directory_read SET first_cookie=?3,end_cookie=?4 WHERE ns=?1 AND owner=?2",
                &[&ns, &owner, &integer(first)?, &integer(next)?], 32)?;
            Ok(NativeCookiePlan { read: read.clone(), first, end: next, entries })
        })
    }
    /// Publish the exact prefix accepted by the bounded native reply buffer,
    /// before its send attempt. An unavailable send outcome keeps these mappings.
    /// A failed/unknown publication retains the original plan; never replay it.
    pub fn publish_native_cookies(
        &self,
        plan: &NativeCookiePlan,
        accepted: usize,
    ) -> OverlayResult<()> {
        if accepted > plan.entries.len() {
            return Err(OverlayError::Invalid("native cookie accepted prefix"));
        }
        self.atomic(|| {
            self.check_native_directory_read(&plan.read)?;
            let ns=plan.read.source.route.ns;
            let source=integer(plan.read.source.owner)?;
            let valid=self.query(StatementKind::Lease,
                "SELECT 1 FROM native_directory_read WHERE ns=?1 AND owner=?2 AND first_cookie=?3 AND end_cookie=?4 AND published=0",
                &[&ns, &source, &integer(plan.first)?, &integer(plan.end)?], 32, |_| Ok(()))?;
            if valid.is_empty() { return Err(OverlayError::Stale); }
            for entry in &plan.entries[..accepted] {
                if !entry.existing {
                    self.execute(StatementKind::Lease, "INSERT INTO native_cookie VALUES(?1,?2,?3,?4)",
                        &[&ns, &integer(plan.read.directory.owner)?, &integer(entry.cookie)?, &entry.name.as_slice()], 24+entry.name.len() as u64)?;
                }
            }
            self.execute(StatementKind::Lease, "UPDATE native_directory_read SET published=1 WHERE ns=?1 AND owner=?2",
                &[&ns, &source], 16)?;
            Ok(())
        })
    }
    pub fn explain_native_directory(
        &self,
        directory: crate::NativeDirectory,
    ) -> OverlayResult<Vec<String>> {
        self.state(directory.mount.route)?;
        let mut plans = Vec::new();
        for (label, sql) in [
            ("cookie-offset", "SELECT name FROM native_cookie WHERE ns=?1 AND owner=?2 AND cookie=?3"),
            ("cookie-window", "SELECT cookie FROM native_cookie WHERE ns=?1 AND owner=?2 AND cookie>?3 ORDER BY cookie LIMIT 64"),
            ("read-fence", "SELECT 1 FROM native_directory_read WHERE ns=?1 AND directory=?2 AND owner>?3 LIMIT 1"),
        ] {
            plans.extend(self.query(StatementKind::Explain, &format!("EXPLAIN QUERY PLAN {sql}"),
                &[&directory.mount.route.ns, &integer(directory.owner)?, &0_i64], 24, |r| Ok(format!("{label}: {}",r.get::<_,String>(3)?)))?);
        }
        plans.extend(self.query(StatementKind::Explain,
            "EXPLAIN QUERY PLAN SELECT cookie FROM native_cookie WHERE ns=?1 AND owner=?2 AND name=?3 ORDER BY cookie LIMIT 1",
            &[&directory.mount.route.ns, &integer(directory.owner)?, &b"x".as_slice()], 17, |r| Ok(format!("cookie-name: {}",r.get::<_,String>(3)?)))?);
        plans.extend(self.query(StatementKind::Explain,
            "EXPLAIN QUERY PLAN SELECT owner FROM native_directory WHERE ns=?1 AND mount=?2 AND closed=0 LIMIT 1",
            &[&directory.mount.route.ns, &integer(directory.mount.owner)?], 16,
            |r| Ok(format!("open-fence: {}",r.get::<_,String>(3)?)))?);
        Ok(plans)
    }
}
