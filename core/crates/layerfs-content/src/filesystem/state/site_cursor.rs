//! Fixed continuation with independent ordered-record digest and exact EOF.
use crate::error::{ContentError, ContentResult};

use super::{BindingSiteState, SiteKey, SiteLedger, SitePage, SitePageLimit, SiteSeal};

/// One advancing selected scan with an independently accumulated exact seal.
pub struct SiteCursor<'a, S: BindingSiteState + ?Sized> {
    state: &'a mut S,
    seal: SiteSeal,
    ledger: SiteLedger,
    after: Option<SiteKey>,
    finished: bool,
    failure: Option<ContentError>,
}

impl<'a, S: BindingSiteState + ?Sized> SiteCursor<'a, S> {
    /// Borrows the owning provider for one complete scan from the start.
    pub fn new(state: &'a mut S, seal: SiteSeal) -> ContentResult<Self> {
        let ledger = SiteLedger::new(seal.scope().clone())?;
        Ok(Self {
            state,
            seal,
            ledger,
            after: None,
            finished: false,
            failure: None,
        })
    }

    /// Checks one bounded page, exact progress and terminal digest before return.
    /// Completion returns none; failure stays terminal without another request.
    pub fn next_page(&mut self, limit: SitePageLimit) -> ContentResult<Option<SitePage>> {
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

    pub(crate) fn state(&mut self) -> &mut S {
        self.state
    }

    fn advance(&mut self, limit: SitePageLimit) -> ContentResult<SitePage> {
        let page = self.state.site_sealed_page(&self.seal, self.after, limit)?;
        page.check_limit(limit)?;
        if page.seal() != &self.seal {
            return Err(ContentError::InvalidOrderingRecord("site page seal"));
        }
        self.ledger.validate_append(page.records())?;
        let records = self
            .ledger
            .records()
            .checked_add(page.records().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if records > self.seal.records() || page.eof() != (records == self.seal.records()) {
            return Err(ContentError::InvalidOrderingRecord("site page EOF"));
        }
        let expected_last = page
            .records()
            .last()
            .map(|record| record.key())
            .or(self.after);
        if page.last() != expected_last {
            return Err(ContentError::InvalidOrderingRecord(
                "site page continuation",
            ));
        }
        self.ledger.acknowledge(page.records())?;
        if page.eof() && self.ledger.seal() != self.seal {
            return Err(ContentError::InvalidOrderingRecord("site page digest"));
        }
        self.after = page.last();
        self.finished = page.eof();
        Ok(page)
    }
}
