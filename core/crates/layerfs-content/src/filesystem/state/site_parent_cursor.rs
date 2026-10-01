//! One advancing immutable parent projection, with terminal failures and MAX.
use super::{BindingSiteState, SiteMembership, SiteParentPage, SiteParentPageLimit};
use crate::error::{ContentError, ContentResult};
/// One scoped parent scan with no previous page or growing frontier retained.
pub struct SiteParentCursor<'a, S: BindingSiteState + ?Sized> {
    state: &'a mut S,
    members: SiteMembership,
    parent: u64,
    after: Option<u32>,
    maximum: Option<Option<u32>>,
    seen: u64,
    finished: bool,
    failure: Option<ContentError>,
}
impl<'a, S: BindingSiteState + ?Sized> SiteParentCursor<'a, S> {
    /// Requires a positive exact parent before querying its projection.
    pub fn new(state: &'a mut S, members: SiteMembership, parent: u64) -> ContentResult<Self> {
        if parent == 0 || parent > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("site parent"));
        }
        Ok(Self {
            state,
            members,
            parent,
            after: None,
            maximum: None,
            seen: 0,
            finished: false,
            failure: None,
        })
    }
    /// Validates exact selected context/progress/MAX; failure never resends.
    pub fn next_page(
        &mut self,
        limit: SiteParentPageLimit,
    ) -> ContentResult<Option<SiteParentPage>> {
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
    fn advance(&mut self, limit: SiteParentPageLimit) -> ContentResult<SiteParentPage> {
        let page = self
            .state
            .site_parent_page(&self.members, self.parent, self.after, limit)?;
        page.check_limit(limit)?;
        if page.members() != &self.members
            || page.parent() != self.parent
            || self.maximum.is_some_and(|max| max != page.maximum())
        {
            return Err(ContentError::InvalidOrderingRecord(
                "site parent selected page",
            ));
        }
        let mut last = self.after;
        for record in page.records() {
            let ordinal = record.point().binding_ordinal();
            if last.is_some_and(|prior| prior >= ordinal) {
                return Err(ContentError::InvalidOrderingRecord(
                    "site parent cursor order",
                ));
            }
            last = Some(ordinal);
        }
        let seen = self
            .seen
            .checked_add(page.records().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if page.last() != last
            || seen > self.members.birth().records()
            || page.eof() != (last == page.maximum())
        {
            return Err(ContentError::InvalidOrderingRecord(
                "site parent cursor EOF",
            ));
        }
        self.seen = seen;
        self.maximum = Some(page.maximum());
        self.after = last;
        self.finished = page.eof();
        Ok(page)
    }
}
