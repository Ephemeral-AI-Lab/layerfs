//! The one durable wait store (#555): one JSON entry per exec under
//! `step/<exec>.json`, plus one idempotency index file per key under
//! `step/index/`. A record is replaced whole by the home's atomic
//! write-and-rename, so a reader sees the old entry or the new one, and the
//! index is created with an exclusive link so one key names exactly one exec
//! across processes. No database is added: the store reuses the home's
//! record files.
//!
//! The Store-level receipt trailer is deliberately not used. A trailer and
//! unique index in a Store commit receipt would change the Store's schema and
//! commit path for a wait that is launcher-owned and lives entirely under the
//! home; that is larger than this slice. The index file above is the unique
//! index and the entry is the receipt, both written whole by the home's
//! atomic write-and-rename. No Store change is faked.
use crate::common::home::{self, entity_lock, paths};
use crate::common::json;
use crate::common::outcome::{failed, refused};
use crate::step::contract::{ExecId, ExecReceipt, ExecRequest, ExecStatus};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// How long a transition waits for another process changing the same entry.
const LOCK_WAIT: std::time::Duration = entity_lock::WAIT;

/// What claiming one idempotency key found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Claim {
    /// The key was free; this exec id owns it now.
    New(ExecId),
    /// The key already names this exec; nothing is started again.
    Reused(ExecId),
}

/// What one compare-and-set over a durable entry found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Transition {
    /// The entry changed to this value.
    Changed(Entry),
    /// The entry already held what the caller would have written.
    Unchanged(Entry),
    /// There is no entry for the id.
    Missing,
}

/// The durable wait store, rooted at `step/` in a home (or a test root).
#[derive(Clone, Debug)]
pub struct WaitStore {
    root: PathBuf,
}

impl WaitStore {
    /// A store whose records live under `root`.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The home's own store, `step/`.
    pub fn in_home(home: &Path) -> Self {
        Self::open(paths::step_dir(home))
    }

    /// The root the records live under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The receipt of `id`, or `None` when no entry names it.
    pub fn receipt(&self, id: &ExecId) -> Result<Option<ExecReceipt>, SdkRefusal> {
        Ok(self.read(id)?.map(|entry| entry.receipt()))
    }

    /// Claims `request.key` for a new exec id, or answers the id an earlier
    /// claim made. A retry never runs a second copy: the index file is
    /// created with an exclusive hard link, so exactly one process wins.
    pub(crate) fn claim(&self, request: &ExecRequest) -> Result<Claim, SdkRefusal> {
        let dir = self.index_dir();
        home::private_dir(&dir)?;
        let path = dir.join(format!("{}.json", key_digest(&request.key)));
        let id = ExecId::mint()?;
        if self.write_new(&path, &index_record(request, &id))? {
            return Ok(Claim::New(id));
        }
        self.reuse(&path, request)
    }

    /// Writes `entry` whole; the caller has established that it is the one
    /// entry for the id.
    pub(crate) fn insert(&self, entry: &Entry) -> Result<(), SdkRefusal> {
        home::private_dir(&self.root)?;
        home::write_entity(&self.entry_path(&entry.id), &entry.to_record())
    }

    /// The entry `id` names, if any.
    pub(crate) fn read(&self, id: &ExecId) -> Result<Option<Entry>, SdkRefusal> {
        let path = self.entry_path(id);
        let value = match home::read_record(&path)? {
            Some(value) => value,
            None => return Ok(None),
        };
        Entry::from_record(&value).map(Some).map_err(|mut error| {
            error.message = format!("{}: {}", path.display(), error.message);
            error
        })
    }

    /// Reads the entry under its per-entry lock, lets `change` decide what it
    /// should become, and writes a change whole. Returning `None` from
    /// `change` writes nothing, so the first terminal writer wins.
    pub(crate) fn transition<F>(&self, id: &ExecId, change: F) -> Result<Transition, SdkRefusal>
    where
        F: FnOnce(&Entry) -> Option<Entry>,
    {
        home::private_dir(&self.root)?;
        let held = format!(
            "exec {}'s durable entry was being changed by another process",
            id.as_str()
        );
        let _lock = entity_lock::take(&self.lock_path(id), LOCK_WAIT, &held)?;
        let Some(current) = self.read(id)? else {
            return Ok(Transition::Missing);
        };
        match change(&current) {
            None => Ok(Transition::Unchanged(current)),
            Some(next) => {
                home::write_entity(&self.entry_path(id), &next.to_record())?;
                Ok(Transition::Changed(next))
            }
        }
    }

