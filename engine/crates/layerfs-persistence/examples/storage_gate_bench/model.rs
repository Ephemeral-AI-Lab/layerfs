//! The needed-groups model (A11: fetched bytes <= 1.1x the model).
//!
//! The model is the byte floor of a read that fetches exactly the encoded
//! groups it needs, computed WITHOUT the product's read plan:
//!
//! 1. **Needed objects.** The read's reference closure: from each root, the
//!    objects whose role has references (FileState, extent nodes) are read
//!    and their references decoded (`decode_references`); leaves (chunks,
//!    WholeFile payloads) are not opened. Locations come from the bench's own
//!    SQL over `object_location` (by full id, through the prefix index).
//! 2. **Delta chains.** For every needed object, its stored record header is
//!    decoded with the decoder's own edge reader (`ChainBases::base_of`) over
//!    whole pack bodies fetched by the bench's own SQL, and each base is
//!    added in turn until a FULL record. This is the chain the record format
//!    states, not the chain the product locates or a hint names.
//! 3. **Group extents.** For every distinct pack holding a needed record, the
//!    bench reads the pack's control header and group directory (body
//!    prefix) and takes each needed group's encoded extent `end - start`.
//!
//! `model_bytes = sum(extent of each distinct needed (pack, group)) +
//! sum(directory bytes = body_offset of each distinct needed pack)`.
//! The directory is counted once per pack because a group read needs it
//! (from `pack.directory` in 2.0, from the body prefix today). The model
//! takes no locate rows, protocol framing or ids into account, so the
//! wire-byte ratio also carries those (small) costs.
//!
//! It runs after the measured read, on its own connection and its own
//! `Storage`, so it never warms or credits the measured phase.
use layerfs_content::ObjectRole;
use layerfs_content::{object::decode_references, ConstructionPolicy, InodeScope, ObjectId};
use layerfs_storage::{
    encoding::{delta::read::ChainBases, DecompressionWorkspace, PackCache},
    location::{ObjectLocation, SignatureRow},
    pack::layout::{directory_group_views, parse_directory_header},
    port::PackPersistence,
    source::Source,
    StorageError, StorageResult,
};
use postgres::{Client, NoTls};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

const PREFIX: i32 = 16 * 1024;

pub struct Model {
    pub objects: usize,
    pub chain_records: usize,
    pub packs: usize,
    pub groups: usize,
    pub group_bytes: u64,
    pub directory_bytes: u64,
    /// Whole bodies of the same packs: what a whole-pack reader fetches.
    pub whole_pack_bytes: u64,
}

impl Model {
    pub fn bytes(&self) -> u64 {
        self.group_bytes + self.directory_bytes
    }
    pub fn line(&self) -> String {
        format!(
            "model_bytes={} group_bytes={} directory_bytes={} needed_objects={} chain_records={} packs={} groups={} whole_pack_bytes={}",
            self.bytes(),
            self.group_bytes,
            self.directory_bytes,
            self.objects,
            self.chain_records,
            self.packs,
            self.groups,
            self.whole_pack_bytes
        )
    }
}

/// The bench's own catalog and body reader over one schema.
struct BenchSource {
    client: RefCell<Client>,
    schema: String,
}

impl BenchSource {
    fn locate(&self, ids: &[ObjectId]) -> HashMap<ObjectId, ObjectLocation> {
        let mut out = HashMap::new();
        for batch in ids.chunks(4096) {
            let raw: Vec<Vec<u8>> = batch.iter().map(|i| i.as_bytes().to_vec()).collect();
            let rows = self
                .client
                .borrow_mut()
                .query(
                    &format!(
                        "/*gate-monitor*/ SELECT x.object_id,x.role::int4,x.canonical_length,x.pack_id,x.group_number::int4,x.record_number::int4 \
                         FROM unnest($1::bytea[]) k(i) CROSS JOIN LATERAL (SELECT * FROM {s}.object_location x \
                         WHERE ('x'||encode(substring(x.object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint = \
                         ('x'||encode(substring(k.i FROM 1 FOR 8),'hex'))::bit(64)::bigint AND x.object_id=k.i OFFSET 0) x",
                        s = self.schema
                    ),
                    &[&raw],
                )
                .expect("model locate");
            for r in rows {
                let id = ObjectId::from_bytes(&r.get::<_, Vec<u8>>(0)).unwrap();
                out.insert(
                    id,
                    ObjectLocation {
                        object_id: id,
                        role: ObjectRole::from_code(r.get::<_, i32>(1) as u8).unwrap(),
                        canonical_length: r.get::<_, i32>(2) as usize,
                        pack_id: r.get(3),
                        group_number: r.get::<_, i32>(4) as usize,
                        record_number: r.get::<_, i32>(5) as usize,
                        base: None,
                    },
                );
            }
        }
        out
    }
}

