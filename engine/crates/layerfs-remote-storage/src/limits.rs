//! What one provider allows.
use crate::error::RemoteError;

/// One provider's size limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Largest file the provider holds as one object.
    pub max_file: u64,
    /// Largest body one request may carry; a driver sends a larger file in
    /// parts (S3 multipart) when the provider supports it.
    pub max_request: u64,
    /// Size of each piece when a file is larger than `max_file` (the last
    /// piece may be smaller). At most `max_file`.
    pub piece_size: u64,
}

impl Limits {
    /// Refuses limits that cannot hold a piece.
    pub fn validate(&self) -> Result<(), RemoteError> {
        if self.max_file == 0
            || self.max_request == 0
            || self.piece_size == 0
            || self.piece_size > self.max_file
        {
            return Err(RemoteError::Invalid(format!("limits {self:?}")));
        }
        Ok(())
    }

    /// How many pieces a file of `bytes` needs: 0 when it fits in one object.
    pub fn pieces(&self, bytes: u64) -> u64 {
        if bytes <= self.max_file {
            0
        } else {
            bytes.div_ceil(self.piece_size)
        }
    }
}
