//! The route of one Store call, chosen from records (H §7.7): a PostgreSQL
//! Store and a sqlite Store on `native` run in this process, under its
//! lease; a sqlite Store
//! on any other machine runs in that machine's worker, started first if it
//! is not running. The caller sends the worker the Store's record with the
//! call, so a guest never reads the home; the worker finds the Store file
//! under its own root.
use crate::calls::StoreOp;
use crate::lease;
use crate::open;
use crate::open_args::{OpenArgs, PgSelection};
use crate::place::{plan, Executor};
use crate::record::{StoreRecord, StoreRef};
use crate::resolve::Target;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::{BranchId, CommitId};
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// Runs Store method `op` on the Store of `target`.
pub fn store_call(
    home: &Path,
    events: &mut Events<'_>,
    target: &Target,
    op: &StoreOp<'_>,
) -> Outcome {
    let (method, args) = (op.method(), op.engine_args());
    let home = home.to_path_buf();
    target.record.unblocked()?;
    // the lease is taken under the Store's lock with the record read there:
    // a move or remove that started since `target` was read refuses it
    let (lease, record) = lease::claim(&home, &target.store)?;
    if let StoreRef::Postgres { .. } = target.store_ref {
        return postgresql(&home, target, method, (args, &record), lease);
    }
    // the record read under the lock, parsed once (R4)
    match plan(&home, record.store_ref()?)? {
        Executor::Local => {
            let file = paths::store_file(&home, &target.store);
            let open = opened(target, &record, file.display().to_string(), None);
            open::run(
                method,
                &open,
                SdkValue::Map(with_project(args, target)),
                Some(lease),
            )
        }
        Executor::Worker(mut worker) => {
            // the host process holds the lease while it sends the call
            let _lease = lease;
            let request = frame(method, with_project(args, target), target, &record);
            worker.call("store.call", request, &mut |event| events.event(event))
        }
    }
}

/// The `store.call` frame: `{ op, args }`, its args the call's, `project`,
/// `store` and `store_record`.
fn frame(
    method: &str,
    mut fields: Vec<(String, SdkValue)>,
    target: &Target,
    record: &StoreRecord,
) -> SdkValue {
    fields.extend([
        ("store".to_owned(), SdkValue::text(&target.store)),
        ("store_record".to_owned(), record.to_record().clone()),
    ]);
    SdkValue::map([
        ("op", SdkValue::text(method)),
        ("args", SdkValue::Map(fields)),
    ])
}

/// A `store.call` frame as a worker reads it.
pub struct CallFrame {
    /// The Store method.
    pub method: String,
    /// The Store it names.
    pub store: String,
    /// The engine's arguments, with the routing keys the open reads.
    pub fields: SdkValue,
}

impl CallFrame {
    /// What opens the Store: the frame's open fields, each what the field
    /// of the same name held as text (`None` when absent or not text).
    pub fn open(&self) -> OpenArgs {
        let fields = &self.fields;
        let text = |name: &str| fields.get_text(name).map(str::to_owned);
        OpenArgs {
            home: text("home").map(std::path::PathBuf::from),
            store: text("store"),
            project: text("project"),
            store_path: text("storePath"),
            store_record: fields.get("store_record").cloned(),
            postgresql: fields.get("postgresql").map(PgSelection::read),
        }
    }

    /// Reads `frame`: no method or no Store is `InvalidInput`.
    pub fn decode(frame: &SdkValue) -> Result<Self, SdkRefusal> {
        let method = frame
            .get_text("op")
            .ok_or_else(|| refused("InvalidInput", "Store operation name"))?
            .to_owned();
        let fields = frame.get("args").cloned().unwrap_or_default();
        let store = fields
            .get_text("store")
            .ok_or_else(|| refused("InvalidInput", "a Store call names its Store"))?
            .to_owned();
        Ok(Self {
            method,
            store,
            fields,
        })
    }
}

/// The id and head of Branch `name` of `target`'s project when `target`
/// is a PostgreSQL Store this process holds no Store service of: read on
/// the process's held read session (#64), under the Store's lease as
/// [`store_call`] takes it. A per-call caller (a new CLI process each
/// time) then opens one light-checked connection for a status instead of a
/// Store service's writer and control reader (a mount reads none: its
/// daemon finds the Branch). `None` for a
/// sqlite Store and for one whose Store service this process already holds
/// open: [`store_call`] reads it there, opening nothing.
pub fn held_branch(
    home: &Path,
    target: &Target,
    name: &str,
) -> Result<Option<(BranchId, Option<CommitId>)>, SdkRefusal> {
    let StoreRef::Postgres { .. } = target.store_ref else {
        return Ok(None);
    };
    let key = target.record.postgresql_key(home, &target.store);
    if open::holds(&key) {
        return Ok(None);
    }
    target.record.unblocked()?;
    let (_lease, record) = lease::claim(home, &target.store)?;
    let postgresql = crate::postgresql::connect::selection(&record)?;
    let key = record.postgresql_key(home, &target.store);
    let read = (target.project.as_str(), name);
    crate::postgresql::reader::branch(&key, &record, &postgresql, read).map(Some)
}

/// A call's arguments with its project: what the engine call gets.
fn with_project(args: Vec<(String, SdkValue)>, target: &Target) -> Vec<(String, SdkValue)> {
    let mut fields = args;
    fields.push(("project".to_owned(), SdkValue::text(&target.project)));
    fields
}

/// What opens `target`'s Store at `store_path`: its project, Store and
/// Store record, and a PostgreSQL Store's server.
fn opened(
    target: &Target,
    record: &StoreRecord,
    store_path: String,
    postgresql: Option<PgSelection>,
) -> OpenArgs {
    OpenArgs {
        home: None,
        store: Some(target.store.clone()),
        project: Some(target.project.clone()),
        store_path: Some(store_path),
        store_record: Some(record.to_record().clone()),
        postgresql,
    }
}

/// A PostgreSQL Store's call in this process: its URL from the record, the
/// password reference resolved now (H §7.4). Its lease is held while this
/// process keeps the Store open, as a sqlite Store's is: `store.disconnect`
/// refuses `InUse` meanwhile.
fn postgresql(
    home: &Path,
    target: &Target,
    method: &str,
    (args, record): (Vec<(String, SdkValue)>, &StoreRecord),
    lease: lease::Lease,
) -> Outcome {
    let url = crate::record::url(&target.record)?;
    let postgresql = PgSelection {
        url: Some(url),
        ..PgSelection::default()
    };
    // one open service per PostgreSQL Store and binding, keyed by a path no
    // file has
    let key = target.record.postgresql_key(home, &target.store);
    let open = opened(target, record, key.display().to_string(), Some(postgresql));
    open::run(
        method,
        &open,
        SdkValue::Map(with_project(args, target)),
        Some(lease),
    )
}
