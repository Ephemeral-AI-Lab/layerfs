//! Place/incarnation authority values.
//!
//! These are the pure decisions phase 3 admission needs before it touches a
//! Store: a changed machine incarnation or a retired Sandbox generation is
//! dead. The adapter owns the records; this crate only names the rule.
//!
//! Place values are ordered. A value minted by this release is
//! `p1:<home id>.<kind>:<zero-padded Unix milliseconds>:<nonce>`, so a newer
//! start compares greater than every younger one without a Store-side
//! counter. Only two values minted by the same home (the same label) are
//! ordered: every home names its own `box` machine and its own Sandboxes, so
//! on a Store two homes share, another home's newer value says nothing about
//! this home's place. A value written before the ordering, or without a home
//! id, cannot be ordered: equal is live and a difference is uncertain, never
//! dead, because a home restore can bring an older record back.

use std::collections::BTreeMap;

/// The durable place and lease evidence carried beside a mount intent.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaceAuthority {
    /// The worker generation that placed the mount.
    pub machine_incarnation: Option<String>,
    /// The Sandbox generation that placed it.
    pub sandbox_generation: Option<String>,
}

/// The current place values admission compares one recorded intent against.
///
/// The adapter building it supplies the current `MachineRecord.incarnation`
/// and `SandboxRecord.generation` it can see, plus the generation a home
/// tombstone records as retired. A missing value is no proof, never death.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaceRecord {
    /// The machine's current incarnation, when the adapter can see it.
    pub machine_incarnation: Option<String>,
    /// The Sandbox's current generation, when its record still exists.
    pub sandbox_generation: Option<String>,
    /// The generation the home tombstone records as retired, when it exists.
    pub sandbox_retired: Option<String>,
}

/// One Sandbox's current and retired generations, as the control layer read
/// them from its home.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SandboxPlace {
    /// The generation its current record names, when the record still exists.
    pub generation: Option<String>,
    /// The generation its machine tombstone names, when it was retired.
    pub retired: Option<String>,
}

/// The bounded place values one admission compares recorded intents against.
///
/// The control layer builds this from the home it can see: each machine's
/// current `MachineRecord.incarnation`, each Sandbox's current
/// `generation`, and the machine-level `sandbox-retired` tombstone. The
/// engine never reads a home file: it receives this view and treats a missing
/// name as **no proof**, never as death. A view therefore stays conservative
/// for a place the caller cannot see (another machine, a missing home).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaceView {
    machines: BTreeMap<String, String>,
    sandboxes: BTreeMap<(String, String), SandboxPlace>,
}

impl PlaceView {
    /// An empty view: no place evidence at all.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the view carries no place evidence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.machines.is_empty() && self.sandboxes.is_empty()
    }

    /// The machines this view can speak for, in name order.
    #[must_use]
    pub fn machines(&self) -> &BTreeMap<String, String> {
        &self.machines
    }

    /// The Sandboxes this view can speak for, keyed by machine and name.
    #[must_use]
    pub fn sandboxes(&self) -> &BTreeMap<(String, String), SandboxPlace> {
        &self.sandboxes
    }

    /// Records machine `machine`'s current incarnation.
    pub fn set_machine(&mut self, machine: impl Into<String>, incarnation: impl Into<String>) {
        self.machines.insert(machine.into(), incarnation.into());
    }

    /// Records Sandbox `sandbox` on `machine`'s current generation.
    pub fn set_sandbox_generation(
        &mut self,
        machine: &str,
        sandbox: &str,
        generation: impl Into<String>,
    ) {
        self.sandbox_place(machine, sandbox).generation = Some(generation.into());
    }

    /// Records the generation `machine`'s tombstone says `sandbox` retired.
    pub fn set_sandbox_retired(
        &mut self,
        machine: &str,
        sandbox: &str,
        retired: impl Into<String>,
    ) {
        self.sandbox_place(machine, sandbox).retired = Some(retired.into());
    }

    fn sandbox_place(&mut self, machine: &str, sandbox: &str) -> &mut SandboxPlace {
        self.sandboxes
            .entry((machine.to_owned(), sandbox.to_owned()))
            .or_default()
    }

    /// What this view can say about the place an intent names. A name the view
    /// does not carry is `None` (or an absent field): no proof, never death.
    #[must_use]
    pub fn record(&self, machine: Option<&str>, sandbox: Option<&str>) -> PlaceRecord {
        let machine_incarnation = machine.and_then(|name| self.machines.get(name).cloned());
        let place = match (machine, sandbox) {
            (Some(machine), Some(sandbox)) => self
                .sandboxes
                .get(&(machine.to_owned(), sandbox.to_owned())),
            _ => None,
        };
        PlaceRecord {
            machine_incarnation,
            sandbox_generation: place.and_then(|place| place.generation.clone()),
            sandbox_retired: place.and_then(|place| place.retired.clone()),
        }
    }
}

/// What the authority proves about a mount owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Liveness {
    /// The place is current.
    Live,
    /// The place may be dead but the evidence is not conclusive.
    Uncertain,
    /// The recorded place is provably gone.
    Dead,
}

/// How a recorded ordered place value relates to the current one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaceOrder {
    /// The two values name the same place.
    Equal,
    /// The recorded value is older than the current one.
    RecordedOlder,
    /// The recorded value is newer than the current one (a restore).
    RecordedNewer,
    /// At least one value is unordered (written before the ordering or
    /// without a home id), the two were minted by different homes, or the
    /// two name the same millisecond with different nonces, and the two
    /// differ.
    Unordered,
}

