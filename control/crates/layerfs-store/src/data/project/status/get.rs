//! `project.get` (call surface 2.0; `project.list` in [`super::list`]): a project with
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
use layerfs_home::names::ProjectRef;
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_home::reply::state::State;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// One project's answer: its state with its companions, then `commit`
/// (main's head, when the Store's list was read) and the record's fields.
pub(super) fn row(
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
pub(super) fn listing(
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
pub(super) fn says<'a>(
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
