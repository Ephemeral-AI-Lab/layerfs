//! `project.delete`: a project is retired in its Store and its home entry is
//! removed (spec §4, §4.2, §5.1 and §10).
//!
//! The marker is the commit point. The caller first saves the legacy
//! own-project identity, then writes the durable intent
//! `store/<store>/delete/<project>.json` with
//! `{store, project, stack_id, deleted_ms, attempt_ms, state}`, then marks the
//! Stack, then removes only its own home entry and unlinks the intent. A
//! retry reads the intent by the recorded Stack id, reuses its `deleted_ms`
//! and finishes the entry: `Marked` means this attempt marked a live Stack,
//! `Retired` means the marker was already there (including a hidden Store
//! project whose home entry is stale), and `Absent` means no live Stack is
//! there, so the stale entry goes. A lost worker reply is `Unknown`; the
//! intent stays and the retry classifies the Stack again. A home record whose
//! Stack differs from the intent is a project made again under the same name:
//! the stale intent is discarded and this call starts over against the record
//! that is there now.
//!
//! The worker op is `project.delete`; its frame is
//! `{store, stack_id, deleted_ms?, project?, op_id?, store_record?}`. The two
//! optional context fields are the ones the retired `project.remove` call sent: the
//! worker opens the Store under its own root with the caller's `store_record`,
//! and the project name lets its reply and its own home entry name the
//! project. The worker saves the legacy own-project identity, writes the
//! marker, removes its own entry and replies `{kind, project, stack_id,
//! deleted_ms}`; the caller removes its own entry and echoes
//! `home_entry_removed`.
//!
//! Before any of that, the home path admits the project scope (T4.6,
//! `project.delete.admit` on the worker route): the same `gc_register` the
//! Store-side removals rely on, taken before the identity save, the sidecar
//! and the marker. A live mount refuses `MountsActive` with its holders and a
//! live operation `Busy`, with no sidecar written and no marker. The marker
//! reuses the admission's operation identity, and the caller releases it on
//! every exit path (`project.delete.release`), so a refused or failed call
//! leaves no admission row behind.
use crate::open_args::OpenArgs;
use crate::place::{self, Executor, Worker};
use crate::project::frame::{DeleteAdmitFrame, DeleteFrame, DeleteReleaseFrame, DeleteReply};
use crate::project::record::ProjectRecord;
use crate::record::{StoreRecord, StoreRef};
use crate::{identity, lease, service};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_bridge::sdk_values::{hex, unhex};
use layerfs_history::HistoryError;
use layerfs_history::LayerStackId;
use layerfs_home::events::Events;
use layerfs_home::home::{now_ms, paths};
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_store_service::history::admission as store_admission;
use layerfs_store_service::history::deletion::{delete_stack_with, Deletion};
use layerfs_store_service::history::lookup;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Saves the legacy own-project identity before the intent and the
    /// marker; idempotent on a retry.
    fn prepare(&mut self) -> Result<()>;

    /// Takes the project-scope admission before the sidecar and the marker:
    /// a live mount is `MountsActive` with its holders, a live operation
    /// `Busy`, an uncertain owner `Unresolved`. A refusal registers nothing.
    /// `reuse` is the identity the sidecar recorded for a retry; the same row
    /// is re-registered under it, so a lost reply is settled, never met as a
    /// second operation.
    fn admit(&mut self, stack: LayerStackId, reuse: Option<[u8; 16]>) -> Result<Admission>;

    /// Releases `admission` on every exit path: success, refusal and an
    /// unknown Store outcome. A provider without admission releases nothing.
    fn release(&mut self, admission: Admission) -> Result<()>;

    /// Marks (or reclassifies) `admission`'s Stack, reusing the admission's
    /// operation identity. A retry passes the intent's `deleted_ms`.
    fn mark(&mut self, admission: Admission, deleted_ms: i64) -> Result<Deletion>;

    /// Removes the caller's home entry; `Ok(())` when nothing remains.
    fn remove_entry(&mut self, entry: &Path) -> Result<()>;
}

