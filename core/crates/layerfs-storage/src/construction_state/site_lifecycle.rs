//! Transactional immutable membership closure, final seal and bounded retirement.

use layerfs_content::filesystem::state::{
    SiteBirthLedger, SiteBirthSeal, SiteKey, SiteLedger, SiteMembership, SiteSeal,
};
use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};

use super::sites::{SiteAttempt, SiteAttemptKind, SiteStage, Sites};
use super::{profile, site_index};

fn birth_projection(
    connection: &Connection,
    state: &Sites,
    allow_facts: bool,
) -> StorageResult<(SiteBirthSeal, Option<SiteKey>)> {
    let maximum = site_index::maximum(connection, &state.scope)?;
    let birth_maximum = site_index::birth_maximum(connection, &state.scope)?;
    if maximum.is_none() != (state.records == 0) || birth_maximum.is_none() != (state.records == 0)
    {
        return Err(StorageError::Integrity(
            "construction scratch site terminal count",
        ));
    }
    let mut ledger = SiteBirthLedger::new(state.scope.clone())?;
    while ledger.records() < state.records {
        let count = (state.records - ledger.records()).min(128) as usize;
        let mut rows = site_index::read_birth(connection, &state.scope, ledger.last(), count)?;
        if rows.len() != count {
            return Err(StorageError::Integrity(
                "construction scratch site birth cardinality",
            ));
        }
        for record in &mut rows {
            if !allow_facts && !record.is_birth() {
                return Err(StorageError::Integrity(
                    "construction scratch premature site facts",
                ));
            }
            *record = record.birth_projection();
        }
        ledger.acknowledge(&rows)?;
    }
    if ledger.last() != birth_maximum
        || ledger.maximum() != maximum
        || ledger.encoded_bytes() != state.records * 60
    {
        return Err(StorageError::Integrity(
            "construction scratch site birth EOF",
        ));
    }
    Ok((ledger.seal(), maximum))
}

pub(crate) fn close(
    connection: &Connection,
    state: &mut Sites,
    expected: &SiteBirthSeal,
) -> StorageResult<SiteMembership> {
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        site_index::verify(connection, state)?;
        let (birth, maximum) = birth_projection(connection, state, false)?;
        let members = SiteMembership::new(birth, maximum)?;
        state.proposed_membership = Some(members.clone());
        if members.birth() != expected {
            return Err(StorageError::Integrity(
                "construction scratch expected site birth seal",
            ));
        }
        if connection.execute("UPDATE site_owner SET stage=1,remaining=?1,birth_digest=?2,birth_max=?3 WHERE id=1 AND stage=0 AND records=?1 AND scope=?4",
            rusqlite::params![state.records as i64, members.birth().digest().as_slice(), maximum.as_ref().map(|key| key.as_bytes().as_slice()), state.scope.as_bytes().as_slice()])? != 1 {
            return Err(StorageError::Integrity("construction scratch site membership acknowledgement"));
        }
        Ok(members)
    })();
    profile::finish_transaction(connection, result)
}

