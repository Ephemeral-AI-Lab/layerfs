//! A PostgreSQL Store's host read session (#65): the one read-only session
//! a process holds for a Sandbox's host operations on a PostgreSQL Store.
//! A create's reach check reads the schema's binding and finds the project
//! on it; a status finds its Branch's head on it, on every route: through a
//! machine's worker, in this process, and straight to a forwarded
//! Sandbox's daemon ([`branch`]). A mount reads nothing here: its daemon
//! finds the Branch by name (#64). The full Store service
//! (writer, control reader, overlay template) opens only for Store calls
//! ([`crate::open`]).
//!
//! It opens with the light check (#63) under the binding the schema
//! records, keyed by the Store's path and the URL it dials, when a call
//! first reads; a call that reads nothing on the host (exec, Commit,
//! unmount, close) opens nothing. The caller's lease covers each call; the
//! session itself holds none. The same reaper closes it after
//! [`crate::open::IDLE`] unused, and [`forget`] when the Store closes. A held session is checked with one empty query before a call
//! uses it; one that does not answer is dropped and a new one opens, so no
//! call runs on a broken connection and none is retried.
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
#[cfg(feature = "postgresql")]
use layerfs_history::HistoryCatalog as _;
#[cfg(feature = "postgresql")]
use layerfs_home::outcome::failed;
use layerfs_home::outcome::refused;
use std::path::Path;
#[cfg(feature = "postgresql")]
use std::path::PathBuf;

type Result<T> = std::result::Result<T, SdkRefusal>;

#[cfg(feature = "postgresql")]
mod held {
    use super::*;
    use layerfs_persistence::{PostgresCheck, PostgresHandles};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    struct Held {
        handles: Arc<PostgresHandles>,
        last: Instant,
    }

    type Key = (PathBuf, String);
    static HELD: Mutex<BTreeMap<Key, Held>> = Mutex::new(BTreeMap::new());

    fn key(key: &Path, postgresql: &SdkValue) -> Result<Key> {
        let probe = SdkValue::map([("postgresql", postgresql.clone())]);
        Ok((
            key.to_path_buf(),
            crate::postgresql::open::url(&probe)?.to_owned(),
        ))
    }

    /// A new session dialled with `postgresql`; `Ok(None)` when the schema
    /// there records another binding than `record`'s.
    fn open(record: &StoreRecord, postgresql: &SdkValue) -> Result<Option<PostgresHandles>> {
        let probe = SdkValue::map([("postgresql", postgresql.clone())]);
        let url = crate::postgresql::open::url(&probe)?;
        let schema = crate::postgresql::open::schema(&probe)?;
        let sync = crate::postgresql::open::setting(&probe)?;
        // the server is reached first, as the reach check always did; a
        // record without a full identity is refused after, unless its
        // binding already differs (the cursor key pages nothing here)
        let identity = crate::identity::identity_in(record);
        let cursor = identity
            .as_ref()
            .map_or([1; 32], |identity| identity.cursor_key);
        let selection = crate::postgresql::open::selection(url, &schema, &sync)
            .with_check(PostgresCheck::Light);
        let (binding, handles) =
            PostgresHandles::attach_read_only(&selection, cursor).map_err(|error| {
                crate::postgresql::open::no_session(&error).unwrap_or_else(|| {
                    refused("Failed", format!("reading schema {schema}: {error}"))
                })
            })?;
        if record.binding() != Some(binding.as_slice()) {
            return Ok(None);
        }
        identity?;
        Ok(Some(handles))
    }

    fn insert(id: Key, handles: Arc<PostgresHandles>) {
        crate::open::reaper();
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        held.insert(
            id,
            Held {
                handles,
                last: Instant::now(),
            },
        );
    }

    /// The held session, checked with one empty query, or a new one.
    pub fn session(
        key_path: &Path,
        record: &StoreRecord,
        postgresql: &SdkValue,
    ) -> Result<Option<Arc<PostgresHandles>>> {
        let id = key(key_path, postgresql)?;
        let kept = HELD
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
            .map(|held| {
                held.last = Instant::now();
                held.handles.clone()
            });
        if let Some(handles) = kept {
            if handles.storage.usable() {
                return Ok(Some(handles));
            }
            HELD.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        }
        let Some(handles) = open(record, postgresql)? else {
            return Ok(None);
        };
        let handles = Arc::new(handles);
        insert(id, handles.clone());
        Ok(Some(handles))
    }

    pub fn close_idle(idle: Duration) -> usize {
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        let before = held.len();
        held.retain(|_, held| held.last.elapsed() < idle);
        before - held.len()
    }

    pub fn forget(key: &Path) {
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        held.retain(|(path, _), _| path != key);
    }
}

