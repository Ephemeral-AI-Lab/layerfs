//! `workspace.list` (call surface 2.0): every mounted Workspace, or one
//! Sandbox's, from Workspace records alone; it asks no daemon. A Sandbox on
//! `native` or reached directly keeps its Workspace records in the home
//! (`sandbox/<name>/workspace/<id>.json`); every other Sandbox's are kept
//! by its machine's worker, asked once per ready machine with the names of
//! that machine's Sandboxes ([`OP`], [`in_worker`]). A machine that is not
//! ready is not contacted: its Sandboxes are named in `not_listed` with
//! `blocked_by`, as is a Sandbox that is not ready itself; a worker that
//! does not answer names them with `reason` (`timeout`, `busy`, `lost`, or
//! `refused` with its refusal in `message`). Rows are ordered by Sandbox,
//! then name, and paged by cursor: a cursor carries the `--sandbox` filter
//! and the last row's place.
use layerfs_home::home::paths;
use layerfs_home::json;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::state::State;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::record::discovery;
use layerfs_sandbox::record::{sandboxes, SandboxRecord, WorkspaceRecord};
use layerfs_store::place::{self, Executor};
use std::collections::BTreeMap;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The worker operation that lists the Workspace records of named Sandboxes.
pub const OP: &str = "workspace.list";
/// Rows in a page when the caller names no limit.
const DEFAULT_LIMIT: i64 = 100;
/// The most rows in one page.
const MAX_LIMIT: i64 = 1000;
/// A page ends early, with a cursor, past this many bytes of rows.
const PAGE_BYTES: usize = 65536;
/// What a cursor of this call says it is for.
const CURSOR_FOR: &str = "workspace.list";

