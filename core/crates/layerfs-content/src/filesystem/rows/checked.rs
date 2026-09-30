//! Public checked iterator cursor for independent scalar-row providers.

use super::{BindingRowSource, DirectoryCompletion, DirectoryHeader};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;

/// An issued iterator selection with exact logical EOF and completion custody.
/// Created through BindingAuthority::cursor, never by forging completion fields.
pub struct CheckedBindings<I> {
    header: DirectoryHeader,
    input: I,
    seen: u32,
    wire_bytes: u64,
    previous: Option<PathName>,
    eof: bool,
    failure: Option<ContentError>,
}

impl<I> CheckedBindings<I> {
    pub(super) fn new(header: DirectoryHeader, input: I) -> Self {
        Self {
            header,
            input,
            seen: 0,
            wire_bytes: 0,
            previous: None,
            eof: false,
            failure: None,
        }
    }
}

impl<I: Iterator<Item = ContentResult<(PathName, Option<u64>)>>> CheckedBindings<I> {
    fn next(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        if self.eof {
            return Ok(None);
        }
        let Some(row) = self.input.next() else {
            if self.seen != self.header.binding_count()
                || self.wire_bytes != self.header.wire_name_bytes()
            {
                return Err(ContentError::InvalidRecord("directory completion"));
            }
            self.eof = true;
            return Ok(None);
        };
        let (name, child) = row?;
        if self.seen >= self.header.binding_count() {
            return Err(ContentError::InvalidRecord("directory binding count"));
        }
        if self
            .previous
            .as_ref()
            .is_some_and(|previous| previous >= &name)
        {
            return Err(ContentError::NonCanonicalOrdering);
        }
        if child.is_some_and(|serial| !super::serial_in_range(serial)) {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let bytes = self
            .wire_bytes
            .checked_add(10 + name.as_bytes().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if bytes > self.header.wire_name_bytes() {
            return Err(ContentError::InvalidRecord("directory byte count"));
        }
        self.seen += 1;
        self.wire_bytes = bytes;
        self.previous = Some(name.clone());
        Ok(Some((name, child)))
    }
}

impl<I: Iterator<Item = ContentResult<(PathName, Option<u64>)>>> BindingRowSource
    for CheckedBindings<I>
{
    fn next_binding(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.next();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn finish(&mut self) -> ContentResult<DirectoryCompletion> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if !self.eof {
            self.failure = Some(ContentError::IncompleteOperation);
            return Err(ContentError::IncompleteOperation);
        }
        Ok(DirectoryCompletion::finished(self.header))
    }
}
