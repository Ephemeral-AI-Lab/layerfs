//! The inspect kind `sandboxes_live` (layerfs-project#164): which of a
//! machine's Sandboxes have a daemon process that still runs, for
//! `sandbox.list`'s `live` option and `machine.stats`. The home names the
//! Sandboxes its records place on the machine; the side that runs them
//! (the home on `native`, the machine's worker otherwise, a Sandbox whose
//! daemon the home reaches directly included) reads each one's daemon
//! record under its own root and checks the process it names by id, start
//! time and boot ([`super::process::live`]). No daemon is asked.
use super::{process, Budget, Side};
use crate::record::SandboxRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::names::SandboxName;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

/// The inspect kind.
pub(super) const KIND: &str = "sandboxes_live";
/// The most names one request holds in one list.
pub(super) const MOST: usize = 4096;
/// Why a value is null: the request's soft budget ended before its turn.
pub(super) const DEADLINE: &str = "deadline";
/// Why `live` is null: the Sandbox's machine records no worker.
pub const MACHINE_STOPPED: &str = "machine_stopped";
/// Why `live` is null: the Sandbox's record on its machine is not read.
const UNREADABLE: &str = "record_unreadable";

/// Whether one Sandbox's daemon process runs: `sandbox.list`'s `live`,
/// and why it is not known when it is `None` (`live_reason`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Live {
    /// The process its daemon record names is that process and runs.
    pub live: Option<bool>,
    /// Why `live` is `None`: [`MACHINE_STOPPED`], `deadline`, `no_pid`,
    /// `record_unreadable`, or the code of the refusal its machine's
    /// request met, in snake_case (`unavailable`, `busy`, `invalid_input`
    /// from a worker of a build without the kind).
    pub reason: Option<String>,
}

impl Live {
    fn unknown(reason: &str) -> Self {
        Self {
            live: None,
            reason: Some(reason.to_owned()),
        }
    }
}

/// [`Live`] of each of `records`, in their order: one inspect request per
/// machine among them, asked at once when there are several, each under
/// the route's own admission and deadlines. A machine that records no
/// worker is not asked a second way and nothing starts it: its Sandboxes
/// are `None` with [`MACHINE_STOPPED`].
pub fn sandboxes_live(home: &Path, records: &[SandboxRecord]) -> Vec<Live> {
    // each machine of the page, with the places of its Sandboxes
    let mut machines: Vec<(&str, Vec<usize>)> = Vec::new();
    for (at, record) in records.iter().enumerate() {
        match machines.iter_mut().find(|(m, _)| *m == record.machine()) {
            Some((_, places)) => places.push(at),
            None => machines.push((record.machine(), vec![at])),
        }
    }
    let one = |(machine, places): &(&str, Vec<usize>)| {
        let names: Vec<&str> = places.iter().map(|at| records[*at].name()).collect();
        on_machine(home, machine, &names)
    };
    let one = &one;
    let answers: Vec<Vec<Live>> = match machines.as_slice() {
        [only] => vec![one(only)],
        several => std::thread::scope(|scope| {
            let asked: Vec<_> = (several.iter())
                .map(|machine| {
                    std::thread::Builder::new()
                        .name("inspect-live".to_owned())
                        .spawn_scoped(scope, move || one(machine))
                })
                .collect();
            (asked.into_iter().zip(several))
                .map(|(thread, (_, places))| {
                    (thread.ok())
                        .and_then(|thread| thread.join().ok())
                        .unwrap_or_else(|| vec![Live::unknown("failed"); places.len()])
                })
                .collect()
        }),
    };
    let mut live = vec![Live::default(); records.len()];
    for ((_, places), answer) in machines.iter().zip(answers) {
        for (at, found) in places.iter().zip(answer) {
            live[*at] = found;
        }
    }
    live
}

/// [`Live`] of the Sandboxes `names` of machine `machine`, in their order.
fn on_machine(home: &Path, machine: &str, names: &[&str]) -> Vec<Live> {
    let request = SdkValue::map([(
        "sandboxes",
        SdkValue::List(names.iter().map(|name| SdkValue::text(*name)).collect()),
    )]);
    let answer = match super::ask(home, machine, KIND, request) {
        Ok(answer) => answer,
        Err(refusal) => return vec![Live::unknown(&why(home, machine, &refusal)); names.len()],
    };
    let found = match answer.get("sandboxes") {
        Some(SdkValue::List(found)) => found.as_slice(),
        _ => &[],
    };
    (names.iter())
        .map(
            |name| match (found.iter()).find(|item| item.get_text("name") == Some(name)) {
                Some(item) => Live {
                    live: match item.get("live") {
                        Some(SdkValue::Bool(live)) => Some(*live),
                        _ => None,
                    },
                    reason: item.get_text("reason").map(str::to_owned),
                },
                None => Live::unknown("not_answered"),
            },
        )
        .collect()
}

