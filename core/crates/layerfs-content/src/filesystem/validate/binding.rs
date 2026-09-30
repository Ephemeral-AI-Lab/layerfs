//! Exact scalar directory passes, with one selected binding cursor at a time.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;
use crate::filesystem::rows::{
    serial_in_range, BindingRowSource, DirectoryHeader, DirectoryHeaderSource, PreparedBindingRows,
};

pub(super) fn selected(
    input: &dyn PreparedBindingRows,
    parent: u64,
) -> ContentResult<Option<DirectoryHeader>> {
    let header = input.directory_header(parent)?;
    if header.is_some_and(|header| header.parent() != parent) {
        return Err(ContentError::InvalidRecord("directory selection"));
    }
    Ok(header)
}

pub(super) struct Headers<'a> {
    source: Box<dyn DirectoryHeaderSource + 'a>,
    expected: usize,
    seen: usize,
    previous: u64,
    ended: bool,
    failure: Option<ContentError>,
}

impl<'a> Headers<'a> {
    pub(super) fn new(input: &'a dyn PreparedBindingRows) -> ContentResult<Self> {
        Ok(Self {
            source: input.directory_headers()?,
            expected: input.directory_rows(),
            seen: 0,
            previous: 0,
            ended: false,
            failure: None,
        })
    }

    pub(super) fn next(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.ended {
            return Ok(None);
        }
        let result = self.pull();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn pull(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        let Some(header) = self.source.next_header()? else {
            if self.seen != self.expected {
                return Err(ContentError::InvalidRecord("directory row count"));
            }
            self.ended = true;
            return Ok(None);
        };
        if self.seen == self.expected {
            return Err(ContentError::InvalidRecord("directory row count"));
        }
        // The descriptor is opaque to this pass. Its issuing provider validates
        // the exact selected slot when bindings opens; sequence order is parent
        // order, independent of how that provider represents the descriptor.
        if header.parent() <= self.previous {
            return Err(ContentError::NonCanonicalOrdering);
        }
        self.previous = header.parent();
        self.seen = self
            .seen
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(Some(header))
    }
}

pub(super) struct Bindings<'a> {
    source: Box<dyn BindingRowSource + 'a>,
    header: DirectoryHeader,
    seen: u32,
    bytes: u64,
    previous: Option<PathName>,
    ended: bool,
    failure: Option<ContentError>,
}

impl<'a> Bindings<'a> {
    pub(super) fn new(
        input: &'a dyn PreparedBindingRows,
        header: DirectoryHeader,
    ) -> ContentResult<Self> {
        Ok(Self {
            source: input.bindings(&header)?,
            header,
            seen: 0,
            bytes: 0,
            previous: None,
            ended: false,
            failure: None,
        })
    }

    pub(super) fn next(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.ended {
            return Ok(None);
        }
        let result = self.pull();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn pull(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        let Some((name, child)) = self.source.next_binding()? else {
            if self.seen != self.header.binding_count()
                || self.bytes != self.header.wire_name_bytes()
            {
                return Err(ContentError::InvalidRecord("directory binding completion"));
            }
            if !self.source.finish()?.matches(&self.header) {
                return Err(ContentError::InvalidRecord("directory binding completion"));
            }
            self.ended = true;
            return Ok(None);
        };
        let seen = self
            .seen
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        let bytes = self
            .bytes
            .checked_add(10 + name.as_bytes().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if seen > self.header.binding_count() || bytes > self.header.wire_name_bytes() {
            return Err(ContentError::InvalidRecord("directory binding completion"));
        }
        if self
            .previous
            .as_ref()
            .is_some_and(|previous| previous >= &name)
        {
            return Err(ContentError::NonCanonicalOrdering);
        }
        if child.is_some_and(|serial| !serial_in_range(serial)) {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        self.seen = seen;
        self.bytes = bytes;
        self.previous = Some(name.clone());
        Ok(Some((name, child)))
    }
}