/// The `project` part of a Stack record from a home record: the `manifest`'s
/// Stack for a local sqlite record, the record's `stack` for a PostgreSQL or
/// worker record.
fn stack_of_value(value: &SdkValue) -> Option<LayerStackId> {
    if let Some(SdkValue::Bytes(bytes)) = value.get("manifest") {
        if let Ok(manifest) = StoreManifest::decode(bytes) {
            if let Ok(stack) = LayerStackId::from_slice(&manifest.stack) {
                return Some(stack);
            }
        }
    }
    value
        .get_text("stack")
        .and_then(unhex)
        .and_then(|bytes| LayerStackId::from_slice(&bytes).ok())
}

/// The Stack a home record names, if it names one.
fn stack_of(record: &ProjectRecord) -> Option<LayerStackId> {
    stack_of_value(record.to_record())
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

/// The mutating part of `delete_with` after the project scope is admitted:
/// the identity save, the sidecar, the marker and the entry removal (spec §4).
fn mark_and_finish(
    home: &Path,
    store: &str,
    project: &str,
    stack: LayerStackId,
    intent: Option<Intent>,
    mut deleted_ms: i64,
    effects: &mut impl Effects,
    admission: Admission,
) -> Outcome {
    effects.prepare()?;
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
    // the sidecar names the admission, so a retry reuses it
    current.op = Some(admission.op);
    if current.state == IntentState::Pending {
        current.attempt_ms = now_ms();
        Intent::write(home, &current)?;
    }

    let deletion = effects.mark(admission, deleted_ms)?;
    deleted_ms = deletion.deleted_ms(deleted_ms);
    current.deleted_ms = deleted_ms;
    current.state = IntentState::Marked;
    current.attempt_ms = now_ms();
    Intent::write(home, &current)?;

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
/// `stack` is the Stack id resolved from the record or the Store listing; a
/// record without one whose Store does not list it is a stale entry and is
/// removed without a marker. The same function serves the local and worker
/// routes, and a fake in tests.
///
/// The project scope is admitted before the identity save, the sidecar and
/// the marker: a live mount is `MountsActive` with its holders and a live
/// operation `Busy`, reported with no sidecar written and no marker. The
/// admission is released on every exit path.
pub fn delete_with(
    home: &Path,
    store: &str,
    project: &str,
    stack: Option<LayerStackId>,
    effects: &mut impl Effects,
) -> Outcome {
    let record = ProjectRecord::read(home, store, project)?;
    let record_stack = record.as_ref().and_then(stack_of);
    let mut intent = Intent::read(home, store, project)?;

    // A different Stack id for the same name is a project made again: the
    // stale intent must not touch its entry, so it is discarded and this
    // call starts over against the record that is there now.
    if let (Some(stale), Some(current)) = (&intent, record_stack) {
        if stale.stack != current {
            Intent::discard(home, store, project)?;
            intent = None;
        }
    }

    let (stack, deleted_ms) = match &intent {
        Some(intent) => (intent.stack, intent.deleted_ms),
        None => {
            let Some(stack) = stack.or(record_stack) else {
                if record.is_none() {
                    return Err(refused(
                        "NoSuchProject",
                        format!("no project {store}/{project}"),
                    ));
                }
                // An untracked stale entry: save a legacy identity before it
                // goes, then remove it. There is no Stack id to mark.
                effects.prepare()?;
                let deleted_ms = now_ms();
                let removed_entry = effects
                    .remove_entry(&paths::project_dir(home, store, project))
                    .is_ok();
                return Ok(removed(store, project, deleted_ms, removed_entry));
            };
            (stack, now_ms())
        }
    };

    if let Some(done) = &intent {
        if done.state == IntentState::Done {
            // The retired-row entry-removal retry needs no admission: the
            // marker is already there and only the home entry is removed.
            effects.prepare()?;
            let removed_entry = effects
                .remove_entry(&paths::project_dir(home, store, project))
                .is_ok();
            let mut settled = done.clone();
            settled.attempt_ms = now_ms();
            let _ = Intent::write(home, &settled);
            let _ = Intent::unlink(home, store, project);
            return Ok(removed(store, project, deleted_ms, removed_entry));
        }
    }

    // Admission before the identity save, the sidecar and the marker. A
    // refusal returns here, so neither the sidecar nor the marker is written.
    // A retry re-registers the identity its sidecar recorded; a refusal then
    // releases that identity, never adopting it as a live operation.
    let reuse = intent.as_ref().and_then(|intent| intent.op);
    let admission = match effects.admit(stack, reuse) {
        Ok(admission) => admission,
        Err(refusal) => {
            if let Some(op) = reuse {
                let _ = effects.release(Admission::new(stack, op));
            }
            return Err(refusal);
        }
    };
    let attempt = mark_and_finish(
        home, store, project, stack, intent, deleted_ms, effects, admission,
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
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failed(format!("removing {}: {error}", entry.display()))),
    }
}

/// The Store-side opens of `project.delete` in this process.
struct Local {
    home: PathBuf,
    store: String,
    project: String,
    open: OpenArgs,
}

impl Effects for Local {
    fn prepare(&mut self) -> Result<()> {
        if self.project != self.store || !identity::legacy(&self.home, &self.store)? {
            return Ok(());
        }
        let id = identity::identity(&self.open)?;
        layerfs_home::home::private_dir(&identity::store_dir(&self.home, &self.store)?)?;
        identity::save_record(&self.home, &self.store, &id)
    }

    fn admit(&mut self, stack: LayerStackId, reuse: Option<[u8; 16]>) -> Result<Admission> {
        let intent = match reuse {
            Some(op) => store_admission::project_removal_at(stack, op),
            None => store_admission::project_removal_from_id(stack),
        };
        let op = intent.op_id;
        service::with_service(&self.open, |service| {
            match service.store.history().gc_register(&intent) {
                Ok(()) => Ok(Admission::new(stack, op)),
                // A provider without admission cannot exclude a removal; the
                // marker's own admission still runs, as it did before T6.
                Err(HistoryError::Unsupported(_)) => Ok(Admission::new(stack, op)),
                Err(error) => Err(lookup::history(error)),
            }
        })
    }

    fn release(&mut self, admission: Admission) -> Result<()> {
        service::with_service(&self.open, |service| {
            match service.store.history().gc_release(&admission.op) {
                Ok(()) | Err(HistoryError::Unsupported(_)) => Ok(()),
                Err(error) => Err(lookup::history(error)),
            }
        })
    }

    fn mark(&mut self, admission: Admission, deleted_ms: i64) -> Result<Deletion> {
        let intent = store_admission::project_removal_at(admission.stack, admission.op);
        service::with_service(&self.open, |service| {
            delete_stack_with(&service.store, admission.stack, deleted_ms, &intent)
        })
    }

    fn remove_entry(&mut self, entry: &Path) -> Result<()> {
        remove_tree(entry)
    }
}

/// The Store-side opens of `project.delete` in a sqlite Store's worker.
struct Remote {
    worker: Worker,
    store: String,
    project: String,
    store_record: SdkValue,
}

impl Effects for Remote {
    fn prepare(&mut self) -> Result<()> {
        // the worker saves its own legacy identity before its marker
        Ok(())
    }

    fn admit(&mut self, stack: LayerStackId, reuse: Option<[u8; 16]>) -> Result<Admission> {
        let op = reuse.unwrap_or_else(store_admission::op_id);
        let frame = DeleteAdmitFrame {
            store: StoreName::parse(&self.store)?,
            stack,
            op,
            store_record: Some(self.store_record.clone()),
        };
        let answer = match self
            .worker
            .call("project.delete.admit", frame.encode(), &mut |_| {})
        {
            Ok(answer) => answer,
            Err(refusal) => {
                // The request went out, so the worker may have registered the
                // row before its answer was lost: settle the identity before
                // reporting the unknown outcome.
                if refusal.code == "Unknown" {
                    let _ = self.release(Admission::new(stack, op));
                }
                return Err(refusal);
            }
        };
        if let Err(refusal) = admit_reply(&answer, stack) {
            let _ = self.release(Admission::new(stack, op));
            return Err(refusal);
        }
        Ok(Admission::new(stack, op))
    }

    fn release(&mut self, admission: Admission) -> Result<()> {
        let frame = DeleteReleaseFrame {
            store: StoreName::parse(&self.store)?,
            op: admission.op,
            store_record: Some(self.store_record.clone()),
        };
        self.worker
            .call("project.delete.release", frame.encode(), &mut |_| {})?;
        Ok(())
    }

    fn mark(&mut self, admission: Admission, deleted_ms: i64) -> Result<Deletion> {
        let frame = DeleteFrame {
            store: StoreName::parse(&self.store)?,
            project: Some(self.project.clone()),
            stack: admission.stack,
            deleted_ms: Some(deleted_ms),
            op_id: Some(admission.op),
            store_record: Some(self.store_record.clone()),
        };
        let answer = self
            .worker
            .call("project.delete", frame.encode(), &mut |_| {})?;
        let reply = DeleteReply::decode(&answer)?;
        if reply.stack != admission.stack {
            return Err(refused(
                "Failed",
                "the worker answered for another LayerStack",
            ));
        }
        Ok(Deletion::Marked {
            deleted_ms: reply.deleted_ms,
        })
    }

    fn remove_entry(&mut self, entry: &Path) -> Result<()> {
        remove_tree(entry)
    }
}

/// Checks the worker's `project.delete.admit` answer.
fn admit_reply(answer: &SdkValue, stack: LayerStackId) -> Result<()> {
    if answer.get_text("kind") != Some("Admitted") {
        return Err(refused(
            "Failed",
            "the worker project.delete admission is not `Admitted`",
        ));
    }
    let answered = answer
        .get_text("stack_id")
        .and_then(unhex)
        .and_then(|bytes| LayerStackId::from_slice(&bytes).ok());
    if answered != Some(stack) {
        return Err(refused("Failed", "the worker admitted another LayerStack"));
    }
    Ok(())
}

/// The `OpenArgs` a local Store call opens with. `record` is absent for a
/// Store made by its own project Init, whose identity is the project entry.
fn open_args(
    home: &Path,
    store: &str,
    project: &str,
    record: Option<&StoreRecord>,
) -> Result<OpenArgs> {
    let (store_path, postgresql) = match record {
        Some(record) => match record.store_ref()? {
            StoreRef::Postgres { .. } => (
                record.postgresql_key(home, store),
                Some(crate::postgresql::connect::selection(record)?),
            ),
            StoreRef::Sqlite { .. } => (paths::store_file(home, store), None),
        },
        None => (paths::store_file(home, store), None),
    };
    Ok(OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(store.to_owned()),
        project: Some(project.to_owned()),
        store_path: Some(store_path.display().to_string()),
        store_record: record.map(|record| record.to_record().clone()),
        postgresql,
    })
}

