//! `project.delete`: a project is retired in its Store and its home entry is
//! removed (spec §4, §4.2, §5.1 and §10).
//!
//! The marker is the commit point. The caller first writes the durable intent
//! `store/<store>/delete/<project>.json` with
//! `{store, project, stack_id, deleted_ms, attempt_ms, state, op_id?}`, then marks the
//! Stack, then removes only its own home entry and unlinks the intent. A
//! retry reads the intent by the recorded Stack id, reuses its `deleted_ms`
//! and finishes the entry: `Marked` means this attempt marked a live Stack,
//! `Retired` means the marker was already there (including a hidden Store
//! project whose home entry is stale), and `Absent` means no live Stack is
//! there, so a retry finishing its sidecar removes the stale entry. A fresh
//! call with no sidecar that meets `Retired` or `Absent` is `NoSuchProject`
//! and keeps its entry. A lost worker reply is `Unknown`; the
//! intent stays and the retry classifies the Stack again. A home record or
//! listing whose Stack differs from the intent is a project made again under
//! the same name: the stale intent is discarded and the call is
//! `NoSuchProject`, so the re-created project and its entry are left alone
//! (spec §4).
//!
//! The worker op is `project.delete`; its frame is
//! `{store, stack_id, deleted_ms?, project?, op_id?, pending_retry?,
//! store_record?}`, and `project.delete.entry` removes only the worker's own
//! entry for a `marked` sidecar (frame `{store, stack_id, project?}`). The two
//! optional context fields are the ones the retired
//! `project.remove` call sent: the worker opens the Store under its own root
//! with the caller's `store_record`, and the project name lets its reply and
//! its own home entry name the project. The worker writes the marker,
//! removes its own entry and replies
//! `{kind, project, stack_id, deleted_ms, retired?}`; the caller removes its
//! own entry and echoes `home_entry_removed`.
//!
//! A caller retrying a sidecar it wrote *before* its admission (no marker
//! confirmed yet) that finds the row still live and no marker only settles:
//! it redoes the marker with the sidecar's `deleted_ms`, marks the sidecar
//! and keeps the caller and worker entries. The next call finds the marker
//! confirmed and finishes the entries. The normal delete and a retry whose
//! marker was already there remove the entries in the same call.
//!
//! A sidecar in `marked` is past the marker: only the entries remain. Its
//! retry takes no admission and writes no marker; it removes the worker's own
//! entry (through `project.delete.entry` when the Store runs there) and then
//! its home entry, and records `done` (spec §5.1). A `done` sidecar likewise
//! needs no Store call and only finishes its entry.
//!
//! The sidecar is written with its admission identity *before* the home path
//! admits the project scope (T4.6, `project.delete.admit` on the worker
//! route): a crash between the registered row and its reply then leaves the
//! identity on disk, so the retry re-registers the same row and settles it.
//! The admission is the same `gc_register` the Store-side removals rely on. A
//! live mount refuses `MountsActive` with its holders and a live operation
//! `Busy`; the pending sidecar stays for the retry and no marker is written.
//! The marker reuses the admission's operation identity, and the caller
//! releases it on every exit path (`project.delete.release`), so a refused or
//! failed call leaves no admission row behind.
use crate::admin::open::args::OpenArgs;
use crate::admin::open::{lease, service};
use crate::admin::record::store::StoreRecord;
use crate::admin::route::executor::{self, Executor, Worker};
use crate::data::project::frame::{
    DeleteAdmitFrame, DeleteEntryFrame, DeleteEntryReply, DeleteFrame, DeleteReleaseFrame,
    DeleteReply,
};
use crate::data::project::record::project::ProjectRecord;
use layerfs_history::{GcScopeKind, HistoryCatalog, HistoryError, LayerStackId};
use layerfs_home::events::Events;
use layerfs_home::home::{now_ms, paths};
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_protocol::sdk::{hex, unhex};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store_service::history::admission as store_admission;
use layerfs_store_service::history::deletion::{delete_stack_with, Deletion};
use layerfs_store_service::history::lookup;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub use super::remote::listed_unknown;
use super::remote::{listed_on_worker, listed_stack, Remote};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The state the durable intent records (`pending`, `marked`, `done`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntentState {
    /// The identity is saved and the intent is durable; the marker may or may
    /// not have been written (a lost reply leaves this state).
    Pending,
    /// The marker is committed. The home entry may still be there.
    Marked,
    /// The home entry was removed. The intent is unlinked next; a crash
    /// between the two leaves this state for the retry to settle.
    Done,
}

impl IntentState {
    /// The state's file and frame text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Marked => "marked",
            Self::Done => "done",
        }
    }

    /// The state `text` names, if any.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "marked" => Some(Self::Marked),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

