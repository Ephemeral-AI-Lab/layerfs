use blake3::{Hasher, KEY_LEN};

use crate::identity::ObjectId;
use crate::object::{encode_object, Object};
use crate::{CoreError, CoreResult};

pub const VALIDATED_SNAPSHOT_RECEIPT_BYTES: usize = 216;
const RECEIPT_INNER_BYTES: usize = 203;
const RECEIPT_MAGIC: &[u8; 8] = b"LFS4VAL\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedSnapshotReceiptV1 {
    pub store_instance_id: [u8; 16],
    pub validation_authority_id: [u8; 32],
    pub integrity_epoch: u64,
    pub head_generation: u64,
    pub child_root_id: ObjectId,
    pub transition_id: ObjectId,
    pub mapping_profile_id: ObjectId,
}

impl ValidatedSnapshotReceiptV1 {
    pub fn validation_authority_id(
        store_instance_id: [u8; 16],
        validation_key: &[u8; KEY_LEN],
    ) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(b"layerfs/validation-authority/v1\0");
        hasher.update(&store_instance_id);
        hasher.update(validation_key);
        *hasher.finalize().as_bytes()
    }

    pub fn encode(&self, validation_key: &[u8; KEY_LEN]) -> CoreResult<[u8; 216]> {
        let mut inner = Vec::with_capacity(RECEIPT_INNER_BYTES);
        inner.extend_from_slice(RECEIPT_MAGIC);
        inner.extend_from_slice(&1_u16.to_be_bytes());
        inner.push(1);
        inner.extend_from_slice(&self.store_instance_id);
        inner.extend_from_slice(&self.validation_authority_id);
        inner.extend_from_slice(&self.integrity_epoch.to_be_bytes());
        inner.extend_from_slice(&self.head_generation.to_be_bytes());
        inner.extend_from_slice(self.child_root_id.as_bytes());
        inner.extend_from_slice(self.transition_id.as_bytes());
        inner.extend_from_slice(self.mapping_profile_id.as_bytes());
        if inner.len() != RECEIPT_INNER_BYTES - 32 {
            return Err(CoreError::InvalidValidationReceipt);
        }
        let mut authenticator = blake3::Hasher::new_keyed(validation_key);
        authenticator.update(b"layerfs/validated-snapshot/v1\0");
        authenticator.update(&inner);
        inner.extend_from_slice(authenticator.finalize().as_bytes());
        if inner.len() != RECEIPT_INNER_BYTES {
            return Err(CoreError::InvalidValidationReceipt);
        }
        let bytes = encode_object(&Object::bytes(inner)?)?;
        bytes
            .try_into()
            .map_err(|_| CoreError::InvalidValidationReceipt)
    }

    pub fn decode(
        bytes: &[u8],
        validation_key: &[u8; KEY_LEN],
        expected_profile: ObjectId,
        expected_authority: [u8; 32],
    ) -> CoreResult<Self> {
        if bytes.len() != VALIDATED_SNAPSHOT_RECEIPT_BYTES {
            return Err(CoreError::InvalidValidationReceipt);
        }
        let Object::Bytes(inner) = crate::object::decode_object(bytes)? else {
            return Err(CoreError::InvalidValidationReceipt);
        };
        if inner.len() != RECEIPT_INNER_BYTES
            || inner[..8] != *RECEIPT_MAGIC
            || inner[8..10] != 1_u16.to_be_bytes()
            || inner[10] != 1
        {
            return Err(CoreError::InvalidValidationReceipt);
        }
        let store_instance_id: [u8; 16] = inner[11..27]
            .try_into()
            .map_err(|_| CoreError::InvalidValidationReceipt)?;
        let validation_authority_id: [u8; 32] = inner[27..59]
            .try_into()
            .map_err(|_| CoreError::InvalidValidationReceipt)?;
        if validation_authority_id != expected_authority
            || validation_authority_id
                != Self::validation_authority_id(store_instance_id, validation_key)
        {
            return Err(CoreError::InvalidValidationReceipt);
        }
        let integrity_epoch = u64::from_be_bytes(
            inner[59..67]
                .try_into()
                .map_err(|_| CoreError::InvalidValidationReceipt)?,
        );
        let head_generation = u64::from_be_bytes(
            inner[67..75]
                .try_into()
                .map_err(|_| CoreError::InvalidValidationReceipt)?,
        );
        let child_root_id = ObjectId::from_bytes(&inner[75..107])?;
        let transition_id = ObjectId::from_bytes(&inner[107..139])?;
        let mapping_profile_id = ObjectId::from_bytes(&inner[139..171])?;
        if mapping_profile_id != expected_profile {
            return Err(CoreError::InvalidValidationReceipt);
        }
        let mut authenticator = blake3::Hasher::new_keyed(validation_key);
        authenticator.update(b"layerfs/validated-snapshot/v1\0");
        authenticator.update(&inner[..171]);
        if authenticator.finalize().as_bytes() != &inner[171..203] {
            return Err(CoreError::InvalidValidationReceipt);
        }
        Ok(Self {
            store_instance_id,
            validation_authority_id,
            integrity_epoch,
            head_generation,
            child_root_id,
            transition_id,
            mapping_profile_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> (ValidatedSnapshotReceiptV1, [u8; KEY_LEN]) {
        let key = [0x5a; KEY_LEN];
        let store = [0x11; 16];
        let authority = ValidatedSnapshotReceiptV1::validation_authority_id(store, &key);
        (
            ValidatedSnapshotReceiptV1 {
                store_instance_id: store,
                validation_authority_id: authority,
                integrity_epoch: u64::MAX,
                head_generation: i64::MAX as u64 + 1,
                child_root_id: ObjectId::for_bytes(b"root"),
                transition_id: ObjectId::for_bytes(b"transition"),
                mapping_profile_id: ObjectId::for_bytes(b"profile"),
            },
            key,
        )
    }

    #[test]
    fn exact_receipt_round_trip_and_boundaries() {
        let (receipt, key) = receipt();
        let bytes = receipt.encode(&key).expect("receipt");
        assert_eq!(bytes.len(), 216);
        assert_eq!(
            ValidatedSnapshotReceiptV1::decode(
                &bytes,
                &key,
                receipt.mapping_profile_id,
                receipt.validation_authority_id,
            )
            .expect("decode"),
            receipt
        );
        let mut short = bytes.to_vec();
        short.pop();
        assert_eq!(
            ValidatedSnapshotReceiptV1::decode(
                &short,
                &key,
                receipt.mapping_profile_id,
                receipt.validation_authority_id,
            ),
            Err(CoreError::InvalidValidationReceipt)
        );
        let mut long = bytes.to_vec();
        long.push(0);
        assert_eq!(
            ValidatedSnapshotReceiptV1::decode(
                &long,
                &key,
                receipt.mapping_profile_id,
                receipt.validation_authority_id,
            ),
            Err(CoreError::InvalidValidationReceipt)
        );
    }

    #[test]
    fn receipt_rejects_each_authority_binding() {
        let (receipt, key) = receipt();
        let bytes = receipt.encode(&key).expect("receipt");
        let wrong_key = [0x7a; KEY_LEN];
        assert!(matches!(
            ValidatedSnapshotReceiptV1::decode(
                &bytes,
                &wrong_key,
                receipt.mapping_profile_id,
                receipt.validation_authority_id,
            ),
            Err(CoreError::InvalidValidationReceipt)
        ));
        assert!(matches!(
            ValidatedSnapshotReceiptV1::decode(
                &bytes,
                &key,
                ObjectId::for_bytes(b"other"),
                receipt.validation_authority_id,
            ),
            Err(CoreError::InvalidValidationReceipt)
        ));
    }
}
