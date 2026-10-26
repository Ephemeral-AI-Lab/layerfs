//! External in-memory implementations shared by port-path vectors.
#![allow(dead_code)]

use layerfs_storage::port::*;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
pub struct MemoryObjects {
    pub bodies: Mutex<BTreeMap<ObjectKey, Vec<u8>>>,
    pub calls: Mutex<Vec<&'static str>>,
}
impl ObjectStore for MemoryObjects {
    fn put_if_absent(&self, key: ObjectKey, body: &[u8]) -> Result<Put, ObjectError> {
        self.calls.lock().unwrap().push("put");
        let mut bodies = self.bodies.lock().unwrap();
        match bodies.entry(key) {
            std::collections::btree_map::Entry::Occupied(_) => Ok(Put::AlreadyPresent),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(body.to_vec());
                Ok(Put::Created)
            }
        }
    }
    fn read(
        &self,
        key: ObjectKey,
        range: Option<ByteRange>,
        out: &mut Vec<u8>,
    ) -> Result<(), ObjectError> {
        self.calls.lock().unwrap().push("read");
        let bodies = self.bodies.lock().unwrap();
        let body = bodies.get(&key).ok_or(ObjectError::Missing)?;
        let selected = if let Some(range) = range {
            let end = range
                .start
                .checked_add(range.length)
                .ok_or(ObjectError::Malformed)?;
            if range.length == 0 || end > body.len() as u64 {
                return Err(ObjectError::Malformed);
            }
            &body[range.start as usize..end as usize]
        } else {
            body.as_slice()
        };
        *out = selected.to_vec();
        Ok(())
    }
    fn head(&self, key: ObjectKey) -> Result<Option<u64>, ObjectError> {
        self.calls.lock().unwrap().push("head");
        Ok(self
            .bodies
            .lock()
            .unwrap()
            .get(&key)
            .map(|body| body.len() as u64))
    }
}
