//! Existing group/pack grammar with bounded immutable placement.
use crate::{
    encoding::CompressionWorkspace,
    error::{StorageError, StorageResult},
    location::{ObjectLocation, PackDomain, PackInfo, ValueGroupRow},
    pack::{
        assemble_consuming, build_group,
        layout::{self, EncodedGroup, PackLane},
    },
    policy::{BATCH_OBJECT_LIMIT, PACK_LIMIT},
    port::ObjectKey,
};
use layerfs_content::{ObjectId, ObjectRole};

pub(super) struct Member {
    pub(super) id: ObjectId,
    pub(super) role: ObjectRole,
    pub(super) length: usize,
    pub(super) references: Vec<ObjectId>,
    pub(super) prefix: bool,
    pub(super) ordinals: Vec<u32>,
}
#[derive(Default)]
pub(super) struct Group {
    pub(super) records: Vec<Vec<u8>>,
    pub(super) members: Vec<Member>,
    pub(super) payload: usize,
    pub(super) canonical: usize,
}
struct Framed {
    group: EncodedGroup,
    members: Vec<Member>,
}
#[derive(Default)]
struct Queue {
    lane: Option<PackLane>,
    groups: Vec<Framed>,
    bytes: usize,
    rows: usize,
}
pub(super) struct Sealed {
    pub(super) info: PackInfo,
    pub(super) body: Vec<u8>,
    pub(super) members: Vec<(ObjectLocation, Member)>,
    pub(super) value_groups: Vec<ValueGroupRow>,
}
pub(super) struct Packer {
    pub(super) groups: [Group; 5],
    queued: Queue,
    pub(super) ready: Vec<Sealed>,
    pub(super) pooled: Option<PooledTail>,
}
impl Packer {
    pub(super) fn new() -> Self {
        Self {
            groups: std::array::from_fn(|_| Group::default()),
            queued: Queue::default(),
            ready: Vec::new(),
            pooled: None,
        }
    }
    pub(super) fn unfinished(&self) -> bool {
        self.groups.iter().any(|group| !group.records.is_empty())
            || self.queued.lane.is_some()
            || self.pooled.is_some()
    }
    pub(super) fn pending(&self, id: ObjectId) -> bool {
        self.groups
            .iter()
            .any(|g| g.members.iter().any(|m| m.id == id))
            || self
                .queued
                .groups
                .iter()
                .any(|g| g.members.iter().any(|m| m.id == id))
    }
    pub(super) fn location(&self, id: ObjectId) -> Option<ObjectLocation> {
        self.ready
            .iter()
            .flat_map(|p| &p.members)
            .find(|(l, _)| l.object_id == id)
            .map(|(l, _)| *l)
    }
    pub(super) fn seal(
        &mut self,
        lane: PackLane,
        codec: &mut CompressionWorkspace,
        next: &mut i64,
        end: i64,
    ) -> StorageResult<()> {
        if self.groups[lane.index()].records.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.groups[lane.index()]);
        let group = build_group(lane, &pending.records, Some(codec))?;
        let bytes = group.body_size(lane)?;
        let rows = pending.members.len();
        let queueable = matches!(
            lane,
            PackLane::Ordinary | PackLane::Native | PackLane::WholeFile
        ) && bytes <= PACK_LIMIT
            && rows <= BATCH_OBJECT_LIMIT;
        if !queueable
            || self.queued.lane.is_some_and(|active| active != lane)
            || self.queued.bytes.saturating_add(bytes) > PACK_LIMIT
            || self.queued.rows.saturating_add(rows) > BATCH_OBJECT_LIMIT
        {
            self.flush(next, end)?;
        }
        self.queued.lane = Some(lane);
        self.queued.bytes += bytes;
        self.queued.rows += rows;
        self.queued.groups.push(Framed {
            group,
            members: pending.members,
        });
        if !queueable {
            self.flush(next, end)?;
        }
        Ok(())
    }
    pub(super) fn seal_pending(
        &mut self,
        ids: &[ObjectId],
        codec: &mut CompressionWorkspace,
        next: &mut i64,
        end: i64,
    ) -> StorageResult<usize> {
        let mut forced = 0;
        for lane in PackLane::ALL {
            if self.groups[lane.index()]
                .members
                .iter()
                .any(|m| ids.contains(&m.id))
            {
                self.seal(lane, codec, next, end)?;
                forced += 1;
            }
        }
        if self
            .queued
            .groups
            .iter()
            .any(|g| g.members.iter().any(|m| ids.contains(&m.id)))
        {
            self.flush(next, end)?;
        }
        Ok(forced)
    }
    pub(super) fn flush_if_contains(
        &mut self,
        ids: &[ObjectId],
        next: &mut i64,
        end: i64,
    ) -> StorageResult<()> {
        if self
            .queued
            .groups
            .iter()
            .any(|g| g.members.iter().any(|m| ids.contains(&m.id)))
        {
            self.flush(next, end)?;
        }
        Ok(())
    }
    pub(super) fn flush(&mut self, next: &mut i64, end: i64) -> StorageResult<()> {
        let Some(lane) = self.queued.lane else {
            return Ok(());
        };
        let queue = std::mem::take(&mut self.queued);
        let mut groups = Vec::new();
        let mut members = Vec::new();
        let mut length = layout::body_area_offset(lane);
        for framed in queue.groups {
            if !groups.is_empty()
                && !layout::append_fits(lane, length, groups.len(), &framed.group)?
            {
                self.place(
                    lane,
                    std::mem::take(&mut groups),
                    std::mem::take(&mut members),
                    next,
                    end,
                )?;
                length = layout::body_area_offset(lane);
            }
            length += framed.group.body_size(lane)?;
            groups.push(framed.group);
            members.push(framed.members);
        }
        self.place(lane, groups, members, next, end)
    }
    fn place(
        &mut self,
        lane: PackLane,
        groups: Vec<EncodedGroup>,
        member_groups: Vec<Vec<Member>>,
        next: &mut i64,
        end: i64,
    ) -> StorageResult<()> {
        if *next <= 0 || *next >= end {
            return Err(StorageError::Integrity("pack reservation exhausted"));
        }
        let pack_id = *next;
        *next = next
            .checked_add(1)
            .ok_or(StorageError::Integrity("pack id overflow"))?;
        let body = assemble_consuming(lane, groups)?;
        let info = PackInfo {
            pack_id,
            domain: PackDomain::for_lane(lane),
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        };
        let mut members = Vec::new();
        for (group_number, group) in member_groups.into_iter().enumerate() {
            for (record_number, member) in group.into_iter().enumerate() {
                members.push((
                    ObjectLocation {
                        object_id: member.id,
                        role: member.role,
                        canonical_length: member.length,
                        pack_id,
                        group_number,
                        record_number,
                    },
                    member,
                ));
            }
        }
        self.ready.push(Sealed {
            info,
            body,
            members,
            value_groups: Vec::new(),
        });
        Ok(())
    }
}

