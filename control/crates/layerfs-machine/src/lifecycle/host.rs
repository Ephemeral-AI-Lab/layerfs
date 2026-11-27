//! The host machine's own start and stop: its recorded state, its worker's
//! start and drain, and the holders a stop reports, which the Sandbox crate
//! reads ([`crate::MachineUsers::host_holders`]).
use crate::{record::machine::MachineRecord, route::HostPaths};
use layerfs_home::{
    disk::with_field,
    lock::home,
    names::MachineName,
    reply::{
        events::Events,
        outcome::{refused, Outcome},
    },
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

use std::path::Path;

fn set(home: &Path, name: &MachineName, stopped: bool) -> Result<bool, SdkRefusal> {
    let _guard = home::take(home)?;
    let record = MachineRecord::of(home, name.as_str())?;
    let previous = record.recorded_state() == Some("stopped");
    if previous != stopped {
        record
            .with(
                "state",
                SdkValue::text(if stopped { "stopped" } else { "enabled" }),
            )
            .write(home)?;
    }
    Ok(previous != stopped)
}

pub(crate) fn start(home: &Path, name: &MachineName) -> Outcome {
    let _operation = crate::force_stop_lock(home, name.as_str())?;
    let record = crate::machine_of(home, name.as_str())?;
    let paths = HostPaths::new(home)?;
    if record.recorded_state() == Some("stopped") && paths.fits() {
        // A stop that reported holders may still have a draining worker.
        // Wait for its lifetime lock before the new start sends anything.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while paths.try_hold()?.is_none() {
            if std::time::Instant::now() >= until {
                return Err(refused(
                    "Unavailable",
                    "the previous host worker is still draining",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let enabled = set(home, name, false)?;
    let was_ready = paths.control.exists();
    paths.for_management()?.start()?;
    Ok(crate::done(
        name,
        (enabled || !was_ready).then_some("started"),
    ))
}

pub(crate) fn stop(
    home: &Path,
    events: &mut Events<'_>,
    name: &MachineName,
    force: bool,
    users: &crate::MachineUsers,
) -> Outcome {
    let _operation = crate::force_stop_lock(home, name.as_str())?;
    if force {
        crate::lifecycle::force_stop::force_record(home, name.as_str())?;
    } else {
        crate::machine_of(home, name.as_str())?;
    }
    let changed = set(home, name, true)?;
    let paths = HostPaths::new(home)?;
    // what the drain answered (also when it had begun already): uncertain
    // write outcomes the worker could not record up to then (§8.5), and the
    // admitted writes still running, which the drain lets finish
    let mut drain = Drained::default();
    let drained = if paths.fits()
        && paths
            .control
            .try_exists()
            .map_err(layerfs_home::reply::outcome::failed)?
    {
        match paths
            .for_management()?
            .call_existing(true, "drain", SdkValue::map([]))
        {
            Ok(reply) => {
                if let Some(reply) = &reply {
                    drain = Drained::of(reply);
                }
                reply.is_some()
            }
            Err(error)
                if error.code == "Unavailable"
                    && error.detail.get_text("state") == Some("draining") =>
            {
                true
            }
            // a forced stop goes on without its worker's answer
            Err(_) if force => false,
            Err(error) => return Err(error),
        }
    } else {
        false
    };
    let forced = if force {
        Some(crate::lifecycle::force_stop::host_locked(
            home, events, name, users,
        )?)
    } else {
        None
    };
    // from here the stop is recorded and a forced one has torn down: a
    // failure keeps the forced report, and a Store that cannot be closed or
    // read is a holder naming why, not a bare error
    let with_forced = |mut error: SdkRefusal| {
        if let Some(outcome) = &forced {
            error.detail = with_field(error.detail, "forced_outcome", outcome.clone());
        }
        error
    };
    let blocking = (users.host_holders)(home, name.as_str()).map_err(with_forced)?;
    if !blocking.is_empty() {
        let mut error = refused(
            "InUse",
            format!(
                "host machine {name} is stopped; holders: {}{}",
                blocking.join(", "),
                match drain.running {
                    0 => String::new(),
                    _ => format!("; {}", drain.note()),
                }
            ),
        );
        error.detail = with_field(error.detail, "holders", texts(blocking));
        if let Some(outcome) = forced {
            error.detail = with_field(error.detail, "forced_outcome", outcome);
        }
        error.detail = drain.add(error.detail);
        return Err(error);
    }
    if let Some(outcome) = forced {
        return Ok(drain.add(outcome));
    }
    Ok(drain.add(crate::with_stopped(
        crate::done(name, (changed || drained).then_some("stopped")),
        false,
        &[],
        &[],
    )))
}
/// What the host worker's drain answered.
#[derive(Default)]
struct Drained {
    /// Uncertain write outcomes it could not record, up to the drain:
    /// listed (the first 32) and counted, when there are any.
    unrecorded: Option<(SdkValue, i64)>,
    /// Admitted writes still running, which finish after the drain.
    running: i64,
}

impl Drained {
    fn of(reply: &SdkValue) -> Self {
        let count = reply.get_int("unrecorded_count").filter(|count| *count > 0);
        Self {
            unrecorded: count.map(|count| {
                let listed = reply.get("unrecorded").cloned().unwrap_or_default();
                (listed, count)
            }),
            running: reply.get_int("writes_running").unwrap_or(0),
        }
    }

    /// The running writes in words.
    fn note(&self) -> String {
        match self.running {
            1 => "1 write still running; see worker.log".to_owned(),
            n => format!("{n} writes still running; see worker.log"),
        }
    }

    /// `value` with that account, when there is one. A running write may
    /// still produce an outcome no record holds: it goes to `worker.log`.
    fn add(&self, mut value: SdkValue) -> SdkValue {
        if let Some((listed, count)) = &self.unrecorded {
            value = with_field(value, "unrecorded_writes", listed.clone());
            value = with_field(value, "unrecorded_writes_count", SdkValue::Int(*count));
        }
        if self.running > 0 {
            value = with_field(value, "running_writes", SdkValue::Int(self.running));
            value = with_field(value, "running_writes_note", SdkValue::Text(self.note()));
        }
        value
    }
}
fn texts(items: Vec<String>) -> SdkValue {
    SdkValue::List(items.into_iter().map(SdkValue::Text).collect())
}
