//! `sandbox.close`, and the close of a Sandbox whose create never recorded
//! an id.
//!
//! B5 (#548): a close is a small protocol. It reads the record under the
//! home and Sandbox locks, refuses a stale `generation`, refuses while an
//! exec lease is live, and then wins a CAS into `Draining` (`generation + 1`
//! plus the `sandbox.close` intent). Only then does it stop the runtime.
//! The outcome is `Closed` (the folder goes and a tombstone records the
//! generation and reason) or `Lost` (nothing is killed; the record stays
//! and `sandbox get` shows it). An unknown outcome keeps the uncertain
//! intent, as before.
use super::create::remove_folder;
use super::direct;
use super::record::{self, SandboxRecord};
use crate::common::clock;
use crate::common::context::Context;
use crate::common::home::{self, paths};
use crate::common::op::{Op, Standing};
use crate::common::outcome::{failed, refused, Outcome};
use crate::sandbox::lifecycle::{
    self, CloseDecision, CloseEvent, CloseRefusal, CloseState, SandboxState,
};
use crate::sandbox::route::on_machine;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `sandbox.close(name, { force, generation, dry_run })`.
pub(super) fn close(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    crate::common::names::SandboxName::parse(name)?;
    let force = matches!(args.get("force"), Some(SdkValue::Bool(true)));
    let dry_run = matches!(args.get("dry_run"), Some(SdkValue::Bool(true)));
    let requested = requested_generation(args)?;
    let op = Op::begin(
        "sandbox.close",
        "draining",
        SdkValue::map([("force", SdkValue::Bool(force))]),
    )?;
    // The generation CAS is the point of no return: from here an ended
    // process leaves an outcome no one can know, so the intent says so.
    let op = op.as_uncertain();
    let (record, current, next) = {
        let _lock = home::lock::take(&home)?;
        // a closed name has no folder: its tombstone answers the replay
        if !paths::sandbox_dir(&home, name).exists() {
            return replay(&home, name, requested);
        }
        let _sandbox = record::lock(&home, name)?;
        let Some(mut record) = SandboxRecord::read(&home, name)? else {
            return replay(&home, name, requested);
        };
        clearable(&record)?;
        let current = record.generation();
        let now = home::now_ms();
        let live = record.live_leases(now);
        let expired = record.expired_leases(now);
        let expected = requested.unwrap_or(current);
        match lifecycle::close_transition(
            CloseState::Ready,
            CloseEvent::Request {
                expected,
                current,
                live_leases: live.len(),
                expired_leases: expired.len(),
            },
        ) {
            CloseDecision::Refuse(CloseRefusal::StaleGeneration) => {
                return Err(lifecycle::stale_generation(name, expected, current));
            }
            CloseDecision::Refuse(CloseRefusal::LeaseActive) => {
                return Err(lifecycle::lease_active(name, &live));
            }
            CloseDecision::Lost => {
                let reason = "an exec lease expired";
                if dry_run {
                    return Ok(close_preview(
                        name,
                        &record,
                        current,
                        "lost",
                        "mark the Sandbox Lost; nothing is killed",
                        reason,
                    ));
                }
                let lost = record.without_op().with_lost(reason, clock::monotonic_ms());
                lost.write(&home)?;
                return Err(lifecycle::lost(name, reason));
            }
            CloseDecision::Drain => {
                if dry_run {
                    let (decision, would, reason) = if record.lost() {
                        let reason = record
                            .to_record()
                            .get_text(lifecycle::CLOSE_REASON)
                            .unwrap_or("the Sandbox is Lost");
                        match record.id().is_none() {
                            true => (
                                "reclaim",
                                "reclaim the unstarted Lost record; no runtime is touched",
                                reason,
                            ),
                            false => (
                                "keep_lost",
                                "keep the Lost record; a later close reclaims it when no lease is live",
                                reason,
                            ),
                        }
                    } else if record.id().is_none() {
                        (
                            "reclaim",
                            "reclaim the unstarted record; no runtime is touched",
                            "the Sandbox never recorded a runtime",
                        )
                    } else {
                        (
                            "drain",
                            "win the generation CAS, then stop the runtime and reclaim the record",
                            lifecycle::OPERATOR_CLOSE,
                        )
                    };
                    return Ok(close_preview(
                        name, &record, current, decision, would, reason,
                    ));
                }
            }
            _ => {
                return Err(refused(
                    "Failed",
                    format!("Sandbox {name} is not in a closeable state"),
                ));
            }
        }
        if record.lost() {
            // A record whose create never recorded an id owns no runtime:
            // reclaim it. A started Lost record keeps its folder: the
            // runtime may still exist, and removing it without a provider
            // probe would be the blind orphan the protocol forbids. B9's
            // provider probe clears that one.
            if record.id().is_none() {
                let generation = current;
                remove_folder(&record.dir(&home))?;
                write_closed(
                    &home,
                    name,
                    generation,
                    Some(current + 1),
                    "lost",
                    Some("lost"),
                )?;
                return Ok(closed_was(name, "lost"));
            }
            let reason = record
                .to_record()
                .get_text("close_reason")
                .unwrap_or("the Sandbox is Lost");
            return Err(lifecycle::lost(name, reason));
        }
        if record.id().is_none() {
            return close_unstarted(&home, &record);
        }
        let next = current + 1;
        record = record
            .with_generation(next)
            .with_activity(clock::monotonic_ms())
            .with_op(&op)
            .with_close_reason(lifecycle::OPERATOR_CLOSE);
        record.write(&home)?;
        (record, current, next)
    };
    let mut more = vec![
        ("force", SdkValue::Bool(force)),
        (
            "generation",
            SdkValue::Int(i64::try_from(next).unwrap_or(i64::MAX)),
        ),
    ];
    // Reclamation on the machine is keyed by the machine and backend, never
    // by the isolation class: `native`/`namespaces` are backends, not kinds.
    more.push(("machine", SdkValue::text(record.machine())));
    more.push((
        "backend",
        SdkValue::text(lifecycle::reclaim_backend(record.to_record()).as_str()),
    ));
    // a Sandbox reached directly keeps its Workspace records here: its
    // worker is told which it holds (#535 G4)
    if direct::forwarded(&record) {
        match direct::held(&home, &record) {
            Ok(held) => more.push((
                "held",
                SdkValue::List(held.into_iter().map(SdkValue::Text).collect()),
            )),
            Err(refusal) => {
                let _lock = home::lock::take(&home)?;
                let _sandbox = record::lock(&home, name)?;
                if let Some(now) = SandboxRecord::read(&home, name)? {
                    now.without_op().without_close_reason().write(&home)?;
                }
                return Err(refusal);
            }
        }
    }
    let closed = on_machine(context, &record, "sandbox.close", more);
    let _lock = home::lock::take(&home)?;
    // on `native` the Sandbox's own side removed the folder already
    let dir = record.dir(&home);
    let _sandbox = match dir.exists() {
        true => Some(record::lock(&home, name)?),
        false => None,
    };
    let event = match &closed {
        Ok(_) => CloseEvent::Stopped,
        Err(refusal) if refusal.code == "NoSuchSandbox" => CloseEvent::Stopped,
        Err(refusal) if refusal.code == "Unknown" => CloseEvent::Unknown,
        Err(refusal) if refusal.code == "Lost" => CloseEvent::ProviderGone,
        Err(_) => CloseEvent::Failed,
    };
    match lifecycle::close_transition(CloseState::Draining, event) {
        CloseDecision::Closed => match closed {
            Ok(value) => {
                direct::release(&home, &record);
                remove_folder(&dir)?;
                write_closed(&home, name, current, Some(next), "operator", None)?;
                Ok(home::with_field(value, "closed", SdkValue::text(name)))
            }
            // a Sandbox already gone loses its record too; the close
            // verified there is no runtime, so this is Closed, not Lost
            Err(_) => {
                direct::release(&home, &record);
                remove_folder(&dir)?;
                write_closed(&home, name, current, Some(next), "gone", Some("closed"))?;
                Ok(SdkValue::map([
                    ("closed", SdkValue::text(name)),
                    ("was_gone", SdkValue::Bool(true)),
                ]))
            }
        },
        // its outcome is not known: the record stays with the intent,
        // uncertain, until another `sandbox.close` clears it
        CloseDecision::Unresolved => {
            let refusal = closed.unwrap_err();
            let op = op.as_uncertain();
            let reason = "the close outcome is unknown; the runtime may or may not be stopped";
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.with_op(&op).with_close_reason(reason).write(&home)?;
            }
            op.log_uncertain(&home, "sandbox", name, &refusal.message);
            Err(refusal)
        }
        // never a blind kill: the provider is gone or reclamation is
        // stopped, and the record says so
        CloseDecision::Lost => {
            let refusal = closed.unwrap_err();
            // Reclamation blocked by a brake names the brake; make that the
            // reason `sandbox get` shows, not a generic code.
            let reason = match refusal.detail.get_text("reclaim") {
                Some(brake) => format!("automatic reclamation was stopped: {brake}"),
                None => refusal
                    .detail
                    .get_text("reason")
                    .unwrap_or("the provider no longer has the Sandbox")
                    .to_owned(),
            };
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.with_lost(&reason, clock::monotonic_ms()).write(&home)?;
            }
            Err(refusal)
        }
        // a certain failure leaves the Sandbox as it was
        CloseDecision::Ready => {
            let refusal = closed.unwrap_err();
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.without_op().without_close_reason().write(&home)?;
            }
            Err(refusal)
        }
        _ => {
            let refusal = closed.unwrap_err();
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.without_op().without_close_reason().write(&home)?;
            }
            Err(refusal)
        }
    }
}

