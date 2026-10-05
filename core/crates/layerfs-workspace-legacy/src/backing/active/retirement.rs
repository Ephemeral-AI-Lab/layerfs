//! Selecting-pin cohorts. An ordinary publication never scans older owners.
use super::{
    page::PageRef,
    pages::{Counter, PageStore},
};
use crate::{
    backing::budget::{Budget, Charge},
    WorkspaceError,
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy)]
struct Retired {
    page: PageRef,
    birth: u64,
    retired: u64,
}

pub(super) struct Retirement {
    cohorts: BTreeMap<u64, Vec<Retired>>,
    failed: Vec<Retired>,
    count: usize,
    charge: Charge,
}

impl Retirement {
    pub(super) fn new(budget: &Arc<Budget>) -> Result<Self, WorkspaceError> {
        Ok(Self {
            cohorts: BTreeMap::new(),
            failed: Vec::new(),
            count: 0,
            charge: budget.reserve(0)?,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.count
    }

    /// Includes bucket storage, capacities and reallocation overlap. Every
    /// eventual entry is reserved before the publication selecting its retire.
    pub(super) fn reserve(&mut self, additional: usize) -> Result<(), WorkspaceError> {
        let bytes = self
            .count
            .checked_add(additional)
            .and_then(|count| count.checked_mul(256))
            .ok_or(WorkspaceError::Capacity)?;
        self.charge.resize(bytes)
    }

    fn hold(&mut self, selector: u64, item: Retired) {
        let bucket = self.cohorts.entry(selector).or_default();
        bucket.reserve_exact(1);
        bucket.push(item);
        self.count += 1;
    }

    fn release_one(&mut self, store: &PageStore, item: Retired) -> Result<u64, WorkspaceError> {
        match store.release(item.page) {
            Ok(bytes) => Ok(bytes),
            Err(error) => {
                self.failed.reserve_exact(1);
                self.failed.push(item);
                self.count += 1;
                Err(error)
            }
        }
    }

    pub(super) fn retire(
        &mut self,
        store: &PageStore,
        page: PageRef,
        retired: u64,
        mut latest: impl FnMut(u64, u64) -> Result<Option<u64>, WorkspaceError>,
    ) -> Result<(), WorkspaceError> {
        let item = Retired {
            page,
            birth: store.birth(page)?,
            retired,
        };
        store.count(Counter::Retirement, 1);
        match latest(item.birth, retired)? {
            Some(selector) => self.hold(selector, item),
            None => {
                self.release_one(store, item)?;
            }
        }
        Ok(())
    }

    /// Only a final selecting-pin release can visit this bucket. Unlink
    /// failures remain in explicit charged custody after the bucket vanishes.
    pub(super) fn release_selector(
        &mut self,
        store: &PageStore,
        selector: u64,
        mut latest: impl FnMut(u64, u64) -> Result<Option<u64>, WorkspaceError>,
    ) -> Result<u64, WorkspaceError> {
        let Some(bucket) = self.cohorts.remove(&selector) else {
            return Ok(0);
        };
        self.count -= bucket.len();
        let mut released = 0;
        let mut failure = None;
        for item in bucket {
            store.count(Counter::Retirement, 1);
            match latest(item.birth, item.retired) {
                Ok(Some(selector)) => self.hold(selector, item),
                Ok(None) => match self.release_one(store, item) {
                    Ok(bytes) => released += bytes,
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                },
                Err(error) => {
                    self.failed.reserve_exact(1);
                    self.failed.push(item);
                    self.count += 1;
                    failure.get_or_insert(error);
                }
            }
        }
        self.charge.resize(self.count * 256)?;
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(released)
    }

    pub(super) fn clear(&mut self) -> Result<(), WorkspaceError> {
        if self.count != 0 {
            return Err(WorkspaceError::Busy);
        }
        self.cohorts = BTreeMap::new();
        self.failed = Vec::new();
        self.charge.resize(0)
    }
}
