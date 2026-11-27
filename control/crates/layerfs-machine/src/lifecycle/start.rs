//! `machine.start` (#535 P1): for UIs, which start a machine before they
//! use it and never rely on a list or get to start one (P4). It starts a
//! stopped machine and its worker and waits until it is ready; a running
//! one answers at once, as does built-in `native`. A call that had nothing
//! to do answers `unchanged: true` and records nothing (call surface 2.0);
//! one that works records its `op` and reports its phases. `machine.stop`
//! is `stop.rs`.
use super::boot::bring_up;
use crate::Related;
use layerfs_home::names::MachineName;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// What a start or a stop answers: `{ unchanged, outcome, machine }`.
/// `changed` is the word for what it did (`started`, `stopped`); a call
/// with nothing to do answers `unchanged: true`.
pub fn done(name: &str, changed: Option<&str>) -> SdkValue {
    SdkValue::map([
        ("unchanged", SdkValue::Bool(changed.is_none())),
        ("outcome", SdkValue::text(changed.unwrap_or("unchanged"))),
        ("machine", SdkValue::text(name)),
    ])
}

/// `machine.start(machine)`: the machine's worker answers its secret
/// (`unchanged: true`), or the machine and its worker are started (under
/// its `start.lock`) and waited for. A readiness wait, not a retry.
pub fn start(
    home: &Path,
    name: &MachineName,
    related: Related,
    events: &mut Events<'_>,
) -> Outcome {
    let started = bring_up(home, name, related, events)?;
    Ok(done(name, started.then_some("started")))
}
