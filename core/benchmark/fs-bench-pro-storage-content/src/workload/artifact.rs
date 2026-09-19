//! The prepared-artifact format for a phase-split row.
//!
//! `benchmark_rules.md` section 6 requires setup, performance and verification to
//! use separate timing and resource scopes, and owner decision D2 already asks for
//! fixture construction to leave the performance invocation. A row whose fixture
//! costs more than its measurement therefore needs the fixture to survive as an
//! **artifact**: acquired once, keyed by the row, and loaded by every later phase.
//!
//! **Why the role is in the format.** A `FinalizedObject` is bytes *plus* the role
//! they were finalized under, and offering bytes under a guessed role would store
//! the wrong envelope. The older `TreeStore::write_to_dir` round trip keeps only
//! the bytes and re-wraps them as chunks, which is why a C2 replay could not use
//! it. This format records the persisted role code beside each object, so a loaded
//! artifact is the same object set the producer built and not a re-interpretation
//! of it.
//!
//! **Why the references and the predecessors are in the format too.** A
//! `FinalizedObject` also carries its direct logical references and its bounded
//! advisory predecessors, and both are *inputs*: `Store::accept` takes the object
//! through `into_parts()`, and the save path reads the predecessor list to choose a
//! physical representation. An artifact that dropped them would hand the product a
//! different object set from the one the producer built, and a family whose
//! counters gate the save's own accounting could not tell the difference. They are
//! bounded (at most four predecessors per object; references are one entry per
//! child) so persisting them costs a fraction of a percent of the payload.
//!
//! **Why the object set is one packed file.** The first version wrote one
//! `<hex id>.bin` per object, which meant one `open`+`read`+`close` per object and
//! one inode per object: a 500 MiB base is ~32,000 objects and a 100k-file control
//! is 100,000 of them. The load is preparation, but it is preparation the row's
//! own budget pays, so the index and the payload are separate files and the payload
//! is read once, sequentially.
//!
//! **What an artifact is not.** It is never a measured result: it holds the
//! *inputs* a measured phase reads and the *expectations* an unmeasured verifier
//! compares against. A post-operation Store is never written here, and the
//! measured phase never writes its output back into an artifact.
//!
//! **Where the manifest and the seal come from.** `test_setup_and_cache_discipline.md`
//! section 3 fixes the build order as *build -> validate (sha256 per file) -> seal
//! (chmod removes 0o222) -> write `manifest.json`*. The child writes the payload and
//! its completion marker; the **acquirer** — `runner.py prepare` — computes the
//! per-file digests, writes `manifest.json`, records the compatibility key and
//! applies the seal. Hashing in the acquirer is deliberate: the harness's own
//! SHA-256 is a scalar implementation at a fraction of `hashlib`'s rate, and the
//! seal is the acquirer's act rather than the producer's.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use layerfs_content::{
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};

use super::oracle::Expectation;
use super::providers::TreeStore;

/// First line of `objects/index.tsv`.
pub const OBJECTS_FORMAT: &str = "fs-bench-artifact-objects-v2";
/// First line of `members.tsv`.
pub const MEMBERS_FORMAT: &str = "fs-bench-artifact-members-v2";
/// First line of `values.tsv`.
pub const VALUES_FORMAT: &str = "fs-bench-artifact-values-v1";

/// Directory the packed object set lives in, relative to the artifact root.
pub const OBJECTS_DIRECTORY: &str = "objects";
/// The one packed payload file.
pub const PACK_FILE: &str = "pack.bin";
/// The index that describes it.
pub const INDEX_FILE: &str = "index.tsv";
/// The base Store a C2 row opens a per-sample copy of, when it has one.
pub const STORE_FILE: &str = "store.sqlite";

/// Read buffer for the packed payload. The payload is read once, sequentially, so
/// the buffer is sized to keep the per-object `read_exact` calls off the syscall
/// path without holding the whole pack in memory twice.
const PACK_BUFFER_BYTES: usize = 4 << 20;

