//! The real kinds of the doctor frame: each kind's list of the check ids
//! its checks give, in the order they run, and the function that runs the
//! checks of one thing into its node. The home's and a machine's lists are
//! the frozen ones (`calls.json` `doctor.checks`); ids of kinds not renamed
//! yet keep today's names; the per-kind packages rename them, and a test
//! compares every list with the frozen one through a pending list.
//!
//! Where things are comes from the home's records (`home_tree`): what is on
//! a thing and its recorded state. A machine blocks what is on it with what
//! its own checks found (stopped, `engine_down`, `missing`, `failed`,
//! `not_answering`, `outdated`, `busy`, `unresolved`, `unsupported`), else
//! with what its records say when its checks gave no verdict.
use super::frame::{CheckId, KindDoctor, Located, Probe, Registry};
use super::report::{Kind, Node, Thing};
use super::target::Checks;
use crate::home_tree;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{detect, Layout};
use layerfs_home::outcome::refused;
use layerfs_home::state::{BlockedBy, Reason, State};
use std::path::Path;
use std::time::Duration;

const fn own(id: &'static str) -> CheckId {
    CheckId::own(id)
}

const fn full(id: &'static str) -> CheckId {
    CheckId::full(id)
}

/// The home's checks.
const HOME: &[CheckId] = &[
    own("home.build"),
    own("home.prerequisites"),
    own("home.layout"),
    own("home.records"),
    own("home.bundle"),
    own("home.native_daemon"),
    own("home.artifacts"),
    own("home.capacity"),
];

/// A machine's checks.
const MACHINE: &[CheckId] = &[
    own("machine.record"),
    own("machine.operation"),
    own("machine.configuration"),
    own("machine.provider"),
    own("machine.state"),
    own("machine.worker.endpoint"),
    own("machine.worker.auth"),
    own("machine.worker.compatibility"),
    own("machine.worker.ping"),
    own("machine.capacity"),
];

/// A Sandbox's checks; its Workspaces' are `workspace_target::CHECKS`.
const SANDBOX: &[CheckId] = &[
    own("sandbox.record"),
    own("sandbox.machine_reference"),
    own("sandbox.store_reference"),
    own("sandbox.project_reference"),
    own("sandbox.store_placement"),
    own("sandbox.operation"),
    own("sandbox.runtime"),
    own("sandbox.route"),
    own("sandbox.forward_process"),
    own("sandbox.worker_compatibility"),
    own("sandbox.daemon_keys"),
    own("sandbox.daemon_connect"),
    own("sandbox.daemon_auth"),
    own("sandbox.daemon_instance"),
    own("sandbox.daemon_ready"),
    own("sandbox.session"),
    own("sandbox.observation"),
];

/// A bucket's checks; the write test is its Full check.
const BUCKET: &[CheckId] = &[
    own("bucket.configuration"),
    own("bucket.credentials"),
    own("bucket.transport"),
    own("bucket.list"),
    full("bucket.write_capability"),
    full("bucket.put"),
    full("bucket.get"),
    full("bucket.delete"),
    full("bucket.reconcile"),
];

/// The real kinds. A Workspace has no doctor of its own: its Sandbox's
/// doctor makes its node from the daemon's list.
pub(super) static KINDS: [KindDoctor; 7] = [
    KindDoctor {
        kind: Kind::Home,
        checks: HOME,
        budget: Duration::from_secs(30),
        run: home,
    },
    KindDoctor {
        kind: Kind::Machine,
        checks: MACHINE,
        budget: Duration::from_secs(30),
        run: machine,
    },
    KindDoctor {
        kind: Kind::Store,
        checks: super::store_target::CHECKS,
        budget: Duration::from_secs(10),
        run: super::store_target::run,
    },
    KindDoctor {
        kind: Kind::Project,
        checks: super::project_target::CHECKS,
        budget: Duration::from_secs(10),
        run: super::project_target::run,
    },
    KindDoctor {
        kind: Kind::Sandbox,
        checks: SANDBOX,
        budget: Duration::from_secs(30),
        run: super::sandbox_target::doctor,
    },
    KindDoctor {
        kind: Kind::Workspace,
        checks: super::workspace_target::CHECKS,
        budget: Duration::from_secs(10),
        run: super::workspace_target::doctor,
    },
    KindDoctor {
        kind: Kind::Bucket,
        checks: BUCKET,
        budget: Duration::from_secs(10),
        run: bucket,
    },
];

