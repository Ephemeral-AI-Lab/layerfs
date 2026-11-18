//! The `workspace` kind (call surface 2.0). A Workspace has no doctor of its
//! own: its Sandbox's doctor reads its daemon's list once
//! (`sandbox.doctor_workspaces`) and gives each Workspace one shallow node,
//! `workspace.mounted` and `workspace.activity`, named `<sandbox>:<name>`.
//! The Workspaces are those the daemon lists and those the Sandbox's folder
//! records (on `native` and for a Sandbox reached directly the home's, on
//! another machine its worker's own); a recorded one the daemon does not
//! hold is a finding. A list that gave no verdict leaves every Workspace
//! `not_checked` with its `reason`; a list never asked for leaves each
//! `blocked_by` its Sandbox, `not_answering`: no probe was tried.
use super::frame::Probe;
use super::report::{NoVerdict, Node, NotChecked, Thing};
use super::target::Checks;
use layerfs_bridge::control::SdkValue;
use layerfs_home::state::BlockedBy;
use std::path::Path;

/// The `workspace` kind's checks, in the order they run.
pub(super) const CHECKS: &[super::frame::CheckId] = &[
    super::frame::CheckId::own("workspace.mounted"),
    super::frame::CheckId::own("workspace.activity"),
];

/// One row of a daemon's Workspace list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Listed {
    /// Its id, 64 hex digits.
    pub id: String,
    /// Its activity: `binding`, `idle`, `committing`, `closing`,
    /// `uncertain`, `failed` or `attaching`.
    pub activity: String,
    /// Whether its mount is present.
    pub mounted: bool,
}

/// What became of a Sandbox's daemon list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Listing {
    /// Not asked: nothing reached the daemon's list.
    #[default]
    NotAsked,
    /// Read whole.
    Read,
    /// Asked, and no whole list came back.
    NoVerdict(NoVerdict),
}

impl Listing {
    /// The word a worker's report carries it as.
    pub fn word(self) -> &'static str {
        match self {
            Listing::NotAsked => "not_asked",
            Listing::Read => "read",
            Listing::NoVerdict(reason) => reason.word(),
        }
    }

    /// The listing a worker's report names; an unknown word is a lost one.
    pub fn of(word: Option<&str>) -> Listing {
        match word {
            Some("not_asked") => Listing::NotAsked,
            Some("read") => Listing::Read,
            Some(word) => Listing::NoVerdict(
                (NoVerdict::ALL.into_iter())
                    .find(|reason| reason.word() == word)
                    .unwrap_or(NoVerdict::Lost),
            ),
            None => Listing::NoVerdict(NoVerdict::Lost),
        }
    }
}

/// One Workspace of a Sandbox: its handle, whether a record names it and
/// its daemon's row when the daemon lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Seen {
    /// `<sandbox>:<name>`: its name, else its path, else its id.
    pub handle: String,
    /// Whether the Sandbox's folder records it.
    pub recorded: bool,
    /// Its daemon's row.
    pub listed: Option<Listed>,
}

impl Seen {
    /// `{ workspace, recorded, activity, mounted }`, as a worker's report
    /// carries it.
    pub fn to_sdk(&self) -> SdkValue {
        let listed = self.listed.as_ref();
        SdkValue::map([
            ("workspace", SdkValue::text(&self.handle)),
            ("id", SdkValue::opt_text(listed.map(|row| row.id.clone()))),
            ("recorded", SdkValue::Bool(self.recorded)),
            (
                "activity",
                SdkValue::opt_text(listed.map(|row| row.activity.clone())),
            ),
            (
                "mounted",
                listed.map_or(SdkValue::Null, |row| SdkValue::Bool(row.mounted)),
            ),
        ])
    }

    /// One entry of a worker's report; none when it is malformed.
    pub fn of(value: &SdkValue) -> Option<Seen> {
        let handle = value.get_text("workspace")?.to_owned();
        let listed = match (value.get_text("activity"), value.get("mounted")) {
            (Some(activity), Some(SdkValue::Bool(mounted))) => Some(Listed {
                id: value.get_text("id")?.to_owned(),
                activity: activity.to_owned(),
                mounted: *mounted,
            }),
            (None, None | Some(SdkValue::Null)) => None,
            _ => return None,
        };
        Some(Seen {
            handle,
            recorded: value.get_bool("recorded"),
            listed,
        })
    }
}

/// The Workspaces Sandbox `sandbox`'s folder `dir` records: `(id, name)`,
/// the name its mount gave it, else its path. A folder that cannot be read
/// records none here; the home's own doctor names it.
pub(super) fn recorded(dir: &Path) -> Vec<(String, String)> {
    let records = layerfs_sandbox::files::workspaces(dir).unwrap_or_default();
    (records.into_iter())
        .filter_map(|(_, record)| {
            let id = record.id()?.to_owned();
            let name = record.name().or(record.path()).unwrap_or(&id).to_owned();
            Some((id, name))
        })
        .collect()
}