/// The durable delete intent at `store/<store>/delete/<project>.json`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Intent {
    /// The Store the project was in, as sent.
    pub store: String,
    /// The project's name in it, as sent.
    pub project: String,
    /// The LayerStack the home record held when the intent was written; a
    /// retry resolves the Store by this id, never by name.
    pub stack: LayerStackId,
    /// The `deleted_ms` the marker holds, or is asked to hold. A retry reuses
    /// this value; a `Retired` marker may replace it with its stored value.
    pub deleted_ms: i64,
    /// When the last attempt started, in milliseconds.
    pub attempt_ms: i64,
    /// Where the attempt got to.
    pub state: IntentState,
    /// The project-scope admission this attempt took, when it took one. A
    /// retry re-registers the same identity, so a call whose reply was lost
    /// settles its own row instead of meeting it as another operation.
    pub op: Option<[u8; 16]>,
}

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

impl Intent {
    /// The intent's path in `home`.
    pub fn path(home: &Path, store: &str, project: &str) -> PathBuf {
        paths::store_dir(home, store)
            .join("delete")
            .join(format!("{project}.json"))
    }

    /// The intent of project `project` of Store `store`, or `None`.
    pub fn read(home: &Path, store: &str, project: &str) -> Result<Option<Self>> {
        let path = Self::path(home, store, project);
        let Some(value) = layerfs_home::home::read_record(&path)? else {
            return Ok(None);
        };
        let malformed = || {
            refused(
                "InvalidInput",
                format!("project.delete intent {} is malformed", path.display()),
            )
        };
        let store = value.get_text("store").ok_or_else(malformed)?.to_owned();
        let project = value.get_text("project").ok_or_else(malformed)?.to_owned();
        let stack = value
            .get_text("stack_id")
            .and_then(unhex)
            .and_then(|bytes| LayerStackId::from_slice(&bytes).ok())
            .ok_or_else(malformed)?;
        let deleted_ms = value.get_int("deleted_ms").ok_or_else(malformed)?;
        let attempt_ms = value.get_int("attempt_ms").ok_or_else(malformed)?;
        let state = value
            .get_text("state")
            .and_then(IntentState::parse)
            .ok_or_else(malformed)?;
        let op = match value.get_text("op_id") {
            Some(text) => {
                let bytes = unhex(text).ok_or_else(malformed)?;
                let body: [u8; 16] = bytes.as_slice().try_into().map_err(|_| malformed())?;
                Some(body)
            }
            None => None,
        };
        Ok(Some(Self {
            store,
            project,
            stack,
            deleted_ms,
            attempt_ms,
            state,
            op,
        }))
    }

