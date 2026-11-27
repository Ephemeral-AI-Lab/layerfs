//! A Store's live project listing read from the caller (spec §4.2, T4.5c):
//! the Store's own filtered list, read where its calls run, and the match of
//! a home record against it.
use crate::admin::open::args::OpenArgs;
use crate::admin::open::service;
use crate::admin::record::store::StoreRecord;
use crate::admin::route::executor::{self, Executor, Worker};
use crate::data::project::frame::{ListedProject, ListingFrame, ListingReply};
use crate::data::project::record::project::ProjectRecord;
use crate::data::project::status::listing;
use layerfs_home::names::StoreName;
use layerfs_protocol::sdk::SdkRefusal;
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
    if let Executor::Worker(mut worker) = executor::plan(home, store_ref)? {
        return on_worker(&mut worker, store, record, stats);
    }
    let open = OpenArgs::of_record(home, store, record)?;
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
        .map_err(|refusal| crate::data::unknown::named_unknown(refusal, "store", store, None))?;
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