/// One member of a supplied-object set: its ordered object list, its root and the
/// expectation its logical bytes must satisfy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Member {
    /// Identities the member offers, in the order the producer emitted them.
    pub ids: Vec<ObjectId>,
    /// The member's logical root.
    pub root: ObjectId,
    /// Expected logical length and digest, derived from the fixture recipe.
    ///
    /// `None` for a member whose expectation the row does not gate — a single-file
    /// base whose *result* is what the oracle checks, for instance. Hashing a
    /// 500 MiB base to record an expectation nobody reads would be preparation the
    /// row pays for nothing.
    pub expectation: Option<Expectation>,
}

/// Named values a later phase reads back: counts, lengths, offsets and the
/// expectation its oracle compares against.
///
/// The expectation is **verification-only**. It is derived from the fixture recipe
/// at acquisition time, it is never offered to the product, and it is never used to
/// prime a range: the measured phase reads the base through the provider exactly as
/// it would if the artifact held no expectation at all.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Values {
    scalars: BTreeMap<String, u64>,
    identities: BTreeMap<String, ObjectId>,
    expectations: BTreeMap<String, Expectation>,
}

impl Values {
    /// Empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares one scalar.
    pub fn set_scalar(&mut self, name: &str, value: u64) {
        self.scalars.insert(name.to_string(), value);
    }

    /// Declares one identity.
    pub fn set_identity(&mut self, name: &str, value: ObjectId) {
        self.identities.insert(name.to_string(), value);
    }

    /// Declares one expectation.
    pub fn set_expectation(&mut self, name: &str, value: Expectation) {
        self.expectations.insert(name.to_string(), value);
    }

    /// One declared scalar, or `None`.
    pub fn scalar(&self, name: &str) -> Option<u64> {
        self.scalars.get(name).copied()
    }

    /// One declared identity, or `None`.
    pub fn identity(&self, name: &str) -> Option<ObjectId> {
        self.identities.get(name).copied()
    }

    /// One declared expectation, or `None`.
    pub fn expectation(&self, name: &str) -> Option<Expectation> {
        self.expectations.get(name).copied()
    }

    /// Every scalar, in name order.
    pub fn scalars(&self) -> impl Iterator<Item = (&str, u64)> + '_ {
        self.scalars.iter().map(|(name, value)| (name.as_str(), *value))
    }

    /// Whether nothing is declared.
    pub fn is_empty(&self) -> bool {
        self.scalars.is_empty() && self.identities.is_empty() && self.expectations.is_empty()
    }
}

/// A prepared artifact: the object set a measured phase reads, the member list it
/// offers, and the named values its oracle needs.
pub struct Artifact {
    /// Every distinct object any member or the base refers to.
    pub objects: TreeStore,
    /// The members, in declaration order.
    pub members: Vec<Member>,
    /// Named scalars, identities and expectations.
    pub values: Values,
}

impl Artifact {
    /// An artifact over one object set, with no members and no values.
    pub fn new(objects: TreeStore) -> Self {
        Self {
            objects,
            members: Vec::new(),
            values: Values::new(),
        }
    }

    /// The single prepared base this row reads.
    ///
    /// A single-file family declares exactly one member: its base. A row that
    /// declares anything else is a driver defect, and is refused rather than
    /// silently read as "the first member".
    pub fn base(&self) -> Result<&Member, String> {
        match self.members.as_slice() {
            [only] => Ok(only),
            other => Err(format!(
                "the artifact declares {} members; this row reads exactly one base",
                other.len()
            )),
        }
    }