    /// Writes the intent whole: a private temporary file, fsync, one rename,
    /// then the directory's fsync. A reader sees the old intent or the new
    /// one, never a part of either.
    pub fn write(home: &Path, intent: &Self) -> Result<()> {
        let path = Self::path(home, &intent.store, &intent.project);
        let parent = path
            .parent()
            .ok_or_else(|| refused("Failed", "a delete intent needs a folder"))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(failed)?;
        let unique = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{}.{}.{}.tmp",
            intent.project,
            std::process::id(),
            unique
        ));
        let mut fields: Vec<(String, SdkValue)> = vec![
            ("store".to_owned(), SdkValue::text(&intent.store)),
            ("project".to_owned(), SdkValue::text(&intent.project)),
            (
                "stack_id".to_owned(),
                SdkValue::text(hex(&intent.stack.to_bytes())),
            ),
            ("deleted_ms".to_owned(), SdkValue::Int(intent.deleted_ms)),
            ("attempt_ms".to_owned(), SdkValue::Int(intent.attempt_ms)),
            ("state".to_owned(), SdkValue::text(intent.state.as_str())),
        ];
        if let Some(op) = intent.op {
            fields.push(("op_id".to_owned(), SdkValue::text(hex(&op))));
        }
        let value = SdkValue::Map(fields);
        let mut text = layerfs_home::json::print(&value);
        text.push('\n');
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(failed)?;
        let written = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(failed);
        drop(file);
        let renamed = written.and_then(|()| fs::rename(&temporary, &path).map_err(failed));
        if let Err(refusal) = renamed {
            let _ = fs::remove_file(&temporary);
            return Err(refusal);
        }
        // the directory entry is durable too; a filesystem that cannot
        // fsync a directory has already made the rename durable
        let _ = File::open(parent).and_then(|folder| folder.sync_all());
        Ok(())
    }

    /// Removes the intent, if it is there.
    pub fn unlink(home: &Path, store: &str, project: &str) -> Result<()> {
        let path = Self::path(home, store, project);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(failed(error)),
        }
        if let Some(parent) = path.parent() {
            let _ = File::open(parent).and_then(|folder| folder.sync_all());
        }
        Ok(())
    }

    /// Drops an intent that no longer describes the home record (a project
    /// made again under the same name).
    pub fn discard(home: &Path, store: &str, project: &str) -> Result<()> {
        Self::unlink(home, store, project)
    }
}

/// The project-scope admission `project.delete` takes before its sidecar and
/// marker: the admission row's operation identity, reused by the marker so the
/// one row is excluded from its own scan and released once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Admission {
    /// The `gc_intent` operation identity.
    pub op: [u8; 16],
    /// The project scope it holds.
    pub stack: LayerStackId,
}

impl Admission {
    /// One admission of `stack` under operation `op`.
    pub const fn new(stack: LayerStackId, op: [u8; 16]) -> Self {
        Self { op, stack }
    }
}

/// The Store side of one `project.delete` attempt and the caller's entry
/// removal, as one seam so the home path is testable without a Store.
pub trait Effects {
    /// Takes the project-scope admission under `op`, the identity the sidecar
    /// recorded before this call: a live mount is `MountsActive` with its
    /// holders, a live operation `Busy`, an uncertain owner `Unresolved`. A
    /// refusal registers nothing under a new identity, and a retry
    /// re-registers `op`'s own row instead of meeting it as another
    /// operation. `project` names the row, so a later caller meeting it
    /// reports `{kind: "project", name}` rather than the hex LayerStack id.
    fn admit(&mut self, stack: LayerStackId, op: [u8; 16], project: &str) -> Result<Admission>;

    /// Releases `admission` on every exit path: success, refusal and an
    /// unknown Store outcome. A provider without admission releases nothing.
    fn release(&mut self, admission: Admission) -> Result<()>;

    /// Marks (or reclassifies) `admission`'s Stack, reusing the admission's
    /// operation identity. A retry passes the intent's `deleted_ms`.
    /// `pending_retry` is true when the caller is retrying a sidecar written
    /// before it admitted (no marker confirmed yet): the worker route then
    /// keeps its own entry, as the caller keeps its home entry.
    fn mark(
        &mut self,
        admission: Admission,
        deleted_ms: i64,
        pending_retry: bool,
    ) -> Result<Deletion>;

    /// Removes the Store-side home entry a `marked` sidecar still names: the
    /// worker's own entry when the Store runs there, nothing on the local
    /// route, where that entry is the caller's home entry. The marker is
    /// already durable, so this takes no admission and writes no marker
    /// (spec §5.1).
    fn settle(&mut self, _stack: LayerStackId) -> Result<()> {
        Ok(())
    }

    /// Removes the caller's home entry; `Ok(())` when nothing remains.
    fn remove_entry(&mut self, entry: &Path) -> Result<()>;
}

