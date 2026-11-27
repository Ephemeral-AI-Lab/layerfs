//! `project.list` (call surface 2.0): one page of the home's projects, each
//! answered as [`super::get`] answers one, Store by Store in name order.
use crate::admin::record::store::StoreRecord;
use crate::admin::status::state::Machines;
use crate::data::project::record::project::ProjectRecord;
use crate::data::project::status::state::{self, Listing};
use layerfs_home::disk::paths;
use layerfs_home::names::StoreName;
use layerfs_home::reply::cursor;
use layerfs_home::reply::outcome::{failed, refused, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

use super::get::{listing, row, says};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The call a `project.list` cursor belongs to.
const LIST: &str = "project.list";
/// The most bytes of rows one page holds before it ends early with a cursor.
const PAGE_BYTES: usize = layerfs_home::log::calls::PAGE_BYTES;

/// `project.list({ store, limit, cursor, with: stats })`.
#[derive(Clone, Debug, Default)]
pub struct ProjectList {
    /// The one Store it lists.
    pub store: Option<StoreName>,
    /// The most rows in one page.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before; it carries the filters only.
    pub cursor: Option<String>,
    /// Whether each row carries `size` (`--with stats`).
    pub stats: bool,
}

/// The Stores a listing reads, in name order: the one named, else every
/// Store the home records from `from` on.
fn stores(home: &Path, named: Option<&str>, from: &str) -> Result<Vec<(String, StoreRecord)>> {
    if let Some(store) = named {
        // a named listing agrees with every Store call: a Store the home
        // does not record is `NoSuchStore`
        let record = StoreRecord::read(home, store)?
            .ok_or_else(|| refused("NoSuchStore", format!("no Store named {store}")))?;
        return Ok(vec![(store.to_owned(), record)]);
    }
    let mut records = Vec::new();
    for store in paths::names(&home.join("store")).map_err(failed)? {
        if store.as_str() < from {
            continue;
        }
        if let Some(record) = StoreRecord::read(home, &store)? {
            records.push((store, record));
        }
    }
    Ok(records)
}

/// `project.list({ store, limit, cursor, with: stats })`: one page of the
/// home's projects by Store, then name. A project of a Store that cannot
/// be opened is listed `blocked`; one the Store does not list is hidden. A
/// page ends at `limit` rows, or early past 64 KiB of rows, with
/// `next_cursor`; the cursor carries the `store` filter only.
pub fn list(home: &Path, request: &ProjectList, machines: Machines) -> Outcome {
    let limit = cursor::limit(LIST, request.limit, cursor::DEFAULT_LIMIT)? as usize;
    let given = request.store.as_ref().map(StoreName::as_str);
    let filters = [("store", SdkValue::opt_text(given))];
    let carried = (request.cursor.as_deref())
        .map(|made| cursor::resume(LIST, made, &filters))
        .transpose()?;
    let named = match &carried {
        Some(carried) => carried.text("store").map(str::to_owned),
        None => given.map(str::to_owned),
    };
    // where the page before ended: its last project, `<store>/<name>`
    let after = match carried.as_ref().map(|carried| carried.place()) {
        Some(SdkValue::Text(at)) => Some(at.clone()),
        _ => None,
    };
    let (after_store, after_name) = match after.as_deref().and_then(|at| at.split_once('/')) {
        Some((store, name)) => (store.to_owned(), Some(name.to_owned())),
        None => (String::new(), None),
    };
    let mut items: Vec<SdkValue> = Vec::new();
    let (mut spent, mut next) = (0usize, None);
    'stores: for (store, record) in stores(home, named.as_deref(), &after_store)? {
        let dir = paths::store_dir(home, &store).join("project");
        let mut names = paths::names(&dir).map_err(failed)?;
        if let (true, Some(after)) = (store == after_store, &after_name) {
            names.retain(|name| name > after);
        }
        if names.is_empty() {
            continue;
        }
        let block = state::store_block(home, &record, machines)?;
        let live = listing(home, &store, &record, block.as_ref(), request.stats);
        for name in names {
            let Some(entry) = ProjectRecord::read(home, &store, &name)? else {
                continue;
            };
            let said = says(&live, &entry, &name);
            let Some(state) = state::derive(home, &store, &name, &entry, block.as_ref(), said)
            else {
                continue;
            };
            let listed = match said {
                Listing::Listed(listed) => Some(listed),
                _ => None,
            };
            let item = row(&entry, &record, &state, listed, request.stats);
            let cost = layerfs_home::json::print(&item).len();
            if items.len() == limit || (!items.is_empty() && spent + cost > PAGE_BYTES) {
                next = items
                    .last()
                    .and_then(|last| last.get_text("project").map(str::to_owned));
                break 'stores;
            }
            spent += cost;
            items.push(item);
        }
    }
    // the next page's cursor carries the filter this page ran with
    let carry = [("store", SdkValue::opt_text(named))];
    let next_cursor = next.map(|at| SdkValue::Text(cursor::make(LIST, SdkValue::Text(at), &carry)));
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("next_cursor", next_cursor.unwrap_or_default()),
    ]))
}