    /// Writes the object set, the member list and the values into `directory`.
    ///
    /// Returns the number of objects written. The completion marker is a separate
    /// call ([`Artifact::seal`]) so a driver that also writes a base Store can do so
    /// before the artifact declares itself finished.
    pub fn write(&self, directory: &Path) -> Result<usize, String> {
        let objects_dir = directory.join(OBJECTS_DIRECTORY);
        std::fs::create_dir_all(&objects_dir).map_err(|error| error.to_string())?;
        let pack_path = objects_dir.join(PACK_FILE);
        let index_path = objects_dir.join(INDEX_FILE);
        let mut pack = std::io::BufWriter::with_capacity(
            PACK_BUFFER_BYTES,
            std::fs::File::create(&pack_path).map_err(|error| error.to_string())?,
        );
        let mut index = String::from(OBJECTS_FORMAT);
        index.push('\n');
        let mut offset = 0_u64;
        let mut written = 0_usize;
        for id in self.objects.insertion_order() {
            let Some(object) = self.objects.object(*id) else {
                continue;
            };
            let canonical = object.canonical();
            pack.write_all(canonical).map_err(|error| error.to_string())?;
            let length = canonical.len() as u64;
            index.push_str(&format!(
                "object\t{id}\t{}\t{offset}\t{length}\t{}\t{}\n",
                object.role().code(),
                render_ids(object.references()),
                render_predecessors(object.predecessors()),
            ));
            offset += length;
            written += 1;
        }
        pack.flush().map_err(|error| error.to_string())?;
        drop(pack);
        std::fs::write(&index_path, index).map_err(|error| error.to_string())?;

        let mut members = String::from(MEMBERS_FORMAT);
        members.push('\n');
        for (position, member) in self.members.iter().enumerate() {
            let ids: Vec<String> = member.ids.iter().map(|id| id.to_string()).collect();
            let (len, digest) = match &member.expectation {
                Some(expectation) => (
                    expectation.logical_len.to_string(),
                    super::digest::hex(&expectation.sha256),
                ),
                None => ("-".to_string(), "-".to_string()),
            };
            members.push_str(&format!(
                "member\t{position}\t{}\t{len}\t{digest}\t{}\n",
                member.root,
                ids.join(","),
            ));
        }
        std::fs::write(directory.join("members.tsv"), members).map_err(|error| error.to_string())?;

        let mut values = String::from(VALUES_FORMAT);
        values.push('\n');
        for (name, value) in self.values.scalars() {
            values.push_str(&format!("scalar\t{name}\t{value}\n"));
        }
        for (name, value) in &self.values.identities {
            values.push_str(&format!("identity\t{name}\t{value}\n"));
        }
        for (name, expectation) in &self.values.expectations {
            values.push_str(&format!(
                "expectation\t{name}\t{}\t{}\n",
                expectation.logical_len,
                super::digest::hex(&expectation.sha256),
            ));
        }
        std::fs::write(directory.join("values.tsv"), values).map_err(|error| error.to_string())?;
        Ok(written)
    }

