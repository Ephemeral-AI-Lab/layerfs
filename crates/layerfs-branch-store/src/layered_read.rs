use crate::BranchStore;
use layerfs_core::ObjectId;
use layerfs_storage_core::internal::{CanonicalObject, ObjectSource};
use layerfs_storage_core::{Result, StorageError};

impl ObjectSource for BranchStore {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        parent_on_missing(self.db.read_object_row(id), || self.parent.read_object(id))
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        let mut rows = self.db.existing_object_rows(ids)?;
        let missing = ids
            .iter()
            .filter(|id| !rows.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for object in self.parent.read_objects(&missing)? {
            rows.insert(object.id, object.bytes);
        }
        ids.iter()
            .map(|id| {
                Ok(CanonicalObject {
                    id: *id,
                    bytes: rows.get(id).cloned().ok_or(StorageError::MissingBaseData)?,
                })
            })
            .collect()
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        let mut ids = ids.to_vec();
        ids.sort();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(StorageError::Integrity("object read IDs"));
        }
        let missing = self.db.missing_objects(&ids)?;
        let mut local = Vec::new();
        let mut parent = Vec::new();
        for (index, id) in ids.into_iter().enumerate() {
            if missing.is_missing(index)? {
                parent.push(id);
            } else {
                local.push(id);
            }
        }
        self.db.visit_object_rows(&local, visitor)?;
        self.parent.visit_objects(&parent, visitor)
    }
}

fn parent_on_missing<T>(local: Result<T>, parent: impl FnOnce() -> Result<T>) -> Result<T> {
    match local {
        Err(StorageError::MissingBaseData) => parent(),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::parent_on_missing;
    use layerfs_storage_core::StorageError;
    use std::cell::Cell;

    #[test]
    fn parent_is_used_only_for_missing_local_data() {
        let called = Cell::new(0);
        let local_error =
            parent_on_missing::<Vec<u8>>(Err(StorageError::Integrity("local object")), || {
                called.set(called.get() + 1);
                Ok(vec![1])
            });
        assert!(matches!(local_error, Err(StorageError::Integrity(_))));
        assert_eq!(called.get(), 0);

        let inherited = parent_on_missing(Err(StorageError::MissingBaseData), || {
            called.set(called.get() + 1);
            Ok(vec![2])
        });
        assert_eq!(inherited.unwrap(), vec![2]);
        assert_eq!(called.get(), 1);
    }
}
