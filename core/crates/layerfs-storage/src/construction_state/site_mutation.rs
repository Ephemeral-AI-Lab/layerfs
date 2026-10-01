//! Bounded atomic site births and monotone base observations.

use layerfs_content::filesystem::state::{ClaimAdmission, SiteObservation, SiteRecord};
use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};

use super::sites::Sites;
use super::{profile, site_index};

pub(crate) fn insert(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    state: &Sites,
    records: &[SiteRecord],
    local_duplicate: bool,
) -> StorageResult<ClaimAdmission> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        site_index::verify(connection, state)?;
        let mut by_key = connection.prepare(site_index::GET)?;
        let mut by_point = connection.prepare(site_index::GET_POINT)?;
        let mut duplicate = local_duplicate;
        // No INSERT precedes validation of every selected stored class/point.
        for record in records {
            let mut rows = by_key.query([record.key().as_bytes().as_slice()])?;
            if let Some(row) = rows.next()? {
                if !site_index::decode(&state.scope, row)?.is_birth() {
                    return Err(StorageError::Integrity(
                        "construction scratch premature site facts",
                    ));
                }
                duplicate = true;
            }
            let mut rows = by_point.query(rusqlite::params![
                record.point().parent() as i64,
                i64::from(record.point().binding_ordinal())
            ])?;
            if let Some(row) = rows.next()? {
                if !site_index::decode(&state.scope, row)?.is_birth() {
                    return Err(StorageError::Integrity(
                        "construction scratch premature site facts",
                    ));
                }
                duplicate = true;
            }
        }
        drop(by_key);
        drop(by_point);
        if duplicate {
            return Ok(ClaimAdmission::Duplicate);
        }
        let count = state
            .records
            .checked_add(records.len() as u64)
            .ok_or(StorageError::Integrity("construction scratch site count"))?;
        if count > state.declared_sites {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch site rows",
                limit: state.declared_sites,
                actual: count,
            });
        }
        let mut statement = connection.prepare("INSERT INTO binding_sites(key,flags,point,parent,binding_ordinal) VALUES(?1,?2,?3,?4,?5)")?;
        for record in records {
            if statement.execute(rusqlite::params![
                record.key().as_bytes().as_slice(),
                i64::from(record.flags()),
                record.point().encode().as_slice(),
                record.point().parent() as i64,
                i64::from(record.point().binding_ordinal())
            ])? != 1
            {
                return Err(StorageError::Integrity(
                    "construction scratch site insertion",
                ));
            }
        }
        drop(statement);
        if connection.execute(
            "UPDATE site_owner SET records=?1 WHERE id=1 AND stage=0 AND records=?2 AND scope=?3",
            rusqlite::params![
                count as i64,
                state.records as i64,
                state.scope.as_bytes().as_slice()
            ],
        )? != 1
        {
            return Err(StorageError::Integrity(
                "construction scratch site birth acknowledgement",
            ));
        }
        Ok(ClaimAdmission::Fresh)
    })();
    match result {
        Ok(ClaimAdmission::Fresh) => {
            profile::finish_write_guarded(connection, Ok(()), engine)?;
            Ok(ClaimAdmission::Fresh)
        }
        Ok(ClaimAdmission::Duplicate) => {
            if let Some(guard) = engine {
                guard
                    .validate()
                    .map_err(|original| StorageError::UnknownOutcome {
                        original: Box::new(original),
                    })?;
            }
            crate::sqlite::write::rollback(connection)?;
            Ok(ClaimAdmission::Duplicate)
        }
        Err(error) => {
            profile::finish_write_guarded(connection, Err(error), engine)?;
            unreachable!()
        }
    }
}

pub(crate) fn observe(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    state: &mut Sites,
    observations: &[SiteObservation],
) -> StorageResult<()> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        site_index::verify(connection, state)?;
        let mut lookup = connection.prepare(site_index::GET)?;
        for observation in observations {
            let mut rows = lookup.query([observation.key().as_bytes().as_slice()])?;
            let row = rows.next()?.ok_or(StorageError::Integrity(
                "construction scratch absent observed site",
            ))?;
            let record = site_index::decode(&state.scope, row)?;
            let mut proposed = record.observe(observation.legal())?.flags();
            let attempt = state.attempt.as_mut().unwrap();
            for (prior, flags) in attempt.records[..attempt.count]
                .iter()
                .zip(&attempt.proposed_flags[..attempt.count])
            {
                if prior.is_some_and(|prior| prior.key() == record.key()) {
                    proposed |= *flags;
                }
            }
            attempt.include(record, proposed);
        }
        drop(lookup);
        let mut statement = connection.prepare("UPDATE binding_sites SET flags=flags|?1 WHERE key=?2 AND point=?3 AND parent=?4 AND binding_ordinal=?5 AND (flags&?6)=?6")?;
        let attempt = state.attempt.as_ref().unwrap();
        for (record, proposed) in attempt.records[..attempt.count]
            .iter()
            .zip(&attempt.proposed_flags[..attempt.count])
        {
            let record = record.unwrap();
            if statement.execute(rusqlite::params![
                i64::from(*proposed),
                record.key().as_bytes().as_slice(),
                record.point().encode().as_slice(),
                record.point().parent() as i64,
                i64::from(record.point().binding_ordinal()),
                i64::from(record.flags())
            ])? != 1
            {
                return Err(StorageError::Integrity(
                    "construction scratch site observation acknowledgement",
                ));
            }
        }
        Ok(())
    })();
    profile::finish_write_guarded(connection, result, engine)
}
