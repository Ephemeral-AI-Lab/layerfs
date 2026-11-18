//! One project's data (call surface 2.0, `project doctor --full`): every
//! object reachable from main's head and from the head of each of the
//! project's live Branches, what a mount of any of them can reach, read once
//! and checked in one pinned read view. Nothing is written to the Store.
//!
//! The checks are the whole-Store scan's ([`super::scan`]), on a smaller
//! set: each tree's namespace is qualified ([`qualify_root`]) and each
//! object is located, reconstructed by the shared [`Reader`], admitted and
//! its stored children's context checked, as the scan checks every stored
//! row. The set is the closure of the heads' roots over each object's
//! references instead of every row of the object table.
//!
//! Memory does not follow the project's size: the heads, the visited set
//! and the queue of objects still to read live in the private SQLite
//! temporary database of [`Scratch`], outside the Store, removed when the
//! check's connection closes; resident state is one bounded batch of
//! identities and the Reader's caches. An object two of the project's trees
//! share is read once; a second project's check has its own visited set.
use super::{
    emit, reader::Reader, scratch::Scratch, transaction::View, Observation, Options, Outcome,
};
use crate::backend::records::{BackendError, Param};
use layerfs_content::{
    filesystem::{
        qualify::{qualify_root, QualificationWork, RootContext},
        FilesystemRoot, FilesystemRootId, InodeScope,
    },
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::LayerStackId;
use layerfs_storage::{StorageError, StorageResult};
use layerfs_telemetry::timer::Timing;
use rusqlite::{params, OptionalExtension};

/// The check id every observation of this audit carries.
pub const CHECK: &str = "project.data";

/// Objects read between two cancellation points and scratch writes.
const BATCH: usize = 64;

/// What one project's audit covered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectCoverage {
    /// Distinct trees read: main's and each Branch's head, one per root.
    pub trees: u64,
    /// Heads resolved: main and each live Branch.
    pub heads: u64,
    /// Distinct objects read and checked.
    pub objects: u64,
    /// Concrete problems found.
    pub failed: u64,
    /// Whether some object or tree could not be checked.
    pub incomplete: bool,
}

/// Progress of one audit: `(done, total)` trees.
pub(crate) type Progress<'a> = dyn FnMut(u64, u64) + 'a;

/// Audits project `stack` in `view`, pinned to packs at or below `ceiling`.
/// Each problem is one observation with id [`CHECK`]: `Fail` for a
/// concrete inconsistency, `Unknown` for something that could not be read.
pub(crate) fn run(
    view: &dyn View,
    ceiling: i64,
    stack: LayerStackId,
    options: Options,
    observer: &mut dyn FnMut(Observation),
    progress: &mut Progress<'_>,
) -> Result<ProjectCoverage, BackendError> {
    let mut scratch = Scratch::new().map_err(|_| BackendError::Capacity)?;
    scratch.observe(options, |scratch| {
        let mut audit = Audit {
            view,
            scratch,
            options,
            observer,
            coverage: ProjectCoverage::default(),
        };
        audit.run(ceiling, stack, progress)?;
        Ok(audit.coverage)
    })
}

struct Audit<'a, 'o> {
    view: &'a dyn View,
    scratch: &'a mut Scratch,
    options: Options<'a>,
    observer: &'o mut dyn FnMut(Observation),
    coverage: ProjectCoverage,
}

/// The project's stack row: its scope and profile.
struct Stack {
    scope: Vec<u8>,
    profile: Vec<u8>,
}

impl Audit<'_, '_> {
    fn run(
        &mut self,
        ceiling: i64,
        stack: LayerStackId,
        progress: &mut Progress<'_>,
    ) -> Result<(), BackendError> {
        self.scratch.0.execute_batch("CREATE TABLE heads(root BLOB NOT NULL,rank INTEGER NOT NULL,label TEXT NOT NULL,PRIMARY KEY(root,rank,label)) WITHOUT ROWID; CREATE TABLE visited(id BLOB PRIMARY KEY,parent BLOB,done INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID; CREATE INDEX visited_pending ON visited(done,id);").map_err(sql)?;
        let Some(found) = self.heads(stack)? else {
            return Ok(());
        };
        let reader =
            Reader::new(self.view, ceiling, self.options).map_err(|_| BackendError::Integrity)?;
        let total: i64 = (self.scratch.0)
            .query_row("SELECT count(DISTINCT root) FROM heads", [], |r| r.get(0))
            .map_err(sql)?;
        let total = u64::try_from(total).unwrap_or(0);
        progress(0, total);
        let mut after = Vec::<u8>::new();
        while let Some((root, label)) = self.next_tree(&after)? {
            self.options.check()?;
            self.tree(&reader, &found, &root, &label)?;
            self.coverage.trees += 1;
            progress(self.coverage.trees, total);
            after = root;
        }
        Ok(())
    }

    fn report(&mut self, outcome: Outcome, message: String) {
        match outcome {
            Outcome::Fail => self.coverage.failed += 1,
            _ => self.coverage.incomplete = true,
        }
        emit(self.observer, CHECK, outcome, message);
    }