pub(crate) fn seal(connection: &Connection, state: &mut Sites) -> StorageResult<SiteSeal> {
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        site_index::verify(connection, state)?;
        // Both complete streams share this one transaction. Membership/point/
        // immutable-base corruption cannot be blessed by the full-flags seal.
        let (birth, maximum) = birth_projection(connection, state, true)?;
        let members = state.membership.as_ref().unwrap();
        if &birth != members.birth() || maximum != members.maximum() {
            return Err(StorageError::Integrity(
                "construction scratch immutable site projection",
            ));
        }
        let mut ledger = SiteLedger::new(state.scope.clone())?;
        while ledger.records() < state.records {
            let count = (state.records - ledger.records()).min(128) as usize;
            let rows = site_index::read(connection, &state.scope, ledger.last(), count)?;
            if rows.len() != count {
                return Err(StorageError::Integrity(
                    "construction scratch final site cardinality",
                ));
            }
            ledger.acknowledge(&rows)?;
        }
        if ledger.last() != maximum || ledger.encoded_bytes() != state.records * 60 {
            return Err(StorageError::Integrity(
                "construction scratch final site EOF",
            ));
        }
        let seal = ledger.seal();
        state.proposed_seal = Some((seal.clone(), maximum));
        if connection.execute("UPDATE site_owner SET stage=2,final_digest=?1,final_max=?2 WHERE id=1 AND stage=1 AND records=?3 AND birth_digest=?4 AND scope=?5",
            rusqlite::params![seal.digest().as_slice(), maximum.as_ref().map(|key| key.as_bytes().as_slice()), state.records as i64, members.birth().digest().as_slice(), state.scope.as_bytes().as_slice()])? != 1 {
            return Err(StorageError::Integrity("construction scratch final site acknowledgement"));
        }
        Ok(seal)
    })();
    profile::finish_transaction(connection, result)
}

pub(crate) struct Retirement {
    pub(crate) remaining: u64,
    pub(crate) after: Option<SiteKey>,
    pub(crate) stage: SiteStage,
}

pub(crate) fn retire_window(
    connection: &Connection,
    state: &mut Sites,
) -> StorageResult<Retirement> {
    state.attempt = Some(SiteAttempt::new(
        SiteAttemptKind::Retire,
        state.remaining,
        state.remaining,
    ));
    state.attempt.as_mut().unwrap().prior_after = state.after;
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        site_index::verify(connection, state)?;
        let count = state.remaining.min(128) as usize;
        let rows = site_index::read(connection, &state.scope, state.after, count)?;
        if rows.len() != count {
            return Err(StorageError::Integrity(
                "construction scratch site retirement cardinality",
            ));
        }
        let remaining = state.remaining - count as u64;
        let after = rows.last().map(|record| record.key()).or(state.after);
        let maximum = state.maximum();
        if after.is_some_and(|key| maximum.is_none_or(|maximum| key > maximum))
            || (remaining == 0 && after != maximum)
        {
            return Err(StorageError::Integrity(
                "construction scratch site retirement terminal key",
            ));
        }
        let attempt = state.attempt.as_mut().unwrap();
        attempt.proposed = remaining;
        attempt.proposed_after = after;
        for record in &rows {
            attempt.include(*record, record.flags());
        }
        let mut statement = connection.prepare("DELETE FROM binding_sites WHERE key=?1 AND flags=?2 AND point=?3 AND parent=?4 AND binding_ordinal=?5")?;
        for record in &rows {
            if statement.execute(rusqlite::params![
                record.key().as_bytes().as_slice(),
                i64::from(record.flags()),
                record.point().encode().as_slice(),
                record.point().parent() as i64,
                i64::from(record.point().binding_ordinal())
            ])? != 1
            {
                return Err(StorageError::Integrity(
                    "construction scratch site retirement affected rows",
                ));
            }
        }
        drop(statement);
        if remaining == 0 && !site_index::empty(connection)? {
            return Err(StorageError::Integrity(
                "construction scratch site retirement EOF",
            ));
        }
        let stage = if remaining == 0 {
            SiteStage::Retired
        } else {
            SiteStage::Retiring
        };
        if connection.execute("UPDATE site_owner SET stage=?1,remaining=?2,after_key=?3 WHERE id=1 AND stage=?4 AND remaining=?5 AND scope=?6 AND final_digest=?7",
            rusqlite::params![stage as u8, remaining as i64, after.as_ref().map(|key| key.as_bytes().as_slice()), state.stage as u8, state.remaining as i64, state.scope.as_bytes().as_slice(), state.seal.as_ref().unwrap().digest().as_slice()])? != 1 {
            return Err(StorageError::Integrity("construction scratch site retirement acknowledgement"));
        }
        Ok(Retirement {
            remaining,
            after,
            stage,
        })
    })();
    profile::finish_transaction(connection, result)
}
