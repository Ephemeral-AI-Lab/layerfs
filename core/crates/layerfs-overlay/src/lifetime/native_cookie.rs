//! The cookies of published READDIR replies: one row per reply, holding
//! exactly the names its buffer accepted.
use super::native_directory_read::decode_names;
use crate::{
    db::{integer, unsigned},
    lifetime::native_visit::Held,
    sql, NativeCookieOffer, NativeDirectoryPage, Overlay, OverlayError, OverlayResult,
    StatementKind, PAGE_ROWS,
};
impl Overlay {
    /// Inside the reading visit's job, for a window that may list a name:
    /// the latest published reply of this open directory listed after the
    /// same name, and a fresh range of `PAGE_ROWS` numbers from the engine's
    /// owner counter. One read; nothing is written, and neither makes an
    /// offset valid.
    pub fn offer_native_cookies(
        &self,
        page: &NativeDirectoryPage,
    ) -> OverlayResult<NativeCookieOffer> {
        let directory = page.directory;
        let after = page.cursor.after_name().unwrap_or(&[]).to_vec();
        let existing = self
            .query(
                StatementKind::Lease,
                sql::COOKIE_PAGE_AFTER,
                &[
                    &directory.mount.route.ns,
                    &integer(directory.owner)?,
                    &after.as_slice(),
                ],
                24 + after.len() as u64,
                |r| Ok((unsigned(r, 0)?, r.get::<_, Vec<u8>>(1)?)),
            )?
            .pop()
            .map(|(first, names)| {
                decode_names(&names)
                    .map(|names| (first, names))
                    .ok_or(OverlayError::Invalid("cookie names"))
            })
            .transpose()?;
        let first = self.mint_owners(PAGE_ROWS as u64)?;
        if first < 3 {
            return Err(OverlayError::Invalid("directory cookie range"));
        }
        Ok(NativeCookieOffer {
            directory,
            after,
            first,
            existing,
        })
    }
    /// READDIR's publishing visit, before the reply is sent: the names the
    /// reply buffer accepted become the offsets `first..first+names.len()`
    /// of the offer's fresh range, as one row, in one transaction behind the
    /// fence on the still open descriptor. A closed or released handle, a
    /// revoked mount and a closed Workspace publish nothing. An unavailable
    /// send outcome keeps the row; a second publication of one offer fails
    /// whole on the row's key. Never replay it.
    pub fn publish_native_cookies(
        &self,
        offer: &NativeCookieOffer,
        names: &[Vec<u8>],
    ) -> OverlayResult<()> {
        let mut blob = Vec::with_capacity(names.iter().map(|name| 1 + name.len()).sum());
        let mut last = offer.after.as_slice();
        for name in names {
            if name.is_empty()
                || name.len() > 255
                || name.as_slice() == b"."
                || name.as_slice() == b".."
                || name.contains(&0)
                || name.contains(&b'/')
                || name.as_slice() <= last
            {
                return Err(OverlayError::Invalid("native cookie names"));
            }
            blob.push(name.len() as u8);
            blob.extend_from_slice(name);
            last = name;
        }
        if names.is_empty() || names.len() > PAGE_ROWS {
            return Err(OverlayError::Invalid("native cookie names"));
        }
        let directory = offer.directory;
        self.atomic(|| {
            let held = Held::Directory(directory.owner);
            self.native_fence(directory.mount, directory.serial, held, true)?;
            self.execute(
                StatementKind::Lease,
                "INSERT INTO native_cookie VALUES(?1,?2,?3,?4,?5)",
                &[
                    &directory.mount.route.ns,
                    &integer(directory.owner)?,
                    &integer(offer.first)?,
                    &offer.after.as_slice(),
                    &blob.as_slice(),
                ],
                40 + (offer.after.len() + blob.len()) as u64,
            )?;
            Ok(())
        })
    }
    pub fn explain_native_directory(
        &self,
        directory: crate::NativeDirectory,
    ) -> OverlayResult<Vec<String>> {
        self.state(directory.mount.route)?;
        let (ns, owner) = (directory.mount.route.ns, integer(directory.owner)?);
        let mut plans = Vec::new();
        for (label, statement) in [
            ("cookie-offset", sql::COOKIE_PAGE),
            ("cookie-window", sql::COOKIE_PAGES),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {statement}"),
                &[&ns, &owner, &0_i64],
                24,
                |r| Ok(format!("{label}: {}", r.get::<_, String>(3)?)),
            )?);
        }
        plans.extend(self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::COOKIE_PAGE_AFTER),
            &[&ns, &owner, &b"x".as_slice()],
            17,
            |r| Ok(format!("cookie-after: {}", r.get::<_, String>(3)?)),
        )?);
        plans.extend(self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::SOURCE_NAMES_KINDS),
            &[
                &ns,
                &1_i64,
                &integer(directory.serial)?,
                &b"".as_slice(),
                &1_i64,
                &0_i64,
            ],
            40,
            |r| Ok(format!("names-kinds: {}", r.get::<_, String>(3)?)),
        )?);
        plans.extend(self.query(StatementKind::Explain,
            "EXPLAIN QUERY PLAN SELECT owner FROM native_directory WHERE ns=?1 AND mount=?2 AND closed=0 LIMIT 1",
            &[&ns, &integer(directory.mount.owner)?], 16,
            |r| Ok(format!("open-fence: {}", r.get::<_, String>(3)?)))?);
        Ok(plans)
    }
}