    /// The path of one entry.
    fn entry_path(&self, id: &ExecId) -> PathBuf {
        self.root.join(format!("{}.json", id.as_str()))
    }

    /// The lock that guards one entry.
    fn lock_path(&self, id: &ExecId) -> PathBuf {
        self.root.join(format!("{}.lock", id.as_str()))
    }

    /// The idempotency index folder.
    fn index_dir(&self) -> PathBuf {
        self.root.join("index")
    }

    /// Answers the claim `path` already holds, or why the retry differs.
    fn reuse(&self, path: &Path, request: &ExecRequest) -> Result<Claim, SdkRefusal> {
        let value = home::read_record(path)?.ok_or_else(|| {
            refused(
                "Failed",
                format!("{} vanished while its claim was read", path.display()),
            )
        })?;
        let key = value.get_text("key").unwrap_or("");
        if key != request.key {
            return Err(refused(
                "Conflict",
                format!(
                    "idempotency key {:?} shares its index digest with another key; \
                     use another key",
                    request.key
                ),
            ));
        }
        let command = value.get_text("command").unwrap_or("");
        let timeout = value
            .get_int("timeout")
            .and_then(|value| u64::try_from(value).ok());
        if command != request.command || timeout != request.timeout_ms {
            return Err(refused(
                "Conflict",
                format!(
                    "idempotency key {:?} already names a different step; a retry never runs \
                     a second copy",
                    request.key
                ),
            ));
        }
        let exec = value.get_text("exec").ok_or_else(|| {
            refused(
                "Failed",
                format!("{} claims a key with no exec id", path.display()),
            )
        })?;
        Ok(Claim::Reused(ExecId::parse(exec)?))
    }

    /// Writes `value` to `path` only if no file is there: a private
    /// temporary file, synced, then linked under the final name. The link is
    /// the unique index; `Ok(false)` means another process won.
    fn write_new(&self, path: &Path, value: &SdkValue) -> Result<bool, SdkRefusal> {
        let mut text = json::print(&home::versioned(value));
        text.push('\n');
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("index");
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_file_name(format!(".{name}.{}.{unique}.tmp", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(failed)?;
        let written = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = written {
            let _ = fs::remove_file(&temporary);
            return Err(failed(error));
        }
        let linked = fs::hard_link(&temporary, path);
        let _ = fs::remove_file(&temporary);
        match linked {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(failed(error)),
        }
    }
}

/// One durable exec entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    id: ExecId,
    key: String,
    command: String,
    timeout_ms: Option<u64>,
    status: ExecStatus,
    handle: Option<SdkValue>,
    started: i64,
    updated: i64,
    deadline: Option<i64>,
}

impl Entry {
    /// The entry of a started exec.
    pub(crate) fn running(
        id: &ExecId,
        request: &ExecRequest,
        handle: SdkValue,
        started: i64,
        deadline: Option<i64>,
    ) -> Self {
        Self {
            id: id.clone(),
            key: request.key.clone(),
            command: request.command.clone(),
            timeout_ms: request.timeout_ms,
            status: ExecStatus::Running,
            handle: Some(handle),
            started,
            updated: started,
            deadline,
        }
    }

    /// The entry of an exec whose start could not be established.
    pub(crate) fn unresolved(id: &ExecId, request: &ExecRequest, now: i64) -> Self {
        Self {
            id: id.clone(),
            key: request.key.clone(),
            command: request.command.clone(),
            timeout_ms: request.timeout_ms,
            status: ExecStatus::Unresolved,
            handle: None,
            started: now,
            updated: now,
            deadline: None,
        }
    }

    /// This entry moved to terminal `status` at `now`.
    pub(crate) fn terminal(&self, status: ExecStatus, now: i64) -> Self {
        Self {
            status,
            updated: now,
            ..self.clone()
        }
    }

