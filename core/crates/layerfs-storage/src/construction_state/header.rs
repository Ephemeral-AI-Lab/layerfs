//! Exact private native headers; earlier profile bytes remain unchanged.

pub(crate) enum Header {
    Earlier([u8; 192]),
    Sites([u8; 200]),
}

impl Header {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Earlier(bytes) => bytes,
            Self::Sites(bytes) => bytes,
        }
    }
}