/// `workspace.list({ sandbox, limit, cursor })`.
#[derive(Clone, Debug, Default)]
pub struct ListRequest {
    /// Only this Sandbox's Workspaces.
    pub sandbox: Option<String>,
    /// The most rows in one page, 1 to 1000.
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// One Workspace row: its Sandbox, name and record.
struct Row {
    sandbox: String,
    name: String,
    item: SdkValue,
}

/// The row of Workspace record `record` of Sandbox `sandbox` (of project
/// `project`). A record from before every Workspace had a name is named by
/// its path.
fn row(sandbox: &str, project: Option<&str>, record: &SdkValue) -> Row {
    let name = (record.get_text("name").or(record.get_text("path")))
        .unwrap_or("")
        .to_owned();
    let mounted = record.get_int("created").map_or(SdkValue::Null, |ms| {
        SdkValue::Text(layerfs_home::time::iso(ms))
    });
    let item = SdkValue::map([
        ("workspace", SdkValue::Text(format!("{sandbox}:{name}"))),
        ("branch", record.get("branch").cloned().unwrap_or_default()),
        ("sandbox", SdkValue::text(sandbox)),
        ("project", SdkValue::opt_text(project)),
        ("path", record.get("path").cloned().unwrap_or_default()),
        ("mounted_at", mounted),
    ]);
    Row {
        sandbox: sandbox.to_owned(),
        name,
        item,
    }
}

/// The records a Sandbox folder `dir` holds, each `{ name, branch, path,
/// created }`.
fn records_in(dir: &Path) -> Result<Vec<SdkValue>> {
    let found = discovery::workspaces(dir)?;
    Ok(found.iter().map(|(_, record)| shown(record)).collect())
}

fn shown(record: &WorkspaceRecord) -> SdkValue {
    SdkValue::map([
        ("name", SdkValue::opt_text(record.name())),
        ("branch", SdkValue::opt_text(record.branch())),
        ("path", SdkValue::opt_text(record.path())),
        (
            "created",
            record.created().map_or(SdkValue::Null, SdkValue::Int),
        ),
    ])
}

/// What a worker's `workspace.list` request runs: `{ sandboxes: [name] }`
/// answered `{ sandboxes: [{ sandbox, workspaces: [record] }] }` from the
/// Sandbox folders under its `root`; a Sandbox it holds no folder of has
/// no records. It reads files only.
pub fn in_worker(root: &Path, args: &SdkValue) -> Outcome {
    let names = match args.get("sandboxes") {
        Some(SdkValue::List(names)) => names.clone(),
        _ => {
            return Err(refused(
                "InvalidInput",
                "workspace.list names its Sandboxes",
            ))
        }
    };
    let mut answered = Vec::new();
    for name in names {
        let SdkValue::Text(name) = name else { continue };
        let name = layerfs_home::names::SandboxName::parse(&name)?;
        let dir = paths::sandbox_dir(root, &name);
        answered.push(SdkValue::map([
            ("sandbox", SdkValue::text(name.as_str())),
            ("workspaces", SdkValue::List(records_in(&dir)?)),
        ]));
    }
    Ok(SdkValue::map([("sandboxes", SdkValue::List(answered))]))
}

/// One `not_listed` entry: `{ sandbox, blocked_by, reason, message }`.
fn not_listed(
    sandbox: &str,
    by: Option<SdkValue>,
    reason: Option<&str>,
    message: String,
) -> SdkValue {
    SdkValue::map([
        ("sandbox", SdkValue::text(sandbox)),
        ("blocked_by", by.unwrap_or_default()),
        ("reason", SdkValue::opt_text(reason)),
        ("message", SdkValue::Text(message)),
    ])
}

/// The cursor after row `(sandbox, name)` of a call filtered by `filter`.
fn cursor(filter: Option<&str>, last: &Row) -> String {
    let mut fields = vec![
        ("for".to_owned(), SdkValue::text(CURSOR_FOR)),
        (
            "after".to_owned(),
            SdkValue::List(vec![
                SdkValue::text(&last.sandbox),
                SdkValue::text(&last.name),
            ]),
        ),
    ];
    fields.extend(filter.map(|sandbox| ("sandbox".to_owned(), SdkValue::text(sandbox))));
    json::base64(json::print(&SdkValue::Map(fields)).as_bytes())
        .trim_end_matches('=')
        .to_owned()
}

/// The place `text` names and the filter it carries; refused
/// `InvalidInput` when it is no cursor of this call or `filter` differs.
fn resume(text: &str, filter: Option<&str>) -> Result<((String, String), Option<String>)> {
    let wrong = || {
        refused(
            "InvalidInput",
            "workspace.list: `cursor` is not the `next_cursor` of a workspace.list page",
        )
    };
    let padded = format!("{text}{}", "=".repeat((4 - text.len() % 4) % 4));
    let made = json::unbase64(&padded)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| json::parse(&text).ok())
        .filter(|value| value.get_text("for") == Some(CURSOR_FOR))
        .ok_or_else(wrong)?;
    let carried = made.get_text("sandbox").map(str::to_owned);
    if filter.is_some() && filter != carried.as_deref() {
        return Err(refused(
            "InvalidInput",
            "workspace.list: the cursor was made with another --sandbox",
        ));
    }
    let place = match made.get("after") {
        Some(SdkValue::List(pair)) => match pair.as_slice() {
            [SdkValue::Text(sandbox), SdkValue::Text(name)] => (sandbox.clone(), name.clone()),
            _ => return Err(wrong()),
        },
        _ => return Err(wrong()),
    };
    Ok((place, carried))
}

/// Why a worker's refusal leaves its Sandboxes out.
fn unanswered(refusal: &SdkRefusal) -> &'static str {
    match refusal.code.as_str() {
        "Busy" => "busy",
        "Unavailable" | "Unknown" if refusal.message.contains("in time") => "timeout",
        "Unavailable" | "Unknown" => "lost",
        _ => "refused",
    }
}

