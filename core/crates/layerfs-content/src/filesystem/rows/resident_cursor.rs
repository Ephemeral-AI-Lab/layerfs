//! Scalar borrowed cursors and explicitly selected whole-row compatibility.

use super::{BindingRowSource, DirectoryCompletion, DirectoryHeader};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;

type Binding = (PathName, Option<u64>);

enum Storage<'a> {
    Borrowed(&'a [Binding]),
    Owned(Vec<Binding>),
}

impl Storage<'_> {
    fn rows(&self) -> &[Binding] {
        match self {
            Self::Borrowed(rows) => rows,
            Self::Owned(rows) => rows,
        }
    }
}

pub(super) struct ResidentBindings<'a> {
    header: DirectoryHeader,
    rows: Storage<'a>,
    at: usize,
    bytes: u64,
    eof: bool,
    failure: Option<ContentError>,
}

impl<'a> ResidentBindings<'a> {
    pub(super) fn borrowed(header: DirectoryHeader, rows: &'a [Binding]) -> Self {
        Self::new(header, Storage::Borrowed(rows))
    }
    pub(super) fn owned(header: DirectoryHeader, rows: Vec<Binding>) -> Self {
        Self::new(header, Storage::Owned(rows))
    }
    fn new(header: DirectoryHeader, rows: Storage<'a>) -> Self {
        Self {
            header,
            rows,
            at: 0,
            bytes: 0,
            eof: false,
            failure: None,
        }
    }
    fn next(&mut self) -> ContentResult<Option<Binding>> {
        if self.eof {
            return Ok(None);
        }
        let rows = self.rows.rows();
        if self.at == self.header.binding_count() as usize {
            if self.at != rows.len() || self.bytes != self.header.wire_name_bytes() {
                return Err(ContentError::InvalidRecord("directory completion"));
            }
            self.eof = true;
            return Ok(None);
        }
        let (name, binding) = rows.get(self.at).ok_or(ContentError::UnexpectedEof)?;
        if self.at > 0 && rows[self.at - 1].0 >= *name {
            return Err(ContentError::NonCanonicalOrdering);
        }
        if binding.is_some_and(|serial| !super::serial_in_range(serial)) {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let next = self
            .bytes
            .checked_add(10 + name.as_bytes().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if next > self.header.wire_name_bytes() {
            return Err(ContentError::InvalidRecord("binding byte count"));
        }
        self.at += 1;
        self.bytes = next;
        Ok(Some((name.clone(), *binding)))
    }
}

impl BindingRowSource for ResidentBindings<'_> {
    fn next_binding(&mut self) -> ContentResult<Option<Binding>> {
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

pub(super) fn binding_bytes(rows: &[Binding]) -> ContentResult<u64> {
    rows.iter().try_fold(0_u64, |bytes, (name, _)| {
        bytes
            .checked_add(10 + name.as_bytes().len() as u64)
            .ok_or(ContentError::LengthOverflow)
    })
}
