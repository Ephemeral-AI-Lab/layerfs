use super::{
    generation::{locator_key, parse_locator},
    index::Index,
    page::PageRef,
};
use crate::WorkspaceError;
use std::collections::{BTreeMap, BTreeSet};

/// A locator is removed in the same index publication only when no final
/// inverse reference to its logical page remains. Mixed pages wait for the
/// later relocation pass. Paged scans cover slots with more than 128 inverse
/// references too; a full first page is not evidence that a later page is live.
fn live_refs(
    index: &Index,
    updates: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    tag: u8,
    logical: u64,
) -> Result<bool, WorkspaceError> {
    let prefix = [vec![tag], logical.to_be_bytes().to_vec()].concat();
    if updates
        .iter()
        .any(|(key, value)| key.starts_with(&prefix) && value.is_some())
    {
        return Ok(true);
    }
    let upper = if logical == u64::MAX {
        vec![tag + 1]
    } else {
        [vec![tag], (logical + 1).to_be_bytes().to_vec()].concat()
    };
    let mut lower = prefix;
    loop {
        let current = index.scan(&lower, &upper, 128)?;
        if current
            .entries()
            .iter()
            .any(|(key, _)| updates.get(key).is_none_or(|value| value.is_some()))
        {
            return Ok(true);
        }
        let Some((last, _)) = current.entries().last() else {
            return Ok(false);
        };
        lower = last.clone();
        lower.push(0);
    }
}

pub(super) fn prune_dead(
    index: &Index,
    updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    retain_logical: Option<u64>,
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
        if retain_logical == Some(logical) {
            continue;
        }
        if live_refs(index, updates, b'R', logical)? {
            continue;
        }
        if let Some(old) = index.get(&locator_key(logical))? {
            dead.push(parse_locator(&old)?);
            updates.insert(locator_key(logical), None);
        }
    }
    Ok(dead)
}

pub(super) fn prune_dead_payloads(
    index: &Index,
    updates: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<Vec<u64>, WorkspaceError> {
    let mut logicals = BTreeSet::new();
    for (key, value) in updates {
        if key.len() == 25 && key[0] == b'L' && value.is_none() {
            logicals.insert(u64::from_be_bytes(
                key[1..9].try_into().map_err(|_| WorkspaceError::Io)?,
            ));
        }
    }
    logicals
        .into_iter()
        .filter_map(|logical| match live_refs(index, updates, b'L', logical) {
            Ok(false) => Some(Ok(logical)),
            Ok(true) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}
