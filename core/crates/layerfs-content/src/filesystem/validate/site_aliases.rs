//! Indexed immutable site membership and monotone base facts without name history.
use super::{charge_directory, walk_limit, FilesystemTopology, ValidationState, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::path::PathName;
use crate::filesystem::rows::{BindingLookup, PreparedBindingRows};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::state::{
    BindingSiteState, SiteKey, SiteMembership, SiteObservation, SiteParentCursor,
    SiteParentPageLimit, SiteRecord, STATE_MAX_PAGE_RECORDS,
};
use crate::object::inode_leaf::InodeKind;
use crate::object::AuthenticatedObjects;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct SiteAliasInput<'a> {
    pub(super) reader: &'a dyn AuthenticatedObjects,
    pub(super) input: &'a dyn PreparedBindingRows,
    pub(super) topology: &'a FilesystemTopology,
    pub(super) members: &'a SiteMembership,
    pub(super) active_stored: u64,
    pub(super) unreachable: &'a BTreeMap<u64, ()>,
}

pub(super) fn check<S: BindingSiteState + ?Sized>(
    context: SiteAliasInput<'_>,
    work: &mut ValidationWork,
    facts: &mut ValidationState,
    sites: &mut S,
) -> ContentResult<()> {
    let SiteAliasInput {
        reader,
        input,
        topology,
        members,
        active_stored,
        unreachable,
    } = context;
    if active_stored == 0 {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    let Some(root) = topology.base else {
        return Ok(());
    };
    let mut observations = Vec::new();
    observations
        .try_reserve_exact(STATE_MAX_PAGE_RECORDS)
        .map_err(|_| ContentError::ResourceUnavailable {
            what: "binding_sites.observation_window",
        })?;
    if observations.capacity() > STATE_MAX_PAGE_RECORDS {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "binding_sites.observation_capacity",
            limit: STATE_MAX_PAGE_RECORDS as u64,
            actual: observations.capacity() as u64,
        });
    }
    // These existing graph owners retain their original work admission. Neither
    // owns a site/name/target-parent or prior-base-binding population.
    let mut pending = vec![root.root_inode().serial()];
    let mut seen = BTreeSet::new();
    let mut visited = 0usize;
    while let Some(parent) = pending.pop() {
        if !seen.insert(parent) {
            continue;
        }
        let record = facts.lookup_one(reader, table, parent, work)?;
        let owns_sites =
            !unreachable.contains_key(&parent) && sites.site_parent_present(members, parent)?;
        let mut after = None;
        loop {
            let mut directory = DirectoryReadWork::default();
            let page = list_after(
                reader,
                DirectoryRoot(record.content_root),
                after.as_ref(),
                64,
                crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
                &mut directory,
            )?;
            charge_directory(work, directory);
            visited = visited.saturating_add(page.entries.len());
            work.entries_examined = work
                .entries_examined
                .saturating_add(page.entries.len() as u64);
            if visited > walk_limit(input) {
                return Err(ContentError::InvalidRecord("cycle check work limit"));
            }
            for (name, serial) in page.entries {
                let restated = if owns_sites {
                    match input.binding_for(parent, name.as_bytes())? {
                        BindingLookup::Present(child) => {
                            match active_site(sites, members, child, unreachable)? {
                                Some(site) if site.point().parent() == parent => {
                                    resolved(input, site)? == name
                                }
                                _ => false,
                            }
                        }
                        BindingLookup::Unmentioned | BindingLookup::Absent => false,
                    }
                } else {
                    false
                };
                // A restatement removes the entire old edge, including an old
                // serial different from the candidate's selected child.
                if restated {
                    continue;
                }
                if facts
                    .lookup_optional(reader, table, serial, work)?
                    .is_some_and(|value| value.kind == InodeKind::Directory)
                {
                    pending.push(serial);
                }
                if let Some(site) = active_site(sites, members, serial, unreachable)? {
                    let same = site.point().parent() == parent && resolved(input, site)? == name;
                    let legal = same || !survives(input, parent, name.as_bytes(), serial)?;
                    observations.push(SiteObservation::new(site.key(), legal));
                    if observations.len() == STATE_MAX_PAGE_RECORDS {
                        sites.site_observe_base_batch(members, &observations)?;
                        observations.clear();
                    }
                }
            }
            match page.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        if !owns_sites {
            continue;
        }
        // The indexed projection visits each changed stored site once in its
        // immutable source order, including new names absent from the base.
        let mut cursor = SiteParentCursor::new(sites, members.clone(), parent)?;
        while let Some(page) = cursor.next_page(SiteParentPageLimit::default())? {
            for site in page.records() {
                resolved(input, *site)?;
                visited = visited.saturating_add(1);
                if visited > walk_limit(input) {
                    return Err(ContentError::InvalidRecord("cycle check work limit"));
                }
                work.entries_examined = work.entries_examined.saturating_add(1);
                if facts
                    .lookup_optional(reader, table, site.key().serial(), work)?
                    .is_some_and(|value| value.kind == InodeKind::Directory)
                {
                    pending.push(site.key().serial());
                }
            }
        }
    }
    if !observations.is_empty() {
        sites.site_observe_base_batch(members, &observations)?;
    }
    Ok(())
}

fn active_site<S: BindingSiteState + ?Sized>(
    sites: &mut S,
    members: &SiteMembership,
    serial: u64,
    unreachable: &BTreeMap<u64, ()>,
) -> ContentResult<Option<SiteRecord>> {
    let scope = members.birth().scope();
    let key = SiteKey::new(scope, serial)?;
    let Some(record) = sites.site_get(scope, key)? else {
        return Ok(None);
    };
    SiteRecord::decode(scope, &record.encode())?;
    if record.key() != key {
        return Err(ContentError::InvalidOrderingRecord("site selected key"));
    }
    Ok(
        (record.has_base() && !unreachable.contains_key(&record.point().parent()))
            .then_some(record),
    )
}
fn resolved(input: &dyn PreparedBindingRows, site: SiteRecord) -> ContentResult<PathName> {
    let (name, child) = input.binding_at(&site.point())?;
    if child != Some(site.key().serial()) {
        return Err(ContentError::InvalidRecord("binding replay"));
    }
    Ok(name)
}
fn survives(
    input: &dyn PreparedBindingRows,
    parent: u64,
    name: &[u8],
    child: u64,
) -> ContentResult<bool> {
    Ok(match input.binding_for(parent, name)? {
        BindingLookup::Unmentioned => true,
        BindingLookup::Absent => false,
        BindingLookup::Present(final_child) => final_child == child,
    })
}
