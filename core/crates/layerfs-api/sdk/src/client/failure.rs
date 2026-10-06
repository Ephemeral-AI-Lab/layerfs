//! Borrowed typed failures; labels and nested originals stay separate from codes.
use layerfs_bridge::contract::{FrameError, FrameResult};
/// Owning error domain; no Display-string classification is used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FailureDomain {
    /// SDK runtime authority/admission/custody and underlying errors.
    Runtime = 1,
    /// Canonical construction/read failure.
    Content = 2,
    /// Storage failure, including original and cleanup branches.
    Storage = 3,
    /// History failure with deciding-transaction context.
    History = 4,
    /// std I/O class/raw OS value and original diagnostic/source.
    Io = 5,
    /// Known PackPersistence error carried by std I/O.
    Persistence = 6,
    /// Erased application's opaque Error source, preserved as diagnostic text.
    Opaque = 7,
}
/// Exact borrowed node of the error record, independent of receipt disposition.
#[derive(Clone, Copy, Debug)]
pub struct RemoteFailure<'a> {
    domain: FailureDomain,
    code: u8,
    payload: &'a [u8],
}
impl<'a> RemoteFailure<'a> {
    /// Checks one node's fields without allocating a recursive error mirror.
    /// Child nodes are checked independently when accessed.
    pub fn decode(bytes: &'a [u8]) -> FrameResult<Self> {
        if bytes.len() < 8
            || bytes[2..4] != [0, 0]
            || u32::from_be_bytes(bytes[4..8].try_into().expect("fixed node length")) as usize
                != bytes.len() - 8
        {
            return Err(FrameError::Invalid("remote failure node"));
        }
        let (domain, max) = match bytes[0] {
            1 => (FailureDomain::Runtime, 10),
            2 => (FailureDomain::Content, 35),
            3 => (FailureDomain::Storage, 15),
            4 => (FailureDomain::History, 15),
            5 => (FailureDomain::Io, 255),
            6 => (FailureDomain::Persistence, 5),
            7 => (FailureDomain::Opaque, 1),
            _ => return Err(FrameError::Invalid("failure domain")),
        };
        if bytes[1] == 0
            || bytes[1] > max
            || (domain == FailureDomain::Io && bytes[1] > 20 && bytes[1] != 255)
        {
            return Err(FrameError::Invalid("failure code"));
        }
        let result = Self {
            domain,
            code: bytes[1],
            payload: &bytes[8..],
        };
        for field in result.fields() {
            field?;
        }
        Ok(result)
    }
    /// Owning typed domain.
    pub const fn domain(self) -> FailureDomain {
        self.domain
    }
    /// Stable variant code within the domain, documented in the wire architecture.
    pub const fn code(self) -> u8 {
        self.code
    }
    /// Checked field iterator borrowing the original credited reply body.
    pub fn fields(self) -> FailureFields<'a> {
        FailureFields {
            bytes: self.payload,
            failed: false,
        }
    }
    /// Finds one checked typed field, retaining its exact numeric/byte representation.
    pub fn field(self, tag: u8) -> FrameResult<Option<FailureField<'a>>> {
        for field in self.fields() {
            let field = field?;
            if field.tag == tag {
                return Ok(Some(field));
            }
        }
        Ok(None)
    }
}
/// One typed field of an original failure; tag meanings belong to its variant.
#[derive(Clone, Copy, Debug)]
pub struct FailureField<'a> {
    /// Variant-local field identity; original=1 and cleanup=2 for nested storage.
    pub tag: u8,
    /// Exact checked value kind.
    pub value: FailureValue<'a>,
}
/// Portable typed error value; large records are borrowed rather than cloned.
#[derive(Clone, Copy, Debug)]
pub enum FailureValue<'a> {
    /// Unsigned scalar, including counts/limits/versions/tokens.
    Unsigned(u64),
    /// Signed scalar, including pack IDs and raw OS codes.
    Signed(i64),
    /// Exact identity or context bytes; owning record codec defines the grammar.
    Bytes(&'a [u8]),
    /// Original label/status/diagnostic; never used to reconstruct error class.
    Text(&'a str),
    /// Original nested error record; decode it with RemoteFailure::decode.
    Node(&'a [u8]),
    /// Complete canonical history StageRecord context.
    Stage(&'a [u8]),
}
/// Forward iterator; malformed fields end it once with an original codec refusal.
pub struct FailureFields<'a> {
    bytes: &'a [u8],
    failed: bool,
}
impl<'a> Iterator for FailureFields<'a> {
    type Item = FrameResult<FailureField<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.bytes.is_empty() {
            return None;
        }
        let result = (|| {
            if self.bytes.len() < 6 {
                return Err(FrameError::Invalid("failure field header"));
            }
            let tag = self.bytes[0];
            let kind = self.bytes[1];
            let size = u32::from_be_bytes(self.bytes[2..6].try_into().expect("fixed field length"))
                as usize;
            let end = size
                .checked_add(6)
                .ok_or(FrameError::Invalid("failure field length"))?;
            let data = self
                .bytes
                .get(6..end)
                .ok_or(FrameError::Invalid("failure field body"))?;
            let value = match kind {
                1 if size == 8 => {
                    FailureValue::Unsigned(u64::from_be_bytes(data.try_into().expect("scalar")))
                }
                2 if size == 8 => {
                    FailureValue::Signed(i64::from_be_bytes(data.try_into().expect("scalar")))
                }
                3 => FailureValue::Bytes(data),
                4 => FailureValue::Text(
                    std::str::from_utf8(data).map_err(|_| FrameError::Invalid("failure UTF-8"))?,
                ),
                5 => FailureValue::Node(data),
                6 => FailureValue::Stage(data),
                _ => return Err(FrameError::Invalid("failure field kind")),
            };
            self.bytes = &self.bytes[end..];
            Ok(FailureField { tag, value })
        })();
        if result.is_err() {
            self.failed = true;
        }
        Some(result)
    }
}
