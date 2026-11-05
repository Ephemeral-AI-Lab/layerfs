//! The `step` family's calls (#555): start one command asynchronously, wait
//! for its terminal status, read it now and cancel it by id. Each call is an
//! ordinary call of the table over the home's durable step store and the
//! process launcher; process detail stays in the adapter and the daemon is
//! not involved.
use crate::common::context::Context;
use crate::common::outcome::{refused, Outcome};
use crate::step::contract::{CancelOutcome, ExecId, ExecRequest, ExecStatus};
use crate::step::launcher::SystemClock;
use crate::step::process::ProcessLauncher;
use crate::step::runtime::StepRuntime;
use crate::step::store::WaitStore;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::time::Duration;

/// The wait `step.exec_wait` uses when the caller names none, in
/// milliseconds.
const DEFAULT_WAIT_MS: u64 = 30_000;

/// The runtime one call builds over the home's store and the real launcher.
type Runtime = StepRuntime<ProcessLauncher, SystemClock>;

/// Runs one `step` call.
pub fn step(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let runtime = Runtime::new(
        WaitStore::in_home(&context.home),
        ProcessLauncher::in_home(&context.home),
        SystemClock,
    );
    match call {
        "exec_async" => exec_async(&runtime, args),
        "exec_wait" => exec_wait(&runtime, args),
        "exec_status" => exec_status(&runtime, args),
        "cancel" => cancel(&runtime, args),
        _ => Err(refused("InvalidInput", format!("no call step.{call}"))),
    }
}

/// `step.exec_async(key, command, { timeout_ms })`.
fn exec_async(runtime: &Runtime, args: &SdkValue) -> Outcome {
    let request = ExecRequest::new(
        text(args, "key")?,
        text(args, "command")?,
        millis(args, "timeout_ms")?,
    )?;
    let started = runtime.exec_async(request)?;
    let mut fields = vec![
        (
            "exec_id".to_owned(),
            SdkValue::text(started.exec_id.as_str()),
        ),
        ("reused".to_owned(), SdkValue::Bool(started.reused)),
        (
            "status".to_owned(),
            SdkValue::text(started.status.as_text()),
        ),
    ];
    exit_code(started.status, &mut fields);
    Ok(SdkValue::Map(fields))
}

/// `step.exec_wait(exec_id, { wait_ms })`.
fn exec_wait(runtime: &Runtime, args: &SdkValue) -> Outcome {
    let id = ExecId::parse(text(args, "exec_id")?)?;
    let wait = millis(args, "wait_ms")?.unwrap_or(DEFAULT_WAIT_MS);
    let status = runtime.exec_wait(&id, Duration::from_millis(wait))?;
    Ok(status_value(&id, status))
}

/// `step.exec_status(exec_id)`.
fn exec_status(runtime: &Runtime, args: &SdkValue) -> Outcome {
    let id = ExecId::parse(text(args, "exec_id")?)?;
    let status = runtime.exec_status(&id)?;
    Ok(status_value(&id, status))
}

/// `step.cancel(exec_id)`.
fn cancel(runtime: &Runtime, args: &SdkValue) -> Outcome {
    let id = ExecId::parse(text(args, "exec_id")?)?;
    let outcome = runtime.cancel(&id)?;
    Ok(SdkValue::map([
        ("exec_id", SdkValue::text(id.as_str())),
        ("outcome", SdkValue::text(cancel_text(outcome))),
    ]))
}

/// One status as a call's result: its id, its status text and, for a
/// command that ended, its exit code.
fn status_value(id: &ExecId, status: ExecStatus) -> SdkValue {
    let mut fields = vec![
        ("exec_id".to_owned(), SdkValue::text(id.as_str())),
        ("status".to_owned(), SdkValue::text(status.as_text())),
    ];
    exit_code(status, &mut fields);
    SdkValue::Map(fields)
}

/// Adds `exit_code` when `status` carries one.
fn exit_code(status: ExecStatus, fields: &mut Vec<(String, SdkValue)>) {
    if let Some(code) = status.exit_code() {
        fields.push(("exit_code".to_owned(), SdkValue::Int(i64::from(code))));
    }
}

/// The cancel answer's text; `cancelled`, `timed_out` and `unknown` stay
/// distinct.
fn cancel_text(outcome: CancelOutcome) -> &'static str {
    match outcome {
        CancelOutcome::Cancelled => "cancelled",
        CancelOutcome::TimedOut => "timed_out",
        CancelOutcome::Unknown => "unknown",
    }
}

/// A required text argument.
fn text<'a>(args: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    args.get_text(name)
        .ok_or_else(|| refused("InvalidInput", format!("`{name}` is required")))
}

/// An optional non-negative millisecond count; a negative one is refused.
fn millis(args: &SdkValue, name: &str) -> Result<Option<u64>, SdkRefusal> {
    match args.get_int(name) {
        None => Ok(None),
        Some(value) if value < 0 => {
            Err(refused("InvalidInput", format!("`{name}` is not negative")))
        }
        Some(value) => Ok(Some(value as u64)),
    }
}
