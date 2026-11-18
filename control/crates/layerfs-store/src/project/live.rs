//! A Store's live project listing read from the caller (spec §4.2, T4.5c):
//! the Store's own filtered list, read where its calls run, and the match of
//! a home record against it.
use crate::open_args::OpenArgs;
use crate::place::{self, Executor, Worker};
use crate::project::frame::{ListedProject, ListingFrame, ListingReply};
use crate::project::listing;
use crate::project::record::ProjectRecord;
use crate::record::{StoreRecord, StoreRef};
use crate::service;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::home::paths;
use layerfs_home::names::StoreName;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The live projects Store `store` lists, after the retired and hidden ones
/// are filtered out (spec §4.2), each with main's head and, when `stats`,
/// its size. Every route reads the listing where the Store's calls run: in
/// this process for a sqlite Store on this machine and for a PostgreSQL
/// Store, in the machine's worker for a sqlite Store on another machine. A
/// Store that cannot be listed is `Err`.
pub fn projects(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    stats: bool,
) -> Result<Vec<ListedProject>> {
    let store_ref = record.store_ref()?;
    record.unblocked()?;
    if let Executor::Worker(mut worker) = place::plan(home, store_ref)? {
        return on_worker(&mut worker, store, record, stats);
    }
    let (store_path, postgresql) = match store_ref {
        StoreRef::Postgres { .. } => (
            record.postgresql_key(home, store),
            Some(crate::postgresql::connect::selection(record)?),
        ),
        StoreRef::Sqlite { .. } => (paths::store_file(home, store), None),
    };
    let open = OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(store.to_owned()),
        store_path: Some(store_path.display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql,
        ..OpenArgs::default()
    };
    service::with_service(&open, |service| listing::listed(service, stats))
}

/// The listing a worker-backed Store sends (T4.5c): the worker runs the
/// same listing under its own root. A listing whose answer was lost is
/// `Unknown`, naming the Store it listed (spec §10 A6).
fn on_worker(
    worker: &mut Worker,
    store: &str,
    record: &StoreRecord,
    stats: bool,
) -> Result<Vec<ListedProject>> {
    let frame = ListingFrame {
        store: StoreName::parse(store)?,
        store_record: Some(record.to_record().clone()),
        stats,
    };
    let answer = worker
        .call("project.list", frame.encode(), &mut |_| {})
        .map_err(|refusal| crate::unknown::named_unknown(refusal, "store", store, None))?;
    Ok(ListingReply::decode(&answer)?.projects)
}

/// The entry of `live` the record of project `name` names: by the stack id
/// the record holds, or, for a record without one, by its name (spec §4.2).
pub fn find<'a>(
    project: &ProjectRecord,
    name: &str,
    live: &'a [ListedProject],
) -> Option<&'a ListedProject> {
    match project.stack() {
        Some(stack) => live
            .iter()
            .find(|item| item.stack.to_bytes().as_slice() == stack.as_slice()),
        None => live.iter().find(|item| item.name == name),
    }
}
