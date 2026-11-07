//! External transactional memory metadata engine shared with the PostgreSQL contract.
#![allow(dead_code)]
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, ObjectLocation, PackInfo, SignatureRow},
    policy::{StoragePolicy, DEPENDENCY_PACK_CACHE_BYTES, READ_OBJECT_LIMIT},
    port::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
/// One pack as this engine stores it; a test may corrupt its stored rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPack {
    pub info: PackInfo,
    pub body: Arc<Vec<u8>>,
}
#[derive(Clone)]
pub struct MetadataState {
    pub policy: StoragePolicy,
    pub packs: BTreeMap<i64, StoredPack>,
    pub objects: BTreeMap<ObjectId, ObjectLocation>,
    pub values: BTreeMap<u32, [u8; 73]>,
    pub signatures: BTreeMap<usize, SignatureRow>,
    pub next_pack: i64,
    pub next_ordinal: u64,
    pub registrations: Vec<Publication>,
    pub fail_register: Option<PersistenceError>,
}
impl Default for MetadataState {
    fn default() -> Self {
        Self {
            policy: StoragePolicy::frozen_default(),
            packs: BTreeMap::new(),
            objects: BTreeMap::new(),
            values: BTreeMap::new(),
            signatures: BTreeMap::new(),
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
    pub acquired_pack_ids: Mutex<Vec<i64>>,
}
impl PackPersistence for MemoryMetadata {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        self.calls.lock().unwrap().push(("policy", 1));
        Ok(self.state.lock().unwrap().policy)
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        assert!(ids.len() <= READ_OBJECT_LIMIT);
        self.calls.lock().unwrap().push(("locate", ids.len()));
        let state = self.state.lock().unwrap();
        out.clear();
        for id in ids.iter().copied().collect::<BTreeSet<_>>() {
            if let Some(location) = state.objects.get(&id) {
                let pack = state
                    .packs
                    .get(&location.pack_id)
                    .ok_or(PersistenceError::Missing)?;
                out.push(LocatedObject {
                    location: *location,
                    pack: pack.info,
                });
            }
        }
        Ok(())
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        self.calls.lock().unwrap().push(("read_pack_selection", 1));
        let state = self.state.lock().unwrap();
        let pack = state.packs.get(&id).ok_or(PersistenceError::Missing)?;
        if pack.info.length != pack.body.len() {
            return Err(PersistenceError::Malformed);
        }
        PersistedPackRead::acquire(pack.info, plan, |offset, out| {
            let bytes = pack
                .body
                .get(offset..offset + out.len())
                .ok_or(PersistenceError::Malformed)?;
            out.copy_from_slice(bytes);
            Ok(())
        })
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Result<AcquiredPackRead, PersistenceError> {
        self.calls.lock().unwrap().push(("read_scoped_pack", 1));
        self.acquired_pack_ids.lock().unwrap().push(id);
        let state = self.state.lock().unwrap();
        let pack = state.packs.get(&id).ok_or(PersistenceError::Missing)?;
        if pack.info.length != pack.body.len() {
            return Err(PersistenceError::Malformed);
        }
        AcquiredPackRead::acquire(pack.info, plan, |offset, out| {
            let bytes = pack
                .body
                .get(offset..offset + out.len())
                .ok_or(PersistenceError::Malformed)?;
            out.copy_from_slice(bytes);
            Ok(())
        })
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        self.calls.lock().unwrap().push(("read_packs", ids.len()));
        let state = self.state.lock().unwrap();
        out.clear();
        for id in ids {
            let pack = state.packs.get(id).ok_or(PersistenceError::Missing)?;
            out.push(PersistedPack::authenticate(
                pack.info,
                pack.body.as_ref().clone(),
            )?);
        }
        assert!(
            out.iter().map(|row| row.body().len()).sum::<usize>() <= DEPENDENCY_PACK_CACHE_BYTES
        );
        Ok(())
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.calls
            .lock()
            .unwrap()
            .push(("inode_values", ordinals.len()));
        assert!(ordinals.len() <= READ_OBJECT_LIMIT);
        assert!(ordinals.windows(2).all(|pair| pair[0] < pair[1]));
        let state = self.state.lock().unwrap();
        out.clear();
        out.extend(ordinals.iter().filter_map(|ordinal| {
            state.values.get(ordinal).map(|value| InodeValueRow {
                ordinal: *ordinal,
                value: *value,
            })
        }));
        Ok(())
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
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
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError> {
        self.calls.lock().unwrap().push(("reserve", 1));
        let mut state = self.state.lock().unwrap();
        let first_pack_id = state.next_pack;
        let first_ordinal =
            u32::try_from(state.next_ordinal).map_err(|_| PersistenceError::Malformed)?;
        state.next_pack += request.packs as i64;
        state.next_ordinal += request.ordinals as u64;
        Ok(Reserved {
            first_pack_id,
            first_ordinal,
        })
    }
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError> {
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
            let info = pack.info();
            if next.packs.contains_key(&info.pack_id) {
                return Err(PersistenceError::Malformed);
            }
            next.next_pack = next.next_pack.max(info.pack_id + 1);
            let body = pack.body().clone();
            next.packs.insert(info.pack_id, StoredPack { info, body });
        }
        for row in &batch.objects {
            if !next.packs.contains_key(&row.pack_id) {
                return Err(PersistenceError::Malformed);
            }
            match next.objects.entry(row.object_id) {
                std::collections::btree_map::Entry::Occupied(_) => lost.push(row.object_id),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(*row);
                }
            }
        }
        for row in &batch.inode_values {
            if row.ordinal == 0 || next.values.insert(row.ordinal, row.value).is_some() {
                return Err(PersistenceError::Malformed);
            }
            next.next_ordinal = next.next_ordinal.max(u64::from(row.ordinal) + 1);
        }
        for row in &batch.signatures {
            next.signatures.insert(row.slot, *row);
        }
        if let Some((first, count)) = batch.release_ordinals {
            if u64::from(first) + count as u64 == next.next_ordinal {
                next.next_ordinal = u64::from(first);
            }
        }
        next.registrations.push(batch.clone());
        *state = next;
        Ok(Published { lost })
    }
}
