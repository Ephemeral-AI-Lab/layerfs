//! Exact ordered seal folds and acknowledged bounded retirement.
use super::{fact_index, ScratchSession};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, FactLedger, FactScope, FactSeal, ParentFact, ParentSeal, BASE_FACT_BYTES,
    PARENT_ELIGIBILITY_BYTES,
};
use rusqlite::Connection;

impl ScratchSession {
    /// Freeze BaseFacts after all dependent validation/graph consumers end.
    pub fn fact_seal(&mut self, scope: &FactScope) -> StorageResult<FactSeal> {
        let seal = self.fact_change(scope, "base_seal", 0, |connection, state| {
            if state.base.stage != 1 {
                return Err(StorageError::Integrity("base fact seal phase"));
            }
            let (seal, bound) = fold(
                connection,
                scope,
                state.base.records,
                state.base.maximum,
                state.memory.clone(),
            )?;
            if bound != 0 || seal.bytes != state.base.bytes {
                return Err(StorageError::Integrity("base fact seal totals"));
            }
            let a = state.attempt.as_mut().unwrap();
            a.after.stage = 2;
            a.after.remaining = a.after.records;
            a.after.digest = Some(seal.digest);
            Ok(seal)
        })?;
        self.resource
            .as_mut()
            .unwrap()
            .facts
            .as_mut()
            .unwrap()
            .base_seal = Some(seal.clone());
        Ok(seal)
    }
    /// Freeze exact declared-parent bound/excluded totals after incoming bindings.
    pub fn parent_seal(&mut self, scope: &FactScope) -> StorageResult<ParentSeal> {
        let seal = self.fact_change(scope, "parent_seal", 0, |connection, state| {
            if state.parent.stage != 2 {
                return Err(StorageError::Integrity("parent seal phase"));
            }
            let (facts, bound) = fold(
                connection,
                scope,
                state.parent.records,
                state.parent.maximum,
                state.memory.clone(),
            )?;
            if bound != state.parent.bound || facts.bytes != state.parent.bytes {
                return Err(StorageError::Integrity("parent sealed counts"));
            }
            let a = state.attempt.as_mut().unwrap();
            a.after.stage = 3;
            a.after.remaining = a.after.records;
            a.after.digest = Some(facts.digest);
            Ok(ParentSeal { facts, bound })
        })?;
        self.resource
            .as_mut()
            .unwrap()
            .facts
            .as_mut()
            .unwrap()
            .parent_seal = Some(seal.clone());
        Ok(seal)
    }
    /// Known exact bounded BaseFacts retirement; no native cleanup or refund.
    pub fn fact_retire(&mut self, seal: &FactSeal) -> StorageResult<()> {
        self.fact_selected(&seal.scope)?;
        if self
            .resource
            .as_ref()
            .unwrap()
            .facts
            .as_ref()
            .unwrap()
            .base_seal
            .as_ref()
            != Some(seal)
        {
            return self.finish(Err(StorageError::Integrity(
                "base fact exact retirement seal",
            )));
        }
        self.retire_facts(seal)
    }
    /// Known exact bounded parent retirement after canonical consumers end.
    pub fn parent_retire(&mut self, seal: &ParentSeal) -> StorageResult<()> {
        self.fact_selected(&seal.facts.scope)?;
        if self
            .resource
            .as_ref()
            .unwrap()
            .facts
            .as_ref()
            .unwrap()
            .parent_seal
            .as_ref()
            != Some(seal)
        {
            return self.finish(Err(StorageError::Integrity("parent exact retirement seal")));
        }
        self.retire_facts(&seal.facts)
    }
    fn retire_facts(&mut self, seal: &FactSeal) -> StorageResult<()> {
        let code = seal.scope.state().table().code();
        loop {
            let count = self
                .resource
                .as_ref()
                .unwrap()
                .facts
                .as_ref()
                .unwrap()
                .section(code)
                .remaining
                .min(128) as usize;
            let kind = if code == 17 {
                "base_retire"
            } else {
                "parent_retire"
            };
            let done = self.fact_change(&seal.scope, kind, count, |connection, state| {
                let section = state.section(code);
                if (code == 17 && !matches!(section.stage, 2 | 3))
                    || (code == 18 && !matches!(section.stage, 3 | 4))
                {
                    return Err(StorageError::Integrity("fact retirement phase"));
                }
                let lower = section.after.map(|s| seal.scope.key(s)).transpose()?;
                let column = if code == 17 { "value" } else { "bound" };
                let sql = format!(
                    "SELECT key,{column} FROM {} WHERE key>?1 ORDER BY key LIMIT ?2",
                    fact_index::table(code)
                );
                let mut statement = connection.prepare(&sql)?;
                let mut rows = statement.query(rusqlite::params![
                    lower.as_ref().map_or(&[][..], |k| k.as_slice()),
                    count as i64
                ])?;
                let mut last = section.after;
                let mut found = 0usize;
                while let Some(row) = rows.next()? {
                    let serial = seal.scope.serial(fact_index::blob(row.get_ref(0)?, 25)?)?;
                    if last.is_some_and(|old| old >= serial) {
                        return Err(StorageError::Integrity("fact retirement order"));
                    }
                    if code == 17 {
                        let fact =
                            BaseFact::decode(serial, fact_index::blob(row.get_ref(1)?, 74)?)?;
                        state.attempt.as_mut().unwrap().bases.push(fact);
                    } else {
                        let bound: u8 = row.get(1)?;
                        if bound > 1 {
                            return Err(StorageError::Integrity("parent retirement bound"));
                        }
                        state.attempt.as_mut().unwrap().parents.push((
                            Some(ParentFact {
                                serial,
                                bound: bound == 1,
                            }),
                            None,
                        ));
                    }
                    found += 1;
                    last = Some(serial);
                }
                if found != count {
                    return Err(StorageError::Integrity("fact retirement count"));
                }
                let a = state.attempt.as_mut().unwrap();
                a.after.remaining -= count as u64;
                a.after.after = last;
                let done = a.after.remaining == 0;
                if done && last != seal.maximum {
                    return Err(StorageError::Integrity("fact retirement last"));
                }
                a.after.stage = if code == 17 {
                    if done {
                        4
                    } else {
                        3
                    }
                } else if done {
                    5
                } else {
                    4
                };
                Ok(done)
            })?;
            if done {
                let result = (|| {
                    let r = self.resource.as_ref().unwrap();
                    let sql = format!(
                        "SELECT NOT EXISTS(SELECT 1 FROM {} LIMIT 1)",
                        fact_index::table(code)
                    );
                    let empty: bool = r.verify()?.query_row(&sql, [], |row| row.get(0))?;
                    if !empty {
                        return Err(StorageError::Integrity("fact retirement EOF"));
                    }
                    Ok(())
                })();
                return self.finish(result);
            }
        }
    }
}
fn fold(
    connection: &Connection,
    scope: &FactScope,
    records: u64,
    maximum: Option<u64>,
    memory: layerfs_content::filesystem::state::GraphMemory,
) -> StorageResult<(FactSeal, u64)> {
    let code = scope.state().table().code();
    let mut ledger = FactLedger::new(scope.clone())?;
    let mut bound = 0u64;
    let width = if code == 17 {
        std::mem::size_of::<BaseFact>()
    } else {
        std::mem::size_of::<ParentFact>()
    };
    let _window = memory.reserve(128 * width + std::mem::size_of::<FactLedger>())?;
    while ledger.records() < records {
        let count = (records - ledger.records()).min(128) as usize;
        let lower = ledger.last().map(|s| scope.key(s)).transpose()?;
        let column = if code == 17 { "value" } else { "bound" };
        let sql = format!(
            "SELECT key,{column} FROM {} WHERE key>?1 ORDER BY key LIMIT ?2",
            fact_index::table(code)
        );
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query(rusqlite::params![
            lower.as_ref().map_or(&[][..], |k| k.as_slice()),
            count as i64
        ])?;
        let mut found = 0;
        while let Some(row) = rows.next()? {
            let serial = scope.serial(fact_index::blob(row.get_ref(0)?, 25)?)?;
            if code == 17 {
                ledger.base(&[BaseFact::decode(
                    serial,
                    fact_index::blob(row.get_ref(1)?, 74)?,
                )?])?;
            } else {
                let value: u8 = row.get(1)?;
                if value > 1 {
                    return Err(StorageError::Integrity("parent seal bound framing"));
                }
                ledger.parents(&[ParentFact {
                    serial,
                    bound: value == 1,
                }])?;
                bound += u64::from(value);
            }
            found += 1;
        }
        if found != count {
            return Err(StorageError::Integrity("fact seal cardinality"));
        }
    }
    let seal = ledger.seal();
    let width = if code == 17 {
        BASE_FACT_BYTES
    } else {
        PARENT_ELIGIBILITY_BYTES
    };
    if seal.maximum != maximum || seal.records != records || seal.bytes != records * width {
        return Err(StorageError::Integrity("fact seal EOF"));
    }
    let sql = format!(
        "SELECT key FROM {} ORDER BY key DESC LIMIT 1",
        fact_index::table(code)
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([])?;
    let actual = match rows.next()? {
        Some(row) => Some(scope.serial(fact_index::blob(row.get_ref(0)?, 25)?)?),
        None => None,
    };
    if actual != maximum {
        return Err(StorageError::Integrity("fact seal maximum"));
    }
    Ok((seal, bound))
}
