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
use crate::admin::open::args::{OpenArgs, PgSelection};
use crate::admin::record::store::StoreRecord;
use layerfs_history::HistoryCatalog as _;
use layerfs_home::outcome::failed;
use layerfs_home::outcome::refused;
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, SdkRefusal>;

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

    fn key(key: &Path, postgresql: &PgSelection) -> Result<Key> {
        Ok((key.to_path_buf(), postgresql.url()?.to_owned()))
    }

    /// A new session dialled with `postgresql`; `Ok(None)` when the schema
    /// there records another binding than `record`'s.
    fn open(record: &StoreRecord, postgresql: &PgSelection) -> Result<Option<PostgresHandles>> {
        let url = postgresql.url()?;
        let schema = postgresql.schema()?;
        let sync = postgresql.setting()?;
        // the server is reached first, as the reach check always did; a
        // record without a full identity is refused after, unless its
        // binding already differs (the cursor key pages nothing here)
        let identity = crate::admin::record::identity::identity_in(record);
        let cursor = identity
            .as_ref()
            .map_or([1; 32], |identity| identity.cursor_key);
        let selection = crate::backend::postgresql::open::selection(url, &schema, &sync)
            .with_check(PostgresCheck::Light);
        let (binding, handles) =
            PostgresHandles::attach_read_only(&selection, cursor).map_err(|error| {
                crate::backend::postgresql::open::no_session(&error).unwrap_or_else(|| {
                    let said = crate::words::persistence(&error);
                    refused("Failed", format!("reading schema {schema}: {said}"))
                })
            })?;
        if record.binding() != Some(binding.as_slice()) {
            return Ok(None);
        }
        identity?;
        Ok(Some(handles))
    }

    fn insert(id: Key, handles: Arc<PostgresHandles>) {
        crate::admin::open::cache::reaper();
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        held.insert(
            id,
            Held {
                handles,
                last: Instant::now(),
            },
        );
    }

    /// The held session, checked locally and, once its connection has been
    /// silent as long as `PostgresStorage::usable` says (30 s by default),
    /// with one empty query; or a new one.
    pub fn session(
        key_path: &Path,
        record: &StoreRecord,
        postgresql: &PgSelection,
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

fn another(store: &str) -> SdkRefusal {
    refused(
        "Unavailable",
        format!("the PostgreSQL server holds another Store than {store}"),
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
    postgresql: &PgSelection,
    project: &str,
) -> Result<std::result::Result<bool, OtherStore>> {
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

/// The id and head of Branch `name` of project `project`, read on this
/// process's held session of the PostgreSQL Store `record` keyed by `key`,
/// dialled with `postgresql` (a forwarded Sandbox's status reads its head
/// here, not through a Store service): one light-checked connection
/// at most, and none when this process already holds a usable one. The
/// refusals are the Store service's lookup's.
pub(crate) fn branch(
    key: &Path,
    record: &StoreRecord,
    postgresql: &PgSelection,
    (project, name): (&str, &str),
) -> Result<(layerfs_history::BranchId, Option<layerfs_history::CommitId>)> {
    let handles = held::session(key, record, postgresql)?.ok_or_else(|| another(record.name()))?;
    use layerfs_store_service::history::lookup;
    let stack = lookup::project_in(project, |wanted| {
        (handles.history.layer_stack_named(wanted)).map_err(lookup::history)
    })?;
    let branch = lookup::branch_in(name, |wanted| {
        (handles.history.branch_named(stack.id, wanted)).map_err(lookup::history)
    })?;
    Ok((branch.id, branch.head_commit))
}

/// The newest Commit of `branch`, read on the held session; `None` when it
/// has none.
pub fn head(
    args: &OpenArgs,
    branch: layerfs_history::BranchId,
) -> Result<Option<layerfs_history::CommitId>> {
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

/// The held session of the call's Store, opened if this process holds none.
fn held_of(args: &OpenArgs) -> Result<std::sync::Arc<layerfs_persistence::PostgresHandles>> {
    let key = PathBuf::from(args.store_path.as_deref().unwrap_or(""));
    let store = args.store.as_deref().unwrap_or("");
    let record = StoreRecord::from_record(store, args.store_record.clone().unwrap_or_default());
    let postgresql = args.postgresql.as_ref().ok_or_else(|| {
        refused(
            "InvalidInput",
            "a PostgreSQL Sandbox call carries its server's resolved postgresql value",
        )
    })?;
    held::session(&key, &record, postgresql)?.ok_or_else(|| another(store))
}

/// Closes every held session unused for at least `idle`; how many.
pub fn close_idle(idle: std::time::Duration) -> usize {
    held::close_idle(idle)
}

/// Closes the held sessions of the Store at `key` (its disconnect or move).
pub fn forget(key: &Path) {
    held::forget(key);
}