impl Source for BenchSource {
    fn location(&self, id: ObjectId, _ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        Ok(self.locate(&[id]).remove(&id))
    }
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>> {
        let row = self
            .client
            .borrow_mut()
            .query_one(
                &format!(
                    "/*gate-monitor*/ SELECT body FROM {}.pack WHERE pack_id=$1",
                    self.schema
                ),
                &[&pack_id],
            )
            .map_err(|_| StorageError::Integrity("model pack body"))?;
        Ok(row.get(0))
    }
    fn inode_values(&self, _ordinals: &[u32]) -> StorageResult<Vec<[u8; 73]>> {
        Err(StorageError::Integrity(
            "model source reads no inode values",
        ))
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        Err(StorageError::Integrity("model source reads no signatures"))
    }
    fn write_signatures(&self, _rows: &[SignatureRow]) -> StorageResult<usize> {
        Err(StorageError::Integrity("model source writes nothing"))
    }
}

/// The needed-groups model of reading every root in `roots` whole.
pub fn needed_groups(
    url: &str,
    schema: &str,
    port: Arc<dyn PackPersistence>,
    roots: &[ObjectId],
) -> Model {
    let source = BenchSource {
        client: RefCell::new(Client::connect(url, NoTls).expect("model connection")),
        schema: schema.to_owned(),
    };
    let storage = layerfs_storage::Storage::new(port).unwrap();
    let reader = storage.reader().unwrap();
    let policy = ConstructionPolicy::frozen_default();

    // 1. Reference closure, level by level.
    let mut needed: BTreeMap<ObjectId, ObjectLocation> = BTreeMap::new();
    let mut frontier: Vec<ObjectId> = roots.to_vec();
    while !frontier.is_empty() {
        frontier.sort();
        frontier.dedup();
        frontier.retain(|id| !needed.contains_key(id));
        let located = source.locate(&frontier);
        let mut next = Vec::new();
        for id in &frontier {
            let location = *located.get(id).expect("needed object is stored");
            needed.insert(*id, location);
            if location.role.has_references() {
                let canonical = reader.read_objects(&[*id]).unwrap().remove(0);
                next.extend(
                    decode_references(
                        location.role,
                        &canonical,
                        policy,
                        InodeScope::from_object(*id),
                    )
                    .unwrap(),
                );
            }
        }
        frontier = next;
    }
    let objects = needed.len();

    // 2. Delta chains from the record headers.
    let mut packs = PackCache::new();
    let mut chain = ChainBases::new(&mut packs);
    let mut workspace = DecompressionWorkspace::new().unwrap();
    let mut records: BTreeMap<ObjectId, ObjectLocation> = needed.clone();
    let mut chain_records = 0usize;
    for location in needed.values() {
        let mut at = *location;
        while let Some(base) = chain.base_of(&source, &mut workspace, &at).unwrap() {
            if let Some(known) = records.get(&base) {
                at = *known;
                continue;
            }
            let found = source
                .base_location(base, i64::MAX)
                .unwrap()
                .expect("base stored");
            records.insert(base, found);
            chain_records += 1;
            at = found;
        }
    }

    // 3. Group extents from each pack's own directory.
    let mut groups: BTreeSet<(i64, usize)> = BTreeSet::new();
    for location in records.values() {
        groups.insert((location.pack_id, location.group_number));
    }
    let pack_ids: Vec<i64> = groups
        .iter()
        .map(|g| g.0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows = source
        .client
        .borrow_mut()
        .query(
            &format!(
                "/*gate-monitor*/ SELECT pack_id,length,substring(body FROM 1 FOR $2) FROM {}.pack WHERE pack_id=ANY($1)",
                schema
            ),
            &[&pack_ids, &PREFIX],
        )
        .expect("model directories");
    let mut model = Model {
        objects,
        chain_records,
        packs: pack_ids.len(),
        groups: groups.len(),
        group_bytes: 0,
        directory_bytes: 0,
        whole_pack_bytes: 0,
    };
    for r in rows {
        let pack: i64 = r.get(0);
        let length = r.get::<_, i64>(1) as usize;
        let prefix: Vec<u8> = r.get(2);
        let header = parse_directory_header(&prefix, length).expect("pack header");
        let views = directory_group_views(&prefix[..header.body_offset], header)
            .or_else(|_| directory_group_views(&prefix, header))
            .expect("group directory");
        model.directory_bytes += header.body_offset as u64;
        model.whole_pack_bytes += length as u64;
        for (number, view) in views.iter().enumerate() {
            if groups.contains(&(pack, number)) {
                model.group_bytes += (view.end - view.start) as u64;
            }
        }
    }
    model
}