    /// Reads an artifact written by [`Artifact::write`].
    ///
    /// Fails closed: a missing file, an unreadable role code, an object whose bytes
    /// do not re-identify to the name it is stored under, or a member that names an
    /// object the object set does not hold are all errors rather than a smaller
    /// artifact.
    ///
    /// **The identity check is free.** `FinalizedObject::new` computes the object's
    /// identity as part of wrapping it, so comparing that with the index's name is
    /// one comparison rather than a second hash. The first version called
    /// `ObjectId::for_bytes` again over every object, which doubled the hashing cost
    /// of a load — and a load is preparation the row's own budget pays.
    pub fn read(directory: &Path) -> Result<Self, String> {
        let objects_dir = directory.join(OBJECTS_DIRECTORY);
        let index_path = objects_dir.join(INDEX_FILE);
        let file = std::fs::File::open(&index_path)
            .map_err(|error| format!("{}: {error}", index_path.display()))?;
        let mut lines = BufReader::with_capacity(1 << 20, file).lines();
        match lines.next() {
            Some(Ok(first)) if first == OBJECTS_FORMAT => {}
            other => return Err(format!("not a {OBJECTS_FORMAT} artifact: {other:?}")),
        }
        let pack_path = objects_dir.join(PACK_FILE);
        let mut pack = BufReader::with_capacity(
            PACK_BUFFER_BYTES,
            std::fs::File::open(&pack_path)
                .map_err(|error| format!("{}: {error}", pack_path.display()))?,
        );
        let mut store = TreeStore::new();
        for line in lines {
            let line = line.map_err(|error| format!("{}: {error}", index_path.display()))?;
            if line.is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            let ["object", id, role, offset, length, references, predecessors] = fields.as_slice()
            else {
                return Err(format!("objects index line: {line:?}"));
            };
            let id: ObjectId = id.parse().map_err(|_| format!("object id: {id}"))?;
            let code: u8 = role.parse().map_err(|_| format!("object role: {role}"))?;
            let role = ObjectRole::from_code(code).map_err(|error| format!("{error:?}"))?;
            let offset: u64 = offset.parse().map_err(|_| format!("object offset: {offset}"))?;
            let length: usize = length.parse().map_err(|_| format!("object length: {length}"))?;
            let mut canonical = vec![0_u8; length];
            pack.read_exact(&mut canonical)
                .map_err(|error| format!("{} at {offset}: {error}", pack_path.display()))?;
            let object = FinalizedObject::new(role, canonical)
                .map_err(|error| format!("{id}: {error:?}"))?
                .with_references(parse_ids(references)?)
                .with_predecessors(parse_predecessors(predecessors)?);
            if object.id() != id {
                return Err(format!("{id} does not re-identify from its stored bytes"));
            }
            store.insert_object(object);
        }

        let members_path = directory.join("members.tsv");
        let text = std::fs::read_to_string(&members_path)
            .map_err(|error| format!("{}: {error}", members_path.display()))?;
        let mut lines = text.lines();
        match lines.next() {
            Some(first) if first == MEMBERS_FORMAT => {}
            other => return Err(format!("not a {MEMBERS_FORMAT} artifact: {other:?}")),
        }
        let mut members: BTreeMap<usize, Member> = BTreeMap::new();
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            let ["member", index, root, len, digest, ids] = fields.as_slice() else {
                return Err(format!("members.tsv line: {line:?}"));
            };
            let index: usize = index.parse().map_err(|_| format!("member index: {index}"))?;
            let root: ObjectId = root.parse().map_err(|_| format!("member root: {root}"))?;
            let expectation = if *len == "-" && *digest == "-" {
                None
            } else {
                let logical_len: u64 = len.parse().map_err(|_| format!("member length: {len}"))?;
                let sha256 = super::digest::unhex(digest).ok_or(format!("member digest: {digest}"))?;
                Some(Expectation { logical_len, sha256 })
            };
            let mut list = Vec::new();
            for id in ids.split(',').filter(|id| !id.is_empty()) {
                let id: ObjectId = id.parse().map_err(|_| format!("member id: {id}"))?;
                if store.object(id).is_none() {
                    return Err(format!("member {index} names {id}, which the object set lacks"));
                }
                list.push(id);
            }
            members.insert(
                index,
                Member {
                    ids: list,
                    root,
                    expectation,
                },
            );
        }
        // The indices must be exactly `0..members.len()`: a gap would silently
        // verify fewer members than the producer declared.
        let declared = members.len();
        let members: Vec<Member> = members.into_values().collect();
        if members.len() != declared {
            return Err("member indices are not contiguous".to_string());
        }

