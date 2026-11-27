//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs this crate's call with the checks its caller passes in
//! ([`StoreUsers`], which the Sandbox crate implements). The Store calls proper (`branch`, `commit`, `file`)
//! and the rest (`store.list`, `store.get`, `store.create`) are
//! `layerfs-store`'s own. `store.attach` and `store.detach` also read the
//! machine, and the first reaches the Store from it as a Sandbox would.
use crate::admin::open::args::OpenArgs;
use crate::admin::route::executor::{self, Executor};
use crate::backend::postgresql::check;
use crate::backend::postgresql::connect::{self, StoreConnect};
use crate::StoreUsers;
use layerfs_home::events::Events;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{refused, with_fix, with_next, Outcome, FIX};
use layerfs_machine::worker::Unreached;
use layerfs_persistence::diagnostic::ConnectionStep;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;
use {crate::admin::record::store::StoreRecord, layerfs_protocol::StoreKind};

pub use crate::close_idle;

/// `store.compact`: reclaim one Store's dead bytes, answering the
/// flattened report (`layerfs_store::report::compact_report`). A refusal
/// gets the call that clears it as its `fix` (spec v19): starting a sqlite
/// Store's machine whose worker was not reached, or replacing a worker of
/// another build (the stop forced when Sandboxes are on the machine); the
/// same `store compact` after a lost reply or an unresolved earlier run.
pub fn compact(
    home: &Path,
    events: &mut Events<'_>,
    name: &StoreName,
    users: &StoreUsers,
) -> Outcome {
    let store = name.as_str();
    crate::admin::maintenance::compact::compact(home, events, store)
        .map_err(|refusal| with_store_fix(home, store, refusal, "compact", users))
}

/// `store.upgrade`: brings an older-format Store to the latest format in
/// place (#804), reading the process-wide cancel flag, refused `InUse`
/// while a Sandbox of one of its projects is recorded ([`StoreUsers`]). A
/// refusal gets the call that clears it as its `fix`: the machine start or
/// worker replacement compaction names, the same `store upgrade` after a
/// lost reply or an unresolved earlier run, and for a format this build
/// cannot upgrade (not one newer than this build) the Store's `store
/// remove` (sqlite) or `store disconnect` (PostgreSQL, which keeps its data).
/// The process-wide cancel request is cleared when the call returns.
pub fn upgrade(
    home: &Path,
    events: &mut Events<'_>,
    name: &StoreName,
    users: &StoreUsers,
) -> Outcome {
    let store = name.as_str();
    let cancel = layerfs_home::cancel::flag();
    // a cancel asked for before the run still stops it; once the answer is
    // decided the request is dropped, so it does not cancel a later call
    let outcome = crate::admin::maintenance::upgrade::upgrade_with(
        home,
        events,
        store,
        cancel,
        users.related,
    );
    layerfs_home::cancel::clear();
    outcome.map_err(|refusal| {
        if refusal.code != "Unsupported"
            || refusal
                .message
                .contains(layerfs_persistence::upgrade::NEWER)
        {
            return with_store_fix(home, store, refusal, "upgrade", users);
        }
        let kind = StoreRecord::read(home, store).ok().flatten();
        let call = match kind.and_then(|record| record.kind().ok()) {
            Some(StoreKind::Postgresql) => "disconnect",
            Some(StoreKind::Sqlite) => "remove",
            None => return refusal,
        };
        with_fix(
            refusal,
            format!("layerfs store {call} {}", shell_word(store)),
        )
    })
}

/// `refusal` of Store call `call` (`compact`, `upgrade`) on Store `store`
/// with the fix that clears it, when it carries none (spec v19): starting a
/// sqlite Store's machine whose worker was not reached, or replacing a
/// worker of another build (the stop forced when Sandboxes are on the
/// machine); the same call after a lost reply or an unresolved earlier run.
fn with_store_fix(
    home: &Path,
    store: &str,
    refusal: SdkRefusal,
    call: &str,
    users: &StoreUsers,
) -> SdkRefusal {
    if refusal
        .detail
        .get(FIX)
        .is_some_and(|fix| *fix != SdkValue::Null)
    {
        return refusal;
    }
    let machine = || {
        let record = StoreRecord::read(home, store).ok().flatten()?;
        let machine = record.machine().ok()?;
        let worker = !layerfs_machine::runs_here(home, &machine).unwrap_or(false);
        (record.kind().ok()? == StoreKind::Sqlite && worker).then_some(machine)
    };
    let fix = match refusal.code.as_str() {
        "Unavailable" => match machine() {
            Some(machine) if layerfs_machine::worker::is_mismatch(&refusal) => {
                let on = (users.sandboxes_on)(home, &machine).map_or(0, |on| on.len());
                Some(layerfs_machine::state::replace_worker(&machine, on))
            }
            Some(machine) => Some(format!(
                "layerfs machine start {machine}",
                machine = shell_word(&machine)
            )),
            None => None,
        },
        "Unknown" | "Unresolved" => Some(format!(
            "layerfs store {call} {store}",
            store = shell_word(store)
        )),
        _ => None,
    };
    match fix {
        Some(fix) => with_fix(refusal, fix),
        None => refusal,
    }
}