    /// Its status.
    pub(crate) fn status(&self) -> ExecStatus {
        self.status
    }

    /// Its durable handle, or `Null` when it has none.
    pub(crate) fn handle(&self) -> SdkValue {
        self.handle.clone().unwrap_or(SdkValue::Null)
    }

    /// Whether its deadline passed at `now`.
    pub(crate) fn deadline_passed(&self, now: i64) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
    }

    /// The receipt a caller reads.
    pub(crate) fn receipt(&self) -> ExecReceipt {
        ExecReceipt {
            exec_id: self.id.clone(),
            key: self.key.clone(),
            command: self.command.clone(),
            timeout_ms: self.timeout_ms,
            status: self.status,
            started: self.started,
            updated: self.updated,
        }
    }

    /// The record this entry is stored as (the home adds `v`).
    pub(crate) fn to_record(&self) -> SdkValue {
        let int = |value: i64| SdkValue::Int(value);
        let opt_int = |value: Option<i64>| value.map_or(SdkValue::Null, SdkValue::Int);
        SdkValue::Map(vec![
            ("exec".to_owned(), SdkValue::text(self.id.as_str())),
            ("key".to_owned(), SdkValue::text(&self.key)),
            ("command".to_owned(), SdkValue::text(&self.command)),
            (
                "timeout".to_owned(),
                self.timeout_ms.map_or(SdkValue::Null, |value| {
                    SdkValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
                }),
            ),
            ("status".to_owned(), SdkValue::text(self.status.wire())),
            (
                "exit".to_owned(),
                self.status
                    .exit_code()
                    .map_or(SdkValue::Null, |code| SdkValue::Int(i64::from(code))),
            ),
            ("handle".to_owned(), self.handle()),
            ("started".to_owned(), int(self.started)),
            ("updated".to_owned(), int(self.updated)),
            ("deadline".to_owned(), opt_int(self.deadline)),
        ])
    }

    /// The entry a record holds, or why it is not one.
    pub(crate) fn from_record(value: &SdkValue) -> Result<Self, SdkRefusal> {
        let text = |name: &str| {
            value
                .get_text(name)
                .map(str::to_owned)
                .ok_or_else(|| refused("Failed", format!("a step entry has no {name}")))
        };
        let number = |name: &str| {
            value
                .get_int(name)
                .ok_or_else(|| refused("Failed", format!("a step entry has no {name}")))
        };
        let optional = |name: &str| value.get_int(name).and_then(|n| u64::try_from(n).ok());
        let handle = match value.get("handle") {
            None | Some(SdkValue::Null) => None,
            Some(handle) => Some(handle.clone()),
        };
        Ok(Self {
            id: ExecId::parse(&text("exec")?)?,
            key: text("key")?,
            command: text("command")?,
            timeout_ms: optional("timeout"),
            status: ExecStatus::from_wire(
                &text("status")?,
                value
                    .get_int("exit")
                    .and_then(|code| i32::try_from(code).ok()),
            )?,
            handle,
            started: number("started")?,
            updated: number("updated")?,
            deadline: value.get_int("deadline"),
        })
    }
}

/// The index file a claimed key holds.
fn index_record(request: &ExecRequest, id: &ExecId) -> SdkValue {
    SdkValue::Map(vec![
        ("key".to_owned(), SdkValue::text(&request.key)),
        ("exec".to_owned(), SdkValue::text(id.as_str())),
        ("command".to_owned(), SdkValue::text(&request.command)),
        (
            "timeout".to_owned(),
            request.timeout_ms.map_or(SdkValue::Null, |value| {
                SdkValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
            }),
        ),
    ])
}

/// A stable filename for one idempotency key: two independent 64-bit
/// FNV-1a lanes, 32 hex digits. The file keeps the whole key, so a digest
/// collision is detected and refused, never served as another key.
fn key_digest(key: &str) -> String {
    fn lane(seed: u64, bytes: &[u8]) -> u64 {
        let mut hash = seed;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
    format!(
        "{:016x}{:016x}",
        lane(0xcbf2_9ce4_8422_2325, key.as_bytes()),
        lane(0x8422_2325_cbf2_9ce4, key.as_bytes())
    )
}
