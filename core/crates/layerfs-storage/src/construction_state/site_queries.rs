//! Bounded acknowledged primary and existing-parent pages, with actual MAX EOF.

use layerfs_content::filesystem::state::{
    SiteKey, SiteMembership, SitePage, SitePageLimit, SiteParentPage, SiteParentPageLimit, SiteSeal,
};

use crate::error::{StorageError, StorageResult};

use super::{site_index, ScratchSession};

impl ScratchSession {
    /// Indexed existence of an immutable existing-site projection for one parent.
    pub fn site_parent_present(
        &mut self,
        members: &SiteMembership,
        parent: u64,
    ) -> StorageResult<bool> {
        self.ensure_site_scope(members.birth().scope())?;
        self.finish((|| {
            check_parent(parent)?;
            let resource = self.site_resource()?;
            let state = resource.sites.as_ref().unwrap();
            state.check_membership(members)?;
            Ok(site_index::parent_maximum(resource.verify()?, &state.scope, parent)?.is_some())
        })())
    }

    /// Only immutable birth flags1, in binding ordinal order; Some(0) is present.
    pub fn site_parent_page(
        &mut self,
        members: &SiteMembership,
        parent: u64,
        after: Option<u32>,
        limit: SiteParentPageLimit,
    ) -> StorageResult<SiteParentPage> {
        self.ensure_site_scope(members.birth().scope())?;
        self.finish((|| {
            check_parent(parent)?;
            let resource = self.site_resource()?;
            let state = resource.sites.as_ref().unwrap();
            state.check_membership(members)?;
            let connection = resource.verify()?;
            let maximum = site_index::parent_maximum(connection, &state.scope, parent)?;
            if after.is_some_and(|ordinal| maximum.is_none_or(|maximum| ordinal > maximum)) {
                return Err(StorageError::Integrity(
                    "construction scratch site parent continuation",
                ));
            }
            let count = if after == maximum {
                0
            } else {
                limit.fitting_records()
            };
            if after != maximum && count == 0 {
                limit.check_records(1)?;
            }
            let rows = site_index::read_parent(connection, &state.scope, parent, after, count)?;
            let last = rows
                .last()
                .map(|record| record.point().binding_ordinal())
                .or(after);
            let eof = last == maximum;
            if last.is_some_and(|ordinal| maximum.is_none_or(|maximum| ordinal > maximum))
                || (!eof && rows.len() != count)
            {
                return Err(StorageError::Integrity(
                    "construction scratch site parent page EOF",
                ));
            }
            let page = SiteParentPage::after(members.clone(), parent, after, maximum, rows, eof)?;
            page.check_limit(limit)?;
            Ok(page)
        })())
    }

    /// The current full flags under one exact immutable acknowledged final seal.
    pub fn site_sealed_page(
        &mut self,
        seal: &SiteSeal,
        after: Option<SiteKey>,
        limit: SitePageLimit,
    ) -> StorageResult<SitePage> {
        self.ensure_site_scope(seal.scope())?;
        self.finish((|| {
            let resource = self.site_resource()?;
            let state = resource.sites.as_ref().unwrap();
            state.check_seal(seal)?;
            let maximum = state.maximum();
            if let Some(key) = after {
                SiteKey::decode(seal.scope(), key.as_bytes())?;
                if maximum.is_none_or(|maximum| key > maximum) {
                    return Err(StorageError::Integrity(
                        "construction scratch site continuation",
                    ));
                }
            }
            let connection = resource.verify()?;
            let count = if after == maximum {
                0
            } else {
                limit.fitting_records()
            };
            if after != maximum && count == 0 {
                limit.check_records(1)?;
            }
            let rows = site_index::read(connection, &state.scope, after, count)?;
            let last = rows.last().map(|record| record.key()).or(after);
            let eof = last == maximum;
            if last.is_some_and(|key| maximum.is_none_or(|maximum| key > maximum))
                || (!eof && rows.len() != count)
            {
                return Err(StorageError::Integrity(
                    "construction scratch site page EOF",
                ));
            }
            let page = SitePage::after(seal.clone(), after, rows, eof)?;
            page.check_limit(limit)?;
            Ok(page)
        })())
    }
}

fn check_parent(parent: u64) -> StorageResult<()> {
    if parent == 0 || parent > i64::MAX as u64 {
        return Err(StorageError::Content(
            layerfs_content::ContentError::InvalidOrderingRecord("site parent"),
        ));
    }
    Ok(())
}