    /// Resolves the stack's heads into the scratch `heads` table: main's
    /// head Layer and each live Branch's newest node, by their roots. `None`
    /// when the Store holds no live project with this id.
    fn heads(&mut self, stack: LayerStackId) -> Result<Option<Stack>, BackendError> {
        let id = stack.as_slice().to_vec();
        let rows = self.view.query("SELECT CASE WHEN blob_length(scope_id)=32 THEN scope_id END,CASE WHEN blob_length(profile_id)=32 THEN profile_id END,CASE WHEN blob_length(head_layer_id)=33 THEN head_layer_id END FROM layer_stack s WHERE layer_stack_id=?1 AND NOT EXISTS(SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=s.layer_stack_id)", vec![Param::Bytes(id.clone())])?;
        let Some(row) = rows.first() else {
            self.report(
                Outcome::Fail,
                "the Store holds no live project with the id its list named".to_owned(),
            );
            return Ok(None);
        };
        let (scope, profile, head) = match (
            row.get::<Option<Vec<u8>>>(0)?,
            row.get::<Option<Vec<u8>>>(1)?,
            row.get::<Option<Vec<u8>>>(2)?,
        ) {
            (Some(scope), Some(profile), Some(head)) => (scope, profile, head),
            _ => {
                self.report(
                    Outcome::Fail,
                    "the project's record in the Store has malformed fields".to_owned(),
                );
                return Ok(None);
            }
        };
        self.head(&id, "layer", &head, 0, "main")?;
        let mut after = Vec::<u8>::new();
        loop {
            self.options.check()?;
            let rows = self.view.query("SELECT branch_id,name,CASE WHEN blob_length(base_layer_id)=33 THEN base_layer_id END,CASE WHEN blob_length(head_commit_id)=33 THEN head_commit_id END FROM branch b WHERE layer_stack_id=?1 AND blob_length(branch_id)=17 AND branch_id>?2 AND NOT EXISTS(SELECT 1 FROM branch_retired r WHERE r.branch_id=b.branch_id) ORDER BY branch_id LIMIT 64", vec![Param::Bytes(id.clone()), Param::Bytes(after.clone())])?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                after = row.get(0)?;
                let name = format!("Branch {}", row.get::<String>(1)?);
                match (
                    row.get::<Option<Vec<u8>>>(3)?,
                    row.get::<Option<Vec<u8>>>(2)?,
                ) {
                    (Some(commit), _) => self.head(&id, "\"commit\"", &commit, 1, &name)?,
                    (None, Some(layer)) => self.head(&id, "layer", &layer, 1, &name)?,
                    (None, None) => {
                        self.report(Outcome::Fail, format!("{name} names no head and no base"))
                    }
                }
            }
        }
        Ok(Some(Stack { scope, profile }))
    }

    /// Records the root of head `node` (a row of `table`) under `label`, or
    /// reports a head that is missing or belongs to another project.
    fn head(
        &mut self,
        stack: &[u8],
        table: &str,
        node: &[u8],
        rank: i64,
        label: &str,
    ) -> Result<(), BackendError> {
        self.coverage.heads += 1;
        let key = if table == "layer" {
            "layer_id"
        } else {
            "commit_id"
        };
        let rows = self.view.query(
            &format!("SELECT layer_stack_id,CASE WHEN blob_length(root_id)=32 THEN root_id END FROM {table} WHERE {key}=?1"),
            vec![Param::Bytes(node.to_vec())],
        )?;
        let found = match rows.first() {
            Some(row) => Some((row.get::<Vec<u8>>(0)?, row.get::<Option<Vec<u8>>>(1)?)),
            None => None,
        };
        match found {
            Some((owner, Some(root))) if owner == stack => {
                (self.scratch.0)
                    .execute(
                        "INSERT OR IGNORE INTO heads(root,rank,label) VALUES(?1,?2,?3)",
                        params![root, rank, label],
                    )
                    .map_err(sql)?;
            }
            Some((owner, _)) if owner != stack => self.report(
                Outcome::Fail,
                format!(
                    "the head of {label} ({}) belongs to another project",
                    hex(node)
                ),
            ),
            Some(_) => self.report(
                Outcome::Fail,
                format!("the head of {label} ({}) names a malformed root", hex(node)),
            ),
            None => self.report(
                Outcome::Fail,
                format!("the head of {label} ({}) is missing", hex(node)),
            ),
        }
        Ok(())
    }

    /// The first root after `after`, with the label of its first head
    /// (main before any Branch).
    fn next_tree(&mut self, after: &[u8]) -> Result<Option<(Vec<u8>, String)>, BackendError> {
        (self.scratch.0)
            .query_row(
                "SELECT root,label FROM heads WHERE root>?1 ORDER BY root,rank,label LIMIT 1",
                [after],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(sql)
    }

    /// One tree: its namespace qualified, then every object it reaches that
    /// no tree read before.
    fn tree(
        &mut self,
        reader: &Reader<'_>,
        stack: &Stack,
        root: &[u8],
        label: &str,
    ) -> Result<(), BackendError> {
        self.scratch.clear().map_err(|_| BackendError::Capacity)?;
        let qualified = (|| {
            let context = RootContext {
                root: FilesystemRootId(ObjectId::from_bytes(root)?),
                scope: InodeScope::from_object(ObjectId::from_bytes(&stack.scope)?),
                profile: ObjectId::from_bytes(&stack.profile)?,
                root_serial: None,
            };
            qualify_root(
                reader,
                &mut *self.scratch,
                &context,
                &mut QualificationWork::default(),
            )
        })();
        self.options.check()?;
        if let Err(error) = qualified {
            let outcome =
                super::observation::storage_outcome(&StorageError::Content(error.clone()));
            self.report(
                outcome,
                format!("the tree of {label} (root {}): {error}", hex(root)),
            );
        }
        self.scratch.clear().map_err(|_| BackendError::Capacity)?;
        (self.scratch.0)
            .execute(
                "INSERT OR IGNORE INTO visited(id,parent) VALUES(?1,NULL)",
                [root],
            )
            .map_err(sql)?;
        let policy = self
            .view
            .policy()
            .map_err(|_| BackendError::Integrity)?
            .construction();
        loop {
            self.options.check()?;
            let batch: Vec<(Vec<u8>, Option<Vec<u8>>)> = {
                let mut statement = (self.scratch.0)
                    .prepare("SELECT id,parent FROM visited WHERE done=0 ORDER BY id LIMIT ?1")
                    .map_err(sql)?;
                let rows = statement
                    .query_map([BATCH as i64], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(sql)?;
                rows.collect::<Result<_, _>>().map_err(sql)?
            };
            if batch.is_empty() {
                return Ok(());
            }
            let mut reached = Vec::new();
            for (id, parent) in &batch {
                self.options.check()?;
                self.coverage.objects += 1;
                match visit(self.view, reader, policy, id) {
                    Ok(Some((children, failed))) => {
                        reached.extend(children.into_iter().map(|c| (c, id)));
                        if let Some(error) = failed {
                            let outcome = super::observation::storage_outcome(&error);
                            self.report(
                                outcome,
                                format!("object {}: {error} (tree of {label})", hex(id)),
                            );
                        }
                    }
                    Ok(None) => {
                        let by = parent.as_deref().map_or_else(
                            || "it is the tree's root".to_owned(),
                            |parent| format!("object {} refers to it", hex(parent)),
                        );
                        self.report(
                            Outcome::Fail,
                            format!("object {} is missing; {by} (tree of {label})", hex(id)),
                        );
                    }
                    Err(error) => {
                        let outcome = super::observation::storage_outcome(&error);
                        self.report(
                            outcome,
                            format!("object {}: {error} (tree of {label})", hex(id)),
                        );
                    }
                }
            }
            let tx = self.scratch.0.transaction().map_err(sql)?;
            for (id, _) in &batch {
                tx.execute("UPDATE visited SET done=1 WHERE id=?1", [id])
                    .map_err(sql)?;
            }
            for (child, parent) in reached {
                tx.execute(
                    "INSERT OR IGNORE INTO visited(id,parent) VALUES(?1,?2)",
                    params![child.as_bytes().as_slice(), parent],
                )
                .map_err(sql)?;
            }
            tx.commit().map_err(sql)?;
        }
    }
}

/// Reads and checks one object as the whole-Store scan does: `None` when
/// the Store does not hold it; else its children, which are visited even
/// when its own check failed, and that failure. A failure of its children's
/// context check that a missing child explains is left to that child.
fn visit(
    view: &dyn View,
    reader: &Reader<'_>,
    policy: layerfs_content::ConstructionPolicy,
    id: &[u8],
) -> StorageResult<Option<Visited>> {
    let id = ObjectId::from_bytes(id)?;
    let Some(location) = view.location(&id)? else {
        return Ok(None);
    };
    let canonical = reader.read(id)?;
    if canonical.len() != location.canonical_length {
        return Err(StorageError::Integrity("canonical length"));
    }
    let scope = if location.role == ObjectRole::FilesystemRoot {
        FilesystemRoot::decode(&canonical)?.scope()
    } else {
        InodeScope::from_object(ObjectId::for_bytes(b"diagnostic-local-grammar"))
    };
    Timing::disabled("diagnostic.admit", |timing| -> StorageResult<_> {
        let object = FinalizedObject::admit(
            id,
            location.role,
            canonical,
            policy,
            scope,
            timing.child("admit"),
        )?;
        let children = object.references().to_vec();
        let failed = match object.validate_stored_children(reader, policy, timing.child("context"))
        {
            Ok(()) => None,
            Err(error) => {
                let mut missing = false;
                for child in &children {
                    missing |= view.location(child)?.is_none();
                }
                (!missing).then_some(StorageError::Content(error))
            }
        };
        Ok(Some((children, failed)))
    })
    .0
}

/// An object's children and the failure of its own check, if it failed.
type Visited = (Vec<ObjectId>, Option<StorageError>);

fn sql(_: rusqlite::Error) -> BackendError {
    BackendError::Capacity
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}
