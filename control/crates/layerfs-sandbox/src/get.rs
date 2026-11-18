//! `sandbox.get`: a Sandbox's state and its Workspaces, a live read (call
//! surface 2.0). Its state starts from records (`crate::state`); a Sandbox
//! its records say is ready, or lost or not answering, is then asked for
//! its Workspaces by its daemon, on the route its other calls take. What the
//! answer says decides the state: a list is `ready`, a machine that says the
//! Sandbox does not run is `broken` `lost`, a daemon that admits no control
//! connection is `broken` `not_answering`, and a machine or forward that does
//! not answer is `unknown` with when its state was last known. The record
//! is written only when that changes its `reason` (which moves the home's
//! generation); when the state was observed is noted in `observed.json`
//! beside it, which does not.
use super::direct;
use super::family::named;
use super::record::{self, SandboxRecord};
use super::state::machine_state;
use crate::host::Call;
use crate::route::reach;
use layerfs_bridge::control::{CapacityModel, SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::observed;
use layerfs_home::state::{Reason, State};
use layerfs_machine::worker::Unreached;
use std::path::Path;

/// `sandbox.get(sandbox, { with: ["stats"] })`.
#[derive(Clone, Debug, Default)]
pub struct GetRequest {
    /// The Sandbox's name.
    pub name: String,
    /// Whether the daemon's control-capacity read model is read too: one
    /// more daemon call, made only for a ready Sandbox (`--with stats`).
    pub stats: bool,
}

/// One Workspace its Sandbox's daemon lists (#536 C3).
pub struct ListedWorkspace {
    /// The daemon's item, as it answered it, with what the side that keeps
    /// the Workspace's record added (layerfs-project#178): its `path`,
    /// `name` and `created` (when it was mounted), the commands `running`
    /// in it and its `overlay_bytes`, and, from the side that runs the
    /// Sandbox, its own cgroup's `usage` with `usage_reason`
    /// (`crate::run_folder::workspace_usage`). A worker from before those
    /// fields answers none of them, and the item then has none.
    pub item: SdkValue,
    /// Its kernel path: the item's `path`, or the daemon's `directory`
    /// from a worker that does not give it.
    pub path: String,
    /// The name its mount gave it, if any.
    pub name: Option<String>,
    /// Its handle: `<sandbox>:<name>` when it has a name, else
    /// `<sandbox>:<path>`.
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

    /// The sentence a `not_answering` state answers as its `message`: what
    /// the refusing daemon said, with its counts.
    pub fn sentence(&self, said: &str) -> String {
        format!(
            "its daemon admits no control connection: {said} ({} on {}: {} pool and {} reserved \
             slots vacant, {} failures retained)",
            self.code, self.resource, self.pool_vacant, self.reserved_vacant, self.retained
        )
    }
}

/// A Sandbox's state and its Workspaces.
pub struct Got {
    /// Its record, as read before the live read.
    pub record: SandboxRecord,
    /// Its state.
    pub state: State,
    /// The sentence behind a `blocked`, `broken` or `unknown` state.
    pub message: Option<String>,
    /// Its Workspaces, as its daemon listed them; none when the daemon was
    /// not asked or did not list them.
    pub workspaces: Option<Vec<ListedWorkspace>>,
    /// Its daemon's control-capacity read model (#540), with `stats` for a
    /// ready Sandbox that answered the bounded read (#543).
    pub daemon_capacity: Option<CapacityModel>,
    /// How many Workspaces a Sandbox holds at once: control's own constant
    /// ([`crate::lifecycle::WORKSPACES`]), not a daemon setting, so it is
    /// there whatever the Sandbox's state (layerfs-project#178, D10).
    pub workspace_capacity: u32,
    /// Its daemon process as the side that runs it saw it
    /// (layerfs-project#164), as the fields its list answered: `daemon`
    /// (`{ pid, started_at }`), `usage` (its cgroup's `{ cpu_usec,
    /// memory_current, memory_peak, memory_file, pids, oom_kills }`, or
    /// null) and, with a null `usage`, `usage_reason`. Empty when its
    /// daemon did not list its Workspaces, and from a worker of a build
    /// before them.
    pub process: Vec<(String, SdkValue)>,
}

/// What the live read found.
enum Found {
    /// The daemon listed its Workspaces.
    Listed(SdkValue),
    /// A broken state with the sentence behind it.
    Broken(Reason, String),
    /// The machine or the forward did not answer: why.
    Unanswered(String),
}

/// The Sandbox's state and, when its daemon was asked and answered, its
/// Workspaces. Reading never clears an intent and never starts a machine.
pub fn get(home: &Path, events: &mut Events<'_>, request: &GetRequest) -> Result<Got, SdkRefusal> {
    let home = home.to_path_buf();
    let record = named(&home, &request.name)?;
    let mut state = record.status(&machine_state(&home, record.machine())?);
    let asked = match &state {
        State::Ready => true,
        State::Broken { reason, .. } => matches!(reason, Reason::Lost | Reason::NotAnswering),
        _ => false,
    };
    let mut got = Got {
        message: None,
        workspaces: None,
        daemon_capacity: None,
        workspace_capacity: crate::lifecycle::WORKSPACES,
        process: Vec::new(),
        state: State::Ready,
        record,
    };
    if asked {
        match found(&home, events, &got.record)? {
            Found::Listed(answer) => {
                settle(&home, &got.record, None);
                state = State::Ready;
                got.workspaces = Some(match answer.get("workspaces") {
                    Some(SdkValue::List(items)) => (items.iter())
                        .map(|item| listed_workspace(got.record.name(), item))
                        .collect(),
                    _ => Vec::new(),
                });
                got.process = crate::run_folder::process_of(&answer);
            }
            Found::Broken(reason, sentence) => {
                settle(&home, &got.record, Some((reason, &sentence)));
                let fix = super::state::remove_fix(got.record.name());
                state = State::broken(reason, Some(&fix));
                got.message = Some(sentence);
            }
            Found::Unanswered(why) => {
                let at = observed::last_known(&got.record.dir(&home).join("record.json"));
                state = State::Unknown { observed_ms: at };
                got.message = Some(why);
            }
        }
    }
    if got.message.is_none() {
        got.message = got.record.message(&state);
    }
    if request.stats && state == State::Ready {
        got.daemon_capacity = capacity(&home, events, &got.record);
    }
    got.state = state;
    Ok(got)
}

/// One live read of Sandbox `record`'s Workspace list, on its route.
fn found(
    home: &Path,
    events: &mut Events<'_>,
    record: &SandboxRecord,
) -> Result<Found, SdkRefusal> {
    // one reached directly lists its daemon's Workspaces from here (#535 G4)
    let listed = match direct::forwarded(record) {
        true => direct::list(home, record),
        false => reach(
            home,
            events,
            record,
            Call::ListWorkspaces { path: None },
            crate::dispatch::call,
        ),
    };
    Ok(match listed {
        Ok(Ok(answer)) => Found::Listed(answer),
        Ok(Err(refusal)) if refusal.code == "NoSuchSandbox" => Found::Broken(
            Reason::Lost,
            format!("its machine says it does not run: {}", refusal.message),
        ),
        // a route that runs the call in a machine's worker, or here, is told
        // by the pool it dialed: `Capacity` on `daemon_control_slot`, with
        // the counts the admission notice carried and no `code`. It reads as
        // the forwarded route's down Sandbox (#649), from the same detail.
        Ok(Err(refusal)) if slot_refused(&refusal) => {
            let tagged = SdkRefusal {
                detail: direct::down_detail("Capacity", refusal.detail.clone()),
                ..refusal.clone()
            };
            Found::Broken(Reason::NotAnswering, not_answering(&tagged))
        }
        Ok(Err(refusal)) => return Err(refusal),
        // a forwarded daemon that refused the dial with its capacity notice
        // answered; any other unanswered dial says nothing of the Sandbox
        Err(Unreached(why)) => match Down::of(&why) {
            Some(_) => Found::Broken(Reason::NotAnswering, not_answering(&why)),
            None => Found::Unanswered(why.message),
        },
    })
}

/// The `not_answering` sentence of refusal `refused`.
fn not_answering(refused: &SdkRefusal) -> String {
    match Down::of(refused) {
        Some(down) => down.sentence(&refused.message),
        None => format!(
            "its daemon admits no control connection: {}",
            refused.message
        ),
    }
}

/// Notes what a live read that answered found of Sandbox `record`: its
/// record is written only when its broken `reason` changes, under its lock
/// (taken only then) and only while it still has a record (a Sandbox
/// removed meanwhile is not made again); when it was observed is noted
/// beside it either way, the one file a get of an unchanged Sandbox
/// writes. A note that cannot be written changes no answer.
fn settle(home: &Path, record: &SandboxRecord, broken: Option<(Reason, &str)>) {
    let word = broken.map(|(reason, _)| reason.word());
    if record.reason_word() != word {
        let Ok(_sandbox) = record::lock(home, record.name()) else {
            return;
        };
        let Ok(Some(now)) = SandboxRecord::read(home, record.name()) else {
            return;
        };
        if now.reason_word() != word {
            let error = broken.map(|(reason, sentence)| (reason.word(), sentence));
            let _ = now.observed_as(error).write(home);
        }
    }
    let file = record.dir(home).join("record.json");
    if file.exists() {
        let _ = observed::write(&file, layerfs_home::home::now_ms());
    }
}

/// Whether `refusal` is a full pool's own refusal: the pool a worker or this
/// process dialed admitted no control connection. A `Capacity` an admitted
/// connection returned (`values::refusal`'s `{phase}`) observed no counts and
/// is not one.
fn slot_refused(refusal: &SdkRefusal) -> bool {
    refusal.code == "Capacity" && refusal.detail.get_text("resource") == Some("daemon_control_slot")
}

/// The daemon's `backend.daemon.connection_failures` read model of a ready
/// Sandbox whose Workspace list just answered (#540): read once, best
/// effort, never failing a get (#543).
fn capacity(home: &Path, events: &mut Events<'_>, record: &SandboxRecord) -> Option<CapacityModel> {
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
