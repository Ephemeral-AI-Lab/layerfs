//! §8 step 10: synchronous-commit strength and the standby predicate.

use super::op::hex;
use super::op::stage;
use crate::backend::postgresql::session::connection::Session;
use crate::backend::postgresql::session::transaction::Mode;
use crate::backend::postgresql::session::transaction::Transaction;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_history::error::OpConflict;
use layerfs_history::records::GcIntentKind;
use layerfs_history::records::GcScopeKind;
use std::sync::Arc;

/// The strength order of §8 step 10: off < local < remote_write < on <
/// remote_apply.
pub fn sync_rank(setting: &str) -> u8 {
    match setting {
        "local" => 1,
        "remote_write" => 2,
        "on" => 3,
        "remote_apply" => 4,
        _ => 0,
    }
}

/// The §8 step 10 setting name for a strength rank; an unknown rank is `off`.
pub fn sync_name(rank: u8) -> &'static str {
    match rank {
        1 => "local",
        2 => "remote_write",
        3 => "on",
        4 => "remote_apply",
        _ => "off",
    }
}

/// The swap's effective strength (§8 step 10): the server's setting or the
/// Store's frozen setting, whichever is stronger. There is no floor: two
/// `off` settings stay `off`, because neither is lowered and neither is
/// raised above the other.
pub fn effective_rank(server: &str, stored: &str) -> u8 {
    sync_rank(server).max(sync_rank(stored))
}

/// §8 step 10: the swap waits iff the effective level is at least
/// `remote_write` and `synchronous_standby_names` names a quorum. The setting
/// is parsed as PostgreSQL does: `FIRST n (names)`, `ANY n (names)`, a bare
/// legacy `names` list (which PostgreSQL reads as `FIRST 1 (names)`), and the
/// lone `*` that matches any standby. A matching standby must be streaming
/// with the `sync_state` that form assigns it, so one of an `ANY 2` pair is
/// still refused.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StandbyRequirement {
    /// `FIRST` counts the named standbys in list order; `ANY` counts any
    /// `count` of them. A bare legacy list is `FIRST 1`.
    pub(crate) first: bool,
    pub(crate) count: usize,
    /// The configured standby names, in priority order.
    pub(crate) members: Vec<String>,
    /// The lone `*`, which matches any standbys.
    pub(crate) any_name: bool,
}

/// The pure parse of `synchronous_standby_names`; `None` when the setting is
/// empty (no standby required).
pub(crate) fn standby_requirement(names: &str) -> Option<StandbyRequirement> {
    let names = names.trim();
    if names.is_empty() {
        return None;
    }
    let lower = names.to_ascii_lowercase();
    let (first, rest) = if lower.starts_with("first") {
        (true, names.get(5..).unwrap_or("").trim())
    } else if lower.starts_with("any") {
        (false, names.get(3..).unwrap_or("").trim())
    } else {
        (true, names)
    };
    // `FIRST n (list)` / `ANY n (list)`: a leading count, then the list.
    // A form with no leading count is PostgreSQL's legacy `standby_name
    // [, ...]`, which is `FIRST 1`, not one synchronous standby per name.
    let (explicit_count, count, list) = match rest.split_once(char::is_whitespace) {
        Some((head, tail)) if head.bytes().all(|byte| byte.is_ascii_digit()) => {
            match head.parse::<usize>() {
                Ok(count) => (true, count, tail.trim()),
                Err(_) => return None,
            }
        }
        _ if rest.starts_with('(') => (true, 1, rest),
        _ => (false, 1, rest),
    };
    let members: Vec<String> = list
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .map(str::trim)
        .filter(|member| !member.is_empty())
        .map(|member| member.trim_matches('"').to_owned())
        .collect();
    let any_name = members.len() == 1 && members[0] == "*";
    let count = if explicit_count { count } else { 1 };
    Some(StandbyRequirement {
        first,
        count,
        members,
        any_name,
    })
}

