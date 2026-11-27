//! `project.get` and `project.list` (call surface 2.0): each project with
//! its state ([`super::state`]), its Store, engine and machine, main's head
//! (`commit`, the full id the Store's list carries) and when it was made.
//! A Store whose records say it is not ready is not asked: its projects
//! answer `blocked`. Any other Store's own list is read where its calls
//! run; one that cannot be read blocks its projects, and a project it does
//! not list is hidden from `project list` and `NoSuchProject` to `project
//! get`, as today.
use crate::admin::record::store::StoreRecord;
use crate::admin::status::state::Machines;
use crate::data::project::frame::ListedProject;
use crate::data::project::record::project::ProjectRecord;
use crate::data::project::status::live;
use crate::data::project::status::state::{self, Listing};
use layerfs_home::cursor;
use layerfs_home::home::paths;
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::state::State;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The call a `project.list` cursor belongs to.
const LIST: &str = "project.list";
/// The most bytes of rows one page holds before it ends early with a cursor.
const PAGE_BYTES: usize = layerfs_home::logs::PAGE_BYTES;

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

/// One project's answer: its state with its companions, then `commit`
/// (main's head, when the Store's list was read) and the record's fields.
fn row(
    project: &ProjectRecord,
    store: &StoreRecord,
    state: &State,
    listed: Option<&ListedProject>,
    stats: bool,
) -> SdkValue {
    let mut fields: Vec<(String, SdkValue)> = project
        .answer(store)
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    let head = listed.and_then(|listed| listed.head.clone());
    fields.push(("commit".to_owned(), SdkValue::opt_text(head)));
    if stats {
        let size = listed.and_then(|listed| listed.size);
        let size = match state {
            State::Blocked(_) => SdkValue::Null,
            _ => size.map_or(SdkValue::Null, SdkValue::Int),
        };
        fields.push(("size".to_owned(), size));
    }
    state.write(SdkValue::Map(fields))
}

/// The Store's list, unless its records block it: `Ok(None)` when it was
/// not asked, the reason it blocks its projects with when it could not be
/// read.
fn listing(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    block: Option<&State>,
    stats: bool,
) -> Option<std::result::Result<Vec<ListedProject>, &'static str>> {
    if block.is_some() {
        return None;
    }
    Some(live::projects(home, store, record, stats).map_err(|refusal| state::unread(&refusal)))
}

/// What `live` says of project `name` whose record is `project`.
fn says<'a>(
    live: &'a Option<std::result::Result<Vec<ListedProject>, &'static str>>,
    project: &ProjectRecord,
    name: &str,
) -> Listing<'a> {
    match live {
        None => Listing::Skipped,
        Some(Err(reason)) => Listing::Unread(reason),
        Some(Ok(live)) => match live::find(project, name, live) {
            Some(listed) => Listing::Listed(listed),
            None => Listing::Hidden,
        },
    }
}

/// `project.get(project)`: one project, its state and main's head. A
/// project of a Store that cannot be opened answers `blocked`; one the
/// Store does not list is `NoSuchProject` even when its home entry
/// remains, unless an Init or a delete of it is recorded (spec §4.2).
pub fn get(home: &Path, project: &ProjectRef, machines: Machines) -> Outcome {
    let (store, name) = (project.store.as_str(), project.project.as_str());
    let record = StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NoSuchStore", format!("no Store named {store}")))?;
    let missing = || refused("NoSuchProject", format!("no project {project}"));
    let entry = ProjectRecord::read(home, store, name)?.ok_or_else(missing)?;
    let block = state::store_block(home, &record, machines)?;
    let live = listing(home, store, &record, block.as_ref(), false);
    let said = says(&live, &entry, name);
    let state = state::derive(home, store, name, &entry, block.as_ref(), said);
    let state = state.ok_or_else(missing)?;
    let listed = match said {
        Listing::Listed(listed) => Some(listed),
        _ => None,
    };
    Ok(row(&entry, &record, &state, listed, false))
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