        let values_path = directory.join("values.tsv");
        let text = std::fs::read_to_string(&values_path)
            .map_err(|error| format!("{}: {error}", values_path.display()))?;
        let mut lines = text.lines();
        match lines.next() {
            Some(first) if first == VALUES_FORMAT => {}
            other => return Err(format!("not a {VALUES_FORMAT} artifact: {other:?}")),
        }
        let mut values = Values::new();
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["scalar", name, value] => {
                    let value: u64 = value.parse().map_err(|_| format!("scalar {name}: {value}"))?;
                    values.set_scalar(name, value);
                }
                ["identity", name, value] => {
                    let value: ObjectId = value.parse().map_err(|_| format!("identity {name}: {value}"))?;
                    values.set_identity(name, value);
                }
                ["expectation", name, len, digest] => {
                    let logical_len: u64 = len.parse().map_err(|_| format!("expectation {name}: {len}"))?;
                    let sha256 =
                        super::digest::unhex(digest).ok_or(format!("expectation {name}: {digest}"))?;
                    values.set_expectation(name, Expectation { logical_len, sha256 });
                }
                other => return Err(format!("values.tsv line: {other:?}")),
            }
        }

        Ok(Self {
            objects: store,
            members,
            values,
        })
    }

    /// The base Store a C2 row opens a per-sample copy of.
    pub fn store_path(directory: &Path) -> PathBuf {
        directory.join(STORE_FILE)
    }

    /// The artifact directory of one case under a preparation root.
    pub fn directory(root: &Path, case_id: &str) -> PathBuf {
        root.join(case_id)
    }

    /// Writes the completion marker, so a later phase can tell a finished artifact
    /// from an interrupted one.
    pub fn seal(&self, directory: &Path, case_id: &str) -> Result<(), String> {
        let path = directory.join("sealed.tsv");
        let mut file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
        writeln!(file, "case\t{case_id}").map_err(|error| error.to_string())?;
        writeln!(file, "objects\t{}", self.objects.len()).map_err(|error| error.to_string())?;
        writeln!(file, "members\t{}", self.members.len()).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())
    }
}

/// Renders an identity list as a comma-separated field, or `-` when empty.
fn render_ids(ids: &[ObjectId]) -> String {
    if ids.is_empty() {
        return "-".to_string();
    }
    ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
}

/// Renders an advisory predecessor list as `<id>:<provenance code>` pairs.
fn render_predecessors(predecessors: &AdvisoryPredecessors) -> String {
    if predecessors.is_empty() {
        return "-".to_string();
    }
    predecessors
        .entries()
        .iter()
        .map(|entry| format!("{}:{}", entry.id(), provenance_code(entry.provenance())))
        .collect::<Vec<_>>()
        .join(",")
}

/// The persisted code of one provenance. The product's enum has no code of its
/// own — this is the harness's own serialization, so the mapping is declared here
/// and nowhere else.
fn provenance_code(provenance: PredecessorProvenance) -> u8 {
    match provenance {
        PredecessorProvenance::OriginalBase => 0,
        PredecessorProvenance::UnchangedPrefix => 1,
        PredecessorProvenance::ReusedRange => 2,
    }
}

/// The inverse of [`provenance_code`].
fn provenance_from_code(code: u8) -> Result<PredecessorProvenance, String> {
    match code {
        0 => Ok(PredecessorProvenance::OriginalBase),
        1 => Ok(PredecessorProvenance::UnchangedPrefix),
        2 => Ok(PredecessorProvenance::ReusedRange),
        other => Err(format!("predecessor provenance code: {other}")),
    }
}

/// Parses a comma-separated identity field.
fn parse_ids(field: &str) -> Result<Vec<ObjectId>, String> {
    if field == "-" || field.is_empty() {
        return Ok(Vec::new());
    }
    field
        .split(',')
        .map(|id| id.parse::<ObjectId>().map_err(|_| format!("reference id: {id}")))
        .collect()
}

/// Parses a `<id>:<code>` predecessor field.
fn parse_predecessors(field: &str) -> Result<AdvisoryPredecessors, String> {
    let mut list = AdvisoryPredecessors::new();
    if field == "-" || field.is_empty() {
        return Ok(list);
    }
    for entry in field.split(',') {
        let (id, code) = entry
            .split_once(':')
            .ok_or_else(|| format!("predecessor entry: {entry}"))?;
        let id: ObjectId = id.parse().map_err(|_| format!("predecessor id: {id}"))?;
        let code: u8 = code.parse().map_err(|_| format!("predecessor code: {code}"))?;
        list.push(id, provenance_from_code(code)?)
            .map_err(|error| format!("{error:?}"))?;
    }
    Ok(list)
}