/// The Stack id a Store lists for project `name`, for a record that does
/// not name one (spec §4.2 fills it from the filtered listing by name).
fn listed_stack(open: &OpenArgs, name: &str) -> Result<Option<LayerStackId>> {
    service::with_service(open, |service| {
        let stacks = service
            .store
            .published(layerfs_project::store::projects)
            .map_err(failed)?;
        Ok(stacks
            .into_iter()
            .find(|stack| stack.name.as_str() == name)
            .map(|stack| stack.id))
    })
}

/// `project.delete(project)`: retires project `project` in its Store and
/// removes the caller's home entry. The marker is the commit point; a home
/// removal failure is success with `home_entry_removed: false`, and the
/// durable intent lets a same-home retry finish it.
pub fn delete(home: &Path, events: &mut Events<'_>, project: &ProjectRef) -> Outcome {
    let _ = events;
    let home = home.to_path_buf();
    let store = project.store.as_str();
    let name = project.project.as_str();
    let record = StoreRecord::read(&home, store)?;
    let project_record = ProjectRecord::read(&home, store, name)?;
    let stack = project_record.as_ref().and_then(stack_of);
    match record {
        Some(record) => {
            record.unblocked()?;
            let store_ref = record.store_ref()?.clone();
            // the caller holds the Store's lease while it sends the call
            // there, as every call that runs in a worker does
            let _lease = lease::hold(&home, store)?;
            match place::plan(&home, &store_ref)? {
                Executor::Local => {
                    let open = open_args(&home, store, name, Some(&record))?;
                    let stack = match stack {
                        Some(stack) => Some(stack),
                        None if project_record.is_some() => listed_stack(&open, name)?,
                        None => None,
                    };
                    let mut effects = Local {
                        home: home.clone(),
                        store: store.to_owned(),
                        project: name.to_owned(),
                        open,
                    };
                    delete_with(&home, store, name, stack, &mut effects)
                }
                Executor::Worker(worker) => {
                    if stack.is_none() && project_record.is_some() {
                        return Err(refused(
                            "Unsupported",
                            format!(
                                "project {store}/{name} has no recorded LayerStack id and its Store is on another machine; \
                                 list it on that machine first"
                            ),
                        ));
                    }
                    let mut effects = Remote {
                        worker,
                        store: store.to_owned(),
                        project: name.to_owned(),
                        store_record: record.to_record().clone(),
                    };
                    delete_with(&home, store, name, stack, &mut effects)
                }
            }
        }
        None => {
            // A Store made by its own project's Init has no Store record: its
            // identity lives in the project entry, and saving it before that
            // entry goes is exactly the legacy own-project identity save.
            if name != store || project_record.is_none() {
                return Err(refused("NoSuchStore", format!("no Store named {store}")));
            }
            let open = open_args(&home, store, name, None)?;
            let stack = match stack {
                Some(stack) => Some(stack),
                None => listed_stack(&open, name)?,
            };
            let mut effects = Local {
                home: home.clone(),
                store: store.to_owned(),
                project: name.to_owned(),
                open,
            };
            delete_with(&home, store, name, stack, &mut effects)
        }
    }
}

