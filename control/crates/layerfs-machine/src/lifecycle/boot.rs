//! `machine.start` (#535 D11: the only call that starts a machine): it
//! dials the worker its `worker.json` records and, when none is recorded or
//! the provider says the machine is not running, starts the machine and its
//! worker, waits until it is ready (a readiness wait, not a retry) and
//! replaces `worker.json`. A recorded worker that does not answer on a
//! machine that runs is never started over: that would end it and every
//! Sandbox on it, so the machine is recorded `broken` `not_answering` and
//! the start refused. Two processes starting the same machine take turns
//! on its `start.lock`.
//!
//! A start that has something to do records its `op` in the machine's
//! record before its first effect (call surface 2.0), moves its phase as
//! it goes (`bundle`, `image` on docker, `boot`, `worker`) and drops it
//! with the record write that ends it: `running`; broken `outdated` or
//! `not_answering` when the worker it started is not one this build may
//! use; broken `failed` when its provider refused. A lost reply keeps the
//! `op`, uncertain, and `machine get <m> --with live` settles it. A worker
//! of another build is never replaced by a start over it (ruling R2).
use super::progress::Phases;
use super::stop::CHECK;
use crate::platform::{self as adapters, Code, HostOs, Platform, Route};
use crate::record::adapter_spec::{
    self as record, machine_of, refusal, save_running, save_unready,
};
use crate::record::machine::{MachineRecord, START};
use crate::route::{self, Endpoint};
use crate::status::state::{replace_worker, settle};
use crate::Related;
use layerfs_home::lock::home;
use layerfs_home::record::intent::Op;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::{refused, shell_word, with_fix};
use layerfs_home::reply::state::{Reason, State};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How long a call waits for another process starting the same machine.
const START_WAIT: Duration = Duration::from_secs(600);

/// The phases of a start, and of the docker machine's when it loads the
/// bundle's image for its worker container.
const PHASES: [&str; 3] = ["bundle", "boot", "worker"];
const DOCKER_PHASES: [&str; 4] = ["bundle", "image", "boot", "worker"];

/// The worker of machine `name`, started (as `machine.start` starts it) if
/// it does not answer its secret; `None` when the machine's calls run in
/// this process (`native` on macOS and Linux). What a call that needs the machine's worker
/// asks before it sends one.
pub fn running(home: &Path, name: &str, related: Related) -> Result<Option<Endpoint>> {
    let mut sink = |_| {};
    bring_up(home, name, related, &mut Events::new(&mut sink))?;
    Ok(record::worker(home, name)?.map(|endpoint| endpoint_of(&endpoint)))
}

/// Makes machine `name`'s worker answer its secret: `false` when it
/// already did (or the machine's calls run in this process), `true` when
/// the machine and its worker were started.
pub(super) fn bring_up(
    home: &Path,
    name: &str,
    related: Related,
    events: &mut Events<'_>,
) -> Result<bool> {
    let record = machine_of(home, name)?;
    let kind = record.platform()?;
    if kind.route(HostOs::current()) == Route::InProcess {
        return Ok(false);
    }
    if answers(home, name, &record, related)? == Answer::Ready {
        return Ok(false);
    }
    let _start = start_lock(home, name)?;
    // another process may have started it, or removed it, meanwhile
    let record = machine_of(home, name)?;
    match answers(home, name, &record, related)? {
        Answer::Ready => return Ok(false),
        Answer::Silent(silent) => not_running(home, name, &record, related, silent)?,
        Answer::Unrecorded => {}
    }
    start(home, name, &record, related, events)
}

/// What the recorded worker said to one hello.
#[derive(PartialEq)]
enum Answer {
    /// It answered its secret and is this build's.
    Ready,
    /// No worker is recorded for the machine.
    Unrecorded,
    /// One is recorded and its hello failed: why.
    Silent(SdkRefusal),
}

/// Whether the recorded worker answers its secret and is this build's; a
/// record that says otherwise then is put right. A worker of another build
/// is refused, never replaced by a start over it: the machine is recorded
/// `outdated`.
fn answers(home: &Path, name: &str, record: &MachineRecord, related: Related) -> Result<Answer> {
    let Some(recorded) = record::worker(home, name)? else {
        return Ok(Answer::Unrecorded);
    };
    let endpoint = endpoint_of(&recorded);
    let hello = match route::hello(&endpoint) {
        Ok(hello) => hello,
        Err(silent) => return Ok(Answer::Silent(silent)),
    };
    if let Some(mismatch) = hello.mismatch(&endpoint.address.to_string()) {
        let noted = {
            let _lock = home::take(home)?;
            record::update(home, name, |record| match record.reason() {
                Some(Reason::Outdated) => record,
                _ => record.broken_as(Reason::Outdated, &mismatch.message),
            })
        };
        return Err(outdated(home, name, mismatch, related, noted.err()));
    }
    if record.recorded(0) != State::Ready {
        let _lock = home::take(home)?;
        record::update(home, name, |record| {
            record.with("state", SdkValue::text("running")).mended()
        })?;
    }
    Ok(Answer::Ready)
}

