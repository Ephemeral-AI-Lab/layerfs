//! Merge-on-commit: inside the Commit's own Save, the candidate is merged into
//! a Branch head that moved since the Workspace was bound, and a conflicted
//! Workspace is refused while its marker blocks remain.
use super::{BoundWorkspace, CommitError, MergeReport};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::ObjectId;
use layerfs_content::{
    compare::{binding_changes, inode_changes, root_changes, DifferenceWork, TreeTable},
    filesystem::FilesystemRootId,
    merge::{merge_roots, paths_with_markers, MergeInput, RulesFrom, Side},
    ContentError, ContentResult,
};
use layerfs_history::BranchSnapshot;
use layerfs_storage::Save;
use layerfs_workspace::{InodeSerials, WorkspaceError};

/// Serials one reservation takes while a merge needs fresh ones.
const MERGE_SERIALS: u64 = 64;
/// Changed paths a conflict reports as written into the Workspace.
const WRITTEN_PATHS: usize = 4096;
/// Changed serials the in-lane step may decide (#355, by serial since M1):
/// the lane's bound on merging what landed after the merge, so its hold
/// stays short and grows with that change, never with the tree.
pub const LANE_MERGE_PATHS: usize = 1024;

/// A conflicted Workspace: its local base is the merge with markers, while
/// its history binding is the head it merged with.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMerge {
    /// Root installed as the Workspace's local base.
    pub local_root: ObjectId,
    /// Paths that conflicted.
    pub conflicted: Vec<String>,
}

impl BoundWorkspace {
    /// The local base root: the pending merge's, else the history binding's.
    pub(super) fn local_root(&self, snapshot: &BranchSnapshot) -> Result<ObjectId, CommitError> {
        Ok(self
            .pending
            .lock()
            .map_err(|_| CommitError::Workspace(WorkspaceError::BindingPoisoned))?
            .as_ref()
            .map_or(snapshot.effective_root, |pending| pending.local_root))
    }

    /// Paths of an installed conflicted merge that is not yet committed.
    pub fn pending_conflicts(&self) -> Result<Option<Vec<String>>, WorkspaceError> {
        Ok(self
            .pending
            .lock()
            .map_err(|_| WorkspaceError::BindingPoisoned)?
            .as_ref()
            .map(|pending| pending.conflicted.clone()))
    }

    /// Refuses a candidate whose previously conflicted files still hold a
    /// complete marker block.
    pub(super) fn refuse_markers(
        &self,
        save: &Save<'_>,
        candidate: FilesystemRootId,
    ) -> Result<(), CommitError> {
        let conflicted = match self
            .pending
            .lock()
            .map_err(|_| CommitError::Workspace(WorkspaceError::BindingPoisoned))?
            .as_ref()
        {
            Some(pending) => pending.conflicted.clone(),
            None => return Ok(()),
        };
        let marked = crate::telemetry::timed("markers", |timing| {
            paths_with_markers(save, candidate, &conflicted, timing)
        });
        let marked = marked.map_err(content)?;
        if marked.is_empty() {
            Ok(())
        } else {
            Err(CommitError::Markers(marked))
        }
    }

    /// Merges the candidate (ours) into the current head (theirs) against
    /// the history root this Workspace is bound to. Merged files and the
    /// merged root go into the same Save. Runs before the publication lane;
    /// its build state goes to merge records in the Store file.
    pub(super) fn merge_candidate(
        &self,
        save: &Save<'_>,
        bound: &BranchSnapshot,
        current: &BranchSnapshot,
        candidate: FilesystemRootId,
    ) -> Result<(FilesystemRootId, MergeReport), CommitError> {
        self.merge_stage(save, bound, current, candidate, candidate, None)
    }

    /// The in-lane step (#355): the head moved from `merged_with` to
    /// `current` after the merge, so only the changes between those two heads
    /// are merged into `result`, in memory and without merge records. More
    /// than [`LANE_MERGE_PATHS`] changed serials to decide is refused `LaneMerge`.
    pub(super) fn merge_in_lane(
        &self,
        save: &Save<'_>,
        merged_with: &BranchSnapshot,
        current: &BranchSnapshot,
        result: FilesystemRootId,
        candidate: FilesystemRootId,
    ) -> Result<(FilesystemRootId, MergeReport), CommitError> {
        let merged = self.merge_stage(
            save,
            merged_with,
            current,
            result,
            candidate,
            Some(LANE_MERGE_PATHS),
        );
        match &merged {
            Ok((_, report)) => self.store.note_lane(report.lane_paths),
            Err(CommitError::LaneMerge { .. }) => self.store.note_lane(None),
            Err(_) => {}
        }
        merged
    }