/// The answer of a `sandbox.close` asked to `--dry-run`: the decision it
/// would take, with nothing changed.
fn close_preview(
    name: &str,
    record: &SandboxRecord,
    generation: u64,
    decision: &str,
    would: &str,
    reason: &str,
) -> SdkValue {
    SdkValue::map([
        ("sandbox", SdkValue::text(name)),
        ("dry_run", SdkValue::Bool(true)),
        ("state", SdkValue::text(record.state(false).as_str())),
        (
            "generation",
            SdkValue::Int(i64::try_from(generation).unwrap_or(i64::MAX)),
        ),
        ("decision", SdkValue::text(decision)),
        ("would", SdkValue::text(would)),
        ("reason", SdkValue::text(reason)),
    ])
}
/// The generation a close names, when it names one.
fn requested_generation(args: &SdkValue) -> Result<Option<u64>> {
    match args.get_int("generation") {
        None => Ok(None),
        Some(generation) if generation > 0 => {
            Ok(Some(u64::try_from(generation).map_err(|_| {
                refused("InvalidInput", "generation is a positive whole number")
            })?))
        }
        Some(_) => Err(refused(
            "InvalidInput",
            "generation is a positive whole number",
        )),
    }
}

/// Refuses a close while another operation's intent holds Sandbox
/// `record` (`Busy`); a gone owner's uncertain intent is what a close
/// clears, and a gone owner's certain one holds nothing.
fn clearable(record: &SandboxRecord) -> Result<()> {
    match record.op() {
        Some(op) if op.standing() == Standing::Unresolved => Ok(()),
        // a close that died after its generation CAS is uncertain even if it
        // predates the uncertain bit; another close may clear it
        Some(op)
            if op.name() == "sandbox.close" && op.phase() == "draining" && !op.owner_alive() =>
        {
            Ok(())
        }
        _ => record.free(),
    }
}

