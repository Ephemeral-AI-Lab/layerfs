//! Bounded local placed snapshots. No pending payload bytes enter SQLite.
use crate::strict_client::Catalog;
use crate::{
    strict_catalog::{CatalogScope, LogicalUse, PlacementDomain, Reference},
    strict_read::{placement, Access},
};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    access::{ObjectLocation, ValueGroupRow},
    encoding::EncodedRecord,
    pack::{
        assemble,
        layout::{self, EncodedGroup, PackLane},
    },
    PackAccess, StorageError, StorageResult,
};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Member {
    pub id: ObjectId,
    pub role: ObjectRole,
    pub length: usize,
    pub logical_use: LogicalUse,
    pub references: Vec<Reference>,
    pub base: Option<ObjectId>,
    pub group: usize,
    pub record: usize,
}
pub struct PendingRecord {
    pub member: Member,
    pub aliases: Vec<Member>,
    pub encoded: EncodedRecord,
    pub canonical: Vec<u8>,
}
pub struct Tail {
    pub domain: PlacementDomain,
    pub lane: PackLane,
    pub order: i64,
    pub digest: [u8; 32],
    pub generation: u64,
    pub groups: Vec<EncodedGroup>,
    pub members: Vec<Member>,
    pub used: usize,
}
impl Tail {
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        assemble::assemble(self.lane, &self.groups).map_err(|e| e.to_string())
    }
    pub fn fits(&self, group: &EncodedGroup) -> Result<bool, String> {
        layout::append_fits(self.lane, self.used, self.groups.len(), group)
            .map_err(|e| e.to_string())
    }
}
#[derive(Default)]
pub struct PrivateWindow {
    pub tails: BTreeMap<(PlacementDomain, usize), Tail>,
    pub pending: BTreeMap<(PlacementDomain, usize), Vec<PendingRecord>>,
    pub groups: BTreeMap<u32, ValueGroupRow>,
    pub generation: u64,
}
impl PrivateWindow {
    pub fn member(&self, domain: PlacementDomain, id: ObjectId) -> Option<(&Tail, &Member)> {
        self.tails
            .values()
            .filter(|t| t.domain == domain)
            .find_map(|t| t.members.iter().find(|m| m.id == id).map(|m| (t, m)))
    }
    pub fn pending_id(&self, domain: PlacementDomain, id: ObjectId) -> bool {
        self.pending
            .iter()
            .filter(|((d, _), _)| *d == domain)
            .any(|(_, records)| records.iter().any(|r| r.member.id == id))
    }
    pub fn members(&self) -> usize {
        self.tails.values().map(|t| t.members.len()).sum::<usize>()
            + self
                .pending
                .values()
                .flatten()
                .map(|r| 1 + r.aliases.len())
                .sum::<usize>()
    }
    pub fn canonical(&self) -> usize {
        self.tails
            .values()
            .flat_map(|t| &t.members)
            .map(|m| m.length)
            .sum::<usize>()
            + self
                .pending
                .values()
                .flatten()
                .map(|r| r.member.length)
                .sum::<usize>()
    }
    pub fn body(&self, order: i64) -> Option<&Tail> {
        self.tails.values().find(|t| t.order == order)
    }
}

pub struct PrivateAccess<'a> {
    pub persisted: Access<'a>,
    pub window: &'a PrivateWindow,
    pub save: i64,
}
impl PackAccess for PrivateAccess<'_> {
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        let role = self
            .window
            .tails
            .values()
            .flat_map(|t| &t.members)
            .find(|m| m.id == id)
            .map(|m| m.role);
        if let Some(role) = role {
            let domain = placement(self.persisted.logical_use, role);
            if let Some((tail, member)) = self
                .window
                .tails
                .values()
                .filter(|t| t.domain == domain)
                .find_map(|t| {
                    t.members
                        .iter()
                        .find(|m| m.id == id && m.logical_use == self.persisted.logical_use)
                        .map(|m| (t, m))
                })
            {
                if self.persisted.scope.own_save != Some(self.save)
                    || tail.order > ceiling
                    || tail.order > self.persisted.scope.ceiling
                {
                    return Err(StorageError::Integrity("private body save/scope"));
                }
                if member.logical_use != self.persisted.logical_use {
                    return Err(StorageError::Integrity("private required use"));
                }
                return Ok(Some(ObjectLocation {
                    object_id: id,
                    role,
                    canonical_length: member.length,
                    pack_id: tail.order,
                    group_number: member.group,
                    record_number: member.record,
                }));
            }
        }
        self.persisted.location(id, ceiling)
    }
    fn pack_bytes(&self, order: i64) -> StorageResult<Vec<u8>> {
        if let Some(tail) = self.window.body(order) {
            if self.persisted.scope.own_save != Some(self.save)
                || order > self.persisted.scope.ceiling
                || (self.persisted.logical_use == LogicalUse::MetadataGraph
                    && tail.domain != PlacementDomain::Metadata)
            {
                return Err(StorageError::Integrity("private domain/body scope"));
            }
            let bytes = tail
                .bytes()
                .map_err(|_| StorageError::Integrity("private envelope"))?;
            if crate::minio::digest(&bytes) != tail.digest {
                return Err(StorageError::Integrity("private body generation"));
            }
            return Ok(bytes);
        }
        self.persisted.pack_bytes(order)
    }
    fn group_for(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
        if let Some((_, row)) = self.window.groups.range(..=ordinal).next_back() {
            if u64::from(ordinal) < u64::from(row.first_ordinal) + row.count as u64 {
                if self.persisted.scope.own_save != Some(self.save)
                    || row.pack_id > self.persisted.scope.ceiling
                {
                    return Err(StorageError::Integrity("private group eligibility"));
                }
                return Ok(Some(*row));
            }
        }
        self.persisted.group_for(ordinal)
    }
    fn metadata_window_start(&self) -> StorageResult<u32> {
        self.persisted.metadata_window_start()
    }
    fn group_page(&self, from: u32, limit: usize) -> StorageResult<Vec<ValueGroupRow>> {
        let mut rows = self.persisted.group_page(from, limit)?;
        if self.persisted.scope.own_save != Some(self.save) {
            return Ok(rows);
        }
        for (_, row) in self.window.groups.range(from..).take(limit) {
            if row.pack_id <= self.persisted.scope.ceiling {
                rows.push(*row)
            }
        }
        rows.sort_by_key(|r| r.first_ordinal);
        rows.truncate(limit);
        Ok(rows)
    }
}

pub fn scope(catalog: &dyn Catalog, save: i64) -> Result<CatalogScope, String> {
    catalog.capture(Some(save))
}
