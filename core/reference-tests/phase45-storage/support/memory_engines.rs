//! External in-memory implementations shared by port-path vectors.
#![allow(dead_code)]

use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo, ValueGroupRow},
    port::*,
};
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
#[path = "memory_metadata.rs"]
mod metadata;
pub use metadata::MemoryMetadata;

/// Captures declared old-path bytes as a test oracle, then installs via the ports.
pub fn snapshot(path: &std::path::Path) -> (Registration, Vec<(ObjectKey, Vec<u8>)>) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut batch = Registration::default();
    let mut payload = Vec::new();
    let mut stmt = connection
        .prepare("SELECT pack_id,data FROM object_packs ORDER BY pack_id")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .unwrap();
    for row in rows {
        let (pack_id, mut body) = row.unwrap();
        let length = layerfs_storage::pack::layout::declared_length(&body).unwrap();
        body.truncate(length);
        let header = layerfs_storage::pack::layout::parse_header(&body).unwrap();
        let domain = PackDomain::for_lane(header.lane);
        let key = ObjectKey::for_bytes(&body);
        let info = PackInfo {
            pack_id,
            domain,
            key,
            length,
        };
        if domain == PackDomain::Payload {
            payload.push((key, body.clone()));
        }
        batch.packs.push(RegisteredPack {
            info,
            body: (domain == PackDomain::Metadata).then_some(body),
        });
    }
    let mut stmt=connection.prepare("SELECT object_id,object_role,canonical_length,pack_id,group_number,record_number FROM objects ORDER BY pack_id,group_number,record_number").unwrap();
    batch.objects = stmt
        .query_map([], |row| {
            Ok(ObjectLocation {
                object_id: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0)?).unwrap(),
                role: layerfs_content::ObjectRole::from_code(row.get::<_, u8>(1)?).unwrap(),
                canonical_length: usize::try_from(row.get::<_, i64>(2)?).unwrap(),
                pack_id: row.get(3)?,
                group_number: usize::try_from(row.get::<_, i64>(4)?).unwrap(),
                record_number: usize::try_from(row.get::<_, i64>(5)?).unwrap(),
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut stmt=connection.prepare("SELECT first_ordinal,count,pack_id,group_number,digest FROM metadata_value_groups ORDER BY first_ordinal").unwrap();
    batch.value_groups = stmt
        .query_map([], |row| {
            Ok(ValueGroupRow {
                first_ordinal: row.get(0)?,
                count: usize::try_from(row.get::<_, i64>(1)?).unwrap(),
                pack_id: row.get(2)?,
                group_number: usize::try_from(row.get::<_, i64>(3)?).unwrap(),
                digest: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(4)?).unwrap(),
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (batch, payload)
}
