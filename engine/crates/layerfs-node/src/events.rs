//! Same-client opened-project subscriptions. Queues live only in this process;
//! an operation emits only when its private routing ID names a live queue.
use crate::job::{refused, Outcome};
use crate::registry::lock;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::value_cost;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const CAPACITY: usize = 1024;
const REPLY_BUDGET: usize = 49_152;
const MAX_SAFE: i64 = 9_007_199_254_740_991;
static SUBSCRIPTIONS: LazyLock<Mutex<BTreeMap<String, Arc<Subscription>>>> =
    LazyLock::new(Default::default);
static POLLS: AtomicUsize = AtomicUsize::new(0);

struct Event {
    seq: i64,
    value: SdkValue,
    cost: usize,
}

struct State {
    seq: i64,
    minimum: i64,
    events: VecDeque<Event>,
    stale: bool,
    closed: bool,
    exhausted: bool,
    waiting: bool,
}

struct Subscription {
    args: SdkValue,
    state: Mutex<State>,
    changed: Condvar,
}

impl Subscription {
    fn new(args: SdkValue) -> Self {
        Self {
            args,
            state: Mutex::new(State {
                seq: 0,
                minimum: 0,
                events: VecDeque::new(),
                stale: false,
                closed: false,
                exhausted: false,
                waiting: false,
            }),
            changed: Condvar::new(),
        }
    }

    fn state(&self) -> Result<MutexGuard<'_, State>, SdkRefusal> {
        lock(&self.state)
    }
}

struct PollGuard(Arc<Subscription>);
impl PollGuard {
    fn start(subscription: Arc<Subscription>, cursor: i64) -> Result<Self, SdkRefusal> {
        let mut state = subscription.state()?;
        if state.closed {
            drop(state);
            return Ok(Self(subscription));
        }
        check_state(&state, cursor)?;
        if state.waiting {
            return Err(refused("InvalidInput", "a poll is already outstanding"));
        }
        state.waiting = true;
        drop(state);
        Ok(Self(subscription))
    }
}
impl Drop for PollGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.waiting = false;
        }
    }
}

struct Admission;
impl Admission {
    fn acquire(timeout: i64) -> Result<Option<Self>, SdkRefusal> {
        if timeout == 0 {
            return Ok(None);
        }
        let workers = std::env::var("UV_THREADPOOL_SIZE")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(4);
        let limit = 16.min(workers - 1).max(0) as usize;
        POLLS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (used < limit).then_some(used + 1)
            })
            .map_err(|_| SdkRefusal {
                code: "Busy".to_owned(),
                message: "long-poll admission is saturated; no wait was started".to_owned(),
                detail: SdkValue::map([
                    ("kind", SdkValue::text("long-poll admission")),
                    ("workers", SdkValue::Int(workers)),
                    ("limit", SdkValue::Int(limit as i64)),
                ]),
            })?;
        Ok(Some(Self))
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        POLLS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn check_state(state: &State, cursor: i64) -> Result<(), SdkRefusal> {
    if state.exhausted {
        return Err(refused("InvalidInput", "cursor exhausted; resubscribe"));
    }
    if cursor > state.seq {
        return Err(refused("InvalidInput", "unknown cursor"));
    }
    Ok(())
}

fn integer(
    args: &SdkValue,
    name: &str,
    default: i64,
    low: i64,
    high: i64,
) -> Result<i64, SdkRefusal> {
    let value = match args.get(name) {
        None => default,
        Some(SdkValue::Int(value)) => *value,
        _ => -1,
    };
    if (low..=high).contains(&value) {
        Ok(value)
    } else {
        Err(refused(
            "InvalidInput",
            format!("{name} must be an integer in {low}..{high}"),
        ))
    }
}

fn subscription_id(args: &SdkValue) -> Result<String, SdkRefusal> {
    args.get_text("subscription")
        .map(str::to_owned)
        .ok_or_else(|| refused("InvalidInput", "unknown subscription"))
}

fn lookup(id: &str) -> Result<Arc<Subscription>, SdkRefusal> {
    lock(&SUBSCRIPTIONS)?
        .get(id)
        .cloned()
        .ok_or_else(|| refused("InvalidInput", "unknown subscription"))
}

pub(crate) fn take(args: SdkValue) -> (Option<String>, SdkValue) {
    match args {
        SdkValue::Map(fields) => {
            let id = fields.iter().find_map(|(name, value)| {
                if name != "subscription" {
                    return None;
                }
                match value {
                    SdkValue::Text(id) => Some(id.clone()),
                    _ => None,
                }
            });
            (
                id,
                SdkValue::Map(
                    fields
                        .into_iter()
                        .filter(|(name, _)| name != "subscription")
                        .collect(),
                ),
            )
        }
        other => (None, other),
    }
}