/// Sandbox `sandbox`'s Workspaces: those `recorded` names (`(id, name)`)
/// and those its daemon `listed`, each once, by handle.
pub(super) fn seen(sandbox: &str, recorded: &[(String, String)], listed: Vec<Listed>) -> Vec<Seen> {
    let mut seen: Vec<Seen> = (recorded.iter())
        .map(|(id, name)| Seen {
            handle: format!("{sandbox}:{name}"),
            recorded: true,
            listed: listed.iter().find(|row| row.id == *id).cloned(),
        })
        .collect();
    for row in listed {
        if !recorded.iter().any(|(id, _)| *id == row.id) {
            seen.push(Seen {
                handle: format!("{sandbox}:{}", row.id),
                recorded: false,
                listed: Some(row),
            });
        }
    }
    seen.sort_by(|a, b| a.handle.cmp(&b.handle));
    seen
}

/// The fix of a Workspace its daemon cannot serve.
fn unmount_fix(handle: &str) -> String {
    format!("layerfs workspace unmount {handle} --discard --force")
}

/// One node per Workspace in `seen` of Sandbox `sandbox`, from the
/// daemon's list as `listing` left it.
pub(super) fn nodes(sandbox: &str, seen: &[Seen], listing: Listing) -> Vec<Node> {
    seen.iter().map(|one| node(sandbox, one, listing)).collect()
}

fn node(sandbox: &str, seen: &Seen, listing: Listing) -> Node {
    let thing = Thing::new(super::report::Kind::Workspace, &seen.handle);
    let reason = match listing {
        Listing::Read => None,
        Listing::NoVerdict(reason) => Some(reason),
        // nothing reached its daemon's list: no probe was tried, so nothing
        // was lost; it waits on its Sandbox
        Listing::NotAsked => return waiting(thing, sandbox),
    };
    if let Some(reason) = reason {
        return unchecked(thing, reason);
    }
    let mut node = Node::new(thing);
    let mut checks = Checks::new("workspace", &seen.handle);
    let handle = seen.handle.as_str();
    const MOUNTED: &str = "workspace.mounted";
    const ACTIVITY: &str = "workspace.activity";
    match &seen.listed {
        None => {
            checks.fail(MOUNTED, "on record, but its daemon does not hold it");
            checks.fix(MOUNTED, unmount_fix(handle));
        }
        Some(row) => {
            match (row.mounted, row.activity.as_str()) {
                (true, _) => checks.pass(MOUNTED, "its daemon reports its mount as present"),
                (false, "binding" | "attaching") => {
                    checks.notice(MOUNTED, "its mount is still being made")
                }
                (false, "closing") => checks.notice(MOUNTED, "its mount is being removed"),
                (false, _) => {
                    checks.fail(MOUNTED, "its daemon holds it, but its mount is not present");
                    checks.fix(MOUNTED, unmount_fix(handle));
                }
            }
            match row.activity.as_str() {
                "failed" => {
                    checks.fail(ACTIVITY, "a local failure stopped its writes");
                    checks.fix(ACTIVITY, unmount_fix(handle));
                }
                "uncertain" => {
                    checks.fail(ACTIVITY, "the outcome of an operation on it is not known");
                    checks.fix(ACTIVITY, unmount_fix(handle));
                }
                _ => checks.pass(ACTIVITY, "no failed or uncertain operation is kept on it"),
            }
            checks.evidence(
                ACTIVITY,
                SdkValue::map([
                    ("id", SdkValue::text(&row.id)),
                    ("activity", SdkValue::text(&row.activity)),
                    ("mounted", SdkValue::Bool(row.mounted)),
                ]),
            );
        }
    }
    checks.fill(&mut node, CHECKS, false);
    node.coverage = super::report::Coverage::Shallow;
    node
}

/// The node of a Workspace whose checks were tried and gave no verdict.
fn unchecked(thing: Thing, reason: NoVerdict) -> Node {
    let mut node = Node::new(thing.clone());
    node.not_checked.push(NotChecked {
        thing,
        check: None,
        blocked_by: None,
        reason: Some(reason),
    });
    node.reason = Some(reason);
    node
}

/// The node of a Workspace no probe was tried on: it waits on Sandbox
/// `sandbox`, whose daemon's list did not reach it (`not_answering`).
fn waiting(thing: Thing, sandbox: &str) -> Node {
    let by = BlockedBy {
        thing: "sandbox",
        handle: sandbox.to_owned(),
        reason: "not_answering",
    };
    Node::blocked(thing, by)
}

/// The `workspace` kind's own run: only its Sandbox's doctor observes a
/// Workspace (from its daemon's list), so one it did not observe was not
/// tried here; it waits on that Sandbox, named before the `:` of its
/// handle.
pub(super) fn doctor(probe: &Probe<'_>, node: &mut Node) {
    let name = probe.thing.name.as_str();
    let sandbox = name.split_once(':').map_or(name, |(sandbox, _)| sandbox);
    *node = waiting(probe.thing.clone(), sandbox);
}
