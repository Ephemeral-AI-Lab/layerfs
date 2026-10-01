use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

use super::{metrics as m, Result};
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
    pub metrics: m::Metrics,
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
            metrics: m::Metrics::default(),
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
        let started = Instant::now();
        let bytes = assemble_consuming(lane, std::mem::take(&mut state.groups))?;
        self.metrics.spans[m::ASSEMBLY].add(started, bytes.len() as u64);
        if bytes.len() > PACK_LIMIT || self.bytes + bytes.len() as u64 > 4 * 1024 * 1024 * 1024 {
            return Err("prepared pack disk admission exceeded".into());
        }
        let id = self.packs;
        let io_started = Instant::now();
        let started = Instant::now();
        fs::write(self.root.join(format!("pack-{id:08}.bin")), &bytes)?;
        self.metrics.pack_write.add(started, bytes.len() as u64);
        let started = Instant::now();
        let digest = blake3::hash(&bytes).to_hex().to_string();
        self.metrics.pack_hash.add(started, bytes.len() as u64);
        self.metrics.spans[m::PACK_IO].add(io_started, bytes.len() as u64);
        let started = Instant::now();
        self.db
            .prepare_cached("INSERT INTO packs VALUES(?1,?2,?3)")?
            .execute(params![
                i64::try_from(id)?,
                i64::try_from(bytes.len())?,
                digest
            ])?;
        self.metrics.spans[m::PACK_INSERT].add(started, 0);
        let started = Instant::now();
        let mut update = self.db.prepare_cached("UPDATE objects SET pack_id=?1,group_no=?2,record_no=?3 WHERE id=?4 AND pack_id IS NULL")?;
        self.metrics.spans[m::LOCATOR_PREPARE].add(started, 0);
        for (group, members) in state.members.drain(..).enumerate() {
            for (record, member) in members.into_iter().enumerate() {
                let started = Instant::now();
                if update.execute(params![
                    i64::try_from(id)?,
                    i64::try_from(group)?,
                    i64::try_from(record)?,
                    member.as_bytes().as_slice()
                ])? != 1
                {
                    return Err("locator publication cardinality".into());
                }
                self.metrics.spans[m::LOCATOR_UPDATE].add(started, 0);
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
        let started = Instant::now();
        let group = build_group(lane, &state.records, codec)?;
        let elapsed = started.elapsed().as_nanos();
        self.metrics.spans[m::GROUP].add_elapsed(elapsed, group.bytes.len() as u64);
        let counters = &mut self.metrics.groups[lane.index()];
        counters.span.add_elapsed(elapsed, group.bytes.len() as u64);
        counters.decoded_bytes += group.decoded_length as u64;
        if group.codec == layerfs_storage::pack::GroupCodec::Raw {
            counters.raw_groups += 1;
        } else {
            counters.compressed_groups += 1;
        }
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
        let started = Instant::now();
        let existing: Option<(u8, i64)> = self
            .db
            .prepare_cached("SELECT role,canonical_length FROM objects WHERE id=?1")?
            .query_row([id.as_bytes().as_slice()], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let elapsed = started.elapsed().as_nanos();
        self.metrics.spans[m::MEMBERSHIP].add_elapsed(elapsed, 0);
        self.metrics.membership[usize::from(existing.is_some())].add_elapsed(elapsed, 0);
        if let Some((role, length)) = existing {
            if role != object.role().code() || usize::try_from(length)? != object.canonical_len() {
                return Err("duplicate identity descriptor mismatch".into());
            }
            self.duplicates += 1;
            return Ok(());
        }
        let started = Instant::now();
        let encoded = encode_full(
            object.canonical(),
            object.role(),
            &self.capacities,
            &mut self.codec,
            &mut self.profile,
        )?;
        let elapsed = started.elapsed().as_nanos();
        self.metrics.spans[m::ENCODE].add_elapsed(elapsed, encoded.width() as u64);
        let role = &mut self.metrics.encodings[object.role().code() as usize - 1];
        role.span.add_elapsed(elapsed, encoded.width() as u64);
        role.canonical_bytes += object.canonical_len() as u64;
        role.raw_bytes += encoded.raw_length as u64;
        role.stored += u64::from(encoded.is_stored());
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
        let started = Instant::now();
        self.db
            .prepare_cached("INSERT INTO objects(id,role,canonical_length) VALUES(?1,?2,?3)")?
            .execute(params![
                id.as_bytes().as_slice(),
                object.role().code(),
                i64::try_from(object.canonical_len())?
            ])?;
        self.metrics.spans[m::OBJECT_INSERT].add(started, 0);
        let state = &mut self.lanes[lane.index()];
        state.payload += encoded.width();
        state.records.push(encoded.record);
        state.ids.push(id);
        self.objects += 1;
        // Bound private MEMORY-journal transaction work by emitted objects.
        if self.objects % 256 == 0 {
            let started = Instant::now();
            self.db.execute_batch("COMMIT; BEGIN;")?;
            self.metrics.spans[m::TRANSACTION].add(started, 0);
            self.metrics.commits_object += 1;
        }
        Ok(())
    }

    pub fn update_file(&mut self, path: &[u8], root: ObjectId, files: u64) -> Result<()> {
        self.update_file_mode(path, root, files, true)
    }
    pub fn update_file_mode(
        &mut self,
        path: &[u8],
        root: ObjectId,
        files: u64,
        cached: bool,
    ) -> Result<()> {
        let started = Instant::now();
        let sql = "UPDATE entries SET content_root=?1 WHERE path=?2";
        if cached {
            self.db
                .prepare_cached(sql)?
                .execute(params![root.as_bytes().as_slice(), path])?;
        } else {
            self.db
                .execute(sql, params![root.as_bytes().as_slice(), path])?;
        }
        self.metrics.spans[m::FILE_ROOT].add(started, 0);
        if files % 256 == 0 {
            let started = Instant::now();
            self.db.execute_batch("COMMIT; BEGIN;")?;
            self.metrics.spans[m::TRANSACTION].add(started, 0);
            self.metrics.commits_file += 1;
        }
        Ok(())
    }
    pub fn probe_ns(&self) -> u64 {
        self.profile.diag.probe_ns
    }
    pub fn finish(&mut self) -> Result<()> {
        for lane in PackLane::ALL {
            self.group(lane)?;
            self.seal(lane)?;
        }
        let started = Instant::now();
        let pending: i64 = self.db.query_row(
            "SELECT count(*) FROM objects WHERE pack_id IS NULL",
            [],
            |r| r.get(0),
        )?;
        self.metrics.spans[m::VALIDATION].add(started, 0);
        if pending != 0 {
            return Err("unsealed objects".into());
        }
        let started = Instant::now();
        self.db.execute_batch("COMMIT;")?;
        self.metrics.spans[m::TRANSACTION].add(started, 0);
        self.metrics.commits_final += 1;
        Ok(())
    }
}

impl FinalizedConsumer for Packer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let bytes = object.canonical_len() as u64;
        let started = Instant::now();
        let result = self.put(object);
        self.metrics.consumer.add(started, bytes);
        result.map_err(|error| {
            eprintln!("pack consumer failed: {error}");
            ContentError::ProviderFailure {
                what: "standalone repository pack consumer",
            }
        })
    }
}