/// A recorded worker whose hello failed (`silent`) is never replaced by a
/// start while its machine runs: the start would end that worker and every
/// Sandbox on it, which only `machine stop` may do (its `InUse` guard, or
/// `--force`). The provider is asked once, of the VM alone, within
/// [`CHECK`]: a machine it reports running is recorded `broken`
/// `not_answering` and refused `Unavailable`, the fix a stop and a start
/// and the message naming how many Sandboxes are on it; one it cannot say
/// of is refused too, its doctor as the fix. A stopped, failed or missing
/// one is started.
fn not_running(
    home: &Path,
    name: &str,
    record: &MachineRecord,
    related: Related,
    silent: SdkRefusal,
) -> Result<()> {
    let spec =
        record::provider_spec(home, record).map_err(|r| related.nameless_fix(home, name, r))?;
    let said = match adapters::provider_within(&spec, Instant::now() + CHECK) {
        Ok(status) if status.state != adapters::State::Running => return Ok(()),
        Ok(_) => format!("it runs and its worker does not answer: {}", silent.message),
        Err(error) => {
            let message = format!(
                "machine {name}'s worker does not answer ({}) and its provider could not say \
                 whether it runs, so it is not started over it: {}",
                silent.message, error.message
            );
            let doctor = format!("layerfs machine doctor {}", shell_word(name));
            return Err(with_fix(refused("Unavailable", message), doctor));
        }
    };
    let noted = {
        let _lock = home::take(home)?;
        record::update(home, name, |record| {
            record.broken_as(Reason::NotAnswering, &said)
        })
    };
    let sandboxes = (related.sandboxes_on)(home, name).map_or(0, |on| on.len());
    let mut message = format!(
        "machine {name} {said}; a start would replace that worker and end every Sandbox on it"
    );
    if sandboxes > 0 {
        message.push_str(&format!("; {}", crate::status::state::count(sandboxes)));
    }
    if let Some(why) = noted.err() {
        message.push_str(&format!(
            "; its record could not be written: {}",
            why.message
        ));
    }
    message.push_str(&format!(
        "; `layerfs machine doctor {}` says why",
        shell_word(name)
    ));
    let refusal = refused("Unavailable", message);
    Err(with_fix(refusal, replace_worker(name, sandboxes)))
}

/// `mismatch` (a worker of another build) as `machine.start` refuses it:
/// with the two commands that replace the worker as its `fix`, the stop
/// forced when Sandboxes are recorded on the machine, the message then
/// naming how many.
fn outdated(
    home: &Path,
    name: &str,
    mut mismatch: SdkRefusal,
    related: Related,
    unrecorded: Option<SdkRefusal>,
) -> SdkRefusal {
    let sandboxes = (related.sandboxes_on)(home, name).map_or(0, |on| on.len());
    match sandboxes {
        0 => {}
        1 => mismatch.message.push_str("; 1 Sandbox is on it"),
        n => mismatch
            .message
            .push_str(&format!("; {n} Sandboxes are on it")),
    }
    if let Some(why) = unrecorded {
        mismatch.message.push_str(&format!(
            "; its record could not be written: {}",
            why.message
        ));
    }
    with_fix(mismatch, replace_worker(name, sandboxes))
}

/// The record of machine `name` with `op` at `phase`, written; the phase
/// begins on the caller's stream.
pub(super) fn enter(
    home: &Path,
    name: &str,
    op: Op,
    phase: &str,
    phases: &mut Phases<'_, '_>,
) -> Result<Op> {
    let op = op.at(phase);
    {
        let _lock = home::take(home)?;
        record::update(home, name, |record| record.with_op(&op))?;
    }
    phases.begin(phase);
    Ok(op)
}

/// How a start ended short of a ready worker.
enum Ended {
    /// Before its provider was asked: the machine is as it was.
    Before(SdkRefusal),
    /// Its provider refused or failed.
    Provider(adapters::MachineError),
    /// It runs, and its worker is not one this build may use.
    Unready(Box<adapters::Running>, Reason, SdkRefusal),
}