/// The real kinds and where things are in a home.
pub(super) static REGISTRY: Registry = Registry {
    kinds: &KINDS,
    locate,
};

/// The real kinds, each with its list of the check ids it can give.
pub fn kinds() -> &'static [KindDoctor] {
    &KINDS
}

/// What `state` of the `word` thing named `name` blocks: the root of the
/// block, when it is not ready.
fn blocker(state: &State, word: &'static str, name: &str) -> Option<BlockedBy> {
    match state.blocks(word, name) {
        Some(State::Blocked(by)) => Some(by),
        _ => None,
    }
}

fn home(probe: &Probe<'_>, node: &mut Node) {
    let mut checks = Checks::new("home", &probe.thing.name);
    super::host_target::diagnose(probe.home, probe.options.build, probe.budget, &mut checks);
    checks.fill(node, probe.checks, probe.budget.expired());
}

fn machine(probe: &Probe<'_>, node: &mut Node) {
    let name = probe.thing.name.as_str();
    let mut checks = Checks::new("machine", name);
    let found = super::machine_target::diagnose(probe.home, name, probe.budget, &mut checks);
    checks.fill(node, probe.checks, probe.budget.expired());
    // what its own checks found blocks what is on it (a machine they found
    // ready blocks nothing); when they gave no verdict, its records say
    let gate = match found {
        Ok(()) => None,
        Err(Some(reason)) => Some(BlockedBy {
            thing: "machine",
            handle: name.to_owned(),
            reason,
        }),
        Err(None) => blocker(probe.state, "machine", name),
    };
    if node.gate.is_none() {
        node.gate = gate;
    }
}

fn bucket(probe: &Probe<'_>, node: &mut Node) {
    let name = probe.thing.name.as_str();
    let mut checks = Checks::new("bucket", name);
    super::backup_target::diagnose(probe.home, name, probe.full, probe.budget, &mut checks);
    checks.fill(node, probe.checks, probe.budget.expired());
}

/// A recorded state as `home.get --with tree` shows it.
fn state_of(node: &SdkValue) -> State {
    let text = |key: &str| node.get_text(key).map(str::to_owned);
    match node.get_text("state") {
        Some("stopped") => State::Stopped,
        Some("busy") => {
            let op = node.get("op").cloned().unwrap_or_default();
            State::Busy {
                name: op.get_text("name").unwrap_or("").to_owned(),
                phase: op.get_text("phase").unwrap_or("").to_owned(),
                started_ms: None,
            }
        }
        Some("blocked") => node
            .get("blocked_by")
            .and_then(blocked_of)
            .map_or(State::Unknown { observed_ms: None }, State::Blocked),
        Some("broken") => State::Broken {
            reason: (Reason::ALL.into_iter())
                .find(|reason| node.get_text("reason") == Some(reason.word()))
                .unwrap_or(Reason::Failed),
            fix: text("fix"),
        },
        Some("unknown") => State::Unknown { observed_ms: None },
        _ => State::Ready,
    }
}

/// `{ <thing>, reason }` as a recorded state's `blocked_by` shows it.
fn blocked_of(by: &SdkValue) -> Option<BlockedBy> {
    let (thing, handle) = ["machine", "store", "sandbox"]
        .into_iter()
        .find_map(|word| by.get_text(word).map(|handle| (word, handle.to_owned())))?;
    let said = by.get_text("reason")?;
    let reason = ["stopped", "busy"]
        .into_iter()
        .chain(Reason::ALL.iter().map(|reason| reason.word()))
        .find(|word| *word == said)?;
    Some(BlockedBy {
        thing,
        handle,
        reason,
    })
}

