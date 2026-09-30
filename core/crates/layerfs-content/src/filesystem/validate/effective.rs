//! A bounded base listing page merged with one scalar changed-name cursor.

use std::cmp::Ordering;

use super::binding::{selected, Bindings};
use super::{charge_directory, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::path::PathName;
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::object::{AuthenticatedObjects, ObjectId};

pub(super) struct EffectiveEntries<'a> {
    reader: &'a dyn AuthenticatedObjects,
    root: Option<DirectoryRoot>,
    page: std::vec::IntoIter<(PathName, u64)>,
    after: Option<PathName>,
    base_ended: bool,
    base: Option<(PathName, u64)>,
    bindings: Option<Bindings<'a>>,
    changed: Option<(PathName, Option<u64>)>,
    changes_ended: bool,
}

impl<'a> EffectiveEntries<'a> {
    pub(super) fn new(
        reader: &'a dyn AuthenticatedObjects,
        input: &'a dyn PreparedBindingRows,
        serial: u64,
        base: Option<ObjectId>,
    ) -> ContentResult<Self> {
        let bindings = selected(input, serial)?
            .map(|header| Bindings::new(input, header))
            .transpose()?;
        let changes_ended = bindings.is_none();
        Ok(Self {
            reader,
            root: base.map(DirectoryRoot),
            page: Vec::new().into_iter(),
            after: None,
            base_ended: base.is_none(),
            base: None,
            bindings,
            changed: None,
            changes_ended,
        })
    }

    pub(super) fn next(
        &mut self,
        visited: &mut usize,
        limit: usize,
        work: &mut ValidationWork,
    ) -> ContentResult<Option<(PathName, u64)>> {
        loop {
            self.fill_base(visited, limit, work)?;
            if self.changed.is_none() && !self.changes_ended {
                self.changed = self.bindings.as_mut().expect("selected bindings").next()?;
                self.changes_ended = self.changed.is_none();
            }
            let result = match (&self.base, &self.changed) {
                (Some((base, _)), Some((changed, _))) => match base.cmp(changed) {
                    Ordering::Less => self.base.take(),
                    Ordering::Greater => self.take_change(),
                    Ordering::Equal => {
                        let _ = self.base.take();
                        self.take_change()
                    }
                },
                (Some(_), None) => self.base.take(),
                (None, Some(_)) => self.take_change(),
                (None, None) => return Ok(None),
            };
            if let Some(result) = result {
                *visited = visited.saturating_add(1);
                if *visited > limit {
                    return Err(ContentError::InvalidRecord("cycle check work limit"));
                }
                return Ok(Some(result));
            }
        }
    }

    fn take_change(&mut self) -> Option<(PathName, u64)> {
        self.changed
            .take()
            .and_then(|(name, serial)| serial.map(|serial| (name, serial)))
    }

    fn fill_base(
        &mut self,
        visited: &mut usize,
        limit: usize,
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        while self.base.is_none() {
            if let Some(entry) = self.page.next() {
                self.base = Some(entry);
                break;
            }
            if self.base_ended {
                break;
            }
            self.page = Vec::new().into_iter();
            let mut directory = DirectoryReadWork::default();
            let page = list_after(
                self.reader,
                self.root.expect("selected base"),
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
            if page.entries.is_empty() && page.continuation.is_some() {
                return Err(ContentError::InvalidRecord("directory listing progress"));
            }
            self.base_ended = page.continuation.is_none();
            self.after = page.continuation;
            self.page = page.entries.into_iter();
        }
        Ok(())
    }
}