/// Why machine `machine` did not say: [`MACHINE_STOPPED`] when it has a
/// worker and records none, else the refusal's code in snake_case.
fn why(home: &Path, machine: &str, refusal: &SdkRefusal) -> String {
    let worker = layerfs_machine::runs_here(home, machine).ok() == Some(false);
    if refusal.code == "Unavailable" && worker && !paths::worker_record(home, machine).exists() {
        return MACHINE_STOPPED.to_owned();
    }
    let mut code = String::new();
    for c in refusal.code.chars() {
        if c.is_ascii_uppercase() && !code.is_empty() {
            code.push('_');
        }
        code.push(c.to_ascii_lowercase());
    }
    code
}

/// The texts of list `field` of `request`, at most [`MOST`]; a request
/// without the list names none.
pub(super) fn listed(request: &SdkValue, field: &str) -> Result<Vec<String>, SdkRefusal> {
    let names: Option<Vec<String>> = match request.get(field) {
        None | Some(SdkValue::Null) => Some(Vec::new()),
        Some(SdkValue::List(items)) if items.len() <= MOST => (items.iter())
            .map(|item| match item {
                SdkValue::Text(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    };
    names.ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("an inspect request's `{field}` is a list of at most {MOST} names"),
        )
    })
}

/// The Sandbox names `request` holds, each checked as a name.
pub(super) fn names(request: &SdkValue) -> Result<Vec<String>, SdkRefusal> {
    let names = listed(request, "sandboxes")?;
    for name in &names {
        SandboxName::parse(name)?;
    }
    Ok(names)
}

/// One Sandbox's check: whether its daemon runs, else why that is not known.
pub(super) type Checked = (Option<bool>, Option<&'static str>);

/// Whether the daemon of each of Sandboxes `names` under `root` runs, and
/// whether every one was checked: once `budget` is spent the rest are
/// `None` with [`DEADLINE`]. A Sandbox with no record or no daemon record
/// under `root` has no process: `Some(false)`.
pub(super) fn checked(root: &Path, names: &[String], budget: &Budget) -> (Vec<Checked>, bool) {
    let mut complete = true;
    let one = |name: &String| {
        if budget.spent() {
            complete = false;
            return (None, Some(DEADLINE));
        }
        let record = SandboxRecord::read_in(&paths::sandbox_dir(root, name), name);
        match record.as_ref().map(|r| r.as_ref().and_then(|r| r.daemon())) {
            Err(_) => (None, Some(UNREADABLE)),
            Ok(None) => (Some(false), None),
            Ok(Some(daemon)) => match process::live(daemon) {
                Ok(live) => (Some(live), None),
                Err(reason) => (None, Some(reason)),
            },
        }
    };
    let checked = names.iter().map(one).collect();
    (checked, complete)
}

/// The request on the side that runs the Sandboxes: `{ sandboxes: [{
/// name, live, reason? }], complete }`, with `reason: "deadline"` when
/// the soft budget ended it early.
pub(super) fn serve(side: &Side<'_>, request: &SdkValue, budget: &Budget) -> Outcome {
    let (Side::Home(root) | Side::Worker(root)) = side;
    let names = names(request)?;
    let (checked, complete) = checked(root, &names, budget);
    let items = names.iter().zip(checked).map(|(name, (live, reason))| {
        let mut fields = vec![
            ("name".to_owned(), SdkValue::text(name)),
            (
                "live".to_owned(),
                live.map_or(SdkValue::Null, SdkValue::Bool),
            ),
        ];
        fields.extend(reason.map(|reason| ("reason".to_owned(), SdkValue::text(reason))));
        SdkValue::Map(fields)
    });
    let mut fields = vec![
        ("sandboxes".to_owned(), SdkValue::List(items.collect())),
        ("complete".to_owned(), SdkValue::Bool(complete)),
    ];
    if !complete {
        fields.push(("reason".to_owned(), SdkValue::text(DEADLINE)));
    }
    Ok(SdkValue::Map(fields))
}
