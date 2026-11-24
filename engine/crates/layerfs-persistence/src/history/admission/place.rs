//! The place authority admission consults: the bounded view the control
//! layer read from its home, plus the place the admitted call itself carries.
//!
//! The Store keeps no place table and the engine reads no home file. The
//! control layer installs a bounded `PlaceView` for one admission (its
//! machines' `incarnation`, its Sandboxes' `generation` and the machine-level
//! `sandbox-retired` tombstone). Admission compares a conflicting intent's
//! recorded place with that view. The owner's namespace/boot record the
//! intent itself carries always applies first; a name the view does not
//! carry adds no evidence beside it, never a guess. Missing evidence is not
//! death.
//!
//! The view is cached once per admission, so every conflicting intent costs
//! one comparison and no read. A caller that could not read a home supplies
//! no authority for it and stays conservative live.

use super::*;
use layerfs_history::{PlaceRecord, PlaceView as SuppliedPlaceView};
use std::cell::RefCell;

thread_local! {
    /// The bounded view the control layer installed for one admission on this
    /// thread. A stack, so a nested call keeps the outer view.
    static SCOPED: RefCell<Vec<SuppliedPlaceView>> = const { RefCell::new(Vec::new()) };
}

/// Installs `view` for the duration of `f`; `None` runs `f` unchanged.
///
/// The control layer calls this around one Store call with the bounded view
/// it built from its home. It is scoped to the thread, so a concurrent call
/// never sees another call's places.
pub fn with_place_view<R>(view: Option<SuppliedPlaceView>, f: impl FnOnce() -> R) -> R {
    let Some(view) = view else {
        return f();
    };
    SCOPED.with(|views| views.borrow_mut().push(view));
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            SCOPED.with(|views| {
                let _ = views.borrow_mut().pop();
            });
        }
    }
    let guard = Guard;
    let result = f();
    drop(guard);
    result
}

/// Whether a control layer installed a view on this thread. A caller that has
/// one must not install a second, less complete one underneath it.
#[must_use]
pub fn has_place_view() -> bool {
    SCOPED.with(|views| !views.borrow().is_empty())
}

fn scoped() -> Option<SuppliedPlaceView> {
    SCOPED.with(|views| views.borrow().last().cloned())
}

/// The current place values one admission can see: the supplied bounded view
/// (control's home records) merged with the place the admitted call itself
/// carries. Cached once, so every conflicting intent costs one comparison.
pub(crate) struct PlaceView {
    supplied: Option<SuppliedPlaceView>,
    fallback_machine: Option<String>,
    fallback_incarnation: Option<String>,
    fallback_sandbox: Option<(String, String)>,
    fallback_generation: Option<String>,
}

impl PlaceView {
    fn current(supplied: Option<&SuppliedPlaceView>) -> Self {
        Self {
            supplied: supplied.cloned().or_else(scoped),
            fallback_machine: None,
            fallback_incarnation: None,
            fallback_sandbox: None,
            fallback_generation: None,
        }
    }

    /// The place `intent` itself carries, for a caller with no supplied view.
    pub(crate) fn for_intent(intent: &GcIntent) -> Self {
        Self::for_intent_with(intent, None)
    }

    /// The place `intent` carries, merged with `supplied` (or the view scoped
    /// on this thread). The supplied view wins for every name it carries; the
    /// call's own place is the fallback for its own machine and Sandbox, so a
    /// caller that could not read a home never loses its own evidence.
    pub(crate) fn for_intent_with(intent: &GcIntent, supplied: Option<&SuppliedPlaceView>) -> Self {
        let mut view = Self::current(supplied);
        view.fallback_machine = intent.machine.clone();
        view.fallback_incarnation = intent.machine_incarnation.clone();
        if let (Some(machine), Some(sandbox)) = (intent.machine.clone(), intent.sandbox.clone()) {
            view.fallback_sandbox = Some((machine, sandbox));
        }
        view.fallback_generation = intent.sandbox_generation.clone();
        view
    }