/// `store.repair_mount_holders(project, branch, action, { op, legacy,
/// force, operator, limit, cursor })` as the call surface takes it.
#[derive(Clone, Debug, Default)]
pub struct RepairRequest {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Branch whose mount holders it repairs.
    pub branch: String,
    /// `list`, `preview`, `settle`, `release`, `settle_legacy` or `audit`.
    pub action: String,
    /// The holder `settle` and `release` act on, as hex.
    pub op: Option<String>,
    /// `settle` only a row without owner or place evidence.
    pub legacy: bool,
    /// `settle` a holder that still looks live.
    pub force: bool,
    /// The operator label a release records.
    pub operator: Option<String>,
    /// The rows of one page (`list`, `preview`, `audit`).
    pub limit: Option<i64>,
    /// The `next_cursor` of the page before.
    pub cursor: Option<String>,
}

/// The call a repair cursor belongs to.
const REPAIR: &str = "store.repair_mount_holders";

/// `store.repair_mount_holders`: first-class explicit operator repair of
/// one Branch's mount intents ([`layerfs_store::repair`]). `list` and
/// `preview` page the holders, each with its row `state` and the
/// `liveness` admission would give it now; `settle` refuses a holder still
/// `live` unless forced; `release` retains a settled row as the durable done
/// audit; `audit` pages the Branch's retained rows. The cursor carries the
/// project, Branch and action. Nothing is automatic. A SQLite Store on
/// another machine is repaired by that machine's worker (`store.repair`),
/// sent this home's place view; a PostgreSQL Store runs here, as its calls
/// do.
pub fn repair(home: &Path, request: &RepairRequest) -> Outcome {
    let limit = layerfs_home::cursor::limit(REPAIR, request.limit, 100)?;
    let filters = [
        ("project", SdkValue::text(&request.project)),
        ("branch", SdkValue::text(&request.branch)),
        ("action", SdkValue::text(&request.action)),
    ];
    let carried = (request.cursor.as_deref())
        .map(|made| layerfs_home::cursor::resume(REPAIR, made, &filters))
        .transpose()?;
    let target = crate::admin::route::resolve::target(home, &request.project)?;
    let sent = crate::admin::maintenance::repair_mount_holders::Repair {
        project: target.project.clone(),
        branch: request.branch.clone(),
        action: request.action.clone(),
        op: crate::admin::maintenance::repair_mount_holders::op_of(request.op.as_deref())?,
        legacy: request.legacy,
        force: request.force,
        operator: request.operator.clone().unwrap_or_default(),
        after: carried.as_ref().map(|carried| carried.place().clone()),
        limit: limit as u32,
    };
    // The host holds the Store's lease while it repairs or sends the call,
    // as `store_call` does: a Store an operation is moving is left.
    let (_lease, record) = crate::admin::open::lease::claim(home, &target.store)?;
    let answer = match executor::plan(home, record.store_ref()?)? {
        Executor::Local => {
            let open = OpenArgs::of_record(home, &target.store, &record)?;
            crate::admin::maintenance::repair_mount_holders::repair_at(&open, &sent)
        }
        Executor::Worker(mut worker) => {
            let mut fields = vec![
                ("store".to_owned(), SdkValue::text(&target.store)),
                ("store_record".to_owned(), record.to_record().clone()),
            ];
            fields.extend(sent.to_fields());
            // list, preview and settle judge holders with this home's places
            if let Some(view) = crate::admin::route::place_view::encoded_from_home(home) {
                fields.push(("place_view".to_owned(), view));
            }
            worker.call("store.repair", SdkValue::Map(fields), &mut |_: SdkValue| {})
        }
    };
    let answer = answer.map_err(|refusal| match refusal.code.as_str() {
        "InUse" => with_fix(
            refusal,
            format!(
                "layerfs store repair-mount-holders {} {} settle --op {} --force",
                shell_word(&request.project),
                shell_word(&request.branch),
                request.op.as_deref().unwrap_or_default()
            ),
        ),
        _ => refusal,
    })?;
    Ok(paged(answer, &filters))
}

