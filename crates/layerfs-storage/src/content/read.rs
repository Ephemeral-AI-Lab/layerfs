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
    Data { token: u64, source_offset: u64 },
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

    pub(crate) const fn data(token: u64, source_offset: u64, len: u64) -> Self {
        Self {
            kind: VerifiedFileSegmentKindV1::Data {
                token,
                source_offset,
            },
            len,
        }
    }
}

/// Opaque authenticated extent source. Implementations retain the concrete
/// object locator and reject stale or forged data tokens.
pub(crate) trait VerifiedFileRangePortV1 {
    fn check_control(&mut self) -> CoreResult<()>;
    fn next_intersection(
        &mut self,
        verification_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    ) -> CoreResult<Option<VerifiedFileSegmentV1>>;
    fn read_data_exact(
        &mut self,
        token: u64,
        source_offset: u64,
        destination: &mut [u8],
    ) -> CoreResult<()>;
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
        let mut emitted = 0_u64;
        while emitted < segment.len {
            port.check_control()?;
            let take = usize::try_from((segment.len - emitted).min(scratch.len() as u64))
                .map_err(|_| CoreError::IntegerOverflow)?;
            match segment.kind {
                VerifiedFileSegmentKindV1::Hole => scratch[..take].fill(0),
                VerifiedFileSegmentKindV1::Data {
                    token,
                    source_offset,
                } => {
                    port.read_data_exact(
                        token,
                        source_offset
                            .checked_add(emitted)
                            .ok_or(CoreError::IntegerOverflow)?,
                        &mut scratch[..take],
                    )?;
                    result.payload_direct_bytes = result
                        .payload_direct_bytes
                        .checked_add(take as u64)
                        .ok_or(CoreError::IntegerOverflow)?;
                    result.payload_direct_calls = result
                        .payload_direct_calls
                        .checked_add(1)
                        .ok_or(CoreError::IntegerOverflow)?;
                }
            }
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
        reads: Vec<(u64, u64, usize)>,
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
            _verification_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        ) -> CoreResult<Option<VerifiedFileSegmentV1>> {
            let segment = self.segments.get(self.next).copied();
            if segment.is_some() {
                self.next += 1;
            }
            Ok(segment)
        }

        fn read_data_exact(
            &mut self,
            token: u64,
            source_offset: u64,
            destination: &mut [u8],
        ) -> CoreResult<()> {
            self.reads.push((token, source_offset, destination.len()));
            for (index, byte) in destination.iter_mut().enumerate() {
                *byte = source_offset
                    .checked_add(index as u64)
                    .ok_or(CoreError::IntegerOverflow)? as u8;
            }
            Ok(())
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
        let crossing_len = COMPARISON_WINDOW_BYTES as u64 + 3;
        let mut port = ScriptedPort {
            segments: vec![
                VerifiedFileSegmentV1::data(7, 11, crossing_len),
                VerifiedFileSegmentV1::hole(2),
                VerifiedFileSegmentV1::data(9, 31, 4),
            ],
            next: 0,
            controls: 0,
            reads: Vec::new(),
        };
        let mut consumer = CollectingConsumer { bytes: Vec::new() };
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let result =
            stream_verified_file_range_v1(crossing_len + 6, &mut port, &mut consumer, &mut scratch)
                .expect("verified intersections stream");

        assert_eq!(result.logical_bytes, crossing_len + 6);
        assert_eq!(result.payload_direct_bytes, crossing_len + 4);
        assert_eq!(result.payload_direct_calls, 3);
        assert_eq!(port.controls, 4);
        assert_eq!(
            port.reads,
            vec![
                (7, 11, COMPARISON_WINDOW_BYTES),
                (7, 11 + COMPARISON_WINDOW_BYTES as u64, 3),
                (9, 31, 4)
            ]
        );
        let expected_data = (0..crossing_len)
            .map(|offset| (11_u64 + offset) as u8)
            .collect::<Vec<_>>();
        assert_eq!(
            &consumer.bytes[..crossing_len as usize],
            expected_data.as_slice()
        );
        assert_eq!(
            &consumer.bytes[crossing_len as usize..crossing_len as usize + 2],
            &[0, 0]
        );
        assert_eq!(
            &consumer.bytes[crossing_len as usize + 2..],
            &[31, 32, 33, 34]
        );
    }
}