/// Closes Sandbox `record`, which has no id: its create's outcome was not
/// known, or its process ended before recording one. The folder goes, a
/// tombstone records the generation and reason, and the name is free. The
/// caller holds the home and Sandbox locks and has refused a live create.
fn close_unstarted(home: &Path, record: &SandboxRecord) -> Outcome {
    let state = record.state(false);
    let generation = record.generation();
    remove_folder(&record.dir(home))?;
    let reason = match state {
        SandboxState::Unresolved => "unresolved",
        _ => "gone",
    };
    write_closed(
        home,
        record.name(),
        generation,
        None,
        reason,
        Some(state.as_str()),
    )?;
    let mut fields = vec![
        ("closed", SdkValue::text(record.name())),
        ("was", SdkValue::text(state.as_str())),
    ];
    if state == SandboxState::Unresolved {
        let left = if record.op().is_some_and(|op| op.name() == "sandbox.close") {
            "its close's outcome was unknown: the runtime it was stopping is not recorded here"
        } else {
            "its create's outcome was unknown: a Sandbox it may have started on its machine is not recorded here"
        };
        fields.push(("left", SdkValue::text(left)));
    }
    // a layout 2 Sandbox whose runtime the conversion found ended
    if let Some(left) = record.to_record().get_text("left") {
        fields.push(("left", SdkValue::text(left)));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// The answer of a close that found the record `Lost` and reclaimed it.
fn closed_was(name: &str, was: &str) -> SdkValue {
    SdkValue::map([
        ("closed", SdkValue::text(name)),
        ("was", SdkValue::text(was)),
    ])
}

/// The tombstone of a closed name: `sandbox/<name>.closed.json`. It keeps
/// the generation a replay names and the reason the close recorded, so a
/// repeated close is a no-op and a stale one is refused.
fn closed_path(home: &Path, name: &str) -> PathBuf {
    home.join("sandbox").join(format!("{name}.closed.json"))
}

fn write_closed(
    home: &Path,
    name: &str,
    generation: u64,
    draining: Option<u64>,
    reason: &str,
    was: Option<&str>,
) -> Result<()> {
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        (
            "generation",
            SdkValue::Int(i64::try_from(generation).unwrap_or(i64::MAX)),
        ),
        ("closed", SdkValue::Int(home::now_ms())),
        ("reason", SdkValue::text(reason)),
    ];
    if let Some(draining) = draining {
        fields.push((
            "draining_generation",
            SdkValue::Int(i64::try_from(draining).unwrap_or(i64::MAX)),
        ));
    }
    if let Some(was) = was {
        fields.push(("was", SdkValue::text(was)));
    }
    home::write_entity(
        &closed_path(home, name),
        &SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()),
    )
}