/// Starts machine `name` (its `record`) and records its worker; under its
/// start lock.
fn start(
    home: &Path,
    name: &str,
    machine: &MachineRecord,
    related: Related,
    events: &mut Events<'_>,
) -> Result<bool> {
    machine
        .vm_present()
        .map_err(|r| related.nameless_fix(home, name, r))?;
    let mut spec = record::spec(home, machine).map_err(|r| related.nameless_fix(home, name, r))?;
    // the docker worker's container runs the bundle's image when the
    // machine names none (P5)
    let loads = spec.kind == Platform::Docker && spec.os_image.is_none();
    let mut phases = match loads {
        true => Phases::new(events, &DOCKER_PHASES),
        false => Phases::new(events, &PHASES),
    };
    let mut op = Op::begin(START, "bundle", SdkValue::Null)?;
    op = enter(home, name, op, "bundle", &mut phases)?;
    let mut run = || -> std::result::Result<Option<adapters::Running>, Ended> {
        let bundle = crate::setup::bundle::bundle().map_err(Ended::Before)?;
        // the first start pins its Engine before anything is made in it (an
        // image loaded, a volume or container made); an Engine that does not
        // answer gets nothing, so a start that cannot reach it pins nothing
        let socket = spec.tools.docker_socket.clone();
        if spec.kind == Platform::Docker
            && adapters::socket_state(&socket, adapters::SOCKET_ANSWER)
                == adapters::SocketState::Answering
        {
            let _lock = home::take(home).map_err(Ended::Before)?;
            record_socket(home, name, &socket).map_err(Ended::Before)?;
        }
        if loads {
            op = enter(home, name, op.clone(), "image", &mut phases).map_err(Ended::Before)?;
            let image = crate::setup::default_image::docker_default(home).map_err(Ended::Before)?;
            spec.os_image = Some(image);
        }
        // A new worker must replace a stopped one: its fresh incarnation
        // would mark every intent the old worker still serves dead. Stop it
        // and confirm the provider and its endpoint are gone; a start that
        // cannot confirm mints no token and settles nothing.
        stop_old(home, name, &spec).map_err(Ended::Before)?;
        // Persist the ordered token before the worker exists, so the worker
        // serves with a value that is already durable in the home record.
        let incarnation = mint_incarnation(home, name).map_err(Ended::Before)?;
        spec.incarnation = Some(incarnation);
        op = enter(home, name, op.clone(), "boot", &mut phases).map_err(Ended::Before)?;
        let Some(running) = adapters::start(&spec, &bundle).map_err(Ended::Provider)? else {
            return Ok(None);
        };
        let unready = |reason, refusal| Ended::Unready(Box::new(running.clone()), reason, refusal);
        let entered = enter(home, name, op.clone(), "worker", &mut phases);
        op = entered.map_err(|refusal| unready(Reason::Failed, refusal))?;
        let endpoint = endpoint_of(&running.endpoint);
        let hello = route::hello(&endpoint)
            .map_err(|unanswered| unready(Reason::NotAnswering, unanswered))?;
        // a worker of another build just started means the bundle is older
        // than this build: a stop and start would start it again
        let address = endpoint.address.to_string();
        match hello.stale_bundle(name, &bundle.dir, &address) {
            Some(stale) => Err(unready(Reason::Outdated, stale)),
            None => Ok(Some(running)),
        }
    };
    let outcome = run();
    let _lock = home::take(home)?;
    let pin = |home: &Path| match spec.kind {
        Platform::Docker => record_socket(home, name, &spec.tools.docker_socket),
        _ => Ok(()),
    };
    match outcome {
        Ok(Some(running)) => {
            save_running(home, name, &running)?;
            pin(home)?;
            phases.end();
            Ok(true)
        }
        Ok(None) => {
            record::update(home, name, MachineRecord::without_op)?;
            Ok(false)
        }
        Err(Ended::Before(refusal)) => {
            record::update(home, name, MachineRecord::without_op)?;
            Err(refusal)
        }
        Err(Ended::Unready(running, reason, refusal)) => {
            save_unready(home, name, &running, reason, &refusal.message)?;
            pin(home)?;
            Err(match reason {
                Reason::Outdated => outdated(home, name, refusal, related, None),
                _ => refusal,
            })
        }
        // a lost reply: what the provider did is not known
        Err(Ended::Provider(error)) if error.code == Code::Unknown => {
            let op = op.as_uncertain();
            record::update(home, name, |record| {
                (record.with_op(&op)).with("error", SdkValue::text(&error.message))
            })?;
            op.log_uncertain(home, "machine", name, &error.message);
            Err(with_fix(refusal(error), settle(name, &op)))
        }
        Err(Ended::Provider(error)) => {
            let reason = match error.code {
                Code::Unsupported => Reason::Unsupported,
                _ => Reason::Failed,
            };
            record::update(home, name, |record| {
                record.broken_as(reason, &error.message).without_op()
            })?;
            Err(refusal(error))
        }
    }
}