/// `workspace.list`: `{ items, next_cursor, not_listed }`.
pub fn list(home: &Path, request: &ListRequest) -> Outcome {
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(refused(
            "InvalidInput",
            format!("workspace.list: `limit` is 1 to {MAX_LIMIT}"),
        ));
    }
    let (after, filter) = match request.cursor.as_deref() {
        Some(text) => {
            let (place, carried) = resume(text, request.sandbox.as_deref())?;
            (Some(place), carried)
        }
        None => (None, request.sandbox.clone()),
    };
    let mut records = match &filter {
        Some(name) => vec![layerfs_sandbox::family::sandbox_of(home, name)?],
        None => sandboxes(home)?,
    };
    records.sort_by(|a, b| a.name().cmp(b.name()));
    records.retain(|record| {
        after
            .as_ref()
            .is_none_or(|(sandbox, _)| record.name() >= sandbox.as_str())
    });
    let mut rows = Vec::new();
    let mut skipped: Vec<(String, SdkValue)> = Vec::new();
    // the Sandboxes each ready machine's worker is asked for
    let mut asked: BTreeMap<String, Vec<&SandboxRecord>> = BTreeMap::new();
    for record in &records {
        let (name, machine) = (record.name(), record.machine());
        let blocked = layerfs_sandbox::status::state::machine_state(home, machine)?
            .blocks("machine", machine)
            .or_else(|| record.status(&State::Ready).blocks("sandbox", name));
        if let Some(State::Blocked(by)) = blocked {
            let message = format!("{} {} is {}", by.thing, by.handle, by.reason);
            skipped.push((
                name.to_owned(),
                not_listed(name, Some(by.to_sdk()), None, message),
            ));
            continue;
        }
        let local = layerfs_sandbox::route::direct::forwarded(record)
            || matches!(place::on_machine_read(home, machine), Ok(Executor::Local));
        if local {
            for value in records_in(&record.dir(home))? {
                rows.push(row(name, record.project(), &value));
            }
        } else {
            asked.entry(machine.to_owned()).or_default().push(record);
        }
    }
    for (machine, held) in asked {
        let names = held
            .iter()
            .map(|record| SdkValue::text(record.name()))
            .collect();
        let args = SdkValue::map([("sandboxes", SdkValue::List(names))]);
        let answer = match place::on_machine_read(home, &machine) {
            Ok(Executor::Worker(mut worker)) => worker.read(OP, args, place::READ),
            Ok(Executor::Local) => Err(refused("Failed", "the machine runs here")),
            Err(refusal) => Err(refusal),
        };
        let answer = match answer {
            Ok(answer) => answer,
            Err(refusal) => {
                let reason = unanswered(&refusal);
                for record in held {
                    let entry =
                        not_listed(record.name(), None, Some(reason), refusal.message.clone());
                    skipped.push((record.name().to_owned(), entry));
                }
                continue;
            }
        };
        let Some(SdkValue::List(listed)) = answer.get("sandboxes") else {
            continue;
        };
        for listed in listed {
            let name = listed.get_text("sandbox").unwrap_or("");
            let Some(record) = held.iter().find(|record| record.name() == name) else {
                continue;
            };
            if let Some(SdkValue::List(values)) = listed.get("workspaces") {
                for value in values {
                    rows.push(row(name, record.project(), value));
                }
            }
        }
    }
    rows.sort_by(|a, b| (&a.sandbox, &a.name).cmp(&(&b.sandbox, &b.name)));
    if let Some((sandbox, name)) = &after {
        rows.retain(|row| (&row.sandbox, &row.name) > (sandbox, name));
    }
    let (mut page, mut bytes, mut more) = (Vec::new(), 0usize, false);
    for row in rows {
        if page.len() as i64 == limit || bytes > PAGE_BYTES {
            more = true;
            break;
        }
        bytes += json::print(&row.item).len();
        page.push(row);
    }
    let next = match (more, page.last()) {
        (true, Some(last)) => SdkValue::Text(cursor(filter.as_deref(), last)),
        _ => SdkValue::Null,
    };
    // the Sandboxes this page reaches: all of them once it is the last
    let reached = |sandbox: &str| match (more, page.last()) {
        (true, Some(last)) => sandbox <= last.sandbox.as_str(),
        _ => true,
    };
    let skipped = (skipped.into_iter())
        .filter(|(sandbox, _)| reached(sandbox))
        .filter(|(sandbox, _)| {
            after
                .as_ref()
                .is_none_or(|(at, _)| sandbox.as_str() > at.as_str())
        })
        .map(|(_, entry)| entry)
        .collect();
    Ok(SdkValue::map([
        (
            "items",
            SdkValue::List(page.into_iter().map(|row| row.item).collect()),
        ),
        ("next_cursor", next),
        ("not_listed", SdkValue::List(skipped)),
    ]))
}