    /// The place of one machine: its start records the new ordered
    /// incarnation, and the reconcile compares that machine's intents.
    pub(crate) fn for_machine(machine: &str, incarnation: &str) -> Self {
        let mut view = Self::current(None);
        view.fallback_machine = Some(machine.to_owned());
        view.fallback_incarnation = Some(incarnation.to_owned());
        view
    }

    /// What this view can say about `intent`'s recorded place, or `None` when
    /// it can say nothing at all.
    fn record(&self, intent: &GcIntent) -> Option<PlaceRecord> {
        let mut record = self
            .supplied
            .as_ref()
            .map(|view| view.record(intent.machine.as_deref(), intent.sandbox.as_deref()))
            .unwrap_or_default();
        if record.machine_incarnation.is_none()
            && intent.machine.is_some()
            && intent.machine == self.fallback_machine
        {
            record.machine_incarnation = self.fallback_incarnation.clone();
        }
        if record.sandbox_generation.is_none()
            && self
                .fallback_sandbox
                .as_ref()
                .is_some_and(|(machine, sandbox)| {
                    intent.machine.as_deref() == Some(machine.as_str())
                        && intent.sandbox.as_deref() == Some(sandbox.as_str())
                })
        {
            record.sandbox_generation = self.fallback_generation.clone();
        }
        if record.machine_incarnation.is_none()
            && record.sandbox_generation.is_none()
            && record.sandbox_retired.is_none()
        {
            return None;
        }
        Some(record)
    }

    /// What the place authority proves about `intent`.
    ///
    /// The owner evidence always applies first, exactly as admission without
    /// place evidence decides: an owner this process proves gone (another
    /// boot of a local Store, a vanished or reused pid in this namespace) is
    /// dead whatever the place says. Otherwise a placed intent whose current
    /// place this admission can see is decided by that place: an older
    /// recorded value of the same home is dead, a newer recorded value (a
    /// restore) is uncertain, and another home's value is no evidence.
    /// Place evidence only adds proofs of death; it never hides the owner's.
    /// Missing evidence is never death.
    ///
    /// On a shared Store (PostgreSQL) a place proof is uncertain, never dead:
    /// two running copies of one home (a cloned home) mint tokens with the
    /// same home id, so the other copy's newer start does not prove this
    /// holder gone. The operator settles and releases such a row.
    pub(crate) fn liveness(
        &mut self,
        _tx: &Transaction<'_>,
        intent: &GcIntent,
        shared_store: bool,
        owners: &OwnerView,
    ) -> HistoryResult<Liveness> {
        if owners.dead(intent, shared_store) {
            return Ok(Liveness::Dead);
        }
        let authority = intent.authority();
        let place =
            match intent.machine_incarnation.is_some() || intent.sandbox_generation.is_some() {
                true => self.record(intent),
                false => None,
            };
        Ok(match authority.liveness(place.as_ref()) {
            Liveness::Dead if shared_store => Liveness::Uncertain,
            verdict => verdict,
        })
    }

    /// True only when the place authority (or, for a place-null intent, the
    /// caller's namespace/boot evidence) proves `intent` dead.
    pub(crate) fn dead(
        &mut self,
        tx: &Transaction<'_>,
        intent: &GcIntent,
        shared_store: bool,
        owners: &OwnerView,
    ) -> HistoryResult<bool> {
        Ok(self.liveness(tx, intent, shared_store, owners)? == Liveness::Dead)
    }

    /// True only when an observer may settle `intent`: the authority proves
    /// it dead and it is not uncertain. An uncertain intent is never settled
    /// by an observer, even when dead: the owning call (or an operator's
    /// `gc_release`) clears it, so the scope reports `Unresolved` (§5.1's
    /// dead-uncertain cell) instead of deleting the evidence that the
    /// operation may not have finished.
    pub(crate) fn settleable(
        &mut self,
        tx: &Transaction<'_>,
        intent: &GcIntent,
        shared_store: bool,
        owners: &OwnerView,
    ) -> HistoryResult<bool> {
        Ok(!intent.uncertain && self.dead(tx, intent, shared_store, owners)?)
    }
}
