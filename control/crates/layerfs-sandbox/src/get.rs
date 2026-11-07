//! `sandbox.get`: a Sandbox's record with one page of its Workspaces.
use super::direct;
use super::family::{page, sandbox_of, Page};
use super::record::SandboxRecord;
use crate::host::Call;
use crate::route::reach;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_machine::worker::Unreached;
use std::path::Path;

/// `sandbox.get(name, { page_size, page_pos })`.
#[derive(Clone, Debug, Default)]
pub struct GetRequest {
    /// The Sandbox's name.
    pub name: String,
    /// The page of its Workspaces it asks for.
    pub page: Page,
}

/// One Workspace its Sandbox's daemon lists (#536 C3).
pub struct ListedWorkspace {
    /// The daemon's item, as it answered it.
    pub item: SdkValue,
    /// Its kernel path: the item's `path`, or the daemon's `directory`
    /// from a worker that does not give it.
    pub path: String,
    /// The name its mount gave it, if any.
    pub name: Option<String>,
    /// `<sandbox>:<name>` when it has a name, else `<sandbox>:<path>`.
    pub ws: String,
}

/// A Sandbox's record with its derived state and one page of its
/// Workspaces.
pub struct Got {
    /// Its record (with its `op` when it has one, H6).
    pub record: SandboxRecord,
    /// Its derived state.
    pub state: &'static str,
    /// The page of its Workspaces.
    pub workspaces: Vec<ListedWorkspace>,
    /// Why its machine did not answer, when it did not.
    pub down_reason: Option<String>,
    /// The next page's position; `None` after the last.
    pub next: Option<usize>,
}

/// Its record with one page of its Workspaces. Reading never clears an
/// intent.
pub fn get(home: &Path, events: &mut Events<'_>, request: &GetRequest) -> Result<Got, SdkRefusal> {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    let record = sandbox_of(&home, name)?;
    // a Sandbox with no id has no daemon to ask; a get starts no machine
    // (P4): a machine that is down answers the record with no Workspaces
    // and why
    let listed = match record.id() {
        // one reached directly lists its daemon's Workspaces from here (#535 G4)
        Some(_) if direct::forwarded(&record) => direct::list(&home, &record),
        Some(_) => reach(
            &home,
            events,
            &record,
            Call::ListWorkspaces { path: None },
            crate::dispatch::call,
        ),
        None => Ok(Ok(SdkValue::Null)),
    };
    let (all, down_reason, gone) = match listed {
        Ok(Err(refusal)) if refusal.code == "NoSuchSandbox" => (Vec::new(), None, true),
        Ok(workspaces) => match workspaces?.get("workspaces") {
            Some(SdkValue::List(items)) => (
                items.iter().map(|i| listed_workspace(name, i)).collect(),
                None,
                false,
            ),
            _ => (Vec::new(), None, false),
        },
        Err(Unreached(why)) => (Vec::new(), Some(why.message), false),
    };
    let (workspaces, next) = page(all, request.page);
    Ok(Got {
        state: record.state(gone),
        record,
        workspaces,
        down_reason,
        next,
    })
}

/// Workspace `item` of Sandbox `sandbox`, as its daemon answered it.
fn listed_workspace(sandbox: &str, item: &SdkValue) -> ListedWorkspace {
    let path = (item.get_text("path"))
        .or_else(|| item.get_text("directory"))
        .unwrap_or("");
    let name = item.get_text("name").filter(|name| !name.is_empty());
    ListedWorkspace {
        item: item.clone(),
        ws: format!("{sandbox}:{}", name.unwrap_or(path)),
        path: path.to_owned(),
        name: name.map(str::to_owned),
    }
}
