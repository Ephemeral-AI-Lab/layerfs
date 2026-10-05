//! Bounded file-length facts from the owning Store catalogue and small file states.
use crate::{
    error::{StorageError, StorageResult},
    source::Source,
    storage::Storage,
};
use layerfs_content::{file::mapping, policy::WHOLE_FILE_CANONICAL_OVERHEAD, ObjectId, ObjectRole};

pub(super) fn read(
    storage: &Storage,
    ids: &[ObjectId],
    read_states: impl FnOnce(&[ObjectId]) -> StorageResult<Vec<Vec<u8>>>,
) -> StorageResult<Vec<u64>> {
    if ids.len() > storage.capacities().read_objects {
        return Err(StorageError::CapacityExceeded {
            what: "storage.file_lengths",
            limit: storage.capacities().read_objects as u64,
            actual: ids.len() as u64,
        });
    }
    storage.source.begin_demand();
    storage.source.locate(ids)?;
    let mut lengths = vec![0; ids.len()];
    let mut states = Vec::new();
    let mut positions = Vec::new();
    for (position, id) in ids.iter().enumerate() {
        let location = storage
            .source
            .location(*id, i64::MAX)?
            .ok_or(StorageError::ObjectMissing(*id))?;
        match location.role {
            ObjectRole::WholeFile => {
                let length = location
                    .canonical_length
                    .checked_sub(WHOLE_FILE_CANONICAL_OVERHEAD)
                    .filter(|length| {
                        *length > 0
                            && *length
                                <= storage
                                    .policy()
                                    .construction()
                                    .capacities()
                                    .whole_file_raw_limit
                    })
                    .ok_or(StorageError::Integrity("whole-file length descriptor"))?;
                lengths[position] = length as u64;
            }
            ObjectRole::FileState => {
                if location.canonical_length != mapping::FILE_STATE_CANONICAL_BYTES {
                    return Err(StorageError::Integrity("file-state length descriptor"));
                }
                states.push(*id);
                positions.push(position);
            }
            _ => return Err(layerfs_content::ContentError::WrongLogicalRole.into()),
        }
    }
    if !states.is_empty() {
        let canonical = read_states(&states)?;
        if canonical.len() != states.len() {
            return Err(StorageError::Integrity("file-state length cardinality"));
        }
        for (position, bytes) in positions.into_iter().zip(canonical) {
            lengths[position] = mapping::decode_file_state(&bytes)?.logical_len;
        }
    }
    Ok(lengths)
}
