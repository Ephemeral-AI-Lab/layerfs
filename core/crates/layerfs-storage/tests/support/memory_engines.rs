//! External in-memory implementations shared by port-path vectors.
#![allow(dead_code)]

use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{LocatedObject, ObjectLocation, PackDomain, PackInfo, SignatureRow, ValueGroupRow},
    policy::{StoragePolicy, DEPENDENCY_PACK_CACHE_BYTES, READ_OBJECT_LIMIT},
    port::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

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
#[derive(Clone)]
pub struct MetadataState {
    pub policy: StoragePolicy,
    pub packs: BTreeMap<i64, RegisteredPack>,
    pub objects: BTreeMap<ObjectId, ObjectLocation>,
    pub groups: BTreeMap<u32, ValueGroupRow>,
    pub signatures: BTreeMap<usize, SignatureRow>,
    pub window: u32,
    pub window_values: usize,
    pub next_pack: i64,
    pub next_ordinal: u64,
    pub registrations: Vec<Registration>,
    pub fail_register: Option<MetadataError>,
}
impl Default for MetadataState {
    fn default() -> Self {
        Self {
            policy: StoragePolicy::frozen_default(),
            packs: BTreeMap::new(),
            objects: BTreeMap::new(),
            groups: BTreeMap::new(),
            signatures: BTreeMap::new(),
            window: 1,
            window_values: 0,
            next_pack: 1,
            next_ordinal: 1,
            registrations: Vec::new(),
            fail_register: None,
        }
    }
}
#[derive(Default)]
pub struct MemoryMetadata {
    pub state: Mutex<MetadataState>,
    pub calls: Mutex<Vec<(&'static str, usize)>>,
}
impl MetadataStore for MemoryMetadata {
    fn policy(&self) -> Result<StoragePolicy, MetadataError> {
        self.calls.lock().unwrap().push(("policy", 1));
        Ok(self.state.lock().unwrap().policy)
    }
    fn locate(&self, ids: &[ObjectId], out: &mut Vec<LocatedObject>) -> Result<(), MetadataError> {
        assert!(ids.len() <= READ_OBJECT_LIMIT);
        self.calls.lock().unwrap().push(("locate", ids.len()));
        let state = self.state.lock().unwrap();
        out.clear();
        for id in ids.iter().copied().collect::<BTreeSet<_>>() {
            if let Some(location) = state.objects.get(&id) {
                let pack = state
                    .packs
                    .get(&location.pack_id)
                    .ok_or(MetadataError::Missing)?;
                out.push(LocatedObject {
                    location: *location,
                    pack: pack.info,
                });
            }
        }
        Ok(())
    }
    fn read_packs(&self, ids: &[i64], out: &mut Vec<MetadataPack>) -> Result<(), MetadataError> {
        self.calls.lock().unwrap().push(("read_packs", ids.len()));
        let state = self.state.lock().unwrap();
        out.clear();
        for id in ids {
            let pack = state.packs.get(id).ok_or(MetadataError::Missing)?;
            if pack.info.domain != PackDomain::Metadata {
                return Err(MetadataError::Malformed);
            }
            out.push(MetadataPack {
                info: pack.info,
                body: pack.body.clone().ok_or(MetadataError::Missing)?,
            });
        }
        assert!(out.iter().map(|row| row.body.len()).sum::<usize>() <= DEPENDENCY_PACK_CACHE_BYTES);
        Ok(())
    }
    fn value_groups(&self, query: ValueGroupQuery<'_>) -> Result<ValueGroups, MetadataError> {
        self.calls.lock().unwrap().push(("value_groups", 1));
        let state = self.state.lock().unwrap();
        let (rows, next) = match query {
            ValueGroupQuery::Ordinals(ids) => {
                assert!(ids.len() <= READ_OBJECT_LIMIT);
                let mut groups = BTreeMap::new();
                for ordinal in ids {
                    if let Some((first, row)) = state.groups.range(..=ordinal).next_back() {
                        if u64::from(*ordinal) < u64::from(*first) + row.count as u64 {
                            groups.insert(*first, *row);
                        }
                    }
                }
                (groups.into_values().collect(), None)
            }
            ValueGroupQuery::Page { from, limit } => {
                assert!(limit <= READ_OBJECT_LIMIT);
                let rows: Vec<_> = state
                    .groups
                    .range(from..)
                    .take(limit)
                    .map(|(_, row)| *row)
                    .collect();
                let next = if limit == 0 {
                    None
                } else {
                    state
                        .groups
                        .range(from..)
                        .nth(limit)
                        .map(|(first, _)| *first)
                };
                (rows, next)
            }
        };
        Ok(ValueGroups {
            rows,
            next,
            window_start: state.window,
        })
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), MetadataError> {
        self.calls.lock().unwrap().push(("signatures", 1));
        *out = self
            .state
            .lock()
            .unwrap()
            .signatures
            .values()
            .copied()
            .collect();
        out.sort_by_key(|row| row.stamp);
        Ok(())
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, MetadataError> {
        self.calls.lock().unwrap().push(("reserve", 1));
        let mut state = self.state.lock().unwrap();
        let first_pack_id = state.next_pack;
        let first_ordinal =
            u32::try_from(state.next_ordinal).map_err(|_| MetadataError::Malformed)?;
        state.next_pack += request.packs as i64;
        state.next_ordinal += request.ordinals as u64;
        Ok(Reserved {
            first_pack_id,
            first_ordinal,
        })
    }
    fn register(&self, batch: &Registration) -> Result<Registered, MetadataError> {
        self.calls
            .lock()
            .unwrap()
            .push(("register", batch.objects.len()));
        let mut state = self.state.lock().unwrap();
        if let Some(error) = state.fail_register.take() {
            return Err(error);
        }
        let mut next = state.clone();
        let mut lost = Vec::new();
        for pack in &batch.packs {
            if next.packs.contains_key(&pack.info.pack_id) {
                return Err(MetadataError::Malformed);
            }
            if (pack.info.domain == PackDomain::Metadata) != pack.body.is_some() {
                return Err(MetadataError::Malformed);
            }
            next.next_pack = next.next_pack.max(pack.info.pack_id + 1);
            next.packs.insert(pack.info.pack_id, pack.clone());
        }
        for row in &batch.objects {
            if !next.packs.contains_key(&row.pack_id) {
                return Err(MetadataError::Malformed);
            }
            match next.objects.entry(row.object_id) {
                std::collections::btree_map::Entry::Occupied(_) => lost.push(row.object_id),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(*row);
                }
            }
        }
        for row in &batch.value_groups {
            if next.window_values + row.count > layerfs_storage::policy::METADATA_INDEX_VALUES {
                next.window = row.first_ordinal;
                next.window_values = row.count;
            } else {
                next.window_values += row.count;
            }
            next.groups.insert(row.first_ordinal, *row);
            next.next_ordinal = next
                .next_ordinal
                .max(u64::from(row.first_ordinal) + row.count as u64);
        }
        for row in &batch.signatures {
            next.signatures.insert(row.slot, *row);
        }
        if let Some(start) = batch.window_start {
            next.window = next.window.max(start);
        }
        if let Some((first, count)) = batch.release_ordinals {
            if u64::from(first) + count as u64 == next.next_ordinal {
                next.next_ordinal = u64::from(first);
            }
        }
        next.registrations.push(batch.clone());
        *state = next;
        Ok(Registered { lost })
    }
}

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