/// The prefix of an ordered place value minted by this release.
pub const ORDERED_TOKEN_PREFIX: &str = "p1:";

/// The `(label, milliseconds, nonce)` an ordered place value names, or
/// `None` when the value predates the ordering or names no minting home. The
/// label is `<home id>.<kind>`: a value whose label has no home id before its
/// first `.` is not ordered.
#[must_use]
pub fn ordered_token_parts(text: &str) -> Option<(&str, u64, &str)> {
    let rest = text.strip_prefix(ORDERED_TOKEN_PREFIX)?;
    let (label, rest) = rest.split_once(':')?;
    let (home, kind) = label.split_once('.')?;
    if home.is_empty() || kind.is_empty() {
        return None;
    }
    let (milliseconds, nonce) = rest.split_once(':')?;
    if nonce.is_empty() {
        return None;
    }
    Some((label, milliseconds.parse().ok()?, nonce))
}

/// Compares a recorded ordered place value with the current one.
///
/// Equal values are the same place. A recorded value older than the current
/// one, minted by the same home, is a place replaced by a newer start (dead);
/// a recorded value newer than the current one can only come from a restore
/// or rollback (uncertain, never dead). Values minted by different homes are
/// unordered: a machine or Sandbox name is a home's own, so another home's
/// start never proves this home's place gone. A value written before the
/// ordering compares equal only to itself; any difference is uncertain.
#[must_use]
pub fn compare_place(recorded: &str, current: &str) -> PlaceOrder {
    if recorded == current {
        return PlaceOrder::Equal;
    }
    match (ordered_token_parts(recorded), ordered_token_parts(current)) {
        // Only the millisecond of one home's values orders them. Two values
        // minted in the same millisecond have different nonces and no order
        // between them: calling one newer would settle a live owner on a coin
        // flip.
        (Some((recorded_label, recorded_ms, _)), Some((current_label, current_ms, _)))
            if recorded_label == current_label =>
        {
            match recorded_ms.cmp(&current_ms) {
                std::cmp::Ordering::Less => PlaceOrder::RecordedOlder,
                std::cmp::Ordering::Greater => PlaceOrder::RecordedNewer,
                std::cmp::Ordering::Equal => PlaceOrder::Unordered,
            }
        }
        _ => PlaceOrder::Unordered,
    }
}

impl PlaceAuthority {
    /// Classifies the owner for `now_ms` against the current place values
    /// `place`, when the adapter can see any.
    ///
    /// Place death wins over lease state: a machine incarnation or Sandbox
    /// generation the current record proves older than itself is
    /// [`Liveness::Dead`] even if the lease is stale. An expired lease
    /// without place death is [`Liveness::Uncertain`], never
    /// [`Liveness::Dead`], because a slow owner is not a dead owner. Missing
    /// place evidence is no proof: only the lease decides.
    pub fn liveness(&self, _now_ms: i64, place: Option<&PlaceRecord>) -> Liveness {
        if let Some(place) = place {
            if let (Some(recorded), Some(current)) = (
                self.machine_incarnation.as_deref(),
                place.machine_incarnation.as_deref(),
            ) {
                match compare_place(recorded, current) {
                    PlaceOrder::Equal => {}
                    PlaceOrder::RecordedOlder => return Liveness::Dead,
                    PlaceOrder::RecordedNewer | PlaceOrder::Unordered => {
                        return Liveness::Uncertain
                    }
                }
            }
            if let Some(recorded) = self.sandbox_generation.as_deref() {
                // The current record decides first: a recreated Sandbox has a
                // newer generation, so it is live even though an older
                // tombstone still names the generation before it.
                match place.sandbox_generation.as_deref() {
                    Some(current) => match compare_place(recorded, current) {
                        PlaceOrder::Equal => {
                            // The record can remain beside the tombstone its
                            // close wrote: only a tombstone naming this very
                            // generation retires it, and an older one names an
                            // earlier Sandbox of the same name. A tombstone
                            // newer than the live record cannot come from this
                            // record's close (a clock step or a restore), so
                            // it is uncertain, never dead.
                            if let Some(retired) = place.sandbox_retired.as_deref() {
                                match compare_place(recorded, retired) {
                                    PlaceOrder::Equal => return Liveness::Dead,
                                    PlaceOrder::RecordedNewer => {}
                                    PlaceOrder::RecordedOlder | PlaceOrder::Unordered => {
                                        return Liveness::Uncertain
                                    }
                                }
                            }
                        }
                        PlaceOrder::RecordedOlder => return Liveness::Dead,
                        PlaceOrder::RecordedNewer | PlaceOrder::Unordered => {
                            return Liveness::Uncertain
                        }
                    },
                    // The record is gone: only its tombstone can prove it
                    // retired. A newer recorded generation is a restore and is
                    // uncertain, never dead.
                    None => {
                        if let Some(retired) = place.sandbox_retired.as_deref() {
                            match compare_place(recorded, retired) {
                                PlaceOrder::Equal | PlaceOrder::RecordedOlder => {
                                    return Liveness::Dead
                                }
                                PlaceOrder::RecordedNewer | PlaceOrder::Unordered => {
                                    return Liveness::Uncertain
                                }
                            }
                        }
                    }
                }
            }
        }
        Liveness::Live
    }
}
