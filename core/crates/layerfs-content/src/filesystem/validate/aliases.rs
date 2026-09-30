//! Existing non-file binding sites and exact indexed changed-name decisions.
//!
//! Site/base-binding/frontier populations retain their existing graph admission;
//! this pass replaces copied directory rows, not the later paged graph authority.

use std::collections::{BTreeMap, BTreeSet};

use super::binding::{selected, Bindings};
use super::{charge_directory, walk_limit, FilesystemTopology, ValidationState, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::path::PathName;
use crate::filesystem::rows::{BindingLookup, PreparedBindingRows};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::object::inode_leaf::InodeKind;
use crate::object::AuthenticatedObjects;

pub(super) struct BindingSite {
    pub(super) parent: u64,
    pub(super) name: PathName,
}

pub(super) fn check(
    reader: &dyn AuthenticatedObjects,
    input: &dyn PreparedBindingRows,
    topology: &FilesystemTopology,
    sites: &BTreeMap<u64, BindingSite>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut ValidationState,
) -> ContentResult<()> {
    let parents: BTreeSet<u64> = sites
        .values()
        .filter(|site| !unreachable.contains_key(&site.parent))
        .map(|site| site.parent)
        .collect();
    if parents.is_empty() {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    let Some(root) = topology.base else {
        return Ok(());
    };
    let mut bound: BTreeMap<u64, Vec<(PathName, u64)>> = BTreeMap::new();
    let mut pending = vec![root.root_inode().serial()];
    let mut visited = 0usize;
    let mut seen = BTreeSet::new();
    while let Some(parent) = pending.pop() {
        if !seen.insert(parent) {
            continue;
        }
        let record = state.lookup_one(reader, table, parent, work)?;
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
                let restated = if parents.contains(&parent) {
                    match input.binding_for(parent, name.as_bytes())? {
                        BindingLookup::Present(child) => sites.get(&child).is_some_and(|site| {
                            site.parent == parent
                                && site.name == name
                                && !unreachable.contains_key(&site.parent)
                        }),
                        BindingLookup::Unmentioned | BindingLookup::Absent => false,
                    }
                } else {
                    false
                };
                if restated {
                    continue;
                }
                if state
                    .lookup_optional(reader, table, serial, work)?
                    .is_some_and(|value| value.kind == InodeKind::Directory)
                {
                    pending.push(serial);
                }
                if sites
                    .get(&serial)
                    .is_some_and(|site| !unreachable.contains_key(&site.parent))
                {
                    bound.entry(serial).or_default().push((name, parent));
                }
            }
            match page.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        if !parents.contains(&parent) {
            continue;
        }
        let header = selected(input, parent)?
            .ok_or(ContentError::InvalidRecord("directory header missing"))?;
        let mut bindings = Bindings::new(input, header)?;
        // Each selected changed site is followed once after the base pass,
        // including a name absent from the base. Never repeat its whole row for
        // every base name that the update restates.
        while let Some((name, child)) = bindings.next()? {
            let Some(child) = child else {
                continue;
            };
            if !sites
                .get(&child)
                .is_some_and(|site| site.parent == parent && site.name == name)
            {
                continue;
            }
            visited = visited.saturating_add(1);
            if visited > walk_limit(input) {
                return Err(ContentError::InvalidRecord("cycle check work limit"));
            }
            work.entries_examined = work.entries_examined.saturating_add(1);
            if state
                .lookup_optional(reader, table, child, work)?
                .is_some_and(|value| value.kind == InodeKind::Directory)
            {
                pending.push(child);
            }
        }
    }
    for (&child, site) in sites {
        if unreachable.contains_key(&site.parent) {
            continue;
        }
        if input.binding_for(site.parent, site.name.as_bytes())? != BindingLookup::Present(child) {
            return Err(ContentError::InvalidRecord("binding replay"));
        }
        let Some(base) = bound.get(&child) else {
            continue;
        };
        let mut legal = false;
        for (name, parent) in base {
            if *parent == site.parent && *name == site.name
                || !survives(input, *parent, name.as_bytes(), child)?
            {
                legal = true;
                break;
            }
        }
        if !legal {
            return Err(ContentError::InvalidRecord("multiple parents"));
        }
    }
    Ok(())
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