/// A repeated close: its tombstone answers `Closed` quietly, a stale
/// generation is refused, and an unknown name stays `NoSuchSandbox`.
fn replay(home: &Path, name: &str, requested: Option<u64>) -> Outcome {
    let Some(closed) = home::read_record(&closed_path(home, name))? else {
        return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
    };
    let generation = closed.get_int("generation").unwrap_or(0).max(0);
    let draining = closed.get_int("draining_generation");
    if let Some(expected) = requested {
        let expected = i64::try_from(expected).unwrap_or(i64::MAX);
        if Some(expected) != Some(generation) && Some(expected) != draining {
            return Err(lifecycle::stale_generation(
                name,
                u64::try_from(expected).unwrap_or(u64::MAX),
                u64::try_from(generation).unwrap_or(0),
            ));
        }
    }
    let mut fields = vec![
        ("closed", SdkValue::text(name)),
        ("generation", SdkValue::Int(generation)),
        ("quiet", SdkValue::Bool(true)),
        (
            "reason",
            SdkValue::opt_text(closed.get_text("reason").map(str::to_owned)),
        ),
    ];
    if let Some(was) = closed.get_text("was") {
        fields.push(("was", SdkValue::text(was)));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// A create claims a name again: its tombstone goes.
pub(super) fn forget_closed(home: &Path, name: &str) -> Result<()> {
    match std::fs::remove_file(closed_path(home, name)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(failed(error)),
        _ => Ok(()),
    }
}
