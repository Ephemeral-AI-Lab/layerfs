//! `sandbox.get`: a Sandbox's record with one page of its Workspaces.
use super::direct;
use super::family::{page, sandbox_of, Page};
use super::record::SandboxRecord;
use crate::host::Call;
use crate::route::reach;
use layerfs_bridge::control::{CapacityModel, SdkRefusal, SdkValue};
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

/// Why a get's daemon was not reached, as far as the refused connection
/// said (#649): a **partial** model, filled only from the daemon's
/// pre-handshake `LAYERFS-CAPACITY` notice. It is not
/// [`CapacityModel`]: that model's missing fields would read as zero, so a
/// notice is never mapped into `daemon_capacity`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Down {
    /// The refusal the notice produced: `Capacity`.
    pub code: String,
    /// The exhausted resource, e.g. `daemon_control_slot`.
    pub resource: String,
    /// Pool slots vacant when the daemon refused.
    pub pool_vacant: i64,
    /// Reserved slots vacant when the daemon refused.
    pub reserved_vacant: i64,
    /// Retained failure records visible when the daemon refused.
    pub retained: i64,
}

impl Down {
    /// The partial model a refusal's detail carries, when it carries every
    /// count. A detail without the capacity fields (a legacy EOF, or a
    /// remote refusal collapsed to `{phase}`) has none, and `None` is
    /// returned rather than zeros.
    pub fn of(refusal: &SdkRefusal) -> Option<Self> {
        let detail = &refusal.detail;
        Some(Self {
            code: detail.get_text("code")?.to_owned(),
            resource: detail.get_text("resource")?.to_owned(),
            pool_vacant: detail.get_int("pool_vacant")?,
            reserved_vacant: detail.get_int("reserved_vacant")?,
            retained: detail.get_int("retained")?,
        })
    }

    /// The SDK value `sandbox.get` shows as `down`.
    pub fn to_sdk(&self) -> SdkValue {
        SdkValue::map([
            ("code", SdkValue::text(self.code.as_str())),
            ("resource", SdkValue::text(self.resource.as_str())),
            ("pool_vacant", SdkValue::Int(self.pool_vacant)),
            ("reserved_vacant", SdkValue::Int(self.reserved_vacant)),
            ("retained", SdkValue::Int(self.retained)),
        ])
    }
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
    /// The structured counts behind `down_reason`, when the refusing
    /// daemon's capacity notice carried them (#649).
    pub down: Option<Down>,
    /// The next page's position; `None` after the last.
    pub next: Option<usize>,
    /// Its daemon's control-capacity read model (#540), when a running
    /// Sandbox answered the bounded read (#543).
    pub daemon_capacity: Option<CapacityModel>,
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
    let (all, down_reason, down, gone) = match listed {
        Ok(Err(refusal)) if refusal.code == "NoSuchSandbox" => (Vec::new(), None, None, true),
        Ok(workspaces) => match workspaces?.get("workspaces") {
            Some(SdkValue::List(items)) => (
                items.iter().map(|i| listed_workspace(name, i)).collect(),
                None,
                None,
                false,
            ),
            _ => (Vec::new(), None, None, false),
        },
        Err(Unreached(why)) => {
            let down = Down::of(&why);
            (Vec::new(), Some(why.message), down, false)
        }
    };
    let (workspaces, next) = page(all, request.page);
    let state = record.state(gone);
    // best effort, bounded, and only for a Sandbox its daemon still serves:
    // this read never fails a get (#540, #543)
    let unreachable = down_reason.is_some();
    let daemon_capacity = capacity(&home, events, &record, gone, state, unreachable);
    Ok(Got {
        state,
        record,
        workspaces,
        down_reason,
        down,
        next,
        daemon_capacity,
    })
}

/// The daemon's `backend.daemon.connection_failures` read model of a Sandbox
/// this process still reaches (#540): read once, best effort, and omitted
/// when the Sandbox is gone, is not running, or its machine or forward is
/// already down — a get dials nothing for one of those (#543).
fn capacity(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
    gone: bool,
    state: &str,
    down: bool,
) -> Option<CapacityModel> {
    if gone || down || state != "running" {
        return None;
    }
    record.id()?;
    // the forwarded daemon is dialed from here, its machine's from its worker:
    // either route reads it with the connection the Workspace list lent
    let answer = if direct::forwarded(record) {
        direct::capacity(home, record)
    } else {
        reach(home, events, record, Call::Capacity, crate::dispatch::call).ok()?
    };
    Some(CapacityModel::parse(&answer.ok()?))
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
