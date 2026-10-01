//! Bounded exact old/proposed frontier transactions with no automatic retry.
use super::alias_state::Aliases;
use super::{alias_index, profile};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{AliasFact, SiteMembership};
use rusqlite::Connection;

pub(crate) fn enqueue_plan(
    connection: &Connection,
    state: &mut Aliases,
    children: &[u64],
) -> StorageResult<()> {
    if children.len() > 128 {
        return Err(StorageError::CapacityExceeded {
            what: "alias enqueue window",
            limit: 128,
            actual: children.len() as u64,
        });
    }
    // Membership164 + bounded old/new facts/jobs are below64KiB at128 items.
    for serial in children {
        AliasFact {
            serial: *serial,
            sequence: 1,
            status: 1,
        }
        .check()?;
        let prior = state
            .attempt
            .as_ref()
            .unwrap()
            .changes
            .iter()
            .rev()
            .find(|(_, new)| new.is_some_and(|f| f.serial == *serial))
            .map(|(_, new)| *new);
        let old = match prior {
            Some(old) => old,
            None => alias_index::fact(connection, state, *serial)?,
        };
        if old.is_some_and(|f| f.status != 1) {
            continue;
        }
        let a = state.attempt.as_mut().unwrap();
        let sequence = a
            .after
            .sequence
            .checked_add(1)
            .filter(|s| *s <= i64::MAX as u64)
            .ok_or(StorageError::Integrity("alias sequence exhausted"))?;
        let facts = a.after.facts + u64::from(old.is_none());
        let jobs = a.after.jobs + u64::from(old.is_none());
        state.capacity.check(facts, jobs)?;
        a.after.sequence = sequence;
        a.after.facts = facts;
        a.after.jobs = jobs;
        a.changes.push((
            old,
            Some(AliasFact {
                serial: *serial,
                sequence,
                status: 1,
            }),
        ));
    }
    Ok(())
}
pub(crate) fn apply(
    connection: &Connection,
    state: &Aliases,
    members: Option<&SiteMembership>,
) -> StorageResult<()> {
    let attempt = state.attempt.as_ref().unwrap();
    for (old, new) in &attempt.changes {
        if let Some(old) = old {
            if old.status == 1
                && connection.execute(
                    "DELETE FROM alias_jobs WHERE key=?1 AND serial=?2",
                    rusqlite::params![
                        alias_index::key(&state.scope, 7, old.sequence).as_slice(),
                        old.serial as i64
                    ],
                )? != 1
            {
                return Err(StorageError::Integrity("alias old job acknowledgement"));
            }
            let affected=match new {
                Some(new)=>connection.execute("UPDATE alias_facts SET sequence=?1,status=?2 WHERE key=?3 AND sequence=?4 AND status=?5",
                    rusqlite::params![new.sequence as i64,new.status,alias_index::key(&state.scope,6,old.serial).as_slice(),old.sequence as i64,old.status])?,
                None=>connection.execute("DELETE FROM alias_facts WHERE key=?1 AND sequence=?2 AND status=?3",
                    rusqlite::params![alias_index::key(&state.scope,6,old.serial).as_slice(),old.sequence as i64,old.status])?,
            };
            if affected != 1 {
                return Err(StorageError::Integrity("alias old fact acknowledgement"));
            }
        } else if let Some(new) = new {
            if connection.execute(
                "INSERT INTO alias_facts VALUES(?1,?2,?3)",
                rusqlite::params![
                    alias_index::key(&state.scope, 6, new.serial).as_slice(),
                    new.sequence as i64,
                    new.status
                ],
            )? != 1
            {
                return Err(StorageError::Integrity("alias fact insertion"));
            }
        }
        if let Some(new) = new {
            if new.status == 1
                && connection.execute(
                    "INSERT INTO alias_jobs VALUES(?1,?2)",
                    rusqlite::params![
                        alias_index::key(&state.scope, 7, new.sequence).as_slice(),
                        new.serial as i64
                    ],
                )? != 1
            {
                return Err(StorageError::Integrity("alias job insertion"));
            }
        }
    }
    let t = attempt.after;
    if connection.execute("UPDATE alias_owner SET stage=?1,sequence=?2,facts=?3,jobs=?4,expanded=?5,remaining=?6,after_serial=?7 WHERE id=1",
        rusqlite::params![attempt.new_stage,t.sequence as i64,t.facts as i64,t.jobs as i64,t.expanded as i64,t.remaining as i64,t.after.map(|n|n as i64)])?!=1 {
        return Err(StorageError::Integrity("alias owner counters acknowledgement"));
    }
    if connection.execute(
        "UPDATE alias_owner SET current_serial=?1,current_sequence=?2,progress=?3 WHERE id=1",
        rusqlite::params![
            attempt.new_current.map(|c| c.serial as i64),
            attempt.new_current.map(|c| c.sequence as i64),
            attempt.new_progress.encode().as_slice()
        ],
    )? != 1
    {
        return Err(StorageError::Integrity(
            "alias owner continuation acknowledgement",
        ));
    }
    if let Some(members) = members {
        if connection.execute(
            "UPDATE alias_owner SET members=?1 WHERE id=1 AND members IS NULL",
            [members.encode().as_slice()],
        )? != 1
        {
            return Err(StorageError::Integrity("alias membership acknowledgement"));
        }
    }
    Ok(())
}
pub(crate) fn transaction<T>(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    state: &mut Aliases,
    body: impl FnOnce(&Connection, &mut Aliases) -> StorageResult<T>,
    members: Option<&SiteMembership>,
) -> StorageResult<T> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        alias_index::verify(connection, state)?;
        let value = body(connection, state)?;
        apply(connection, state, members)?;
        Ok(value)
    })();
    profile::finish_transaction_guarded(connection, result, engine)
}
