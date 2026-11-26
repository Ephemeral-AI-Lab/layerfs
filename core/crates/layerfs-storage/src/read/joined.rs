//! Admission of a Save demand's whole packs returned alongside its locators.
use super::Fetch;
use crate::{
    encoding::PackCache,
    location::{LocatedObject, PackDomain, PackInfo},
    pack::layout::{VERSION_ORDINARY_PAGES, VERSION_ORDINARY_SIZED},
    policy::PACK_LIMIT,
    port::PersistedPack,
    StorageError, StorageResult,
};
use layerfs_content::ObjectRole;
use std::collections::{BTreeMap, BTreeSet};

impl Fetch {
    pub(super) fn admit_joined_packs(
        &self,
        packs: &mut PackCache,
        rows: &[LocatedObject],
        bodies: Vec<PersistedPack>,
    ) -> StorageResult<()> {
        let bad = || StorageError::Integrity("joined locator pack reply");
        let mut demand: BTreeMap<i64, (Option<PackInfo>, BTreeSet<usize>)> = BTreeMap::new();
        for row in rows.iter().filter(|row| {
            matches!(
                row.location.role,
                ObjectRole::InodeLeaf
                    | ObjectRole::InodeBranch
                    | ObjectRole::DirectoryLeaf
                    | ObjectRole::DirectoryBranch
            )
        }) {
            let (info, groups) = demand.entry(row.pack.pack_id).or_default();
            if info.replace(row.pack).is_some_and(|old| old != row.pack) {
                return Err(bad());
            }
            groups.insert(row.location.group_number);
        }
        let mut returned = BTreeSet::new();
        let mut total = 0;
        for body in &bodies {
            let info = body.info();
            total += info.length;
            let (expected, _) = demand.get(&info.pack_id).ok_or_else(bad)?;
            let version = body
                .body()
                .get(8..12)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_le_bytes);
            if *expected != Some(info)
                || info.domain != PackDomain::Metadata
                || !matches!(
                    version,
                    Some(VERSION_ORDINARY_SIZED | VERSION_ORDINARY_PAGES)
                )
                || self.published_has(info.pack_id)
                || !returned.insert(info.pack_id)
                || total > PACK_LIMIT
            {
                return Err(bad());
            }
        }
        for body in bodies {
            let (info, body) = body.into_parts();
            let (_, groups) = demand.remove(&info.pack_id).ok_or_else(bad)?;
            let acquisition = self.whole_reply(true, "joined locator pack reply", info, body)?;
            packs.admit_groups(self, info.pack_id, &groups, &[], acquisition)?;
            self.note(|c| {
                c.joined_packs += 1;
                c.joined_pack_bytes += info.length as u64;
                c.pack_read_bytes += info.length as u64;
                c.pack_misses += 1;
            });
        }
        Ok(())
    }
}