#[cfg(feature = "postgresql")]
fn another(store: &str) -> SdkRefusal {
    refused(
        "Unavailable",
        format!("the PostgreSQL server holds another Store than {store}"),
    )
}

#[cfg(not(feature = "postgresql"))]
fn unbuilt() -> SdkRefusal {
    refused(
        "Unsupported",
        "this LayerFS build has no PostgreSQL engine; build the binding with --features postgresql",
    )
}

/// The schema's binding was another Store's.
pub struct OtherStore;

/// Whether the server `postgresql` names holds the Store `record` describes
/// and the project `project` (#65: a create's reach check, its binding
/// check and project lookup on one session): `Err(OtherStore)` inside when
/// its schema records another binding, `Ok(false)` when it holds no such
/// project. A refusal says why no session was had.
pub fn reach(
    key: &Path,
    record: &StoreRecord,
    postgresql: &SdkValue,
    project: &str,
) -> Result<std::result::Result<bool, OtherStore>> {
    #[cfg(feature = "postgresql")]
    {
        let Some(handles) = held::session(key, record, postgresql)? else {
            return Ok(Err(OtherStore));
        };
        let found = layerfs_store_service::history::lookup::project_in(project, |name| {
            handles
                .history
                .layer_stack_named(name)
                .map_err(layerfs_store_service::history::lookup::history)
        });
        match found {
            Ok(_) => Ok(Ok(true)),
            Err(refusal) if refusal.code == "NoSuchProject" => Ok(Ok(false)),
            Err(refusal) => Err(refusal),
        }
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (key, record, postgresql, project);
        Err(unbuilt())
    }
}

/// The id and head of Branch `name` of project `project`, read on this
/// process's held session of the PostgreSQL Store `record` keyed by `key`,
/// dialled with `postgresql` (a forwarded Sandbox's status reads its head
/// here, not through a Store service): one light-checked connection
/// at most, and none when this process already holds a usable one. The
/// refusals are the Store service's lookup's.
pub(crate) fn branch(
    key: &Path,
    record: &StoreRecord,
    postgresql: &SdkValue,
    (project, name): (&str, &str),
) -> Result<(layerfs_history::BranchId, Option<layerfs_history::CommitId>)> {
    #[cfg(feature = "postgresql")]
    {
        let handles =
            held::session(key, record, postgresql)?.ok_or_else(|| another(record.name()))?;
        use layerfs_store_service::history::lookup;
        let stack = lookup::project_in(project, |wanted| {
            (handles.history.layer_stack_named(wanted)).map_err(lookup::history)
        })?;
        let branch = lookup::branch_in(name, |wanted| {
            (handles.history.branch_named(stack.id, wanted)).map_err(lookup::history)
        })?;
        Ok((branch.id, branch.head_commit))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (key, record, postgresql, project, name);
        Err(unbuilt())
    }
}

/// The newest Commit of `branch`, read on the held session; `None` when it
/// has none.
pub fn head(
    args: &SdkValue,
    branch: layerfs_history::BranchId,
) -> Result<Option<layerfs_history::CommitId>> {
    #[cfg(feature = "postgresql")]
    {
        let page = held_of(args)?
            .history
            .commit_history(&layerfs_history::CommitHistoryRequest {
                branch,
                start: None,
                cursor: None,
                limit: 1,
            })
            .map_err(failed)?;
        Ok(page.records.first().map(|record| record.id))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (args, branch);
        Err(unbuilt())
    }
}

/// The held session of the call's Store, opened if this process holds none.
#[cfg(feature = "postgresql")]
fn held_of(args: &SdkValue) -> Result<std::sync::Arc<layerfs_persistence::PostgresHandles>> {
    let key = PathBuf::from(args.get_text("storePath").unwrap_or(""));
    let store = args.get_text("store").unwrap_or("");
    let record =
        StoreRecord::from_record(store, args.get("store_record").cloned().unwrap_or_default());
    let postgresql = args.get("postgresql").ok_or_else(|| {
        refused(
            "InvalidInput",
            "a PostgreSQL Sandbox call carries its server's resolved postgresql value",
        )
    })?;
    held::session(&key, &record, postgresql)?.ok_or_else(|| another(store))
}

/// Closes every held session unused for at least `idle`; how many.
pub fn close_idle(idle: std::time::Duration) -> usize {
    #[cfg(feature = "postgresql")]
    return held::close_idle(idle);
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = idle;
        0
    }
}

/// Closes the held sessions of the Store at `key` (its disconnect or move).
pub fn forget(key: &Path) {
    #[cfg(feature = "postgresql")]
    held::forget(key);
    #[cfg(not(feature = "postgresql"))]
    let _ = key;
}
