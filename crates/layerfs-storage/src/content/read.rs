//! Verified regular-file extent and exact logical-range streaming.
//!
//! This module owns no tree walk, FsCas namespace, locator, pack, path, or
//! closure behavior. Its input port represents an already verified file and
//! yields only authenticated intersections with the selected logical range.

use crate::identity::COMPARISON_WINDOW_BYTES;
use crate::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VerifiedFileSegmentKindV1 {
    Hole,
    Data {
        scratch_offset: usize,
        authenticated_payload_bytes: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedFileSegmentV1 {
    kind: VerifiedFileSegmentKindV1,
    len: u64,
}

impl VerifiedFileSegmentV1 {
    pub(crate) const fn hole(len: u64) -> Self {
        Self {
            kind: VerifiedFileSegmentKindV1::Hole,
            len,
        }
    }

    pub(crate) const fn data(
        scratch_offset: usize,
        len: u64,
        authenticated_payload_bytes: u64,
    ) -> Self {
        Self {
            kind: VerifiedFileSegmentKindV1::Data {
                scratch_offset,
                authenticated_payload_bytes,
            },
            len,
        }
    }
}

/// Opaque authenticated extent source. Data intersections remain in the
/// supplied bounded scratch window for immediate delivery.
pub(crate) trait VerifiedFileRangePortV1 {
    fn check_control(&mut self) -> CoreResult<()>;
    fn next_intersection(
        &mut self,
        verification_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    ) -> CoreResult<Option<VerifiedFileSegmentV1>>;
}

/// Root extraction owns digest framing and sink transactions. This callback
/// receives only verified logical file bytes in order.
pub(crate) trait VerifiedFileBytesConsumerV1 {
    fn write_verified_bytes(&mut self, bytes: &[u8]) -> CoreResult<()>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedFileStreamResultV1 {
    pub(crate) logical_bytes: u64,
    pub(crate) payload_direct_bytes: u64,
    pub(crate) payload_direct_calls: u64,
}

pub(crate) fn stream_verified_file_range_v1<P, W>(
    expected_logical_bytes: u64,
    port: &mut P,
    consumer: &mut W,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<VerifiedFileStreamResultV1>
where
    P: VerifiedFileRangePortV1 + ?Sized,
    W: VerifiedFileBytesConsumerV1 + ?Sized,
{
    let mut result = VerifiedFileStreamResultV1 {
        logical_bytes: 0,
        payload_direct_bytes: 0,
        payload_direct_calls: 0,
    };
    while let Some(segment) = port.next_intersection(scratch)? {
        if segment.len == 0 {
            return Err(CoreError::LogicalLength);
        }
        if let VerifiedFileSegmentKindV1::Data {
            scratch_offset,
            authenticated_payload_bytes,
        } = segment.kind
        {
            port.check_control()?;
            let len = usize::try_from(segment.len).map_err(|_| CoreError::IntegerOverflow)?;
            let end = scratch_offset
                .checked_add(len)
                .ok_or(CoreError::IntegerOverflow)?;
            let bytes = scratch
                .get(scratch_offset..end)
                .ok_or(CoreError::LogicalLength)?;
            let next_payload_direct_bytes = result
                .payload_direct_bytes
                .checked_add(authenticated_payload_bytes)
                .ok_or(CoreError::IntegerOverflow)?;
            let next_payload_direct_calls = result
                .payload_direct_calls
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            let next_logical_bytes = result
                .logical_bytes
                .checked_add(segment.len)
                .ok_or(CoreError::IntegerOverflow)?;
            consumer.write_verified_bytes(bytes)?;
            result = VerifiedFileStreamResultV1 {
                logical_bytes: next_logical_bytes,
                payload_direct_bytes: next_payload_direct_bytes,
                payload_direct_calls: next_payload_direct_calls,
            };
            continue;
        }
        let mut emitted = 0_u64;
        while emitted < segment.len {
            port.check_control()?;
            let take = usize::try_from((segment.len - emitted).min(scratch.len() as u64))
                .map_err(|_| CoreError::IntegerOverflow)?;
            scratch[..take].fill(0);
            consumer.write_verified_bytes(&scratch[..take])?;
            emitted = emitted
                .checked_add(take as u64)
                .ok_or(CoreError::IntegerOverflow)?;
            result.logical_bytes = result
                .logical_bytes
                .checked_add(take as u64)
                .ok_or(CoreError::IntegerOverflow)?;
        }
    }
    if result.logical_bytes != expected_logical_bytes {
        return Err(CoreError::LogicalLength);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedPort {
        segments: Vec<VerifiedFileSegmentV1>,
        next: usize,
        controls: u64,
    }

    impl VerifiedFileRangePortV1 for ScriptedPort {
        fn check_control(&mut self) -> CoreResult<()> {
            self.controls = self
                .controls
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            Ok(())
        }

        fn next_intersection(
            &mut self,
            verification_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        ) -> CoreResult<Option<VerifiedFileSegmentV1>> {
            let segment = self.segments.get(self.next).copied();
            if let Some(segment) = segment {
                self.next += 1;
                if let VerifiedFileSegmentKindV1::Data {
                    scratch_offset: _,
                    authenticated_payload_bytes,
                } = segment.kind
                {
                    let len = usize::try_from(authenticated_payload_bytes)
                        .map_err(|_| CoreError::IntegerOverflow)?;
                    for (offset, byte) in verification_scratch
                        .get_mut(..len)
                        .ok_or(CoreError::LogicalLength)?
                        .iter_mut()
                        .enumerate()
                    {
                        *byte = offset as u8;
                    }
                }
            }
            Ok(segment)
        }
    }

    struct CollectingConsumer {
        bytes: Vec<u8>,
    }

    impl VerifiedFileBytesConsumerV1 for CollectingConsumer {
        fn write_verified_bytes(&mut self, bytes: &[u8]) -> CoreResult<()> {
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
    }

    #[test]
    fn verified_streamer_emits_data_hole_and_crossing_segments() {
        let mut port = ScriptedPort {
            segments: vec![
                VerifiedFileSegmentV1::data(11, 17, 31),
                VerifiedFileSegmentV1::hole(2),
                VerifiedFileSegmentV1::data(31, 4, 41),
            ],
            next: 0,
            controls: 0,
        };
        let mut consumer = CollectingConsumer { bytes: Vec::new() };
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let result = stream_verified_file_range_v1(23, &mut port, &mut consumer, &mut scratch)
            .expect("verified intersections stream");

        assert_eq!(result.logical_bytes, 23);
        assert_eq!(result.payload_direct_bytes, 72);
        assert_eq!(result.payload_direct_calls, 2);
        assert_eq!(port.controls, 3);
        assert_eq!(&consumer.bytes[..17], &(11_u8..28).collect::<Vec<_>>());
        assert_eq!(&consumer.bytes[17..19], &[0, 0]);
        assert_eq!(&consumer.bytes[19..], &[31, 32, 33, 34]);
    }
}
