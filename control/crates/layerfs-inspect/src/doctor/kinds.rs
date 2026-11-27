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
use super::runner::{CheckId, KindDoctor, Located, Probe, Registry};
use super::report::{Kind, Node, Thing};
use super::budget::Checks;
use crate::home::tree;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::home::{detect, paths, Layout};
use layerfs_home::outcome::refused;
use layerfs_home::state::{BlockedBy, Reason, State};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The home's checks.
const HOME: &[CheckId] = &[
    CheckId::own("home.build"),
    CheckId::own("home.prerequisites"),
    CheckId::own("home.layout"),
    CheckId::own("home.records"),
    CheckId::own("home.bundle"),
    CheckId::own("home.host_daemon"),
    CheckId::own("home.artifacts"),
    CheckId::own("home.capacity"),
];

/// A machine's checks.
const MACHINE: &[CheckId] = &[
    CheckId::own("machine.record"),
    CheckId::own("machine.operation"),
    CheckId::own("machine.configuration"),
    CheckId::own("machine.provider"),
    CheckId::own("machine.state"),
    CheckId::own("machine.worker.endpoint"),
    CheckId::own("machine.worker.auth"),
    CheckId::own("machine.worker.compatibility"),
    CheckId::own("machine.worker.ping"),
    CheckId::own("machine.worker.retained"),
    CheckId::own("machine.capacity"),
    CheckId::own("machine.network"),
];

/// A Sandbox's checks; its Workspaces' are `workspace_target::CHECKS`.
const SANDBOX: &[CheckId] = &[
    CheckId::own("sandbox.record"),
    CheckId::own("sandbox.machine_reference"),
    CheckId::own("sandbox.store_reference"),
    CheckId::own("sandbox.project_reference"),
    CheckId::own("sandbox.store_placement"),
    CheckId::own("sandbox.operation"),
    CheckId::own("sandbox.runtime"),
    CheckId::own("sandbox.network"),
    CheckId::own("sandbox.route"),
    CheckId::own("sandbox.forward_process"),
    CheckId::own("sandbox.worker_compatibility"),
    CheckId::own("sandbox.daemon_keys"),
    CheckId::own("sandbox.daemon_connect"),
    CheckId::own("sandbox.daemon_auth"),
    CheckId::own("sandbox.daemon_instance"),
    CheckId::own("sandbox.daemon_ready"),
    CheckId::own("sandbox.session"),
    CheckId::own("sandbox.observation"),
];

/// A bucket's checks; the write test is its Full check.
const BUCKET: &[CheckId] = &[
    CheckId::own("bucket.configuration"),
    CheckId::own("bucket.credentials"),
    CheckId::own("bucket.transport"),
    CheckId::own("bucket.list"),
    CheckId::full("bucket.write_capability"),
    CheckId::full("bucket.put"),
    CheckId::full("bucket.get"),
    CheckId::full("bucket.delete"),
    CheckId::full("bucket.reconcile"),
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
        checks: super::super::store::doctor::CHECKS,
        budget: Duration::from_secs(10),
        run: super::super::store::doctor::run,
    },
    KindDoctor {
        kind: Kind::Project,
        checks: super::super::store::project::CHECKS,
        budget: Duration::from_secs(10),
        run: super::super::store::project::run,
    },
    KindDoctor {
        kind: Kind::Sandbox,
        checks: SANDBOX,
        budget: Duration::from_secs(30),
        run: super::super::sandbox::doctor::doctor,
    },
    KindDoctor {
        kind: Kind::Workspace,
        checks: super::super::sandbox::workspace::CHECKS,
        budget: Duration::from_secs(10),
        run: super::super::sandbox::workspace::doctor,
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
    let mut checks = Checks::default();
    super::super::home::doctor::diagnose(probe.home, probe.options.build, probe.budget, &mut checks);
    checks.fill(node, probe.checks, probe.budget.expired());
}

fn machine(probe: &Probe<'_>, node: &mut Node) {
    let name = probe.thing.name.as_str();
    let mut checks = Checks::default();
    let found = super::super::machine::doctor::diagnose(probe.home, name, probe.budget, &mut checks);
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
    let mut checks = Checks::default();
    super::super::backup_target::diagnose(probe.home, name, probe.full, probe.budget, &mut checks);
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
    // the tree lists every record that reads (R4 F5); a home folder it
    // cannot list leaves the home with no children (its records check names
    // it), and any other doctor is refused
    let tree = match current.then(|| tree::tree(home)) {
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
    // a machine only a Store or Sandbox names has no record of its own:
    // it is not there, and the record that names it reports the reference
    let recorded = |node: &&SdkValue| {
        let name = node.get_text("machine").unwrap_or("");
        layerfs_home::names::BUILT_IN.contains(&name) || paths::machine_record(home, name).exists()
    };
    // a record the tree could not place (unreadable) is still there:
    // its own `.record` check names it
    let plain = !thing.name.contains('/') && !thing.name.starts_with('.');
    let unplaced = |record: PathBuf| match plain && record.exists() {
        true => Ok(Located {
            state: State::broken(Reason::Failed, None),
            blocked: None,
            children: Vec::new(),
        }),
        false => Err(missing()),
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
            let mut children: Vec<_> = (machines.iter().filter(recorded))
                .filter_map(|node| {
                    let name = node.get_text("machine")?;
                    Some((Thing::new(Kind::Machine, name), state_of(node)))
                })
                .collect();
            children.extend(things(&tree, "stores", Kind::Store));
            children.extend(things(&tree, "buckets", Kind::Bucket));
            at(State::Ready, children)
        }
        Kind::Machine => {
            let Some(node) = machines
                .iter()
                .filter(recorded)
                .find(|m| named(m, "machine"))
            else {
                return unplaced(paths::machine_record(home, &thing.name));
            };
            let mut children = things(node, "stores", Kind::Store);
            children.extend(things(node, "sandboxes", Kind::Sandbox));
            at(state_of(node), children)
        }
        Kind::Store => {
            let Some(node) = stores().find(|s| named(s, "store")) else {
                return unplaced(paths::store_record(home, &thing.name));
            };
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
            let Some(node) = placed.find(|s| named(s, "sandbox")) else {
                return unplaced(paths::sandbox_record(home, &thing.name));
            };
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
