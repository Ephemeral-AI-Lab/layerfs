use super::{Result, SyncError};

pub const MAX_BATCH_BYTES: usize = 1024 * 1024;
pub const MAX_BATCH_OBJECTS: usize = 1024;
pub const MAX_QUEUED_BATCHES: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Direction {
    Fetch,
    Push,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferResult {
    TransferredNoVisibility,
    ReconciledNoTransfer,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResumeToken {
    pub(crate) next_object_index: u64,
    pub(crate) binding: [u8; 32],
}

impl ResumeToken {
    pub const fn next_object_index(self) -> u64 {
        self.next_object_index
    }

    pub(crate) fn encode(self) -> [u8; 40] {
        let mut encoded = [0; 40];
        encoded[..8].copy_from_slice(&self.next_object_index.to_be_bytes());
        encoded[8..].copy_from_slice(&self.binding);
        encoded
    }

    pub(crate) fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() < 40 {
            return Err(SyncError::InvalidResume);
        }
        let mut index = [0; 8];
        index.copy_from_slice(&encoded[..8]);
        let mut binding = [0; 32];
        binding.copy_from_slice(&encoded[8..40]);
        Ok(Self {
            next_object_index: u64::from_be_bytes(index),
            binding,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferReceipt {
    pub request_id: [u8; 32],
    pub source_storage_id: [u8; 32],
    pub destination_storage_id: [u8; 32],
    pub direction: Direction,
    pub result: TransferResult,
    pub objects_examined: u64,
    pub known_present_objects: u64,
    pub missing_objects: u64,
    pub transferred_objects: u64,
    pub unique_bytes: u64,
    pub resumed_bytes: u64,
    pub retransmitted_bytes: u64,
    pub batches: u64,
    pub largest_batch_bytes: u64,
    pub largest_batch_objects: u64,
    pub negotiation_ns: u128,
    pub source_read_ns: u128,
    pub receiver_admission_ns: u128,
    pub complete_wall_ns: u128,
    pub terminal_buffer_bytes: u64,
    pub terminal_queued_batches: u64,
    pub resume: ResumeToken,
}

impl TransferReceipt {
    pub(crate) fn default_for(direction: Direction, request_id: [u8; 32]) -> Self {
        Self {
            request_id,
            source_storage_id: [0; 32],
            destination_storage_id: [0; 32],
            direction,
            result: TransferResult::TransferredNoVisibility,
            objects_examined: 0,
            known_present_objects: 0,
            missing_objects: 0,
            transferred_objects: 0,
            unique_bytes: 0,
            resumed_bytes: 0,
            retransmitted_bytes: 0,
            batches: 0,
            largest_batch_bytes: 0,
            largest_batch_objects: 0,
            negotiation_ns: 0,
            source_read_ns: 0,
            receiver_admission_ns: 0,
            complete_wall_ns: 0,
            terminal_buffer_bytes: 0,
            terminal_queued_batches: 0,
            resume: ResumeToken::default(),
        }
    }
}
