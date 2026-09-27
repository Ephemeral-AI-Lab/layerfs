use super::{
    generation::{locator_key, parse_locator},
    index::Index,
    page::{Kind, PageRef},
    pages::PageStore,
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

pub(super) struct RetiredPack {
    pub page: PageRef,
    pub birth: u64,
    pub retired: u64,
}

/// A locator is removed in the same index publication only when no final
/// inverse reference to its logical page remains. Mixed pages wait for the
/// later relocation pass; a full 128-row page is conservatively retained.
pub(super) fn prune_dead(
    index: &Index,
    updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<Vec<PageRef>, WorkspaceError> {
    let mut logicals = BTreeSet::new();
    for (key, value) in updates.iter() {
        if key.len() == 27 && key[0] == b'R' && value.is_none() {
            logicals.insert(u64::from_be_bytes(
                key[1..9].try_into().map_err(|_| WorkspaceError::Io)?,
            ));
        }
    }
    let mut dead = Vec::new();
    for logical in logicals {
        let lower = [vec![b'R'], logical.to_be_bytes().to_vec()].concat();
        let upper = if logical == u64::MAX {
            vec![b'S']
        } else {
            [vec![b'R'], (logical + 1).to_be_bytes().to_vec()].concat()
        };
        let current = index.scan(&lower, &upper, 128)?;
        if current.entries().len() == 128 {
            continue;
        }
        let old_live = current
            .entries()
            .iter()
            .any(|(key, _)| updates.get(key).is_none_or(|value| value.is_some()));
        let new_live = updates
            .iter()
            .any(|(key, value)| key.starts_with(&lower) && value.is_some());
        if old_live || new_live {
            continue;
        }
        if let Some(old) = index.get(&locator_key(logical))? {
            dead.push(parse_locator(&old)?);
            updates.insert(locator_key(logical), None);
        }
    }
    Ok(dead)
}

pub(super) fn retire_pack(
    store: &PageStore,
    index: &Index,
    retired: &mut Vec<RetiredPack>,
    charge: &mut Charge,
    page: PageRef,
    revision: u64,
) -> Result<(), WorkspaceError> {
    let birth = store.read(page, Kind::Pack)?.revision();
    if !index.frozen_between(birth, revision)? {
        match store.release(page) {
            Ok(_) => return Ok(()),
            Err(WorkspaceError::Busy) => {}
            Err(error) => return Err(error),
        }
    }
    let needed = retired
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(size_of::<RetiredPack>()))
        .ok_or(WorkspaceError::Capacity)?;
    let charged = (needed * 2).max(retired.capacity() * size_of::<RetiredPack>());
    charge.resize(charged)?;
    retired
        .try_reserve_exact(1)
        .map_err(|_| WorkspaceError::Capacity)?;
    charge.resize(charged.max(retired.capacity() * size_of::<RetiredPack>()))?;
    retired.push(RetiredPack {
        page,
        birth,
        retired: revision,
    });
    Ok(())
}

pub(super) fn maintain_pack(
    store: &PageStore,
    index: &Index,
    retired: &mut Vec<RetiredPack>,
) -> Result<(), WorkspaceError> {
    let mut at = 0;
    while at < retired.len() {
        let item = &retired[at];
        if index.frozen_between(item.birth, item.retired)? {
            at += 1;
            continue;
        }
        match store.release(item.page) {
            Ok(_) => {
                retired.swap_remove(at);
            }
            Err(WorkspaceError::Busy) => at += 1,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
