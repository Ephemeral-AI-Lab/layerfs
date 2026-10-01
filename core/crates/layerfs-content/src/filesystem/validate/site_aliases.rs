//! Indexed immutable site membership and monotone base facts without name history.
use super::fact_access::FactAccess;
use super::{charge_directory, walk_limit, FilesystemTopology, ValidationState, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::path::PathName;
use crate::filesystem::rows::{BindingLookup, PreparedBindingRows};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::filesystem::state::{
    AliasCapacity, AliasFrontier, AliasProgress, BindingSiteState, ResidentAliasFrontier, SiteKey,
    SiteMembership, SiteObservation, SiteParentPageLimit, SiteRecord, STATE_MAX_PAGE_RECORDS,
};
use crate::filesystem::state::{EligibilityView, NamespaceConstructionState, ParentCalls};
use crate::object::inode_leaf::InodeKind;
use crate::object::AuthenticatedObjects;

pub(super) struct SiteAliasInput<'a> {
    pub(super) reader: &'a dyn AuthenticatedObjects,
    pub(super) input: &'a dyn PreparedBindingRows,
    pub(super) topology: &'a FilesystemTopology,
    pub(super) members: &'a SiteMembership,
    pub(super) active_stored: u64,
    pub(super) unreachable: EligibilityView<'a>,
}

pub(super) fn check<S: BindingSiteState + ?Sized>(
    context: SiteAliasInput<'_>,
    work: &mut ValidationWork,
    facts: &mut ValidationState,
    sites: &mut S,
) -> ContentResult<()> {
    let records = (walk_limit(context.input) as u64)
        .checked_add(1)
        .ok_or(ContentError::LengthOverflow)?;
    let site_bytes = context
        .members
        .birth()
        .records()
        .checked_mul(60)
        .ok_or(ContentError::LengthOverflow)?;
    let bytes = records
        .checked_mul(79)
        .and_then(|n| n.checked_add(site_bytes))
        .and_then(|n| n.checked_add(crate::filesystem::state::ALIAS_FIXED_BYTES))
        .ok_or(ContentError::LengthOverflow)?;
    let aliases = ResidentAliasFrontier::new(AliasCapacity::new(
        records,
        bytes,
        context.members.birth().records(),
    )?);
    let mut compatibility = crate::filesystem::state::CompatibilityAliases { sites, aliases };
    check_frontier(context, work, facts, &mut compatibility)
}

pub(super) fn check_frontier<S: BindingSiteState + AliasFrontier + ?Sized>(
    context: SiteAliasInput<'_>,
    work: &mut ValidationWork,
    facts: &mut ValidationState,
    sites: &mut S,
) -> ContentResult<()> {
    let members = context.members;
    let result = walk(
        context,
        work,
        facts,
        sites,
        FactAccess::compatibility(),
        ParentCalls::compatibility(),
    );
    if result.is_err() {
        let _ = sites.alias_abandon(members);
    }
    result
}

pub(super) fn check_namespace<S: NamespaceConstructionState + ?Sized>(
    context: SiteAliasInput<'_>,
    work: &mut ValidationWork,
    facts: &mut ValidationState,
    sites: &mut S,
) -> ContentResult<()> {
    let members = context.members;
    let result = walk(
        context,
        work,
        facts,
        sites,
        FactAccess::supplied(),
        ParentCalls::supplied(),
    );
    if result.is_err() {
        let _ = sites.alias_abandon(members);
    }
    result
}

