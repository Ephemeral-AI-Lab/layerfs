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
//!
//! **Per read, not per row (#758 phase 4 correction).** The spec's model is
//! per read: "the cache is unbounded within one read" (read-path spec
//! section 6.2, figures are means per read), and the product's pack cache is
//! operation-owned (one `PackCache` per `Reader`, `read/objects.rs`), while
//! `common::cold_read` opens a fresh reader per root, exactly as the
//! baseline bench did. So the gate figure is `model_per_read`: the sum over
//! roots of each root's own model (its distinct groups and directories,
//! counted once within that root's read). `model_union` (each (pack, group)
//! and directory counted once over all roots, the phase-0 figure) is printed
//! beside it. For one root (B3) the two are equal by construction.
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
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

const PREFIX: i32 = 16 * 1024;

#[derive(Default)]
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
    /// The model's fields, each name prefixed with `prefix`.
    pub fn line(&self, prefix: &str) -> String {
        format!(
            "{prefix}model_bytes={} {prefix}group_bytes={} {prefix}directory_bytes={} {prefix}needed_objects={} {prefix}chain_records={} {prefix}packs={} {prefix}groups={} {prefix}whole_pack_bytes={}",
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

/// Both forms of the model of one cold read of `roots` (see the header).
pub struct Models {
    /// Gate figure: the sum of each root's own model.
    pub per_read: Model,
    /// Phase-0 figure: one model over the union of all roots.
    pub union: Model,
}

impl Models {
    /// `model_per_read`, `model_union`, then both detail sets.
    pub fn line(&self) -> String {
        format!(
            "model_per_read={} model_union={} {} {}",
            self.per_read.bytes(),
            self.union.bytes(),
            self.per_read.line("per_read_"),
            self.union.line("union_")
        )
    }
    /// `ratio_product` (against the per-read model, the gate ratio) and
    /// `ratio_union` (against the union, the phase-0 ratio) for `fetched`,
    /// then the wire ratios for server tx bytes `tx` in the same two forms.
    pub fn ratios(&self, fetched: u64, tx: &str) -> String {
        let wire = |model: &Model| {
            tx.parse::<f64>().map_or("NA".to_owned(), |t| {
                format!("{:.3}", t / model.bytes() as f64)
            })
        };
        format!(
            "ratio_product={:.3} ratio_union={:.3} ratio_wire={} ratio_wire_union={}",
            fetched as f64 / self.per_read.bytes() as f64,
            fetched as f64 / self.union.bytes() as f64,
            wire(&self.per_read),
            wire(&self.union)
        )
    }
}

/// One root's needed records and groups.
struct RootModel {
    needed: BTreeSet<ObjectId>,
    records: BTreeSet<ObjectId>,
    groups: BTreeSet<(i64, usize)>,
}

/// The needed-groups model of reading every root in `roots` whole, one
/// read per root (per read) and over all roots at once (union). Locations,
/// decoded references and record-header base edges are facts of the stored
/// objects, so the bench memoizes them across roots; only the sets are per
/// root.
pub fn needed_groups(
    url: &str,
    schema: &str,
    port: Arc<dyn PackPersistence>,
    roots: &[ObjectId],
) -> Models {
    let source = BenchSource {
        client: RefCell::new(Client::connect(url, NoTls).expect("model connection")),
        schema: schema.to_owned(),
    };
    let storage = layerfs_storage::Storage::new(port).unwrap();
    let reader = storage.reader().unwrap();
    let policy = ConstructionPolicy::frozen_default();
    let mut locations: HashMap<ObjectId, ObjectLocation> = HashMap::new();
    let mut references: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut bases: HashMap<ObjectId, Option<ObjectId>> = HashMap::new();
    let mut packs = PackCache::new();
    let mut chain = ChainBases::new(&mut packs);
    let mut workspace = DecompressionWorkspace::new().unwrap();
    let mut per_root = Vec::with_capacity(roots.len());
    for root in roots {
        // 1. Reference closure, level by level.
        let mut needed: BTreeSet<ObjectId> = BTreeSet::new();
        let mut frontier: Vec<ObjectId> = vec![*root];
        while !frontier.is_empty() {
            frontier.sort();
            frontier.dedup();
            frontier.retain(|id| !needed.contains(id));
            let unknown: Vec<ObjectId> = frontier
                .iter()
                .copied()
                .filter(|id| !locations.contains_key(id))
                .collect();
            locations.extend(source.locate(&unknown));
            let mut next = Vec::new();
            for id in &frontier {
                let location = *locations.get(id).expect("needed object is stored");
                needed.insert(*id);
                if location.role.has_references() {
                    if !references.contains_key(id) {
                        let canonical = reader.read_objects(&[*id]).unwrap().remove(0);
                        let decoded = decode_references(
                            location.role,
                            &canonical,
                            policy,
                            InodeScope::from_object(*id),
                        )
                        .unwrap();
                        references.insert(*id, decoded);
                    }
                    next.extend(references[id].iter().copied());
                }
            }
            frontier = next;
        }

        // 2. Delta chains from the record headers.
        let mut records = needed.clone();
        for id in &needed {
            let mut at = *id;
            loop {
                let base = match bases.get(&at) {
                    Some(base) => *base,
                    None => {
                        let location = locations[&at];
                        let base = chain.base_of(&source, &mut workspace, &location).unwrap();
                        bases.insert(at, base);
                        base
                    }
                };
                let Some(base) = base else { break };
                if let std::collections::hash_map::Entry::Vacant(slot) = locations.entry(base) {
                    let found = source
                        .base_location(base, i64::MAX)
                        .unwrap()
                        .expect("base stored");
                    slot.insert(found);
                }
                records.insert(base);
                at = base;
            }
        }

        // 3. This read's groups.
        let groups = records
            .iter()
            .map(|id| (locations[id].pack_id, locations[id].group_number))
            .collect();
        per_root.push(RootModel {
            needed,
            records,
            groups,
        });
    }

    // Group extents and directories from each pack's own directory, read once.
    let union = RootModel {
        needed: per_root
            .iter()
            .flat_map(|r| r.needed.iter().copied())
            .collect(),
        records: per_root
            .iter()
            .flat_map(|r| r.records.iter().copied())
            .collect(),
        groups: per_root
            .iter()
            .flat_map(|r| r.groups.iter().copied())
            .collect(),
    };
    let pack_ids: Vec<i64> = union
        .groups
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
    // pack -> (directory bytes, whole length, extent of each group).
    let mut extents: HashMap<i64, (u64, u64, Vec<u64>)> = HashMap::new();
    for r in rows {
        let pack: i64 = r.get(0);
        let length = r.get::<_, i64>(1) as usize;
        let prefix: Vec<u8> = r.get(2);
        let header = parse_directory_header(&prefix, length).expect("pack header");
        let views = directory_group_views(&prefix[..header.body_offset], header)
            .or_else(|_| directory_group_views(&prefix, header))
            .expect("group directory");
        extents.insert(
            pack,
            (
                header.body_offset as u64,
                length as u64,
                views.iter().map(|v| (v.end - v.start) as u64).collect(),
            ),
        );
    }
    let model_of = |read: &RootModel| {
        let read_packs: BTreeSet<i64> = read.groups.iter().map(|g| g.0).collect();
        let mut model = Model {
            objects: read.needed.len(),
            chain_records: read.records.len() - read.needed.len(),
            packs: read_packs.len(),
            groups: read.groups.len(),
            ..Model::default()
        };
        for pack in &read_packs {
            let (directory, length, _) = &extents[pack];
            model.directory_bytes += directory;
            model.whole_pack_bytes += length;
        }
        for (pack, group) in &read.groups {
            // As phase 0: a group the directory does not list adds nothing.
            model.group_bytes += extents[pack].2.get(*group).copied().unwrap_or(0);
        }
        model
    };
    let mut per_read = Model::default();
    for read in &per_root {
        let m = model_of(read);
        per_read.objects += m.objects;
        per_read.chain_records += m.chain_records;
        per_read.packs += m.packs;
        per_read.groups += m.groups;
        per_read.group_bytes += m.group_bytes;
        per_read.directory_bytes += m.directory_bytes;
        per_read.whole_pack_bytes += m.whole_pack_bytes;
    }
    Models {
        per_read,
        union: model_of(&union),
    }
}
