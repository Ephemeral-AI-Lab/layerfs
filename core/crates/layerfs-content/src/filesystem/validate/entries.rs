//! The existing effective-name merge, consumed with one base page/lookahead.
use super::{charge_directory, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::rows::view::DirectoryRow;
use crate::filesystem::{DirectoryRoot, PathName};
use crate::object::{AuthenticatedObjects, ObjectId};

pub(super) struct EffectiveEntries<'a> {
    reader: &'a dyn AuthenticatedObjects,
    base: Option<DirectoryRoot>,
    after: Option<PathName>,
    page: std::vec::IntoIter<(PathName, u64)>,
    base_next: Option<(PathName, u64)>,
    base_done: bool,
    changes: Box<dyn Iterator<Item = ContentResult<(PathName, Option<u64>)>> + 'a>,
    change_next: Option<(PathName, Option<u64>)>,
    changes_done: bool,
}
impl<'a> EffectiveEntries<'a> {
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        base: Option<ObjectId>,
        row: Option<&'a DirectoryRow<'_>>,
    ) -> ContentResult<Self> {
        let changes: Box<dyn Iterator<Item = ContentResult<(PathName, Option<u64>)>> + 'a> =
            match row {
                Some(row) => row.changes()?,
                None => Box::new(std::iter::empty()),
            };
        Ok(Self {
            reader,
            base: base.map(DirectoryRoot),
            after: None,
            page: Vec::new().into_iter(),
            base_next: None,
            base_done: base.is_none(),
            changes,
            change_next: None,
            changes_done: false,
        })
    }
    fn base_next(
        &mut self,
        visited: &mut usize,
        limit: usize,
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        while self.base_next.is_none() {
            if let Some(entry) = self.page.next() {
                self.base_next = Some(entry);
                break;
            }
            if self.base_done {
                break;
            }
            let mut directory = DirectoryReadWork::default();
            let page = list_after(
                self.reader,
                self.base.expect("base cursor"),
                self.after.as_ref(),
                64,
                crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
                &mut directory,
            )?;
            charge_directory(work, directory);
            *visited = visited.saturating_add(page.entries.len());
            work.entries_examined = work
                .entries_examined
                .saturating_add(page.entries.len() as u64);
            if *visited > limit {
                return Err(ContentError::InvalidRecord("cycle check work limit"));
            }
            self.base_done = page.continuation.is_none();
            self.after = page.continuation;
            self.page = page.entries.into_iter();
        }
        Ok(())
    }
    pub fn next_entry(
        &mut self,
        visited: &mut usize,
        limit: usize,
        work: &mut ValidationWork,
    ) -> ContentResult<Option<(PathName, u64)>> {
        loop {
            self.base_next(visited, limit, work)?;
            if self.change_next.is_none() && !self.changes_done {
                self.change_next = self.changes.next().transpose()?;
                self.changes_done = self.change_next.is_none();
            }
            let ordering = match (&self.change_next, &self.base_next) {
                (Some((change, _)), Some((base, _))) => Some(change.cmp(base)),
                (Some(_), None) => Some(std::cmp::Ordering::Less),
                (None, Some(_)) => Some(std::cmp::Ordering::Greater),
                (None, None) => None,
            };
            match ordering {
                None => return Ok(None),
                Some(std::cmp::Ordering::Greater) => return Ok(self.base_next.take()),
                Some(ordering) => {
                    if ordering == std::cmp::Ordering::Equal {
                        let _ = self.base_next.take();
                    }
                    let (name, binding) = self.change_next.take().expect("change lookahead");
                    if let Some(serial) = binding {
                        return Ok(Some((name, serial)));
                    }
                }
            }
        }
    }
}
