//! The one runner of the six doctors (call surface 2.0). A doctor checks
//! its own thing, then gives each thing directly on it a shallow check
//! (that kind's own checks without its Full-only ones, nothing beneath).
//! Each thing runs under its own time budget and sends one progress event
//! when it is finished: `{ phase, done, total, node }`, `phase` `self` for
//! the named thing, else the list word of the kind being checked, `done`
//! and `total` counting things (the total is known from records before the
//! first check). A parent that failed or is stopped blocks what is on it:
//! those things are not tried, each one `not_checked` with `blocked_by`
//! naming the root of the block. Nothing is retried.
//!
//! Checks register per kind ([`KindDoctor`]: the ids its checks can give,
//! in the order they run, each marked when only a Full check makes it, and
//! the function that makes them), so a kind's package changes its own list
//! and functions and nothing here. The real kinds are in `kinds`; a test
//! hands [`run_with`] made-up ones.
use super::report::{Coverage, Kind, NoVerdict, Node, NotChecked, Report, Thing};
use super::target::Budget;
use crate::home::Build;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::events::Events;
use layerfs_home::state::{BlockedBy, State};
use std::path::Path;
use std::time::Duration;

/// One check a kind can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckId {
    /// Its id, as reports show it.
    pub id: &'static str,
    /// Whether only the kind's Full check makes it.
    pub full_only: bool,
}

impl CheckId {
    /// A check of the kind's own shallow list.
    pub const fn own(id: &'static str) -> Self {
        CheckId {
            id,
            full_only: false,
        }
    }

    /// A check only the kind's Full check makes.
    pub const fn full(id: &'static str) -> Self {
        CheckId {
            id,
            full_only: true,
        }
    }
}

/// What a doctor call asks beyond its thing.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The Full check of the named thing (`--full`).
    pub full: bool,
    /// The build making the call, which the home's daemon check compares
    /// the native daemon's stamp with.
    pub build: Build<'static>,
    /// One more machine to probe a PostgreSQL Store's connection from
    /// (`store doctor --machine`).
    pub machine: Option<String>,
}

/// What a kind's check function is given for one thing.
pub struct Probe<'a> {
    /// The home.
    pub home: &'a Path,
    /// The thing.
    pub thing: &'a Thing,
    /// Its kind's list, which every row it gives must be in.
    pub checks: &'static [CheckId],
    /// Whether this is its Full check.
    pub full: bool,
    /// Its own time budget (none for a Full check).
    pub budget: &'a Budget,
    /// Its state as the home's records say it.
    pub state: &'a State,
    /// What above it is not ready, as the records say: its live checks
    /// wait on that.
    pub blocked: Option<&'a BlockedBy>,
    /// The call's options.
    pub options: &'a Options,
}

/// One kind's doctor.
#[derive(Clone, Copy)]
pub struct KindDoctor {
    /// The kind.
    pub kind: Kind,
    /// The checks it can give, in the order they run.
    pub checks: &'static [CheckId],
    /// The time budget of one thing of the kind.
    pub budget: Duration,
    /// Makes the checks of one thing into its node: its rows, findings and
    /// `not_checked` entries, and in `gate` what blocks the things on it.
    pub run: fn(&Probe<'_>, &mut Node),
}

/// Where a thing sits, from the home's records.
pub struct Located {
    /// Its state.
    pub state: State,
    /// What above it is not ready.
    pub blocked: Option<BlockedBy>,
    /// The things directly on it, each with its state.
    pub children: Vec<(Thing, State)>,
}

/// The kinds a runner calls, and how it finds a thing and what is on it.
#[derive(Clone, Copy)]
pub struct Registry {
    /// One doctor per kind.
    pub kinds: &'static [KindDoctor],
    /// Finds a thing; a thing that is not there is a refusal.
    pub locate: fn(&Path, &Thing) -> Result<Located, SdkRefusal>,
}

impl Registry {
    /// The doctor of `kind`, when it has one.
    pub fn kind(&self, kind: Kind) -> Option<&'static KindDoctor> {
        self.kinds.iter().find(|doctor| doctor.kind == kind)
    }
}

/// The doctor of `root` with the real kinds: its report, once made.
pub fn run(
    home: &Path,
    root: Thing,
    options: &Options,
    events: &mut Events<'_>,
) -> Result<Report, SdkRefusal> {
    run_with(&super::kinds::REGISTRY, home, root, options, events)
}

/// The doctor of `root` with `registry`'s kinds: `root` under its own
/// budget (its Full check when `options.full`), then a shallow check of
/// each thing on it, or, when `root` blocks them, a `not_checked` node each
/// that tried nothing. Refused only when `root` cannot be found.
pub fn run_with(
    registry: &Registry,
    home: &Path,
    root: Thing,
    options: &Options,
    events: &mut Events<'_>,
) -> Result<Report, SdkRefusal> {
    let located = (registry.locate)(home, &root)?;
    let total = 1 + located.children.len();
    let mut tree = visit(
        registry,
        home,
        &root,
        options.full,
        (&located.state, located.blocked.as_ref()),
        options,
    );
    finished(events, "self", 1, total, &tree);
    for (at, (child, state)) in located.children.into_iter().enumerate() {
        let phase = child.kind.list();
        let node = match &tree.gate {
            Some(by) => Node::blocked(child, by.clone()),
            None => visit(registry, home, &child, false, (&state, None), options),
        };
        finished(events, phase, at + 2, total, &node);
        tree.children.push(node);
    }
    Ok(Report {
        tree,
        observed_ms: layerfs_home::home::now_ms(),
    })
}

/// One thing's own checks, under its own budget.
fn visit(
    registry: &Registry,
    home: &Path,
    thing: &Thing,
    full: bool,
    (state, blocked): (&State, Option<&BlockedBy>),
    options: &Options,
) -> Node {
    let mut node = Node::new(thing.clone());
    let Some(doctor) = registry.kind(thing.kind) else {
        return node;
    };
    let full = full && doctor.checks.iter().any(|check| check.full_only);
    // a Full check has no budget: it runs until it finishes or is stopped
    let budget = Budget::new((!full).then_some(doctor.budget));
    // what blocks a thing blocks what is on it
    node.gate = blocked.cloned();
    let probe = Probe {
        home,
        thing,
        checks: doctor.checks,
        full,
        budget: &budget,
        state,
        blocked,
        options,
    };
    (doctor.run)(&probe, &mut node);
    let tried = node.blocked_by.is_none() && node.reason.is_none();
    if tried && node.rows.is_empty() && budget.expired() {
        node.reason = Some(NoVerdict::Timeout);
        node.not_checked.push(NotChecked {
            thing: thing.clone(),
            check: None,
            blocked_by: None,
            reason: Some(NoVerdict::Timeout),
        });
    }
    node.coverage = match (node.blocked_by.is_none() && node.reason.is_none(), full) {
        (false, _) => Coverage::None,
        (true, true) => Coverage::Full,
        (true, false) => Coverage::Shallow,
    };
    node
}

/// One progress event for a finished thing.
fn finished(events: &mut Events<'_>, phase: &str, done: usize, total: usize, node: &Node) {
    let count = |n: usize| Some(i64::try_from(n).unwrap_or(i64::MAX));
    events.phase(
        phase,
        count(done),
        count(total),
        vec![("node", node.to_sdk(false))],
    );
}
