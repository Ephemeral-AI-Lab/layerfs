//! The daemon's advisory pre-handshake capacity notice (#540).
//!
//! When the accept loop finds no vacant control slot it would otherwise just
//! drop the socket, leaving the caller with a bare EOF. This fixed, bounded
//! frame tells the caller which resource was exhausted and how many failure
//! records are retained, so the product refusal can name the real resource.
//! It is written *before* the Noise handshake and is therefore unauthenticated:
//! it admits nothing, carries no secret, and every operation still crosses the
//! authenticated handshake and authorization. A forged notice can at most
//! cause a retry, exactly like a forged reset.
use std::io::Write;
use std::net::TcpStream;

/// Magic that starts a capacity notice body.
pub const MAGIC: &[u8; 16] = b"LAYERFS-CAPACITY";
/// The only notice version this build writes or reads.
pub const VERSION: u8 = 1;
/// Length of the body after the two-byte record header.
pub const BODY_LEN: usize = 24;

/// Why no connection was admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionReason {
    /// Every pool slot was busy (or retained; the count is in `retained`).
    NoPoolSlot = 1,
    /// The daemon is still starting and admits no control yet.
    Starting = 2,
    /// The reserved slot is in use and only pool capacity was requested.
    ReservedBusy = 3,
}

impl AdmissionReason {
    fn of(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::NoPoolSlot),
            2 => Some(Self::Starting),
            3 => Some(Self::ReservedBusy),
            _ => None,
        }
    }

    /// Stable text for a refusal detail.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoPoolSlot => "no-pool-slot",
            Self::Starting => "starting",
            Self::ReservedBusy => "reserved-busy",
        }
    }
}

/// One decoded advisory notice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmissionNotice {
    /// Notice format version (always [`VERSION`] for a decoded notice).
    pub version: u8,
    /// Why admission ended.
    pub reason: AdmissionReason,
    /// Visible retained failure records, saturating at 255.
    pub retained: u8,
    /// Vacant pool slots, saturating at 255.
    pub pool_vacant: u8,
    /// Vacant reserved slots, saturating at 255.
    pub reserved_vacant: u8,
}

impl AdmissionNotice {
    /// A notice with the current format version.
    pub fn new(
        reason: AdmissionReason,
        retained: u8,
        pool_vacant: u8,
        reserved_vacant: u8,
    ) -> Self {
        Self {
            version: VERSION,
            reason,
            retained,
            pool_vacant,
            reserved_vacant,
        }
    }
}

/// The exact 24-byte body of `notice`.
pub fn encode(notice: &AdmissionNotice) -> [u8; BODY_LEN] {
    let mut body = [0u8; BODY_LEN];
    body[..16].copy_from_slice(MAGIC);
    body[16] = notice.version;
    body[17] = notice.reason as u8;
    body[18] = notice.retained;
    body[19] = notice.pool_vacant;
    body[20] = notice.reserved_vacant;
    body
}

/// The notice in `bytes`, or `None` when the record is anything else. A
/// random record that is not exactly a valid notice is never accepted.
pub fn classify(bytes: &[u8]) -> Option<AdmissionNotice> {
    if bytes.len() != BODY_LEN || &bytes[..16] != MAGIC {
        return None;
    }
    if bytes[16] != VERSION {
        return None;
    }
    let reason = AdmissionReason::of(bytes[17])?;
    if bytes[21..].iter().any(|byte| *byte != 0) {
        return None;
    }
    Some(AdmissionNotice {
        version: bytes[16],
        reason,
        retained: bytes[18],
        pool_vacant: bytes[19],
        reserved_vacant: bytes[20],
    })
}

/// Writes `notice` as one length-prefixed record, then forces the write end
/// shut. The caller sets the socket timeouts.
pub fn write(stream: &mut TcpStream, notice: &AdmissionNotice) -> std::io::Result<()> {
    let body = encode(notice);
    let mut framed = [0u8; 2 + BODY_LEN];
    framed[..2].copy_from_slice(&(BODY_LEN as u16).to_be_bytes());
    framed[2..].copy_from_slice(&body);
    stream.write_all(&framed)?;
    stream.flush()?;
    stream.shutdown(std::net::Shutdown::Write)
}