/// `{kind: "Removed", project, deleted_ms, home_entry_removed}` (spec §10;
/// `home_entry_removed: false` is still success, the marker being the commit
/// point).
fn removed(store: &str, project: &str, deleted_ms: i64, home_entry_removed: bool) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text("Removed")),
        ("project", SdkValue::text(format!("{store}/{project}"))),
        ("deleted_ms", SdkValue::Int(deleted_ms)),
        ("home_entry_removed", SdkValue::Bool(home_entry_removed)),
    ])
}

/// The side effects and the project-scope admission one finishing attempt
/// runs against: together, so `mark_and_finish` stays a small call.
struct Finish<'a> {
    effects: &'a mut dyn Effects,
    admission: Admission,
    /// Whether the call had no durable intent: a fresh call may not finish a
    /// retired project, while a retry resolves itself through its sidecar.
    fresh: bool,
}

/// The mutating part of `delete_with` after the sidecar is durable: the marker
/// and the entry removal (spec §4). The sidecar names the admission, so the
/// retry that redoes this marker reuses the same operation identity.
fn mark_and_finish(
    home: &Path,
    store: &str,
    project: &str,
    mut current: Intent,
    mut deleted_ms: i64,
    pending_retry: bool,
    finish: Finish<'_>,
) -> Outcome {
    let Finish {
        effects,
        admission,
        fresh,
    } = finish;
    let deletion = effects.mark(admission, deleted_ms, pending_retry)?;
    if fresh && !matches!(deletion, Deletion::Marked { .. }) {
        // The call had no sidecar, so it had to find a live project, and the
        // Store says this Stack is already retired (or was never there). A
        // stale entry is not a project: `NoSuchProject`, with the sidecar
        // this call wrote removed and the entry left in place (spec §4).
        Intent::unlink(home, store, project)?;
        return Err(refused(
            "NoSuchProject",
            format!("no project {store}/{project}"),
        ));
    }
    deleted_ms = deletion.deleted_ms(deleted_ms);
    current.deleted_ms = deleted_ms;
    current.state = IntentState::Marked;
    current.attempt_ms = now_ms();
    Intent::write(home, &current)?;

    // A retry of a sidecar written before the admission, whose marker was
    // just written over a live row and no marker existed yet, settles only:
    // the marker is now confirmed and the sidecar is `marked`, so the next
    // call finishes the entry. The home and worker entries stay for that
    // call; the normal delete path and a retry whose marker was already
    // there (§ `Retired`) remove the entry now.
    if pending_retry && matches!(deletion, Deletion::Marked { .. }) {
        return Ok(removed(store, project, deleted_ms, false));
    }

    let home_entry_removed = effects
        .remove_entry(&paths::project_dir(home, store, project))
        .is_ok();
    if home_entry_removed {
        current.state = IntentState::Done;
        Intent::write(home, &current)?;
        Intent::unlink(home, store, project)?;
    }
    Ok(removed(store, project, deleted_ms, home_entry_removed))
}

