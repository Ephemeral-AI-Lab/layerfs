use crate::{Result, ServiceError, MAX_WIRE_BYTES};
use std::io::Write;

pub(crate) fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut body = LimitedVec::new(MAX_WIRE_BYTES);
    serde_json::to_writer(&mut body, value)
        .map_err(|error| ServiceError::Wire(error.to_string()))?;
    if body.bytes.is_empty() {
        return Err(ServiceError::Wire("request limit".into()));
    }
    Ok(body.bytes)
}

pub(crate) fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|error| ServiceError::Wire(error.to_string()))
}

struct LimitedVec {
    bytes: Vec<u8>,
    limit: usize,
}

impl LimitedVec {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(64 * 1024)),
            limit,
        }
    }
}

impl Write for LimitedVec {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > self.limit)
        {
            return Err(std::io::Error::other("wire frame limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