pub(super) struct PooledTail {
    pub(super) pack_id: i64,
    pub(super) groups: Vec<EncodedGroup>,
    pub(super) rows: Vec<ValueGroupRow>,
    length: usize,
}
impl Packer {
    pub(super) fn pooled_row(&self, ordinal: u32) -> Option<ValueGroupRow> {
        self.pooled
            .iter()
            .flat_map(|tail| &tail.rows)
            .chain(self.ready.iter().flat_map(|pack| &pack.value_groups))
            .find(|row| {
                ordinal >= row.first_ordinal
                    && u64::from(ordinal) < u64::from(row.first_ordinal) + row.count as u64
            })
            .copied()
    }
    pub(super) fn pooled_body(&self, id: i64) -> StorageResult<Option<Vec<u8>>> {
        self.pooled
            .as_ref()
            .filter(|tail| tail.pack_id == id)
            .map(|tail| crate::pack::assemble(PackLane::PooledMetadata, &tail.groups))
            .transpose()
    }
    pub(super) fn add_values(
        &mut self,
        first: u32,
        group: crate::encoding::pool::BuiltGroup,
        next: &mut i64,
        end: i64,
    ) -> StorageResult<()> {
        let lane = PackLane::PooledMetadata;
        let fits = self
            .pooled
            .as_ref()
            .map(|tail| layout::append_fits(lane, tail.length, tail.groups.len(), &group.group))
            .transpose()?
            .unwrap_or(false);
        if !fits {
            self.seal_pool()?;
            if *next <= 0 || *next >= end {
                return Err(StorageError::Integrity("pooled pack reservation exhausted"));
            }
            let pack_id = *next;
            *next = next
                .checked_add(1)
                .ok_or(StorageError::Integrity("pack id overflow"))?;
            self.pooled = Some(PooledTail {
                pack_id,
                groups: Vec::new(),
                rows: Vec::new(),
                length: layout::body_area_offset(lane),
            });
        }
        let tail = self
            .pooled
            .as_mut()
            .ok_or(StorageError::Integrity("pooled tail"))?;
        tail.rows.push(ValueGroupRow {
            first_ordinal: first,
            count: group.count,
            pack_id: tail.pack_id,
            group_number: tail.groups.len(),
            digest: group.digest,
        });
        tail.length += group.group.body_size(lane)?;
        tail.groups.push(group.group);
        Ok(())
    }
    pub(super) fn seal_pool(&mut self) -> StorageResult<()> {
        let Some(tail) = self.pooled.take() else {
            return Ok(());
        };
        let body = assemble_consuming(PackLane::PooledMetadata, tail.groups)?;
        let info = PackInfo {
            pack_id: tail.pack_id,
            domain: PackDomain::Metadata,
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        };
        self.ready.push(Sealed {
            info,
            body,
            members: Vec::new(),
            value_groups: tail.rows,
        });
        Ok(())
    }
    pub(super) fn close_ordinals(&mut self, ordinals: &[u32]) -> StorageResult<usize> {
        let needed = self.pooled.as_ref().is_some_and(|tail| {
            tail.rows.iter().any(|row| {
                ordinals.iter().any(|ordinal| {
                    *ordinal >= row.first_ordinal
                        && u64::from(*ordinal) < u64::from(row.first_ordinal) + row.count as u64
                })
            })
        });
        if needed {
            self.seal_pool()?;
        }
        Ok(usize::from(needed))
    }
}
