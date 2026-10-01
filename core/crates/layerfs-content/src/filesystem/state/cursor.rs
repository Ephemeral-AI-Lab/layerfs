//! Fixed continuation with independent ordered-record digest and exact EOF.
use crate::error::{ContentError, ContentResult};

use super::{IndexedState, PageLimit, StateKey, StateLedger, StatePage, StateSeal};

/// One advancing selected scan with an independently accumulated exact seal.
pub struct StateCursor<'a, S: IndexedState + ?Sized = dyn IndexedState> {
    state: &'a mut S,
    seal: StateSeal,
    ledger: StateLedger,
    after: Option<StateKey>,
    finished: bool,
    failure: Option<ContentError>,
}

impl<'a, S: IndexedState + ?Sized> StateCursor<'a, S> {
    /// Borrows the owning provider for one complete scan from the start.
    pub fn new(state: &'a mut S, seal: StateSeal) -> Self {
        let ledger = StateLedger::new(seal.scope().clone());
        Self {
            state,
            seal,
            ledger,
            after: None,
            finished: false,
            failure: None,
        }
    }

    /// Checks one bounded page, exact progress and terminal digest before return.
    /// Completion returns none; failure stays terminal without another request.
    pub fn next_page(&mut self, limit: PageLimit) -> ContentResult<Option<StatePage>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.finished {
            return Ok(None);
        }
        match self.advance(limit) {
            Ok(page) => Ok(Some(page)),
            Err(error) => {
                self.failure = Some(error.clone());
                Err(error)
            }
        }
    }

    fn advance(&mut self, limit: PageLimit) -> ContentResult<StatePage> {
        let page = self.state.page(&self.seal, self.after, limit)?;
        page.check_limit(limit)?;
        if page.seal() != &self.seal {
            return Err(ContentError::InvalidOrderingRecord("state page seal"));
        }
        self.ledger.validate_append(page.records())?;
        let records = self
            .ledger
            .records()
            .checked_add(page.records().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if records > self.seal.records() || page.eof() != (records == self.seal.records()) {
            return Err(ContentError::InvalidOrderingRecord("state page EOF"));
        }
        let expected_last = page
            .records()
            .last()
            .map(|record| record.key())
            .or(self.after);
        if page.last() != expected_last {
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