/// Runs `project.delete` for a home entry whose Store side is `effects`.
///
/// `stack` is the Stack id the Store lists for the project, or the record's
/// id on a worker route, where the caller cannot list. A fresh call without
/// one is `NoSuchProject`: only a same-home retry, through its durable
/// intent, resolves a retired Stack (spec §4, §4.2). A listing or record that
/// names a different Stack than the sidecar is a re-created project: the
/// sidecar is discarded and the call is `NoSuchProject`, so that project's
/// entry is never touched. The same function serves the local and worker
/// routes, and a fake in tests.
///
/// The identity save and then the sidecar, naming the admission, precede the
/// project scope's admission and the marker: a live mount is `MountsActive`
/// with its holders and a live operation `Busy`, reported with the pending
/// sidecar kept and no marker. The admission is released on every exit path.
pub fn delete_with(
    home: &Path,
    store: &str,
    project: &str,
    stack: Option<LayerStackId>,
    effects: &mut impl Effects,
) -> Outcome {
    let record = ProjectRecord::read(home, store, project)?;
    let record_stack = record.as_ref().and_then(ProjectRecord::stack_id);
    let intent = Intent::read(home, store, project)?;

    // The Stack the name resolves to now: the caller's listing first, then
    // the record. A different Stack id for the same name is a project made
    // again: the stale intent must not touch its entry, so it is discarded
    // and the call is `NoSuchProject` (spec §4). The listing is checked too:
    // a re-created project whose home record is missing or names no Stack
    // must not be deleted by an older intent.
    let current_stack = stack.or(record_stack);
    if let (Some(stale), Some(current)) = (&intent, current_stack) {
        if stale.stack != current {
            Intent::discard(home, store, project)?;
            return Err(refused(
                "NoSuchProject",
                format!("no project {store}/{project}"),
            ));
        }
    }

    // Whether this call had a durable intent to fall back on. A fresh call
    // must find a live project; a retry finishes a retired one (spec §4).
    let fresh = intent.is_none();

    // A sidecar read in `pending` was written before an earlier admission: a
    // retry of it redoes the marker but does not finish the entry.
    let pending_retry = intent
        .as_ref()
        .is_some_and(|intent| intent.state == IntentState::Pending);

    let (stack, deleted_ms) = match &intent {
        Some(intent) => (intent.stack, intent.deleted_ms),
        None => {
            // A fresh call resolves the project only through the Stack its
            // Store lists now, never through the home record: a record from
            // another home may name a retired Stack, and the record's id is
            // not evidence of a live project (spec §4, §4.2). The exception
            // is a same-home retry, which has the durable intent read above.
            let Some(stack) = stack else {
                return Err(refused(
                    "NoSuchProject",
                    format!("no project {store}/{project}"),
                ));
            };
            (stack, now_ms())
        }
    };

    if let Some(settled) = &intent {
        if settled.state == IntentState::Marked {
            // The marker is durable and only the entries remain: no
            // admission and no second marker (spec §5.1). The worker's own
            // entry goes first, then the caller's home entry; a failed home
            // removal leaves the sidecar `marked` for the next retry.
            effects.settle(stack)?;
            let removed_entry = effects
                .remove_entry(&paths::project_dir(home, store, project))
                .is_ok();
            if removed_entry {
                let mut done = settled.clone();
                done.state = IntentState::Done;
                done.attempt_ms = now_ms();
                Intent::write(home, &done)?;
                Intent::unlink(home, store, project)?;
            }
            return Ok(removed(store, project, deleted_ms, removed_entry));
        }
        if settled.state == IntentState::Done {
            // The done sidecar needs no Store call: the marker is already
            // there and only the home entry is removed.
            let removed_entry = effects
                .remove_entry(&paths::project_dir(home, store, project))
                .is_ok();
            let mut done = settled.clone();
            done.attempt_ms = now_ms();
            let _ = Intent::write(home, &done);
            let _ = Intent::unlink(home, store, project);
            return Ok(removed(store, project, deleted_ms, removed_entry));
        }
    }

    // The sidecar with its operation identity, then the admission, then the
    // marker. Writing the sidecar first means a crash between the registered
    // row and its reply leaves it on disk for the retry; a refusal keeps the
    // pending sidecar and writes no marker.
    let mut current = match intent {
        Some(intent) => intent,
        None => Intent {
            store: store.to_owned(),
            project: project.to_owned(),
            stack,
            deleted_ms,
            attempt_ms: now_ms(),
            state: IntentState::Pending,
            op: None,
        },
    };
    let op = current.op.unwrap_or_else(store_admission::op_id);
    current.op = Some(op);
    if current.state == IntentState::Pending {
        current.attempt_ms = now_ms();
        Intent::write(home, &current)?;
    }
    let admission = match effects.admit(stack, op, project) {
        Ok(admission) => admission,
        Err(refusal) => {
            let _ = effects.release(Admission::new(stack, op));
            return Err(refusal);
        }
    };
    let attempt = mark_and_finish(
        home,
        store,
        project,
        current,
        deleted_ms,
        pending_retry,
        Finish {
            effects: &mut *effects,
            admission,
            fresh,
        },
    );
    let released = effects.release(admission);
    match attempt {
        Ok(outcome) => {
            released?;
            Ok(outcome)
        }
        // The original failure is the reported one; a failed release is named
        // so a leaked admission row is not silent.
        Err(error) => Err(match released {
            Ok(()) => error,
            Err(cause) => SdkRefusal {
                message: format!(
                    "{}; releasing the project admission: {}",
                    error.message, cause.message
                ),
                ..error
            },
        }),
    }
}

