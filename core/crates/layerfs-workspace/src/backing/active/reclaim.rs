use super::{
    generation::{locator_key, parse_locator},
    index::Index,
    page::PageRef,
};
use crate::{backing::budget::Charge, WorkspaceError};
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

fn touched_logicals(
    index: &Index,
    updates: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    tag: u8,
    length: usize,
) -> Result<(BTreeSet<u64>, Charge), WorkspaceError> {
    let mut logicals = BTreeSet::new();
    let mut charge = index.budget().reserve(0)?;
    for (key, value) in updates {
        if key.len() == length && key[0] == tag && value.is_none() {
            let logical = u64::from_be_bytes(key[1..9].try_into().map_err(|_| WorkspaceError::Io)?);
            if !logicals.contains(&logical) {
                charge.resize(
                    logicals
                        .len()
                        .checked_add(1)
                        .and_then(|n| n.checked_mul(96))
                        .ok_or(WorkspaceError::Capacity)?,
                )?;
                logicals.insert(logical);
            }
        }
    }
    Ok((logicals, charge))
}

pub(super) fn prune_dead(
    index: &Index,
    updates: &mut BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    retain_logical: Option<u64>,
) -> Result<(Vec<PageRef>, Charge), WorkspaceError> {
    let (logicals, mut charge) = touched_logicals(index, updates, b'R', 27)?;
    // Keep the set, exact result capacity and possible P deletions charged
    // through the publication that consumes them.
    charge.resize(
        logicals
            .len()
            .checked_mul(96 + 16 + 128 + 9)
            .ok_or(WorkspaceError::Capacity)?,
    )?;
    let mut dead = Vec::with_capacity(logicals.len());
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
    Ok((dead, charge))
}

pub(super) fn prune_dead_payloads(
    index: &Index,
    updates: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<(Vec<u64>, Charge), WorkspaceError> {
    let (logicals, mut charge) = touched_logicals(index, updates, b'L', 25)?;
    charge.resize(
        logicals
            .len()
            .checked_mul(96 + 8)
            .ok_or(WorkspaceError::Capacity)?,
    )?;
    let mut dead = Vec::with_capacity(logicals.len());
    for logical in logicals {
        if !live_refs(index, updates, b'L', logical)? {
            dead.push(logical);
        }
    }
    Ok((dead, charge))
}
