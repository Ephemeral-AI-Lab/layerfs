//! Explicit retained whole-directory API over the single scalar spool codec.

use super::{BindingRows, DirectoryRowSource, RowSpool};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::input::DirectoryUpdate;

impl RowSpool {
    pub(super) fn compatibility_directory(
        &self,
        parent: u64,
    ) -> ContentResult<Option<DirectoryUpdate>> {
        self.directory_header(parent)?
            .map(|header| self.collect_directory(header))
            .transpose()
    }

    fn collect_directory(&self, header: super::DirectoryHeader) -> ContentResult<DirectoryUpdate> {
        let mut cursor = self.bindings(&header)?;
        let mut changes = Vec::new();
        while let Some(binding) = cursor.next_binding()? {
            changes.push(binding);
        }
        if !cursor.finish()?.matches(&header) {
            return Err(ContentError::InvalidRecord("directory completion"));
        }
        Ok(DirectoryUpdate {
            parent: header.parent(),
            changes,
        })
    }
}

pub(super) struct SpoolDirectories<'a> {
    spool: &'a RowSpool,
    at: usize,
    failure: Option<ContentError>,
}

impl<'a> SpoolDirectories<'a> {
    pub(super) fn new(spool: &'a RowSpool) -> Self {
        Self {
            spool,
            at: 0,
            failure: None,
        }
    }
}

impl DirectoryRowSource for SpoolDirectories<'_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryUpdate>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.spool.directory_slot(self.at).and_then(|slot| {
            slot.map(|slot| {
                self.spool
                    .slot_header(self.at, &slot)
                    .and_then(|header| self.spool.collect_directory(header))
            })
            .transpose()
        });
        match &result {
            Ok(Some(_)) => self.at += 1,
            Err(error) => self.failure = Some(error.clone()),
            _ => {}
        }
        result
    }
}