/// Stops the machine's worker (and VM where it has one) and confirms it no
/// longer runs before a new ordered incarnation is minted. The provider state
/// and the recorded endpoint must both say the old worker is gone; a stop that
/// cannot confirm one of them aborts the start, so no live mount is settled
/// by a token minted over its owner.
fn stop_old(home: &Path, name: &str, spec: &adapters::Spec) -> Result<()> {
    adapters::stop(spec).map_err(refusal)?;
    let state = adapters::provider_state(spec).map_err(refusal)?;
    // A running provider is never replaced. A broken one is not replaced
    // either, except on `docker`, where the start itself `docker rm -f`s the
    // container before making the new one; its endpoint still has to say the
    // old worker is gone below.
    if state == adapters::State::Running
        || (state == adapters::State::Failed && spec.kind != Platform::Docker)
    {
        return Err(refused(
            "Failed",
            format!(
                "the old worker of machine {name} did not stop (provider state {}); no new incarnation is minted and no intent is settled",
                state.as_str()
            ),
        ));
    }
    if let Some(endpoint) = recorded(home, name)? {
        if crate::route::hello(&endpoint).is_ok() {
            return Err(refused(
                "Failed",
                format!(
                    "the old worker of machine {name} still answers after it was stopped; no new incarnation is minted and no intent is settled"
                ),
            ));
        }
    }
    Ok(())
}

/// Mints the ordered incarnation for the worker this start launches and
/// records it in the home `MachineRecord` before the provider starts. It is
/// this home's value and is minted past the previous incarnation, so a clock
/// stepped backwards never makes the new start look older. The write is read
/// back: a start that cannot make its token durable never serves, rather than
/// mixing this start's intents with the previous one's.
fn mint_incarnation(home: &Path, name: &str) -> Result<String> {
    let id = layerfs_home::disk::opened(home)?.id;
    let _lock = layerfs_home::lock::home::take(home)?;
    let record = MachineRecord::of(home, name)?;
    let previous = record.incarnation().unwrap_or_default();
    let incarnation = layerfs_home::record::token::ordered_token(&id, "worker", &[previous]);
    record.with_incarnation(&incarnation).write(home)?;
    let saved = MachineRecord::of(home, name)?;
    if saved.incarnation() != Some(incarnation.as_str()) {
        return Err(refused(
            "Failed",
            format!("machine {name}'s ordered incarnation could not be recorded; the worker did not start"),
        ));
    }
    Ok(incarnation)
}

/// The docker machine's first start records the socket it starts on: once
/// the Engine answers, before anything is made in it, and again once the
/// provider started (a start that reached it without a ping), so it never
/// moves to another Engine (its data volume and loaded image are in that
/// one) until `machine update docker --docker ""` clears it. A socket
/// recorded meanwhile is kept.
fn record_socket(home: &Path, name: &str, socket: &Path) -> Result<()> {
    record::update(home, name, |record| match record.docker() {
        Some(_) => record,
        None => record.with("docker", SdkValue::text(socket.display().to_string())),
    })
}

/// Machine `name`'s `start.lock`, held until dropped. Starting, stopping,
/// creating and removing a machine take it, always before the home lock,
/// so a start never runs beside a create, a stop or a remove of the same
/// VM. A wait for it, at most [`START_WAIT`], is not a retry.
pub type StartLock = layerfs_home::lock::entity::EntityLock;

/// Take the machine lifecycle lock before a start, stop, create or remove.
pub fn start_lock(home: &Path, name: &str) -> Result<StartLock> {
    let path = layerfs_home::disk::paths::machine_dir(home, name).join("start.lock");
    let held = format!("machine {name} was being started, made or removed by another process");
    layerfs_home::lock::entity::take(&path, START_WAIT, &held)
}

/// The worker endpoint `machine/<name>/worker.json` records, as the client
/// dials it: the recorded address and secret, never a start and never a
/// dial. `None` when the machine records none.
pub fn recorded(home: &Path, name: &str) -> Result<Option<Endpoint>> {
    match record::worker(home, name)? {
        Some(endpoint) => Ok(Some(endpoint_of(&endpoint))),
        None => Ok(None),
    }
}

pub(super) fn endpoint_of(endpoint: &adapters::Endpoint) -> Endpoint {
    Endpoint {
        address: endpoint.address,
        secret: endpoint.secret.clone(),
    }
}