pub(crate) fn subscribe(args: SdkValue) -> Outcome {
    let id = layerfs_bridge::sdk_values::hex(&crate::registry::random()?);
    let subscription = Arc::new(Subscription::new(args));
    let mut subscriptions = lock(&SUBSCRIPTIONS)?;
    if subscriptions.contains_key(&id) {
        return Err(refused("Failed", "subscription identity collision"));
    }
    subscriptions.insert(id.clone(), subscription.clone());
    drop(subscriptions);
    match snapshot(&subscription.args) {
        Ok(snapshot) => Ok(SdkValue::map([
            ("subscription", SdkValue::text(id)),
            ("cursor", SdkValue::Int(0)),
            ("snapshot", snapshot),
        ])),
        Err(error) => {
            lock(&SUBSCRIPTIONS)?.remove(&id);
            let mut state = subscription.state()?;
            state.closed = true;
            state.events.clear();
            subscription.changed.notify_all();
            Err(error)
        }
    }
}

pub(crate) fn unsubscribe(args: &SdkValue) -> Outcome {
    let id = subscription_id(args)?;
    let subscription = lock(&SUBSCRIPTIONS)?
        .remove(&id)
        .ok_or_else(|| refused("InvalidInput", "unknown subscription"))?;
    let mut state = subscription.state()?;
    state.closed = true;
    state.events.clear();
    subscription.changed.notify_all();
    Ok(SdkValue::map([("kind", SdkValue::text("Unsubscribed"))]))
}

enum Action {
    Page(Vec<Event>, i64, bool),
    Resync(i64),
    Wait,
}

pub(crate) fn poll(args: &SdkValue) -> Outcome {
    let id = subscription_id(args)?;
    let cursor = integer(args, "cursor", -1, 0, MAX_SAFE)?;
    let timeout = integer(args, "timeoutMs", 0, 0, 30_000)?;
    let limit = integer(args, "limit", 50, 1, 500)? as usize;
    let subscription = lookup(&id)?;
    let _polling = PollGuard::start(subscription.clone(), cursor)?;
    let _admission = Admission::acquire(timeout)?;
    let deadline = Instant::now() + Duration::from_millis(timeout as u64);
    loop {
        let action = {
            let mut state = subscription.state()?;
            if state.closed {
                return Ok(SdkValue::map([
                    ("events", SdkValue::List(Vec::new())),
                    ("cursor", SdkValue::Int(state.seq)),
                    ("hasMore", SdkValue::Bool(false)),
                    ("stale", SdkValue::Bool(false)),
                    ("closed", SdkValue::Bool(true)),
                ]));
            }
            check_state(&state, cursor)?;
            if state.stale || cursor < state.minimum {
                let current = state.seq;
                state.events.clear();
                state.minimum = current;
                state.stale = false;
                Action::Resync(current)
            } else if state.events.front().is_some_and(|event| event.seq > cursor) {
                let mut events = Vec::new();
                let mut spent = 0;
                while let Some(event) = state.events.front() {
                    if event.seq <= cursor || events.len() == limit {
                        break;
                    }
                    if !events.is_empty() && spent + event.cost > REPLY_BUDGET {
                        break;
                    }
                    let event = state.events.pop_front().expect("front above");
                    spent += event.cost;
                    state.minimum = event.seq;
                    events.push(event);
                    if spent >= REPLY_BUDGET {
                        break;
                    }
                }
                let last = events.last().map_or(cursor, |event| event.seq);
                let more = !state.events.is_empty();
                Action::Page(events, last, more)
            } else if Instant::now() >= deadline {
                Action::Page(Vec::new(), cursor, false)
            } else {
                Action::Wait
            }
        };
        match action {
            Action::Page(events, cursor, has_more) => {
                return Ok(SdkValue::map([
                    (
                        "events",
                        SdkValue::List(events.into_iter().map(|event| event.value).collect()),
                    ),
                    ("cursor", SdkValue::Int(cursor)),
                    ("hasMore", SdkValue::Bool(has_more)),
                    ("stale", SdkValue::Bool(false)),
                    ("closed", SdkValue::Bool(false)),
                ]));
            }
            Action::Resync(current) => match snapshot(&subscription.args) {
                Ok(snapshot) => {
                    let state = subscription.state()?;
                    if state.closed {
                        return Ok(SdkValue::map([
                            ("events", SdkValue::List(Vec::new())),
                            ("cursor", SdkValue::Int(state.seq)),
                            ("hasMore", SdkValue::Bool(false)),
                            ("stale", SdkValue::Bool(false)),
                            ("closed", SdkValue::Bool(true)),
                        ]));
                    }
                    return Ok(SdkValue::map([
                        ("events", SdkValue::List(Vec::new())),
                        ("cursor", SdkValue::Int(current)),
                        ("hasMore", SdkValue::Bool(false)),
                        ("stale", SdkValue::Bool(true)),
                        ("closed", SdkValue::Bool(false)),
                        ("snapshot", snapshot),
                    ]));
                }
                Err(error) => {
                    subscription.state()?.stale = true;
                    return Err(error);
                }
            },
            Action::Wait => {
                let state = subscription.state()?;
                let remaining = deadline.saturating_duration_since(Instant::now());
                let (state, _) = subscription
                    .changed
                    .wait_timeout(state, remaining)
                    .map_err(|_| refused("Failed", "binding state poisoned"))?;
                drop(state);
            }
        }
    }
}