/// Removes a directory tree, treating an absent entry as removed.
fn remove_tree(entry: &Path) -> Result<()> {
    match fs::remove_dir_all(entry) {
        Ok(()) => {
            layerfs_home::home::generation::moved_above(entry);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failed(format!("removing {}: {error}", entry.display()))),
    }
}

/// The Store-side opens of `project.delete` in this process.
struct Local {
    project: String,
    open: OpenArgs,
}

impl Effects for Local {
    fn admit(&mut self, stack: LayerStackId, op: [u8; 16], project: &str) -> Result<Admission> {
        let intent = store_admission::project_removal_at(stack, op, Some(project));
        // the removal's admission compares placed mount intents with the
        // caller's place view, as `Remote` sends it
        let open = &self.open;
        crate::admin::route::place_view::scoped(open, || {
            service::with_service(open, |service| {
                match service.store.history().gc_register(&intent) {
                    Ok(()) => Ok(Admission::new(stack, op)),
                    // A provider without admission cannot exclude a removal; the
                    // marker's own admission still runs, as it did before T6.
                    // A real provider's own `Unsupported` refuses (#776 C1).
                    Err(error) if error.no_admission() => Ok(Admission::new(stack, op)),
                    Err(error) => Err(lookup::history_in(
                        error,
                        GcScopeKind::Project,
                        project,
                        None,
                        Some(&op),
                    )),
                }
            })
        })
    }

    fn release(&mut self, admission: Admission) -> Result<()> {
        let project = self.project.clone();
        service::with_service(
            &self.open,
            |service| match store_admission::release_or_mark(service.store.history(), &admission.op)
            {
                Ok(()) | Err(HistoryError::Unsupported(_)) => Ok(()),
                Err(error) => Err(lookup::history_in(
                    error,
                    GcScopeKind::Project,
                    &project,
                    None,
                    Some(&admission.op),
                )),
            },
        )
    }

    fn mark(
        &mut self,
        admission: Admission,
        deleted_ms: i64,
        _pending_retry: bool,
    ) -> Result<Deletion> {
        let intent = store_admission::project_removal_at(
            admission.stack,
            admission.op,
            Some(self.project.as_str()),
        );
        let open = &self.open;
        crate::admin::route::place_view::scoped(open, || {
            service::with_service(open, |service| {
                delete_stack_with(&service.store, admission.stack, deleted_ms, &intent)
            })
        })
    }

    fn remove_entry(&mut self, entry: &Path) -> Result<()> {
        remove_tree(entry)
    }
}

/// The Store-side opens of `project.delete` in a sqlite Store's worker.
///
/// `project.delete(project)`: retires project `project` in its Store and
/// removes the caller's home entry. The marker is the commit point; a home
/// removal failure is success with `home_entry_removed: false`, and the
/// durable intent lets a same-home retry finish it.
pub fn delete(
    home: &Path,
    events: &mut Events<'_>,
    project: &ProjectRef,
    related: crate::users::Related,
) -> Outcome {
    let _ = events;
    let home = home.to_path_buf();
    let store = project.store.as_str();
    let name = project.project.as_str();
    let record = StoreRecord::read(&home, store)?;
    let project_record = ProjectRecord::read(&home, store, name)?;
    // §5: a recorded Sandbox is another user of the project, even with
    // nothing mounted; refuse before the sidecar and the marker.
    if record.is_some() || project_record.is_some() {
        (related.can_remove_project)(&home, project.to_string().as_str())?;
    }
    let stack = project_record.as_ref().and_then(ProjectRecord::stack_id);
    match record {
        Some(record) => {
            record.unblocked()?;
            let store_ref = record.store_ref()?.clone();
            // the caller holds the Store's lease while it sends the call
            // there, as every call that runs in a worker does
            let _lease = lease::hold(&home, store)?;
            match executor::plan(&home, &store_ref)? {
                Executor::Local => {
                    let open = OpenArgs::of_record(&home, store, &record)?.with_project(name);
                    // Resolve the project by the Stack its Store lists now,
                    // never by the home record alone: a fresh delete of a
                    // retired project is `NoSuchProject`, and a stale record
                    // from another home is not evidence (spec §4.2). A retry
                    // resolves itself through its sidecar inside
                    // `delete_with`.
                    let stack = if project_record.is_some() {
                        listed_stack(&open, name)?
                    } else {
                        None
                    };
                    let mut effects = Local {
                        project: name.to_owned(),
                        open,
                    };
                    delete_with(&home, store, name, stack, &mut effects)
                }
                Executor::Worker(worker) => {
                    // a record with no Stack id (an Init through the worker
                    // whose outcome is not known) asks the Store's own list
                    let stack = match (stack, &project_record) {
                        (None, Some(_)) => listed_on_worker(&home, name, &record)?,
                        (stack, _) => stack,
                    };
                    let mut effects = Remote {
                        worker,
                        store: store.to_owned(),
                        project: name.to_owned(),
                        store_record: record.to_record().clone(),
                        place: crate::admin::route::place_view::encoded_from_home(&home),
                    };
                    delete_with(&home, store, name, stack, &mut effects)
                }
            }
        }
        None => Err(refused("NoSuchStore", format!("no Store named {store}"))),
    }
}

