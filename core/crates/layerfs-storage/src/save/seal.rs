//! Existing group/pack grammar with bounded immutable placement.
use crate::{
    encoding::CompressionWorkspace,
    error::{StorageError, StorageResult},
    location::{ObjectLocation, PackDomain},
    pack::{
        assemble_consuming, build_group,
        layout::{self, EncodedGroup, PackLane},
    },
    policy::{BATCH_OBJECT_LIMIT, PACK_LIMIT},
    port::PublishedPack,
};
use layerfs_content::{ObjectId, ObjectRole};

pub(super) struct Member {
    pub(super) id: ObjectId,
    pub(super) role: ObjectRole,
    pub(super) length: usize,
    pub(super) references: Vec<ObjectId>,
    pub(super) prefix: bool,
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
    pub(super) pack: PublishedPack,
    pub(super) members: Vec<(ObjectLocation, Member)>,
}
pub(super) struct Packer {
    pub(super) groups: [Group; 4],
    queued: [Queue; 4],
    pub(super) ready: Vec<Sealed>,
    pub(super) group_ns: u64,
}
impl Packer {
    pub(super) fn new() -> Self {
        Self {
            groups: std::array::from_fn(|_| Group::default()),
            queued: std::array::from_fn(|_| Queue::default()),
            ready: Vec::new(),
            group_ns: 0,
        }
    }
    pub(super) fn unfinished(&self) -> bool {
        self.groups.iter().any(|group| !group.records.is_empty())
            || self.queued.iter().any(|queue| queue.lane.is_some())
    }
    pub(super) fn pending(&self, id: ObjectId) -> bool {
        self.groups
            .iter()
            .any(|g| g.members.iter().any(|m| m.id == id))
            || self
                .queued
                .iter()
                .flat_map(|queue| &queue.groups)
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
        let started = std::time::Instant::now();
        let group = build_group(lane, &pending.records, Some(codec));
        self.group_ns += started.elapsed().as_nanos() as u64;
        let group = group?;
        let bytes = group.body_size(lane)?;
        let rows = pending.members.len();
        let queueable = matches!(
            lane,
            PackLane::Ordinary | PackLane::Chunk | PackLane::WholeFile
        ) && bytes <= PACK_LIMIT
            && rows <= BATCH_OBJECT_LIMIT;
        // Independent lanes share the original single queue's byte/row budget.
        // Charge each live lane's framing too; a lane switch need not seal the
        // preceding partially filled pack or allocate a larger buffering window.
        let charged: usize = self
            .queued
            .iter()
            .filter_map(|queue| {
                queue
                    .lane
                    .map(|lane| queue.bytes + layout::body_area_offset(lane))
            })
            .sum();
        let framing = if self.queued[lane.index()].lane.is_none() {
            layout::body_area_offset(lane)
        } else {
            0
        };
        let queued_rows: usize = self.queued.iter().map(|queue| queue.rows).sum();
        if !queueable
            || charged.saturating_add(bytes).saturating_add(framing) > PACK_LIMIT
            || queued_rows.saturating_add(rows) > BATCH_OBJECT_LIMIT
        {
            self.flush(next, end)?;
        }
        let queue = &mut self.queued[lane.index()];
        queue.lane = Some(lane);
        queue.bytes += bytes;
        queue.rows += rows;
        queue.groups.push(Framed {
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
        self.flush_if_contains(ids, next, end)?;
        Ok(forced)
    }
    pub(super) fn flush_if_contains(
        &mut self,
        ids: &[ObjectId],
        next: &mut i64,
        end: i64,
    ) -> StorageResult<()> {
        for lane in PackLane::ALL {
            if self.queued[lane.index()]
                .groups
                .iter()
                .any(|g| g.members.iter().any(|m| ids.contains(&m.id)))
            {
                self.flush_lane(lane, next, end)?;
            }
        }
        Ok(())
    }
    pub(super) fn flush(&mut self, next: &mut i64, end: i64) -> StorageResult<()> {
        for lane in PackLane::ALL {
            self.flush_lane(lane, next, end)?;
        }
        Ok(())
    }
    pub(super) fn finish_pack_bound(&self) -> usize {
        self.groups
            .iter()
            .filter(|group| !group.records.is_empty())
            .count()
            + self
                .queued
                .iter()
                // Live queues share PACK_LIMIT bytes including each lane's
                // directory. Every directory-sized subset therefore fits the
                // byte bound; only the fixed group-count limit splits it.
                // Unsealed groups can still force a flush, so retain one ID
                // per nonempty unsealed group above this queued-pack bound.
                .map(|queue| {
                    queue.lane.map_or(0, |lane| {
                        queue.groups.len().div_ceil(lane.group_count_limit())
                    })
                })
                .sum::<usize>()
    }
    fn flush_lane(&mut self, lane: PackLane, next: &mut i64, end: i64) -> StorageResult<()> {
        let queue = std::mem::take(&mut self.queued[lane.index()]);
        if queue.lane.is_none() {
            return Ok(());
        }
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
        // The one SHA-256 of these bytes; Persistence trusts it.
        let pack = PublishedPack::seal(
            pack_id,
            PackDomain::for_lane(lane),
            std::sync::Arc::new(body),
        );
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
        self.ready.push(Sealed { pack, members });
        Ok(())
    }
}