/// The `OpenArgs` a worker's `project.delete` ops open the Store with: the
/// Store file under the worker's own root and the caller's Store record.
fn worker_open(
    root: &Path,
    store: &str,
    project: Option<String>,
    store_record: SdkValue,
) -> OpenArgs {
    OpenArgs {
        home: Some(root.to_path_buf()),
        store: Some(store.to_owned()),
        project,
        store_path: Some(paths::store_file(root, store).display().to_string()),
        store_record: Some(store_record),
        postgresql: None,
    }
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
/// operation with `Busy`, with no sidecar written.
pub fn admit_project(root: &Path, frame: &DeleteAdmitFrame) -> Outcome {
    let store_record = frame.store_record.clone().ok_or_else(|| {
        refused(
            "InvalidInput",
            "project.delete.admit needs the Store's record",
        )
    })?;
    let open = worker_open(root, frame.store.as_str(), None, store_record);
    let intent = store_admission::project_removal_at(frame.stack, frame.op);
    service::with_service(&open, |service| {
        match service.store.history().gc_register(&intent) {
            Ok(()) => Ok(admitted(frame.stack)),
            // A provider without admission cannot exclude a removal; the
            // marker still admits, as it did before T6.
            Err(HistoryError::Unsupported(_)) => Ok(admitted(frame.stack)),
            Err(error) => Err(lookup::history(error)),
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
    let open = worker_open(root, frame.store.as_str(), None, store_record);
    service::with_service(&open, |service| {
        match service.store.history().gc_release(&frame.op) {
            Ok(()) | Err(HistoryError::Unsupported(_)) => Ok(released(frame.op)),
            Err(error) => Err(lookup::history(error)),
        }
    })
}

/// The worker's `project.delete` on a Store under `root`: save the legacy
/// own-project identity, write the marker, remove the worker's own entry and
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
    let open = worker_open(root, store, frame.project.clone(), store_record);
    // The legacy own-project entry carries the Store's identity until the
    // marker; save it first, idempotently.
    let own = frame.project.as_deref() == Some(store) && identity::legacy(root, store)?;
    if own {
        let id = identity::identity(&open)?;
        layerfs_home::home::private_dir(&identity::store_dir(root, store)?)?;
        identity::save_record(root, store, &id)?;
    }
    let op = frame.op_id.unwrap_or_else(store_admission::op_id);
    let intent = store_admission::project_removal_at(frame.stack, op);
    let requested = frame.deleted_ms.unwrap_or_else(now_ms);
    let deletion = service::with_service(&open, |service| {
        delete_stack_with(&service.store, frame.stack, requested, &intent)
    })?;
    let deleted_ms = deletion.deleted_ms(requested);
    let project = remove_worker_entry(root, store, frame.stack, frame.project.as_deref())?
        .or_else(|| frame.project.clone())
        .unwrap_or_else(|| store.to_owned());
    Ok(DeleteReply {
        project,
        stack: frame.stack,
        deleted_ms,
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
        let matches = stack_of_value(&value) == Some(stack) || name == store;
        if matches {
            remove_tree(&paths::project_dir(root, store, &name))?;
            removed.get_or_insert(name);
        }
    }
    Ok(removed)
}