/// The project name a worker `project.delete*` op records: the caller's, or
/// the Stack record's when the caller sent none, so a later `Busy` or
/// `Unresolved` reports `{kind: "project", name}` rather than the hex
/// LayerStack id. A Stack record that is already gone leaves the id, which is
/// all the caller left to go on.
fn worker_project(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    project: Option<String>,
) -> Result<Option<String>> {
    if project.is_some() {
        return Ok(project);
    }
    Ok(history
        .layer_stack(stack)
        .map_err(lookup::history)?
        .map(|record| record.name.as_str().to_owned()))
}

/// The `{kind: "Admitted", stack_id}` answer.
fn admitted(stack: LayerStackId) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text("Admitted")),
        ("stack_id", SdkValue::text(hex(&stack.to_bytes()))),
    ])
}

/// The `{kind: "Released", op_id}` answer.
fn released(op: [u8; 16]) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text("Released")),
        ("op_id", SdkValue::text(hex(&op))),
    ])
}

/// The worker's `project.delete.admit`: registers the caller's project-scope
/// admission before the caller writes its sidecar and before the marker, so a
/// live mount refuses with `MountsActive` (its holders reported) and a live
/// operation with `Busy`, with no sidecar written. The registered row names
/// the project from the frame, or from the Stack record when the caller sent
/// no name, so a later `Busy`/`Unresolved` reports `{kind: "project", name}`
/// rather than the hex LayerStack id.
pub fn admit_project(root: &Path, frame: &DeleteAdmitFrame) -> Outcome {
    let store_record = frame.store_record.clone().ok_or_else(|| {
        refused(
            "InvalidInput",
            "project.delete.admit needs the Store's record",
        )
    })?;
    let open = OpenArgs::in_root(root, frame.store.as_str(), Some(store_record));
    service::with_service(&open, |service| {
        let project = worker_project(service.store.history(), frame.stack, frame.project.clone())?;
        let intent = store_admission::project_removal_at(frame.stack, frame.op, project.as_deref());
        match service.store.history().gc_register(&intent) {
            Ok(()) => Ok(admitted(frame.stack)),
            // A provider without admission cannot exclude a removal; the
            // marker still admits, as it did before T6. A real provider's
            // own `Unsupported` refuses (#776 C1).
            Err(error) if error.no_admission() => Ok(admitted(frame.stack)),
            Err(error) => {
                let entity = project.unwrap_or_else(|| hex(frame.stack.as_slice()));
                Err(lookup::history_in(
                    error,
                    GcScopeKind::Project,
                    &entity,
                    None,
                    Some(&frame.op),
                ))
            }
        }
    })
}

