use super::DiskTable;
use crate::{EngineError, EngineResult};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScratchObservation {
    pub tables: u64,
    pub statements: u64,
    pub rows: u64,
    pub high_water_bytes: u64,
    pub owner_setup_statements: u64,
    pub derived_setup_statements: u64,
    pub operation_statements: u64,
    pub store_reopens: u64,
    pub store_inspection_statements: u64,
    pub store_inspection_wall_ns: u64,
    pub setup_wall_ns: u64,
    pub operation_wall_ns: u64,
}

impl ScratchObservation {
    pub fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            tables: self.tables.checked_sub(before.tables)?,
            statements: self.statements.checked_sub(before.statements)?,
            rows: self.rows.checked_sub(before.rows)?,
            high_water_bytes: self.high_water_bytes.checked_sub(before.high_water_bytes)?,
            owner_setup_statements: self
                .owner_setup_statements
                .checked_sub(before.owner_setup_statements)?,
            derived_setup_statements: self
                .derived_setup_statements
                .checked_sub(before.derived_setup_statements)?,
            operation_statements: self
                .operation_statements
                .checked_sub(before.operation_statements)?,
            store_reopens: self.store_reopens.checked_sub(before.store_reopens)?,
            store_inspection_statements: self
                .store_inspection_statements
                .checked_sub(before.store_inspection_statements)?,
            store_inspection_wall_ns: self
                .store_inspection_wall_ns
                .checked_sub(before.store_inspection_wall_ns)?,
            setup_wall_ns: self.setup_wall_ns.checked_sub(before.setup_wall_ns)?,
            operation_wall_ns: self
                .operation_wall_ns
                .checked_sub(before.operation_wall_ns)?,
        })
    }
}

impl DiskTable {
    pub fn observation(&self) -> EngineResult<ScratchObservation> {
        self.observe_storage()?;
        Ok(ScratchObservation {
            tables: 1,
            statements: self.statements.get(),
            rows: self.rows.get(),
            high_water_bytes: self.high_water_bytes.get(),
            owner_setup_statements: self.owner_setup_statements.get(),
            derived_setup_statements: self.derived_setup_statements.get(),
            operation_statements: self.operation_statements.get(),
            store_reopens: self.store_reopens,
            store_inspection_statements: self.store_inspection_statements,
            store_inspection_wall_ns: self.store_inspection_wall_ns,
            setup_wall_ns: self.setup_wall_ns,
            operation_wall_ns: self.operation_wall_ns.get(),
        })
    }

    pub(super) fn mark_statement(&self) -> EngineResult<()> {
        self.statements.set(
            self.statements
                .get()
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
        );
        self.operation_statements.set(
            self.operation_statements
                .get()
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
        );
        Ok(())
    }

    pub(super) fn mark_owner_setup_statement(&self) -> EngineResult<()> {
        self.statements.set(
            self.statements
                .get()
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
        );
        self.owner_setup_statements.set(
            self.owner_setup_statements
                .get()
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
        );
        Ok(())
    }

    pub(super) fn mark_derived_setup_statement(&self) -> EngineResult<()> {
        self.statements.set(
            self.statements
                .get()
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
        );
        self.derived_setup_statements.set(
            self.derived_setup_statements
                .get()
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
        );
        Ok(())
    }

    pub(super) fn mark_rows(&self, rows: u64) -> EngineResult<()> {
        self.rows.set(
            self.rows
                .get()
                .checked_add(rows)
                .ok_or(EngineError::CounterOverflow)?,
        );
        Ok(())
    }

    pub(super) fn observe_storage(&self) -> EngineResult<()> {
        self.high_water_bytes
            .set(self.high_water_bytes.get().max(self.storage_bytes()?));
        Ok(())
    }

    pub(super) fn observe_operation_time(&self, started: Instant) {
        let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.operation_wall_ns
            .set(self.operation_wall_ns.get().saturating_add(elapsed));
    }
}