fn walk<S: BindingSiteState + AliasFrontier + ?Sized>(
    context: SiteAliasInput<'_>,
    work: &mut ValidationWork,
    facts: &mut ValidationState,
    sites: &mut S,
    access: FactAccess<S>,
    parents: ParentCalls<S>,
) -> ContentResult<()> {
    let SiteAliasInput {
        reader,
        input,
        topology,
        members,
        active_stored,
        unreachable,
    } = context;
    sites.alias_capacity(members)?;
    let root_serial = if active_stored != 0 {
        topology
            .base
            .filter(|_| topology.table.is_some())
            .map(|root| root.root_inode().serial())
    } else {
        None
    };
    sites.alias_begin(members, root_serial)?;
    let Some(table) = topology.table.filter(|_| root_serial.is_some()) else {
        let seal = sites.alias_finish(members)?;
        return sites.alias_retire(&seal);
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
    let mut visited = 0usize;
    while let Some(current) = sites.alias_take(members)? {
        let parent = current.serial;
        let mut progress = AliasProgress::initial();
        let record = access
            .lookup(facts, sites, reader, table, parent, work)?
            .ok_or(ContentError::InvalidRecord("missing base inode"))?;
        let owns_sites = !unreachable.contains(sites, parent, parents)?
            && sites.site_parent_present(members, parent)?;
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
            let mut children = [0_u64; STATE_MAX_PAGE_RECORDS];
            let mut child_count = 0;
            for (name, serial) in page.entries {
                let restated = if owns_sites {
                    match input.binding_for(parent, name.as_bytes())? {
                        BindingLookup::Present(child) => {
                            match active_site(sites, members, child, unreachable, parents)? {
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
                if access
                    .lookup(facts, sites, reader, table, serial, work)?
                    .is_some_and(|value| value.kind == InodeKind::Directory)
                {
                    children[child_count] = serial;
                    child_count += 1;
                }
                if let Some(site) = active_site(sites, members, serial, unreachable, parents)? {
                    let same = site.point().parent() == parent && resolved(input, site)? == name;
                    let legal = same || !survives(input, parent, name.as_bytes(), serial)?;
                    observations.push(SiteObservation::new(site.key(), legal));
                    if observations.len() == STATE_MAX_PAGE_RECORDS {
                        sites.site_observe_base_batch(members, &observations)?;
                        observations.clear();
                    }
                }
            }
            sites.alias_enqueue(members, &children[..child_count])?;
            let next = AliasProgress::base(page.continuation.clone());
            sites.alias_advance(members, current, &progress, &next)?;
            progress = next;
            match page.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        if !owns_sites {
            let next = AliasProgress::complete();
            sites.alias_advance(members, current, &progress, &next)?;
            sites.alias_complete(members, current)?;
            continue;
        }
        // The indexed projection visits each changed stored site once in its
        // immutable source order, including new names absent from the base.
        let mut maximum = None;
        let mut seen = 0_u64;
        loop {
            let limit = SiteParentPageLimit::default();
            let page = sites.site_parent_page(members, parent, progress.ordinal(), limit)?;
            page.check_limit(limit)?;
            if page.members() != members
                || page.parent() != parent
                || maximum.is_some_and(|max| max != page.maximum())
            {
                return Err(ContentError::InvalidOrderingRecord(
                    "site parent selected page",
                ));
            }
            maximum = Some(page.maximum());
            let mut last = progress.ordinal();
            let mut children = [0_u64; STATE_MAX_PAGE_RECORDS];
            let mut child_count = 0;
            for site in page.records() {
                if last.is_some_and(|prior| prior >= site.point().binding_ordinal()) {
                    return Err(ContentError::InvalidOrderingRecord(
                        "site parent cursor order",
                    ));
                }
                last = Some(site.point().binding_ordinal());
                resolved(input, *site)?;
                visited = visited.saturating_add(1);
                if visited > walk_limit(input) {
                    return Err(ContentError::InvalidRecord("cycle check work limit"));
                }
                work.entries_examined = work.entries_examined.saturating_add(1);
                if access
                    .lookup(facts, sites, reader, table, site.key().serial(), work)?
                    .is_some_and(|value| value.kind == InodeKind::Directory)
                {
                    children[child_count] = site.key().serial();
                    child_count += 1;
                }
            }
            seen = seen
                .checked_add(page.records().len() as u64)
                .ok_or(ContentError::LengthOverflow)?;
            if page.last() != last
                || seen > members.birth().records()
                || page.eof() != (last == page.maximum())
            {
                return Err(ContentError::InvalidOrderingRecord(
                    "site parent cursor EOF",
                ));
            }
            sites.alias_enqueue(members, &children[..child_count])?;
            let next = AliasProgress::sites(page.last(), page.eof());
            sites.alias_advance(members, current, &progress, &next)?;
            progress = next;
            if page.eof() {
                break;
            }
        }
        sites.alias_complete(members, current)?;
    }
    if !observations.is_empty() {
        sites.site_observe_base_batch(members, &observations)?;
    }
    let seal = sites.alias_finish(members)?;
    sites.alias_retire(&seal)
}

fn active_site<S: BindingSiteState + ?Sized>(
    sites: &mut S,
    members: &SiteMembership,
    serial: u64,
    unreachable: EligibilityView<'_>,
    parents: ParentCalls<S>,
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
        (record.has_base() && !unreachable.contains(sites, record.point().parent(), parents)?)
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