/// The worker's `project.delete.release`: deletes the caller's admission row
/// after the marker, on every exit path.
pub fn release_project(root: &Path, frame: &DeleteReleaseFrame) -> Outcome {
    let store_record = frame.store_record.clone().ok_or_else(|| {
        refused(
            "InvalidInput",
            "project.delete.release needs the Store's record",
        )
    })?;
    let open = OpenArgs::in_root(root, frame.store.as_str(), Some(store_record));
    service::with_service(&open, |service| {
        match store_admission::release_or_mark(service.store.history(), &frame.op) {
            Ok(()) | Err(HistoryError::Unsupported(_)) => Ok(released(frame.op)),
            Err(error) => Err(lookup::history(error)),
        }
    })
}

/// The worker's `project.delete` on a Store under `root`: write the marker, remove the worker's own entry and
/// answer `{kind, project, stack_id, deleted_ms}`. The marker reuses the
/// caller's admission operation identity when its frame carries one, so the
/// caller's admitted row is excluded from its own scan; without one the
/// marker takes a fresh identity and its own admission, as before.
pub fn delete_project(root: &Path, frame: &DeleteFrame) -> Outcome {
    let store = frame.store.as_str();
    let store_record = frame
        .store_record
        .clone()
        .ok_or_else(|| refused("InvalidInput", "project.delete needs the Store's record"))?;
    let open = OpenArgs {
        project: frame.project.clone(),
        ..OpenArgs::in_root(root, store, Some(store_record))
    };
    let op = frame.op_id.unwrap_or_else(store_admission::op_id);
    let requested = frame.deleted_ms.unwrap_or_else(now_ms);
    let deletion = service::with_service(&open, |service| {
        let project = worker_project(service.store.history(), frame.stack, frame.project.clone())?;
        let intent = store_admission::project_removal_at(frame.stack, op, project.as_deref());
        delete_stack_with(&service.store, frame.stack, requested, &intent)
    })?;
    let deleted_ms = deletion.deleted_ms(requested);
    let retired = matches!(deletion, Deletion::Retired { .. });
    // A caller retrying a sidecar written before its admission, over a live
    // row with no marker, settles only: it keeps its home entry, so the
    // worker keeps its own copy for the call that finishes the delete.
    let deferred =
        frame.pending_retry.unwrap_or(false) && matches!(deletion, Deletion::Marked { .. });
    let project = if deferred {
        frame.project.clone().unwrap_or_else(|| store.to_owned())
    } else {
        remove_worker_entry(root, store, frame.stack, frame.project.as_deref())?
            .or_else(|| frame.project.clone())
            .unwrap_or_else(|| store.to_owned())
    };
    Ok(DeleteReply {
        project,
        stack: frame.stack,
        deleted_ms,
        retired: Some(retired),
    }
    .encode())
}

/// The worker's `project.delete.entry`: removes the worker's own home entry
/// for a Stack whose marker is already durable, answering
/// `{kind: "Removed", project, stack_id}`. It opens no Store and writes no
/// marker (spec §5.1).
pub fn remove_project_entry(root: &Path, frame: &DeleteEntryFrame) -> Outcome {
    let store = frame.store.as_str();
    let project = remove_worker_entry(root, store, frame.stack, frame.project.as_deref())?
        .or_else(|| frame.project.clone())
        .unwrap_or_else(|| store.to_owned());
    Ok(DeleteEntryReply {
        project,
        stack: frame.stack,
    }
    .encode())
}

/// Removes the worker's own home entry for `stack`; returns the project name
/// it removed, if any.
#[doc(hidden)]
pub fn remove_worker_entry(
    root: &Path,
    store: &str,
    stack: LayerStackId,
    hint: Option<&str>,
) -> Result<Option<String>> {
    let names = match hint {
        Some(name) => vec![name.to_owned()],
        None => paths::names(&paths::store_dir(root, store).join("project")).map_err(failed)?,
    };
    let mut removed = None;
    for name in names {
        let record = paths::project_record(root, store, &name);
        let Some(value) = layerfs_home::home::read_record(&record)? else {
            continue;
        };
        let matches = ProjectRecord::from_record(store, &name, value).stack_id() == Some(stack)
            || name == store;
        if matches {
            remove_tree(&paths::project_dir(root, store, &name))?;
            removed.get_or_insert(name);
        }
    }
    Ok(removed)
}
