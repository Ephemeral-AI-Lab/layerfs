//! Full-width FIFO jobs and external LIFO directory cursor frames.
use super::{BaseFact, CanonicalScope, ReleaseName};
use crate::object::inode_leaf::InodeValue;
use crate::{ContentError, ContentResult, ObjectId};
/// One native queued or currently taken job with its already-known base answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseJob {
    /// Checked monotone FIFO rank, never reused after a take.
    pub sequence: u64,
    /// Original positive inode serial.
    pub serial: u64,
    /// Already established base answer; None is known absence, not unknown.
    pub base: Option<InodeValue>,
}
impl ReleaseJob {
    /// Full serial8 plus the same exact established presence/value74 payload.
    pub fn encode_value(self) -> ContentResult<[u8; 82]> {
        let mut b = [0; 82];
        b[..8].copy_from_slice(&self.serial.to_be_bytes());
        b[8..].copy_from_slice(
            &BaseFact {
                serial: self.serial,
                value: self.base,
            }
            .encode_value()?,
        );
        Ok(b)
    }
    /// Decode exact selected rank and full serial/presence fields.
    pub fn decode(scope: &CanonicalScope, key: &[u8], value: &[u8]) -> ContentResult<Self> {
        if scope.state().table().code() != 21 || value.len() != 82 {
            return Err(ContentError::InvalidOrderingRecord("release job width"));
        }
        let sequence = scope.scalar(key)?;
        let serial = u64::from_be_bytes(value[..8].try_into().unwrap());
        let base = BaseFact::decode(serial, &value[8..])?.value;
        Ok(Self {
            sequence,
            serial,
            base,
        })
    }
}
/// One selected directory cursor; arbitrary admitted depth resides in SQL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseFrame {
    /// Positive external stack depth/rank.
    pub depth: u64,
    /// Exact directory content root selected by the current count/value state.
    pub root: ObjectId,
    /// Full current listing continuation, never a truncated hash or prefix.
    pub after: Option<ReleaseName>,
    /// True after exact selected directory listing EOF.
    pub finished: bool,
}
impl ReleaseFrame {
    /// Full root32/after-present1/length2/full255/finished1 payload.
    pub fn encode_value(self) -> [u8; 291] {
        let mut b = [0; 291];
        b[..32].copy_from_slice(self.root.as_bytes());
        if let Some(after) = self.after {
            b[32] = 1;
            let name = after.as_str().as_bytes();
            b[33..35].copy_from_slice(&(name.len() as u16).to_be_bytes());
            b[35..35 + name.len()].copy_from_slice(name);
        }
        b[290] = u8::from(self.finished);
        b
    }
    /// Check selected rank, UTF-8, full name and canonical absent/trailing bytes.
    pub fn decode(scope: &CanonicalScope, key: &[u8], value: &[u8]) -> ContentResult<Self> {
        if scope.state().table().code() != 22
            || value.len() != 291
            || value[32] > 1
            || value[290] > 1
        {
            return Err(ContentError::InvalidOrderingRecord("release frame framing"));
        }
        let depth = scope.scalar(key)?;
        let len = u16::from_be_bytes(value[33..35].try_into().unwrap()) as usize;
        let after = if value[32] == 0 {
            if value[33..290] != [0; 257] {
                return Err(ContentError::InvalidOrderingRecord(
                    "release absent continuation",
                ));
            }
            None
        } else {
            if len == 0 || len > 255 || value[35 + len..290].iter().any(|b| *b != 0) {
                return Err(ContentError::InvalidOrderingRecord("release name framing"));
            }
            let name = std::str::from_utf8(&value[35..35 + len])
                .map_err(|_| ContentError::InvalidOrderingRecord("release name UTF-8"))?;
            Some(ReleaseName::new(name)?)
        };
        Ok(Self {
            depth,
            root: ObjectId::from_bytes(&value[..32])?,
            after,
            finished: value[290] == 1,
        })
    }
    /// Validate unchanged root/depth and strict selected page continuation advancement.
    pub fn advances_to(self, next: Self) -> ContentResult<()> {
        if self.finished
            || self.depth != next.depth
            || self.root != next.root
            || (!next.finished && next.after <= self.after)
        {
            return Err(ContentError::InvalidOrderingRecord(
                "release cursor progress",
            ));
        }
        if self.after.is_some() && next.after.is_some() && next.after < self.after {
            return Err(ContentError::InvalidOrderingRecord("release cursor order"));
        }
        Ok(())
    }
}
/// Exact empty live frontier after every known job/frame completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseSeal {
    /// Complete issued table21/phase7 authenticated context.
    pub scope: CanonicalScope,
    /// Last burned checked FIFO rank, including completed jobs.
    pub sequence: u64,
    /// Exact jobs completed, including initial candidates and queued children.
    pub jobs: u64,
    /// Exact directory frames pushed and fully popped.
    pub directories: u64,
    /// Largest actual external cursor depth observed.
    pub maximum_depth: u64,
}