/// A repair answer with its `next` place made the `next_cursor` the caller
/// takes back; an answer that is no page is as the Store side gave it.
fn paged(answer: SdkValue, filters: &[(&str, SdkValue)]) -> SdkValue {
    let SdkValue::Map(fields) = answer else {
        return answer;
    };
    let fields = (fields.into_iter())
        .map(|(name, value)| match (name.as_str(), value) {
            ("next", SdkValue::Null) => ("next_cursor".to_owned(), SdkValue::Null),
            ("next", place) => (
                "next_cursor".to_owned(),
                SdkValue::Text(layerfs_home::cursor::make(REPAIR, place, filters)),
            ),
            (_, value) => (name, value),
        })
        .collect();
    SdkValue::Map(fields)
}

/// `store.remove`: a sqlite Store no Sandbox uses, with its file.
pub fn remove(home: &Path, name: &StoreName, users: &StoreUsers) -> Outcome {
    crate::backend::sqlite::remove::remove(home, name, users.related)
}

/// `store.move`: a sqlite Store from the machine that holds it to
/// `machine`, one of the two being this computer (C13).
pub fn move_to(
    home: &Path,
    events: &mut Events<'_>,
    store: &str,
    machine: &str,
    users: &StoreUsers,
) -> Outcome {
    crate::backend::sqlite::move_phases::move_to(home, events, store, machine, users.related)
}

/// `store.disconnect(store)`: forget a PostgreSQL Store, its saved login
/// and its attachments (its data stays), refused `InUse` while a process
/// holds it or a Sandbox of its projects exists. A sqlite Store is
/// `InvalidInput`, naming `store remove`.
pub fn disconnect(home: &Path, name: &StoreName, users: &StoreUsers) -> Outcome {
    connect::disconnect(home, name, users.related)
}

/// `store.connect(store, url, { user, password, synchronous_commit,
/// dry_run })`: a new PostgreSQL Store, or a connected one's address,
/// login or setting, checked from this process before anything is saved.
/// A change of a connected Store's address, user or password is then
/// checked from every machine attached to it, one after another, as its
/// Sandboxes would dial the new values: one that fails refuses the
/// connect `Conflict` ([`cut_off`]), nothing saved, so no attached machine
/// is cut off unseen. A dry run makes the same checks (the machines' for
/// any change, and only once this computer passed), saves and records
/// nothing and is refused by none of them: it answers
/// [`connect::tested`].
pub fn connect(
    home: &Path,
    events: &mut Events<'_>,
    request: &StoreConnect,
    users: &StoreUsers,
) -> Outcome {
    let Some(update) = connect::prepare(home, request)? else {
        return connect::connect(home, request);
    };
    if !request.dry_run {
        if let Err(refusal) = &update.here {
            return Err(refusal.clone());
        }
    }
    let machines = update.trial().attached();
    let mut checked = Vec::new();
    if update.here.is_ok() && (request.dry_run || update.redials()) {
        for machine in &machines {
            let url = update.dial().to_owned();
            let seen = check_dialling(home, events, update.trial(), machine, url, users).1;
            checked.push((machine.as_str(), seen));
        }
    }
    let name = request.store.as_str();
    if request.dry_run {
        let row = |machine: &str, tested: SdkValue| {
            let mut fields = vec![("machine".to_owned(), SdkValue::text(machine))];
            if let SdkValue::Map(tested) = tested {
                fields.extend(tested);
            }
            SdkValue::Map(fields)
        };
        let rows = match update.here.is_ok() {
            true => (checked.iter())
                .map(|(machine, seen)| row(machine, check::tested(seen)))
                .collect(),
            // this computer not let in: each machine would only repeat it
            false => (machines.iter())
                .map(|machine| row(machine, not_checked()))
                .collect(),
        };
        let here = check::tested(&update.here);
        return Ok(connect::tested(name, &request.url, here, rows));
    }
    let failed: Vec<_> = (checked.iter())
        .filter_map(|(machine, seen)| seen.as_ref().err().map(|refusal| (*machine, refusal)))
        .collect();
    if !failed.is_empty() {
        return Err(cut_off(name, &failed));
    }
    update.save(home, request)
}