fn field(value: &SdkValue, name: &str) -> SdkValue {
    value.get(name).cloned().unwrap_or(SdkValue::Null)
}

fn text(value: &SdkValue, name: &str) -> Result<String, SdkRefusal> {
    value
        .get_text(name)
        .map(str::to_owned)
        .ok_or_else(|| refused("Failed", format!("snapshot item has no text {name}")))
}

fn snapshot(args: &SdkValue) -> Outcome {
    let mut branches = Vec::new();
    let mut page_pos = 0;
    loop {
        let paged = match args {
            SdkValue::Map(fields) => SdkValue::Map(
                fields
                    .iter()
                    .cloned()
                    .chain([
                        ("pageSize".to_owned(), SdkValue::Int(1000)),
                        ("pagePos".to_owned(), SdkValue::Int(page_pos)),
                    ])
                    .collect(),
            ),
            other => other.clone(),
        };
        let page = crate::direct::call("project.list_branches", paged, false)?;
        let items = match page.get("items") {
            Some(SdkValue::List(items)) => items,
            _ => return Err(refused("Failed", "Branch snapshot page has no items")),
        };
        branches.extend(items.iter().map(|item| {
            SdkValue::map([("name", field(item, "name")), ("head", field(item, "head"))])
        }));
        if !page.get_bool("hasMore") {
            break;
        }
        page_pos = page
            .get_int("nextPagePos")
            .ok_or_else(|| refused("Failed", "Branch snapshot page has no next position"))?;
    }
    let listed = crate::workspace::sandboxes(args)?;
    let items = match listed.get("items") {
        Some(SdkValue::List(items)) => items,
        _ => return Err(refused("Failed", "Sandbox snapshot has no items")),
    };
    let mut sandboxes = Vec::new();
    let mut workspaces = Vec::new();
    for item in items {
        let sandbox = text(item, "sandbox")?;
        sandboxes.push(SdkValue::map([("sandbox", SdkValue::text(&sandbox))]));
        let args = crate::registry::naming(args, &sandbox);
        let listed = crate::dispatch::sandbox_workspaces(&args)?;
        let records = match listed.get("workspaces") {
            Some(SdkValue::List(records)) => records,
            _ => return Err(refused("Failed", "Workspace snapshot has no records")),
        };
        for record in records {
            if !record.get_bool("mounted") {
                continue;
            }
            let path = text(record, "directory")?;
            workspaces.push(SdkValue::map([
                (
                    "workspace",
                    SdkValue::map([
                        ("sandbox", SdkValue::text(&sandbox)),
                        ("path", SdkValue::text(path)),
                    ]),
                ),
                ("branch", field(record, "branch")),
            ]));
        }
    }
    Ok(SdkValue::map([
        ("branches", SdkValue::List(branches)),
        (
            "workspaces",
            SdkValue::List(
                workspaces
                    .into_iter()
                    .map(|workspace| {
                        let identity = field(&workspace, "workspace");
                        SdkValue::map([
                            ("sandbox", field(&identity, "sandbox")),
                            ("path", field(&identity, "path")),
                            ("branch", field(&workspace, "branch")),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("sandboxes", SdkValue::List(sandboxes)),
    ]))
}

pub(crate) fn workspace_branch(args: &SdkValue) -> Option<String> {
    let state = crate::registry::sandbox(args).ok()?;
    let path = args.get_text("path")?;
    let branch = lock(&state.mounts)
        .ok()?
        .get(path)
        .map(|mount| mount.branch.clone());
    branch
}

pub(crate) fn emit(
    id: Option<String>,
    op: &str,
    args: &SdkValue,
    value: &SdkValue,
    workspace_branch: Option<String>,
) {
    let Some(id) = id else { return };
    let Ok(subscription) = lookup(&id) else {
        return;
    };
    let kind = value.get_text("kind");
    let (event_type, source, payload) = match (op, kind) {
        ("workspace.commit", Some("Committed")) => (
            "commit.created",
            "store",
            SdkValue::map([
                ("branch", SdkValue::opt_text(workspace_branch)),
                (
                    "workspace",
                    SdkValue::map([
                        ("sandbox", field(args, "sandbox")),
                        ("path", field(args, "path")),
                    ]),
                ),
                ("commit", field(value, "commit")),
            ]),
        ),
        (
            op @ ("fs.write" | "fs.apply" | "fs.edit" | "fs.mkdir" | "fs.rm" | "fs.mv" | "fs.cp"),
            Some("Committed"),
        ) => {
            let moving = matches!(op, "fs.mv" | "fs.cp");
            let plain = matches!(op, "fs.write" | "fs.edit" | "fs.mkdir" | "fs.rm");
            (
                "fs.committed",
                "store",
                SdkValue::map([
                    ("branch", field(value, "branch")),
                    ("method", SdkValue::text(op)),
                    (
                        "path",
                        if plain {
                            field(args, "path")
                        } else {
                            SdkValue::Null
                        },
                    ),
                    (
                        "from",
                        if moving {
                            field(args, "from")
                        } else {
                            SdkValue::Null
                        },
                    ),
                    (
                        "to",
                        if moving {
                            field(args, "to")
                        } else {
                            SdkValue::Null
                        },
                    ),
                    ("commit", field(value, "commit")),
                ]),
            )
        }
        ("branch.fork", Some("Forked")) => {
            let head = field(value, "head");
            let fork_point = if matches!(head, SdkValue::Text(_)) {
                SdkValue::map([("kind", SdkValue::text("commit")), ("id", head)])
            } else {
                SdkValue::map([
                    ("kind", SdkValue::text("layer")),
                    ("id", field(value, "base")),
                ])
            };
            (
                "branch.forked",
                "store",
                SdkValue::map([
                    ("branch", field(value, "branch")),
                    ("forkPoint", fork_point),
                ]),
            )
        }
        ("branch.merge", Some("Merged")) => (
            "branch.merged",
            "store",
            SdkValue::map([("merge", value.clone())]),
        ),
        ("branch.publish", Some("Added")) => {
            let source = field(value, "source");
            let layer = field(value, "publishedLayer");
            (
                "branch.published",
                "store",
                SdkValue::map([
                    ("branch", field(&source, "branch")),
                    ("commit", field(&source, "commit")),
                    ("layer", field(&layer, "id")),
                    ("basedOn", field(value, "basedOn")),
                ]),
            )
        }
        ("workspace.mount", Some("Mounted")) => (
            "workspace.mounted",
            "daemon",
            SdkValue::map([
                (
                    "workspace",
                    SdkValue::map([
                        ("sandbox", field(value, "sandbox")),
                        ("path", field(value, "path")),
                    ]),
                ),
                ("branch", field(value, "branch")),
            ]),
        ),
        ("workspace.unmount", Some("Unmounted" | "ForceUnmounted")) => (
            "workspace.unmounted",
            "daemon",
            SdkValue::map([(
                "workspace",
                SdkValue::map([
                    ("sandbox", field(args, "sandbox")),
                    ("path", field(args, "path")),
                ]),
            )]),
        ),
        ("sandbox.create", _) => (
            "sandbox.created",
            "daemon",
            SdkValue::map([("sandbox", field(value, "sandbox"))]),
        ),
        ("sandbox.close", Some("Closed")) => (
            "sandbox.closed",
            "daemon",
            SdkValue::map([("sandbox", field(args, "sandbox"))]),
        ),
        _ => return,
    };
    let Ok(mut state) = subscription.state() else {
        return;
    };
    if state.closed || state.exhausted {
        return;
    }
    if state.seq == MAX_SAFE {
        state.exhausted = true;
        subscription.changed.notify_all();
        return;
    }
    state.seq += 1;
    let seq = state.seq;
    let value = SdkValue::map([
        ("seq", SdkValue::Int(seq)),
        ("type", SdkValue::text(event_type)),
        ("source", SdkValue::text(source)),
        ("payload", payload),
    ]);
    let event = Event {
        seq,
        cost: value_cost(&value),
        value,
    };
    if state.events.len() == CAPACITY {
        if let Some(dropped) = state.events.pop_front() {
            state.minimum = dropped.seq;
            state.stale = true;
        }
    }
    state.events.push_back(event);
    subscription.changed.notify_all();
}
