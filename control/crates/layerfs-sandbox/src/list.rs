//! `sandbox.list`: one page of the Sandbox records, each with its state from
//! records (call surface 2.0). Nothing is asked of a machine or a daemon.
//! A page is the Sandboxes by name after its cursor; the cursor carries
//! the call's filters, so a later page lists the same Sandboxes whatever
//! filters it is given, and is refused when it is given others.
use super::record::{SandboxRecord, WorkspaceRecord};
use super::state::machine_state;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::json;
use layerfs_home::outcome::refused;
use layerfs_home::state::State;
use std::collections::BTreeMap;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The rows a page holds when the call names no `limit`.
pub const LIMIT: i64 = 100;
/// The most rows a page holds.
pub const MAX_LIMIT: i64 = 1000;
/// The call a cursor of this list belongs to.
const CURSOR_FOR: &str = "sandbox.list";

/// `sandbox.list({ project, machine, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct ListRequest {
    /// Only this project's Sandboxes.
    pub project: Option<String>,
    /// Only the Sandboxes on this machine.
    pub machine: Option<String>,
    /// At most this many rows.
    pub limit: Option<i64>,
    /// A page's `next_cursor`: the Sandboxes after it.
    pub cursor: Option<String>,
}

/// The filters a page lists by: those given, or those its cursor carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Filters {
    project: Option<String>,
    machine: Option<String>,
}

impl Filters {
    fn keeps(&self, record: &SandboxRecord) -> bool {
        (self.project.is_none() || record.project() == self.project.as_deref())
            && (self.machine.is_none() || Some(record.machine()) == self.machine.as_deref())
    }
}

/// One Sandbox of a page.
pub struct ListedSandbox {
    /// Its record.
    pub record: SandboxRecord,
    /// Its state from records.
    pub state: State,
    /// The sentence behind a `blocked` or `broken` state.
    pub message: Option<String>,
    /// Its Workspaces as the home records them, `(handle, Branch)`; none
    /// when the home does not record them ([`Self::not_listed`]) or the
    /// Sandbox is blocked.
    pub workspaces: Option<Vec<(String, Option<String>)>>,
    /// Whether its Workspaces are kept by its machine's worker, not here, so
    /// a list of records cannot name them (C10).
    pub not_listed: bool,
}

/// One page of Sandboxes.
pub struct Listed {
    /// The page's Sandboxes, by name.
    pub items: Vec<ListedSandbox>,
    /// Whether more Sandboxes follow the page.
    pub more: bool,
    filters: Filters,
}

impl Listed {
    /// The `next_cursor` of a page that ended at Sandbox `last`: the place
    /// and the page's filters, as one opaque word.
    pub fn cursor(&self, last: &str) -> String {
        let mut fields = vec![
            ("for".to_owned(), SdkValue::text(CURSOR_FOR)),
            ("after".to_owned(), SdkValue::text(last)),
        ];
        for (name, value) in [
            ("project", &self.filters.project),
            ("machine", &self.filters.machine),
        ] {
            fields.extend(
                value
                    .clone()
                    .map(|text| (name.to_owned(), SdkValue::Text(text))),
            );
        }
        json::base64(json::print(&SdkValue::Map(fields)).as_bytes())
            .trim_end_matches('=')
            .to_owned()
    }
}

/// The place `cursor` names and the filters it carries; a filter `given`
/// must be the one it carries.
fn resume(cursor: &str, given: &Filters) -> Result<(String, Filters)> {
    let padded = format!("{cursor}{}", "=".repeat((4 - cursor.len() % 4) % 4));
    let made = json::unbase64(&padded)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| json::parse(&text).ok())
        .filter(|value| value.get_text("for") == Some(CURSOR_FOR))
        .and_then(|value| Some((value.get_text("after")?.to_owned(), value)))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "sandbox.list: `cursor` is not the `next_cursor` of a sandbox.list page",
            )
        })?;
    let (after, made) = made;
    let carried = Filters {
        project: made.get_text("project").map(str::to_owned),
        machine: made.get_text("machine").map(str::to_owned),
    };
    let differs =
        |given: &Option<String>, carried: &Option<String>| given.is_some() && given != carried;
    if differs(&given.project, &carried.project) || differs(&given.machine, &carried.machine) {
        return Err(refused(
            "InvalidInput",
            "sandbox.list: the cursor carries the filters of the call that made it; give the \
             same filters or none",
        ));
    }
    Ok((after, carried))
}

/// One page of the Sandbox records, each with its state from records: its
/// own record and its machine's, each machine read once. A Sandbox on
/// `native` or reached directly lists the Workspaces the home records; one
/// reached through its machine's worker says they are not listed.
pub fn list(home: &Path, request: &ListRequest) -> Result<Listed> {
    let limit = request.limit.unwrap_or(LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(refused(
            "InvalidInput",
            format!("sandbox.list: `limit` is 1 to {MAX_LIMIT}"),
        ));
    }
    let given = Filters {
        project: request.project.clone(),
        machine: request.machine.clone(),
    };
    let (after, filters) = match &request.cursor {
        Some(cursor) => {
            let (after, carried) = resume(cursor, &given)?;
            (Some(after), carried)
        }
        None => (None, given),
    };
    let mut kept = super::record::sandboxes(home)?
        .into_iter()
        .filter(|record| filters.keeps(record))
        .filter(|record| after.as_deref().is_none_or(|after| record.name() > after));
    let page: Vec<SandboxRecord> = kept.by_ref().take(limit as usize).collect();
    let more = kept.next().is_some();
    let mut machines: BTreeMap<String, State> = BTreeMap::new();
    let mut items = Vec::new();
    for record in page {
        let machine = record.machine().to_owned();
        if !machines.contains_key(&machine) {
            let state = machine_state(home, &machine)?;
            machines.insert(machine.clone(), state);
        }
        let state = record.status(&machines[&machine]);
        items.push(listed(home, record, state)?);
    }
    Ok(Listed {
        items,
        more,
        filters,
    })
}

/// Sandbox `record` in state `state` as a page lists it.
fn listed(home: &Path, record: SandboxRecord, state: State) -> Result<ListedSandbox> {
    let message = record.message(&state);
    let kept_here = record.machine() == "native" || super::direct::forwarded(&record);
    let (workspaces, not_listed) = match (&state, kept_here) {
        (State::Blocked(_), _) => (None, false),
        (_, true) => (Some(recorded(home, &record)?), false),
        (_, false) => (None, true),
    };
    Ok(ListedSandbox {
        record,
        state,
        message,
        workspaces,
        not_listed,
    })
}

/// The handle of Workspace `workspace` of Sandbox `sandbox`:
/// `<sandbox>:<name>`, or `<sandbox>:<path>` for one mounted with no name.
pub fn handle(sandbox: &str, workspace: &WorkspaceRecord) -> Option<String> {
    let at = workspace.name().or(workspace.path())?;
    Some(format!("{sandbox}:{at}"))
}

/// The Workspaces Sandbox `record`'s folder in the home records, by handle:
/// `(handle, Branch)`.
pub fn recorded(home: &Path, record: &SandboxRecord) -> Result<Vec<(String, Option<String>)>> {
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    for (_, workspace) in super::files::workspaces(&record.dir(home))? {
        if let Some(handle) = handle(record.name(), &workspace) {
            found.push((handle, workspace.branch().map(str::to_owned)));
        }
    }
    found.sort();
    Ok(found)
}