/// A dry run's row of a machine not checked: this computer's check failed
/// first.
fn not_checked() -> SdkValue {
    SdkValue::map([
        ("ok", SdkValue::Null),
        (
            "message",
            SdkValue::text("not checked: this computer's check failed first"),
        ),
    ])
}

/// The `Conflict` of a connect whose new address or login the attached
/// machines `failed` do not reach: their names in the message, `machines`
/// `[{ machine, ok: false, step, message }]` beside it, and as its fix the
/// detach of the first (or connect it with values they reach).
fn cut_off(store: &str, failed: &[(&str, &SdkRefusal)]) -> SdkRefusal {
    let names: Vec<_> = failed.iter().map(|(machine, _)| *machine).collect();
    let message = format!(
        "the new connection of PostgreSQL Store {store} does not reach attached machine(s) {}; nothing was saved",
        names.join(", ")
    );
    let rows = failed.iter().map(|(machine, refusal)| {
        SdkValue::map([
            ("machine", SdkValue::text(*machine)),
            ("ok", SdkValue::Bool(false)),
            ("step", SdkValue::opt_text(check::step_of(refusal))),
            ("message", SdkValue::text(&refusal.message)),
        ])
    });
    let refusal = with_next(
        refused("Conflict", message),
        "machines",
        SdkValue::List(rows.collect()),
    );
    let first = names.first().copied().unwrap_or_default();
    with_fix(
        refusal,
        format!(
            "layerfs store detach {} --machine {}",
            shell_word(store),
            shell_word(first)
        ),
    )
}

/// The PostgreSQL Store `name` a machine attaches to or detaches from: a
/// sqlite Store is `InvalidInput` naming `store move` (it lives on one
/// machine).
fn attachable(home: &Path, name: &StoreName, machine: &str) -> Result<StoreRecord, SdkRefusal> {
    let record = StoreRecord::of(home, name)?;
    if record.kind()? == StoreKind::Sqlite {
        let on = record.machine().unwrap_or_default();
        return Err(refused(
            "InvalidInput",
            format!(
                "Store {name} is a sqlite Store on machine {on}; machines attach to PostgreSQL Stores only, and `layerfs store move {name} {machine}` moves it"
            ),
        ));
    }
    Ok(record)
}

/// `store.attach(store, { machine, dry_run })`: lets machine `machine` run
/// Sandboxes of PostgreSQL Store `store`. The machine must exist and its
/// platform run PostgreSQL Sandboxes; the Store is checked from it with
/// the URL and login this computer saved, as its Sandboxes dial them,
/// before it is recorded in the Store's `attached` and its reach observed.
/// Attaching an attached machine checks it again (`outcome: checked`, its
/// reach recorded either way). Answers `{ ok, outcome: attached | checked,
/// store, machine, latency_ms, server_version }`; a refused check names
/// its `step` and the `machine`. A dry run makes the same checks, takes no
/// lock and records nothing, neither the attachment nor the reach
/// (`outcome: tested`), so dry runs of several machines may run at once.
pub fn attach(
    home: &Path,
    events: &mut Events<'_>,
    name: &StoreName,
    machine: &MachineName,
    dry_run: bool,
    users: &StoreUsers,
) -> Outcome {
    let machine = machine.as_str();
    let record = attachable(home, name, machine)?;
    runs_postgresql_sandboxes(home, name, machine)?;
    let attached = record.is_attached(machine);
    let checked = check_from(home, events, &record, machine, users).1;
    if dry_run {
        return Ok(attach_answer("tested", name, machine, &checked?));
    }
    if attached || checked.is_ok() {
        let seen = crate::admin::status::observed::Reach::of(&checked);
        crate::admin::status::observed::record(home, name, Some(machine), &seen);
    }
    let checked = checked?;
    let new = check::attach(home, name, machine, record.binding())?;
    let outcome = if new { "attached" } else { "checked" };
    Ok(attach_answer(outcome, name, machine, &checked))
}

