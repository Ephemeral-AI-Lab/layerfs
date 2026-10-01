//! Nonborrowing exact DirectoryRoots scan, permitting typed sibling point consumers.
use super::{IndexedState, PageLimit, StateKey, StateLedger, StatePage, StateSeal};
use crate::{ContentError, ContentResult};
pub(crate) struct RootCursor {
    seal: StateSeal,
    ledger: StateLedger,
    after: Option<StateKey>,
    finished: bool,
    failure: Option<ContentError>,
}
impl RootCursor {
    pub(crate) fn new(seal: StateSeal) -> Self {
        Self {
            ledger: StateLedger::new(seal.scope().clone()),
            seal,
            after: None,
            finished: false,
            failure: None,
        }
    }
    pub(crate) fn page_capacity(&self, limit: PageLimit) -> usize {
        let remaining = self.seal.records() - self.ledger.records();
        limit
            .fitting_records()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX))
    }
    pub(crate) fn page_working_bytes(&self, limit: PageLimit) -> usize {
        std::mem::size_of::<StatePage>()
            + self.page_capacity(limit) * std::mem::size_of::<super::StateRecord>()
    }
    pub(crate) fn next_page<S: IndexedState + ?Sized>(
        &mut self,
        state: &mut S,
        limit: PageLimit,
        admitted: bool,
    ) -> ContentResult<Option<StatePage>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.finished {
            return Ok(None);
        }
        let result = self.advance(state, limit, admitted);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result.map(Some)
    }
    fn advance<S: IndexedState + ?Sized>(
        &mut self,
        state: &mut S,
        limit: PageLimit,
        admitted: bool,
    ) -> ContentResult<StatePage> {
        let page = state.page(&self.seal, self.after, limit)?;
        page.check_limit(limit)?;
        if admitted && page.retained_capacity() > self.page_capacity(limit) {
            return Err(ContentError::InvalidOrderingRecord(
                "state admitted page capacity",
            ));
        }
        if page.seal() != &self.seal {
            return Err(ContentError::InvalidOrderingRecord("state page seal"));
        }
        self.ledger.validate_append(page.records())?;
        let count = self
            .ledger
            .records()
            .checked_add(page.records().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if count > self.seal.records() || page.eof() != (count == self.seal.records()) {
            return Err(ContentError::InvalidOrderingRecord("state page EOF"));
        }
        let expected = page.records().last().map(|r| r.key()).or(self.after);
        if page.last() != expected {
            return Err(ContentError::InvalidOrderingRecord(
                "state page continuation",
            ));
        }
        self.ledger.acknowledge(page.records())?;
        if page.eof() && self.ledger.seal() != self.seal {
            return Err(ContentError::InvalidOrderingRecord("state page digest"));
        }
        self.after = page.last();
        self.finished = page.eof();
        Ok(page)
    }
}