/// One `pg_stat_replication` row, as the predicate reads it.
pub struct Replica {
    /// The standby's `application_name`.
    pub name: String,
    /// Whether the standby is streaming.
    pub streaming: bool,
    /// Its `sync_state` (`sync`, `quorum`, `async`, `potential`).
    pub sync_state: String,
}

/// True when the streaming named standby is one PostgreSQL would count as
/// synchronous for this form: `sync` for `FIRST`, `quorum` (or `sync`) for
/// `ANY`.
pub(crate) fn replica_counts(replica: &Replica, first: bool) -> bool {
    if !replica.streaming {
        return false;
    }
    if first {
        replica.sync_state == "sync"
    } else {
        matches!(replica.sync_state.as_str(), "sync" | "quorum")
    }
}

/// The §8 step 10 predicate, without a server: `rank` is the effective
/// `synchronous_commit` strength, `names` the setting and `replicas` the
/// rows `pg_stat_replication` reports.
pub fn standby_satisfied(rank: u8, names: &str, replicas: &[Replica]) -> bool {
    if rank < 2 {
        return true;
    }
    let Some(requirement) = standby_requirement(names) else {
        return true;
    };
    if requirement.count == 0 {
        return true;
    }
    if requirement.any_name {
        let matched = replicas
            .iter()
            .filter(|replica| replica_counts(replica, requirement.first))
            .count();
        return matched >= requirement.count;
    }
    let mut matched = 0;
    for member in &requirement.members {
        matched += replicas
            .iter()
            .filter(|replica| replica.name == *member && replica_counts(replica, requirement.first))
            .count();
        if matched >= requirement.count {
            return true;
        }
    }
    false
}

/// The effective `synchronous_commit` and whether the standby predicate
/// applies, read from the server and the Store's frozen setting. The read is
/// a read-only snapshot: a write transaction has already applied the Store
/// setting with `SET LOCAL`, so `current_setting` there would only echo it.
pub(crate) fn effective_sync_rank(session: &Arc<Session>) -> HistoryResult<u8> {
    session.run_as(Mode::Read, |tx| {
        let rows = tx.query("SELECT current_setting('synchronous_commit')", Vec::new())?;
        let server: String = rows[0].get(0)?;
        let stored = session.settings.synchronous_commit.setting();
        Ok(effective_rank(&server, stored))
    })
}

/// §8 step 10: the standby predicate checked before Phase M.
pub(crate) fn standby_precheck(
    session: &Arc<Session>,
    rank: u8,
    op_id: &[u8; 16],
    phase: &str,
) -> HistoryResult<()> {
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT current_setting('synchronous_standby_names')",
            Vec::new(),
        )?;
        let names: String = rows[0].get(0)?;
        standby_check(tx, rank, &names, op_id, phase)
    })
}

pub(crate) fn standby_check(
    tx: &Transaction<'_>,
    rank: u8,
    names: &str,
    op_id: &[u8; 16],
    phase: &str,
) -> HistoryResult<()> {
    if rank < 2 || names.trim().is_empty() {
        return Ok(());
    }
    let rows = tx.query(
        "SELECT application_name::text, state::text, sync_state::text \
         FROM pg_stat_replication",
        Vec::new(),
    )?;
    let replicas = rows
        .iter()
        .map(|row| {
            let state: String = row.get(1)?;
            Ok(Replica {
                name: row.get(0)?,
                streaming: state == "streaming",
                sync_state: row.get(2)?,
            })
        })
        .collect::<HistoryResult<Vec<_>>>()?;
    if !standby_satisfied(rank, names, &replicas) {
        // The quorum is missing, not the Store: name the waiting operation
        // instead of a holderless `Busy`.
        return Err(HistoryError::BusyOp(Box::new(OpConflict::new(
            GcIntentKind::Compaction,
            GcScopeKind::Store,
            "store",
            Some(phase.to_owned()),
            hex(op_id),
            false,
        ))));
    }
    Ok(())
}