/// `store.attach`'s answer for the check `checked` of `machine`.
fn attach_answer(outcome: &str, name: &StoreName, machine: &str, checked: &SdkValue) -> SdkValue {
    let copied = |key: &str| checked.get(key).cloned().unwrap_or_default();
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text(outcome)),
        ("store", SdkValue::text(name.as_str())),
        ("machine", SdkValue::text(machine)),
        ("latency_ms", copied("latency_ms")),
        ("server_version", copied("server_version")),
    ])
}

/// `store.detach(store, { machine })`: machine `machine` no longer runs
/// Sandboxes of PostgreSQL Store `store`, and its reach is forgotten.
/// Refused `InUse` while a Sandbox of the Store's projects is on it,
/// `NotFound` when it is not attached. Answers `{ ok, outcome: detached,
/// store, machine }`.
pub fn detach(home: &Path, name: &StoreName, machine: &MachineName, users: &StoreUsers) -> Outcome {
    let machine = machine.as_str();
    attachable(home, name, machine)?;
    let idle = |home: &Path| (users.store_idle_on)(home, name, machine);
    check::detach(home, name, machine, &idle)?;
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("detached")),
        ("store", SdkValue::text(name.as_str())),
        ("machine", SdkValue::text(machine)),
    ]))
}

/// The refusal of a call naming machine `machine` for PostgreSQL Store
/// `store` it is not attached to, with the attach as its fix.
pub fn not_attached(store: &str, machine: &str) -> SdkRefusal {
    with_fix(
        refused(
            "InvalidInput",
            format!("machine {machine} is not attached to PostgreSQL Store {store}"),
        ),
        format!(
            "layerfs store attach {} --machine {}",
            shell_word(store),
            shell_word(machine)
        ),
    )
}

/// Refused unless machine `machine` exists and its platform runs
/// PostgreSQL Sandboxes on this host (the support table's reason, word for
/// word), as `sandbox.create` refuses one.
fn runs_postgresql_sandboxes(home: &Path, store: &str, machine: &str) -> Result<(), SdkRefusal> {
    #[cfg(feature = "machine")]
    {
        use layerfs_machine::platform::{sandbox_allowed, HostOs};
        let platform = layerfs_machine::machine_of(home, machine)?.platform()?;
        sandbox_allowed(platform, HostOs::current(), StoreKind::Postgresql).map_err(|reason| {
            refused(
                "Unsupported",
                format!(
                    "a Sandbox of postgresql Store {store} cannot run on machine {machine} ({}): {reason}",
                    platform.name()
                ),
            )
        })
    }
    #[cfg(not(feature = "machine"))]
    {
        let _ = store;
        match layerfs_machine::record::MachineRecord::read(home, machine)? {
            Some(_) => Ok(()),
            None => Err(refused("NotFound", format!("no machine named {machine}"))),
        }
    }
}

/// `refusal` of the check from machine `machine`, naming it, the Store and
/// the host it dialled (never the URL), with the platform's `hint` only
/// when no connection was made (a server that answered, with a refused
/// login or another Store, was reached), and the step the check named. A
/// worker from a bundle without the check says how to update it.
fn from_machine(
    refusal: SdkRefusal,
    (store, machine, host): (&str, &str, &str),
    hint: Option<&str>,
) -> SdkRefusal {
    let step = check::step_of(&refusal).map(str::to_owned);
    let message = refusal.message;
    let made = match refusal.code.as_str() {
        "InvalidInput" if message.contains("no operation store.reach") => {
            return refused(
                "Unsupported",
                format!(
                    "machine {machine}'s worker predates this check; `layerfs machine stop {machine}` and `layerfs machine start {machine}` update it"
                ),
            )
        }
        "Unavailable" if !message.starts_with(check::ANOTHER_STORE) => {
            let hint = hint
                .filter(|_| crate::backend::postgresql::open::not_connected(&message))
                .map(|hint| format!("; {hint}"))
                .unwrap_or_default();
            refused(
                "Unavailable",
                format!(
                    "machine {machine} cannot reach PostgreSQL Store {store}'s server at {host}: {message}{hint}"
                ),
            )
        }
        code => refused(
            code,
            format!("machine {machine}, PostgreSQL Store {store} at {host}: {message}"),
        ),
    };
    match step {
        Some(step) => with_next(made, check::STEP, SdkValue::text(step)),
        None => made,
    }
}

