//! Canonical reachability of explicit saved snapshots under one global object
//! budget and one canonical-byte budget.
//!
//! Each snapshot is walked from its filesystem root, breadth first, visiting
//! every object it reaches once. A visit locates the object, charges its
//! canonical length to the byte budget, reads and authenticates it and decodes
//! its direct references. Before more objects are queued, the queue must fit
//! in the visits left, so a walk never stops halfway through a reachable set
//! it already promised to read. Any budget miss leaves the totals null.
use crate::store::{Store, StorePorts};
use layerfs_bridge::control::SdkValue;
use layerfs_content::{
    filesystem::FilesystemRoot, AuthenticatedObjects, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::read::is_payload;
use layerfs_telemetry::timer::Timing;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::sync::Arc;

type Answer = crate::history::lookup::Answer;

/// Objects located per demand; a read demand is also capped by bytes.
const WINDOW: usize = 64;
/// Canonical bytes one read demand returns at most.
const READ_BYTES: u64 = 16 * 1024 * 1024;
const PHYSICAL_REASON: &str = "Canonical reachability within the supplied population does not \
     attribute encoded packs, delta bases, database overhead or reclaimable disk bytes";
const FRONTIER: &str = "object frontier exceeds remaining visit budget";
const BYTES: &str = "canonical byte read budget exhausted";

/// One resolved snapshot.
pub(crate) struct Pin {
    /// The project, as the Store names it.
    pub project: String,
    /// The branch context the caller named.
    pub branch: String,
    /// The saved record's full id.
    pub saved: String,
    /// Its filesystem root.
    pub root: ObjectId,
    /// Its saved reference (`id`, `kind`, `branch`, `root`).
    pub at: SdkValue,
}

/// The call's budgets and options.
pub(crate) struct Limits {
    pub objects: u64,
    pub bytes: u64,
    pub encoding: bool,
}

#[derive(Default)]
struct Walk {
    examined: u64,
    read: u64,
    reason: Option<&'static str>,
}

/// One visited object of one snapshot.
#[derive(Clone, Copy)]
struct Visit {
    id: ObjectId,
    bytes: u64,
    payload: bool,
}

/// The report over `pins`, in their order, read through `reader`.
pub(crate) fn report(
    store: &Arc<Store>,
    reader: &StorePorts,
    pins: &[Pin],
    limits: &Limits,
) -> Answer {
    let mut walk = Walk::default();
    let mut visits: Vec<Vec<Visit>> = Vec::with_capacity(pins.len());
    for pin in pins {
        match snapshot(store, reader, pin.root, limits, &mut walk)? {
            Some(found) => visits.push(found),
            None => break,
        }
    }
    let complete = walk.reason.is_none();
    let mut fields: Vec<(&str, SdkValue)> = vec![
        ("definitionVersion", SdkValue::Int(1)),
        ("populationId", SdkValue::text(population(pins))),
        ("population", SdkValue::text("explicit_saved_snapshots")),
        (
            "snapshots",
            SdkValue::List(
                pins.iter()
                    .map(|pin| {
                        SdkValue::map([
                            ("project", SdkValue::text(&pin.project)),
                            ("branch", SdkValue::text(&pin.branch)),
                            ("at", pin.at.clone()),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("complete", SdkValue::Bool(complete)),
        ("examinedObjects", int(walk.examined)),
        ("canonicalBytesRead", int(walk.read)),
    ];
    let union = union(pins, &visits);
    let measured = |value: u64| if complete { int(value) } else { SdkValue::Null };
    let referenced: u64 = visits.iter().flatten().map(|v| v.bytes).sum();
    let unique: u64 = union.values().map(|(bytes, _, _)| *bytes).sum();
    fields.push(("referencedCanonicalBytes", measured(referenced)));
    fields.push(("uniqueCanonicalBytes", measured(unique)));
    fields.push(("projects", projects(pins, &visits, &union, complete)));
    fields.push(("physicalBytes", SdkValue::Null));
    fields.push(("reclaimableBytes", SdkValue::Null));
    fields.push(("physicalReason", SdkValue::text(PHYSICAL_REASON)));
    if let Some(reason) = walk.reason {
        fields.push(("reason", SdkValue::text(reason)));
    }
    if limits.encoding {
        let encoding = match walk.reason {
            Some(reason) => unavailable(reason),
            None => encoding(reader, &visits, &union)?,
        };
        fields.push(("payloadEncoding", encoding));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    ))
}

/// Every object `root` reaches once, or `None` once a budget stops the walk.
fn snapshot(
    store: &Arc<Store>,
    reader: &StorePorts,
    root: ObjectId,
    limits: &Limits,
    walk: &mut Walk,
) -> Result<Option<Vec<Visit>>, layerfs_bridge::control::SdkRefusal> {
    let policy = store.policy().construction();
    let mut seen = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    let mut found = Vec::new();
    let mut scope = None;
    while !queue.is_empty() {
        if queue.len() as u64 > limits.objects - walk.examined {
            walk.reason = Some(FRONTIER);
            return Ok(None);
        }
        let window: Vec<ObjectId> = queue.drain(..queue.len().min(WINDOW)).collect();
        let described = reader.describe(&window, false).map_err(failed)?;
        let mut batch: Vec<(ObjectId, ObjectRole, u64)> = Vec::new();
        let mut batch_bytes = 0;
        for (id, object) in window.iter().zip(described) {
            walk.examined += 1;
            if walk.read + object.canonical_length > limits.bytes {
                walk.reason = Some(BYTES);
                break;
            }
            if batch_bytes + object.canonical_length > READ_BYTES {
                read(
                    reader, &batch, policy, &mut scope, &mut seen, &mut queue, &mut found,
                )?;
                batch.clear();
                batch_bytes = 0;
            }
            walk.read += object.canonical_length;
            batch_bytes += object.canonical_length;
            batch.push((*id, object.role, object.canonical_length));
        }
        read(
            reader, &batch, policy, &mut scope, &mut seen, &mut queue, &mut found,
        )?;
        if walk.reason.is_some() {
            return Ok(None);
        }
    }
    Ok(Some(found))
}

/// Reads and authenticates `batch`, queueing each child not yet seen.
fn read(
    reader: &StorePorts,
    batch: &[(ObjectId, ObjectRole, u64)],
    policy: layerfs_content::policy::ConstructionPolicy,
    scope: &mut Option<layerfs_content::filesystem::InodeScope>,
    seen: &mut HashSet<ObjectId>,
    queue: &mut VecDeque<ObjectId>,
    found: &mut Vec<Visit>,
) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    if batch.is_empty() {
        return Ok(());
    }
    let ids: Vec<ObjectId> = batch.iter().map(|(id, _, _)| *id).collect();
    let canonical = reader.read_canonical_batch(&ids).map_err(content)?;
    if canonical.len() != batch.len() {
        return Err(crate::history::lookup::failed("object demand cardinality"));
    }
    for ((id, role, bytes), canonical) in batch.iter().zip(canonical) {
        if canonical.len() as u64 != *bytes {
            return Err(crate::history::lookup::failed("canonical length"));
        }
        if *role == ObjectRole::FilesystemRoot {
            let root = FilesystemRoot::decode(&canonical).map_err(content)?;
            scope.get_or_insert(root.scope());
        }
        let Some(inode_scope) = *scope else {
            return Err(crate::history::lookup::failed(
                "snapshot root is not a filesystem root",
            ));
        };
        let (object, _) = Timing::disabled("storage_report.admit", |timing| {
            FinalizedObject::admit(
                *id,
                *role,
                canonical,
                policy,
                inode_scope,
                timing.child("admit"),
            )
        });
        let object = object.map_err(content)?;
        for child in object.references() {
            if seen.insert(*child) {
                queue.push_back(*child);
            }
        }
        found.push(Visit {
            id: *id,
            bytes: *bytes,
            payload: is_payload(*role),
        });
    }
    Ok(())
}

/// Each distinct object: its bytes, whether it is a payload and the projects
/// (by first index in `pins`) that reach it.
fn union(pins: &[Pin], visits: &[Vec<Visit>]) -> BTreeMap<ObjectId, (u64, bool, BTreeSet<usize>)> {
    let mut union: BTreeMap<ObjectId, (u64, bool, BTreeSet<usize>)> = BTreeMap::new();
    for (pin, found) in pins.iter().zip(visits) {
        let project = pins
            .iter()
            .position(|other| other.project == pin.project)
            .unwrap_or_default();
        for visit in found {
            union
                .entry(visit.id)
                .or_insert_with(|| (visit.bytes, visit.payload, BTreeSet::new()))
                .2
                .insert(project);
        }
    }
    union
}

/// One record per project, in name order.
fn projects(
    pins: &[Pin],
    visits: &[Vec<Visit>],
    union: &BTreeMap<ObjectId, (u64, bool, BTreeSet<usize>)>,
    complete: bool,
) -> SdkValue {
    let mut names: Vec<(&str, usize)> = Vec::new();
    for (index, pin) in pins.iter().enumerate() {
        if !names.iter().any(|(name, _)| *name == pin.project) {
            names.push((&pin.project, index));
        }
    }
    names.sort();
    SdkValue::List(
        names
            .into_iter()
            .map(|(name, index)| {
                let (referenced, exclusive, shared) = if complete {
                    let referenced: u64 = pins
                        .iter()
                        .zip(visits)
                        .filter(|(pin, _)| pin.project == name)
                        .flat_map(|(_, found)| found.iter().map(|v| v.bytes))
                        .sum();
                    let (mut exclusive, mut shared) = (0, 0);
                    for (bytes, _, owners) in union.values() {
                        if owners.contains(&index) {
                            if owners.len() == 1 {
                                exclusive += bytes;
                            } else {
                                shared += bytes;
                            }
                        }
                    }
                    (int(referenced), int(exclusive), int(shared))
                } else {
                    (SdkValue::Null, SdkValue::Null, SdkValue::Null)
                };
                SdkValue::map([
                    ("project", SdkValue::text(name)),
                    ("referencedCanonicalBytes", referenced),
                    ("exclusiveWithinPopulationCanonicalBytes", exclusive),
                    ("sharedWithinPopulationCanonicalBytes", shared),
                ])
            })
            .collect(),
    )
}

/// The selected payload objects' own stored records.
fn encoding(
    reader: &StorePorts,
    visits: &[Vec<Visit>],
    union: &BTreeMap<ObjectId, (u64, bool, BTreeSet<usize>)>,
) -> Answer {
    let payloads: Vec<ObjectId> = union
        .iter()
        .filter(|(_, (_, payload, _))| *payload)
        .map(|(id, _)| *id)
        .collect();
    let selected: BTreeSet<ObjectId> = payloads.iter().copied().collect();
    let (mut encoded, mut external) = (0u64, 0u64);
    for window in payloads.chunks(WINDOW) {
        for described in reader.describe(window, true).map_err(failed)? {
            let Some(record) = described.payload_record else {
                return Ok(unavailable(
                    "a selected payload record shares a compressed group; its own width is not measurable",
                ));
            };
            encoded += record.encoded_bytes;
            if record.base.is_some_and(|base| !selected.contains(&base)) {
                external += 1;
            }
        }
    }
    let referenced: u64 = visits
        .iter()
        .flatten()
        .filter(|v| v.payload)
        .map(|v| v.bytes)
        .sum();
    let unique: u64 = union
        .values()
        .filter(|(_, payload, _)| *payload)
        .map(|(bytes, _, _)| *bytes)
        .sum();
    Ok(payload_encoding(
        true,
        [int(referenced), int(unique), int(encoded), int(external)],
        None,
    ))
}

fn unavailable(reason: &str) -> SdkValue {
    payload_encoding(
        false,
        [
            SdkValue::Null,
            SdkValue::Null,
            SdkValue::Null,
            SdkValue::Null,
        ],
        Some(reason),
    )
}

fn payload_encoding(available: bool, values: [SdkValue; 4], reason: Option<&str>) -> SdkValue {
    let [referenced, unique, encoded, external] = values;
    let mut fields = vec![
        ("definitionVersion", SdkValue::Int(1)),
        (
            "population",
            SdkValue::text("selected_payload_object_records"),
        ),
        ("available", SdkValue::Bool(available)),
        ("referencedCanonicalBytes", referenced),
        ("uniqueCanonicalBytes", unique),
        ("encodedRecordBytes", encoded),
        ("externalDirectBaseCount", external),
        ("dependencyClosureIncluded", SdkValue::Bool(false)),
        ("sharedFramingIncluded", SdkValue::Bool(false)),
    ];
    if let Some(reason) = reason {
        fields.push(("reason", SdkValue::text(reason)));
    }
    SdkValue::Map(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

/// An order-independent identity of the snapshot set.
fn population(pins: &[Pin]) -> String {
    let keys: BTreeSet<(&str, &str)> = pins
        .iter()
        .map(|pin| (pin.project.as_str(), pin.saved.as_str()))
        .collect();
    let mut hash = ObjectId::stream();
    hash.update(b"layerfs/storage-report/population/v1\0");
    for (project, saved) in keys {
        hash.update(project.as_bytes());
        hash.update(b"\0");
        hash.update(saved.as_bytes());
        hash.update(b"\0");
    }
    hash.finish().to_string()
}

fn int(value: u64) -> SdkValue {
    SdkValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
}

fn failed(error: Arc<crate::store::PortError>) -> layerfs_bridge::control::SdkRefusal {
    crate::history::lookup::failed(error)
}

fn content(error: layerfs_content::ContentError) -> layerfs_bridge::control::SdkRefusal {
    crate::history::lookup::content(error)
}
