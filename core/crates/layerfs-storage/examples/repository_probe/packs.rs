use std::fs;
use std::path::{Path, PathBuf};

use layerfs_content::{ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId};
use layerfs_storage::cas::SaveProfile;
use layerfs_storage::encoding::{encode_full, CompressionWorkspace};
use layerfs_storage::pack::{
    append_fits, assemble_consuming, assembled_length, build_group, framed_group_length,
    EncodedGroup, PackLane,
};
use layerfs_storage::policy::{GROUP_TARGET, PACK_LIMIT, RECORD_COUNT_LIMIT};
use layerfs_storage::{StorageCapacities, StoragePolicy};
use rusqlite::{params, Connection, OptionalExtension};

use super::Result;
#[derive(Default)]
struct Lane {
    records: Vec<Vec<u8>>,
    ids: Vec<ObjectId>,
    payload: usize,
    groups: Vec<EncodedGroup>,
    members: Vec<Vec<ObjectId>>,
    used: usize,
}

pub struct Packer {
    pub db: Connection,
    root: PathBuf,
    lanes: [Lane; 5],
    codec: CompressionWorkspace,
    capacities: StorageCapacities,
    profile: SaveProfile,
    pub objects: u64,
    pub duplicates: u64,
    pub packs: u64,
    pub bytes: u64,
}

impl Packer {
    pub fn new(root: &Path) -> Result<Self> {
        let db = Connection::open(root.join("catalog.sqlite"))?;
        db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA cache_size=-512; PRAGMA mmap_size=0; PRAGMA busy_timeout=0; PRAGMA temp_store=FILE;
          ALTER TABLE entries ADD COLUMN content_root BLOB;
          CREATE TABLE objects(id BLOB PRIMARY KEY,role INTEGER NOT NULL,canonical_length INTEGER NOT NULL,pack_id INTEGER,group_no INTEGER,record_no INTEGER) WITHOUT ROWID;
          CREATE TABLE packs(id INTEGER PRIMARY KEY,size INTEGER NOT NULL,blake3 TEXT NOT NULL);
          CREATE TABLE publication(ready INTEGER NOT NULL CHECK(ready IN(0,1)));
          INSERT INTO publication VALUES(0); BEGIN;")?;
        Ok(Self {
            db,
            root: root.join("packs"),
            lanes: std::array::from_fn(|_| Lane::default()),
            codec: CompressionWorkspace::new()?,
            capacities: StorageCapacities::from_policy(StoragePolicy::default())?,
            profile: SaveProfile::default(),
            objects: 0,
            duplicates: 0,
            packs: 0,
            bytes: 0,
        })
    }

    fn seal(&mut self, lane: PackLane) -> Result<()> {
        let state = &mut self.lanes[lane.index()];
        if state.groups.is_empty() {
            return Ok(());
        }
        let bytes = assemble_consuming(lane, std::mem::take(&mut state.groups))?;
        if bytes.len() > PACK_LIMIT || self.bytes + bytes.len() as u64 > 4 * 1024 * 1024 * 1024 {
            return Err("prepared pack disk admission exceeded".into());
        }
        let id = self.packs;
        fs::write(self.root.join(format!("pack-{id:08}.bin")), &bytes)?;
        self.db
            .prepare_cached("INSERT INTO packs VALUES(?1,?2,?3)")?
            .execute(params![
                i64::try_from(id)?,
                i64::try_from(bytes.len())?,
                blake3::hash(&bytes).to_hex().to_string()
            ])?;
        let mut update = self.db.prepare_cached("UPDATE objects SET pack_id=?1,group_no=?2,record_no=?3 WHERE id=?4 AND pack_id IS NULL")?;
        for (group, members) in state.members.drain(..).enumerate() {
            for (record, member) in members.into_iter().enumerate() {
                if update.execute(params![
                    i64::try_from(id)?,
                    i64::try_from(group)?,
                    i64::try_from(record)?,
                    member.as_bytes().as_slice()
                ])? != 1
                {
                    return Err("locator publication cardinality".into());
                }
            }
        }
        self.packs += 1;
        self.bytes += bytes.len() as u64;
        state.used = 0;
        Ok(())
    }

    fn group(&mut self, lane: PackLane) -> Result<()> {
        let state = &mut self.lanes[lane.index()];
        if state.records.is_empty() {
            return Ok(());
        }
        let codec = if lane == PackLane::Ordinary {
            Some(&mut self.codec)
        } else {
            None
        };
        let group = build_group(lane, &state.records, codec)?;
        state.records.clear();
        state.payload = 0;
        let members = std::mem::take(&mut state.ids);
        if !state.groups.is_empty() && !append_fits(lane, state.used, state.groups.len(), &group)? {
            self.seal(lane)?;
        }
        let state = &mut self.lanes[lane.index()];
        state.groups.push(group);
        state.members.push(members);
        state.used = assembled_length(lane, &state.groups)?;
        Ok(())
    }

    fn put(&mut self, object: FinalizedObject) -> Result<()> {
        let id = object.id();
        let existing: Option<(u8, i64)> = self
            .db
            .prepare_cached("SELECT role,canonical_length FROM objects WHERE id=?1")?
            .query_row([id.as_bytes().as_slice()], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if let Some((role, length)) = existing {
            if role != object.role().code() || usize::try_from(length)? != object.canonical_len() {
                return Err("duplicate identity descriptor mismatch".into());
            }
            self.duplicates += 1;
            return Ok(());
        }
        let encoded = encode_full(
            object.canonical(),
            object.role(),
            &self.capacities,
            &mut self.codec,
            &mut self.profile,
        )?;
        let lane = encoded.lane;
        if lane == PackLane::Singleton || lane == PackLane::PooledMetadata {
            return Err("unexpected default file construction lane".into());
        }
        let state = &self.lanes[lane.index()];
        let next = framed_group_length(state.records.len() + 1, state.payload + encoded.width())?;
        if !state.records.is_empty()
            && (next > GROUP_TARGET || state.records.len() >= RECORD_COUNT_LIMIT)
        {
            self.group(lane)?;
        }
        self.db
            .prepare_cached("INSERT INTO objects(id,role,canonical_length) VALUES(?1,?2,?3)")?
            .execute(params![
                id.as_bytes().as_slice(),
                object.role().code(),
                i64::try_from(object.canonical_len())?
            ])?;
        let state = &mut self.lanes[lane.index()];
        state.payload += encoded.width();
        state.records.push(encoded.record);
        state.ids.push(id);
        self.objects += 1;
        // Bound private MEMORY-journal transaction work by emitted objects.
        if self.objects % 256 == 0 {
            self.db.execute_batch("COMMIT; BEGIN;")?;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        for lane in PackLane::ALL {
            self.group(lane)?;
            self.seal(lane)?;
        }
        let pending: i64 = self.db.query_row(
            "SELECT count(*) FROM objects WHERE pack_id IS NULL",
            [],
            |r| r.get(0),
        )?;
        if pending != 0 {
            return Err("unsealed objects".into());
        }
        self.db.execute_batch("COMMIT;")?;
        Ok(())
    }
}

impl FinalizedConsumer for Packer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.put(object).map_err(|error| {
            eprintln!("pack consumer failed: {error}");
            ContentError::ProviderFailure {
                what: "standalone repository pack consumer",
            }
        })
    }
}
