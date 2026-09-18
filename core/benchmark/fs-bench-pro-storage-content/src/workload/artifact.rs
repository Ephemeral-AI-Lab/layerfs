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
//! **What an artifact is not.** It is never a measured result: it holds the
//! *inputs* a measured phase reads and the *expectations* an unmeasured verifier
//! compares against. A post-operation Store is never written here, and the
//! measured phase never writes its output back into an artifact.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};

use super::oracle::Expectation;
use super::providers::TreeStore;

/// First line of `objects.tsv`.
pub const OBJECTS_FORMAT: &str = "fs-bench-artifact-objects-v1";
/// First line of `members.tsv`.
pub const MEMBERS_FORMAT: &str = "fs-bench-artifact-members-v1";

/// One member of a supplied-object set: its ordered object list, its root and the
/// expectation its logical bytes must satisfy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Member {
    /// Identities the member offers, in the order the producer emitted them.
    pub ids: Vec<ObjectId>,
    /// The member's logical root.
    pub root: ObjectId,
    /// Expected logical length and digest, derived from the fixture recipe.
    pub expectation: Expectation,
}

/// A prepared artifact: the object set a measured phase reads, and the member
/// list it offers.
pub struct Artifact {
    /// Every distinct object any member or the base refers to.
    pub objects: TreeStore,
    /// The members, in declaration order.
    pub members: Vec<Member>,
}

impl Artifact {
    /// Writes the artifact into `directory`, returning the object count.
    pub fn write(&self, directory: &Path) -> Result<usize, String> {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let mut objects = String::from(OBJECTS_FORMAT);
        objects.push('\n');
        let mut written = 0_usize;
        for id in self.objects.insertion_order() {
            let Some(object) = self.objects.object(*id) else {
                continue;
            };
            let path = directory.join(format!("{id}.bin"));
            std::fs::write(&path, object.canonical()).map_err(|error| error.to_string())?;
            objects.push_str(&format!("object\t{id}\t{}\n", object.role().code()));
            written += 1;
        }
        std::fs::write(directory.join("objects.tsv"), objects).map_err(|error| error.to_string())?;

        let mut members = String::from(MEMBERS_FORMAT);
        members.push('\n');
        for (index, member) in self.members.iter().enumerate() {
            let ids: Vec<String> = member.ids.iter().map(|id| id.to_string()).collect();
            members.push_str(&format!(
                "member\t{index}\t{}\t{}\t{}\t{}\n",
                member.root,
                member.expectation.logical_len,
                super::digest::hex(&member.expectation.sha256),
                ids.join(","),
            ));
        }
        std::fs::write(directory.join("members.tsv"), members).map_err(|error| error.to_string())?;
        Ok(written)
    }

    /// Reads an artifact written by [`Artifact::write`].
    ///
    /// Fails closed: a missing file, an unreadable role code, an object whose
    /// bytes do not re-identify to the name it is stored under, or a member that
    /// names an object the object set does not hold are all errors rather than a
    /// smaller artifact.
    pub fn read(directory: &Path) -> Result<Self, String> {
        let objects_path = directory.join("objects.tsv");
        let text = std::fs::read_to_string(&objects_path)
            .map_err(|error| format!("{}: {error}", objects_path.display()))?;
        let mut lines = text.lines();
        match lines.next() {
            Some(first) if first == OBJECTS_FORMAT => {}
            other => return Err(format!("not a {OBJECTS_FORMAT} artifact: {other:?}")),
        }
        let mut store = TreeStore::new();
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            let ["object", id, role] = fields.as_slice() else {
                return Err(format!("objects.tsv line: {line:?}"));
            };
            let id: ObjectId = id.parse().map_err(|_| format!("object id: {id}"))?;
            let code: u8 = role.parse().map_err(|_| format!("object role: {role}"))?;
            let role = ObjectRole::from_code(code).map_err(|error| format!("{error:?}"))?;
            let path = directory.join(format!("{id}.bin"));
            let canonical =
                std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            if ObjectId::for_bytes(&canonical) != id {
                return Err(format!("{id} does not re-identify from its stored bytes"));
            }
            let object = FinalizedObject::new(role, canonical)
                .map_err(|error| format!("{id}: {error:?}"))?;
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
            let logical_len: u64 = len.parse().map_err(|_| format!("member length: {len}"))?;
            let sha256 = super::digest::unhex(digest).ok_or(format!("member digest: {digest}"))?;
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
                    expectation: Expectation {
                        logical_len,
                        sha256,
                    },
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
        Ok(Self {
            objects: store,
            members,
        })
    }

    /// Writes one SQLite Store's worth of objects to `path` by copying an
    /// already-prepared Store file, so a later phase does not rebuild it.
    pub fn copy_store(source: &Path, destination: &Path) -> Result<(), String> {
        std::fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| format!("{} -> {}: {error}", source.display(), destination.display()))
    }

    /// The artifact directory of one case under a preparation root.
    pub fn directory(root: &Path, case_id: &str) -> PathBuf {
        root.join(case_id)
    }

    /// Writes a small completion marker so a later phase can tell a finished
    /// artifact from an interrupted one.
    pub fn seal(&self, directory: &Path, case_id: &str) -> Result<(), String> {
        let path = directory.join("sealed.tsv");
        let mut file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
        writeln!(file, "case\t{case_id}").map_err(|error| error.to_string())?;
        writeln!(file, "objects\t{}", self.objects.len()).map_err(|error| error.to_string())?;
        writeln!(file, "members\t{}", self.members.len()).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())
    }
}