fn list(value: Option<&SdkValue>) -> &[SdkValue] {
    match value {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

/// The nodes of `kind` under `key` of `parent`, each with its state.
fn things(parent: &SdkValue, key: &str, kind: Kind) -> Vec<(Thing, State)> {
    let word = kind.word();
    (list(parent.get(key)).iter())
        .filter_map(|node| Some((Thing::new(kind, node.get_text(word)?), state_of(node))))
        .collect()
}

/// Finds `thing` in the home's records: its state, what above it blocks
/// it, and the things directly on it. A thing that is not there is
/// `NotFound`; the home itself always is, with nothing on it until it is a
/// current-layout home.
fn locate(home: &Path, thing: &Thing) -> Result<Located, SdkRefusal> {
    let current = match detect(home) {
        Ok(Layout::Current(_)) => true,
        Ok(_) => false,
        Err(refusal) if thing.kind != Kind::Home => return Err(refusal),
        Err(_) => false,
    };
    // a record the tree cannot read leaves the home with no children: its
    // own records check names that record; any other doctor is refused
    let tree = match current.then(|| home_tree::tree(home)) {
        None => SdkValue::Null,
        Some(Ok(tree)) => tree,
        Some(Err(_)) if thing.kind == Kind::Home => SdkValue::Null,
        Some(Err(refusal)) => return Err(refusal),
    };
    let machines = list(tree.get("machines"));
    let stores = || {
        let placed = machines.iter().flat_map(|m| list(m.get("stores")).iter());
        placed.chain(list(tree.get("stores")))
    };
    let named = |node: &&SdkValue, word: &str| node.get_text(word) == Some(&thing.name);
    let missing = || {
        let word = thing.kind.word();
        refused("NotFound", format!("no {word} named {}", thing.name))
    };
    let at = |state: State, children| {
        let blocked = match &state {
            State::Blocked(by) => Some(by.clone()),
            _ => None,
        };
        Ok(Located {
            state,
            blocked,
            children,
        })
    };
    match thing.kind {
        Kind::Home => {
            let mut children = things(&tree, "machines", Kind::Machine);
            children.extend(things(&tree, "stores", Kind::Store));
            children.extend(things(&tree, "buckets", Kind::Bucket));
            at(State::Ready, children)
        }
        Kind::Machine => {
            let node = machines
                .iter()
                .find(|m| named(m, "machine"))
                .ok_or_else(missing)?;
            let mut children = things(node, "stores", Kind::Store);
            children.extend(things(node, "sandboxes", Kind::Sandbox));
            at(state_of(node), children)
        }
        Kind::Store => {
            let node = stores().find(|s| named(s, "store")).ok_or_else(missing)?;
            let projects = list(node.get("projects")).iter().filter_map(|p| match p {
                SdkValue::Text(project) => Some((
                    Thing::new(Kind::Project, format!("{}/{project}", thing.name)),
                    State::Ready,
                )),
                _ => None,
            });
            at(state_of(node), projects.collect())
        }
        Kind::Project => {
            let (store, project) = thing.name.split_once('/').ok_or_else(missing)?;
            let node =
                (stores().find(|s| s.get_text("store") == Some(store))).ok_or_else(missing)?;
            let recorded = list(node.get("projects"));
            if !recorded.contains(&SdkValue::text(project)) {
                return Err(missing());
            }
            Ok(Located {
                state: State::Ready,
                blocked: blocker(&state_of(node), "store", store),
                children: Vec::new(),
            })
        }
        Kind::Sandbox => {
            let mut placed = machines
                .iter()
                .flat_map(|m| list(m.get("sandboxes")).iter());
            let node = placed.find(|s| named(s, "sandbox")).ok_or_else(missing)?;
            // the Workspaces the home records; its doctor adds those its
            // daemon lists
            at(state_of(node), things(node, "workspaces", Kind::Workspace))
        }
        Kind::Bucket => {
            let found = list(tree.get("buckets"))
                .iter()
                .any(|b| named(&b, "bucket"));
            match found {
                true => at(State::Ready, Vec::new()),
                false => Err(missing()),
            }
        }
        Kind::Workspace => Err(missing()),
    }
}