/// The check of machine `machine`'s connection to PostgreSQL Store
/// `record` (the Store's URL with its login), run from that machine: in
/// this process, or in its worker as `store.reach`. Answers the check,
/// `{ at, server_version, latency_ms }`, or its refusal, which names the
/// `machine` and, once the check began, the connection `step` it stopped
/// at (`configuration` for a login this computer does not hold, or an
/// address the machine cannot dial); either way with the host it
/// dialled, once known.
pub fn check_from(
    home: &Path,
    events: &mut Events<'_>,
    record: &StoreRecord,
    machine: &str,
    users: &StoreUsers,
) -> (Option<String>, Result<SdkValue, SdkRefusal>) {
    match crate::admin::record::store::url(record) {
        Ok(url) => check_dialling(home, events, record, machine, url, users),
        Err(refusal) => {
            let refusal = check::at_step(refusal, ConnectionStep::Configuration);
            (None, Err(on_machine(refusal, machine)))
        }
    }
}

/// `refusal` of a check from machine `machine`, naming it.
fn on_machine(refusal: SdkRefusal, machine: &str) -> SdkRefusal {
    with_next(refusal, "machine", SdkValue::text(machine))
}

/// [`check_from`] dialling `url`, the Store's address with a login in it,
/// as the machine's Sandboxes' calls build it (`store.connect` checks new
/// values before it saves them).
fn check_dialling(
    home: &Path,
    events: &mut Events<'_>,
    record: &StoreRecord,
    machine: &str,
    url: String,
    users: &StoreUsers,
) -> (Option<String>, Result<SdkValue, SdkRefusal>) {
    let (host, checked) = dial_from(home, events, record, machine, url, users);
    (
        host,
        checked.map_err(|refusal| on_machine(refusal, machine)),
    )
}

/// [`check_dialling`] before its refusal names the machine.
fn dial_from(
    home: &Path,
    events: &mut Events<'_>,
    record: &StoreRecord,
    machine: &str,
    url: String,
    users: &StoreUsers,
) -> (Option<String>, Result<SdkValue, SdkRefusal>) {
    let name = record.name();
    let configuration = |refusal| check::at_step(refusal, ConnectionStep::Configuration);
    let (postgresql, hint) = match connect::dialled(home, machine, record, url) {
        Ok(Ok(reached)) => reached,
        // an address this machine cannot dial (a loopback of this host it
        // does not share), refused before any dial
        Ok(Err(refusal)) => {
            return (
                None,
                Err(configuration((users.nameless_fix)(home, machine, refusal))),
            )
        }
        Err(refusal) => return (None, Err((users.nameless_fix)(home, machine, refusal))),
    };
    // the host named is read from the address the machine dials, with no
    // login: the URL dialled holds the password
    let address = record.text("url").unwrap_or_default();
    let host = match connect::target(address) {
        Ok((host, _, _)) => host,
        Err(refusal) => return (None, Err(configuration(refusal))),
    };
    let executor = match executor::on_machine(home, machine) {
        Ok(executor) => executor,
        Err(refusal) => return (Some(host), Err(refusal)),
    };
    let checked = match executor {
        Executor::Local => check::check(record, &postgresql),
        Executor::Worker(mut worker) => {
            let request = SdkValue::map([
                ("store", SdkValue::text(name)),
                ("store_record", record.to_record().clone()),
                ("postgresql", postgresql.encode()),
            ]);
            let mut sink = |event| events.event(event);
            match worker.attempt("store.reach", request, &mut sink) {
                Ok(outcome) => outcome,
                // the machine's worker was not reached: its own refusal,
                // which names the start, not a server the check never dialled
                Err(Unreached(refusal)) => return (Some(host), Err(refusal)),
            }
        }
    };
    let checked =
        checked.map_err(|refusal| from_machine(refusal, (name, machine, &host), hint.as_deref()));
    (Some(host), checked)
}

/// `store.reach` in a machine's worker: `{ store, store_record, postgresql }`
/// from the caller, which put the saved login into the URL this machine
/// dials. Answers the check `store.attach` and `store.doctor` ask for.
pub fn reach_in_worker(args: &SdkValue) -> Outcome {
    let store = args.get_text("store").unwrap_or("");
    StoreName::parse(store)?;
    let record =
        StoreRecord::from_record(store, args.get("store_record").cloned().unwrap_or_default());
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "InvalidInput",
            format!("Store {store} is not a PostgreSQL Store"),
        ));
    }
    let postgresql = args
        .get("postgresql")
        .map(crate::admin::open::args::PgSelection::read)
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "store.reach carries the resolved postgresql value",
            )
        })?;
    check::check(&record, &postgresql)
}