    /// One merge of `ours` into `current`'s root against `bound`'s root.
    /// With `lane`, the paths are named from the head's side and the result
    /// is built in memory on `ours`; otherwise they are named from ours and
    /// the result is built on the head with merge records. `shown` is the
    /// Workspace's candidate, against which written paths are listed.
    fn merge_stage(
        &self,
        save: &Save<'_>,
        bound: &BranchSnapshot,
        current: &BranchSnapshot,
        ours: FilesystemRootId,
        shown: FilesystemRootId,
        lane: Option<usize>,
    ) -> Result<(FilesystemRootId, MergeReport), CommitError> {
        let ports = self.store.ports(bound.scope);
        let mut range = (0u64, 0u64);
        let mut refused = None;
        let mut fresh = || -> ContentResult<u64> {
            if range.0 == range.1 {
                let (start, count) = ports.reserve(MERGE_SERIALS).map_err(|error| {
                    refused = Some(error);
                    ContentError::ResourceUnavailable {
                        what: "merge inode serials",
                    }
                })?;
                range = (start, start.saturating_add(count));
            }
            range.0 += 1;
            Ok(range.0 - 1)
        };
        let theirs = match current.branch.head_commit {
            Some(head) => format!("{}@{head}", current.branch.name.as_str()),
            None => current.branch.name.as_str().to_owned(),
        };
        let policy = self.store.policy().construction();
        let capacities = policy.capacities();
        let hints = super::CatalogHints::new(&*self.store.history);
        let input = MergeInput {
            base: FilesystemRootId(bound.effective_root),
            ours,
            theirs: FilesystemRootId(current.effective_root),
            labels: ["workspace", &theirs],
            rules_from: RulesFrom::Theirs,
            policy,
            capacities: &capacities,
            hints: Some(&hints),
            decide_from: if lane.is_some() {
                Side::Theirs
            } else {
                Side::Ours
            },
            bound: lane,
        };
        let merged = match lane {
            Some(_) => crate::telemetry::timed("merge.lane", |timing| {
                Ok((
                    merge_roots(save, &mut save.sink(), &input, &mut fresh, None, timing),
                    None,
                ))
            }),
            None => crate::telemetry::timed("merge", |timing| {
                self.store.merge_records(|records| {
                    merge_roots(
                        save,
                        &mut save.sink(),
                        &input,
                        &mut fresh,
                        Some(records),
                        timing,
                    )
                })
            }),
        };
        let (merged, scratch) = merged.map_err(|error| CommitError::Storage(error.into()))?;
        let merged = match (merged, refused) {
            (Ok(merged), _) => {
                self.store.note_merge(&merged);
                merged
            }
            (Err(_), Some(error)) => return Err(CommitError::Workspace(error)),
            (Err(_), None) if scratch.is_some() => {
                let failure = scratch.expect("checked failure");
                return Err(CommitError::Storage(failure.into()));
            }
            (Err(_), None) if hints.failure.borrow().is_some() => {
                let error = hints.failure.take().expect("checked failure");
                return Err(CommitError::History(error));
            }
            (
                Err(ContentError::BoundedCapacityExceeded {
                    what: "merge.bound",
                    limit,
                    actual,
                }),
                None,
            ) => {
                return Err(CommitError::LaneMerge {
                    paths: actual,
                    limit,
                })
            }
            (Err(error), None) => return Err(content(error)),
        };
        let (written, written_truncated) = if merged.clean() {
            (Vec::new(), false)
        } else {
            let changes = root_changes(save, shown, merged.root, WRITTEN_PATHS).map_err(content)?;
            let paths = changes.entries.into_iter().map(|change| change.path);
            (paths.collect(), changes.truncated)
        };
        Ok((
            merged.root,
            MergeReport {
                from: bound.branch.head_commit,
                head: current.branch.head_commit,
                conflicts: merged.conflicts,
                lww: merged.lww,
                written,
                written_truncated,
                lane_paths: lane.map(|_| merged.decided),
            },
        ))
    }
}

/// One kernel cache entry an install made stale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stale<'a> {
    /// The inode's attributes, pages or listing.
    Inode(u64),
    /// One name's binding in a directory.
    Entry { parent: u64, name: &'a str },
}

/// Why a walk of what an install changed stopped.
#[derive(Debug)]
pub enum StaleError {
    Workspace(WorkspaceError),
    Content(ContentError),
    Visit(std::io::Error),
}

impl BoundWorkspace {
    /// Visits what the kernel may cache from the root the Workspace showed
    /// that the installed root changed: every inode whose value changed or
    /// vanished, and every rebound name of a directory present in both. An
    /// inode only the installed root has is not visited; nothing could have
    /// cached it. The walk streams; it holds no list of the changes.
    pub fn stale_after_install(
        &self,
        shown: ObjectId,
        installed: ObjectId,
        mut visit: impl FnMut(Stale<'_>) -> std::io::Result<()>,
    ) -> Result<(), StaleError> {
        let scope = self.snapshot().map_err(StaleError::Workspace)?.scope;
        let ports = self.store.ports(scope);
        let reader = &*ports;
        let open = |root| TreeTable::open(reader, FilesystemRootId(root));
        let (before, after) = (
            open(shown).map_err(StaleError::Content)?,
            open(installed).map_err(StaleError::Content)?,
        );
        let mut visited = None;
        let mut work = DifferenceWork::default();
        let walked = inode_changes(reader, &before, &after, &mut work, |serial, was, now| {
            let Some(was) = was else { return Ok(()) };
            if let Err(error) = visit(Stale::Inode(serial)) {
                visited = Some(error);
                return Err(ContentError::OutputRejected);
            }
            let Some(now) = now
                .filter(|now| now.kind == InodeKind::Directory && was.kind == InodeKind::Directory)
            else {
                return Ok(());
            };
            let mut inner = DifferenceWork::default();
            binding_changes(
                reader,
                Some(was.content_root),
                Some(now.content_root),
                &mut inner,
                |name, _, _| {
                    visit(Stale::Entry {
                        parent: serial,
                        name: name.as_str(),
                    })
                    .map_err(|error| {
                        visited = Some(error);
                        ContentError::OutputRejected
                    })
                },
            )
        });
        match (walked, visited) {
            (_, Some(error)) => Err(StaleError::Visit(error)),
            (Err(error), None) => Err(StaleError::Content(error)),
            (Ok(()), None) => Ok(()),
        }
    }
}

fn content(error: ContentError) -> CommitError {
    CommitError::Content {
        error,
        provider: None,
    }
}
