//! Parent/child scope lifecycle and disabled execution.
//!
//! A [`Timing`] recording hands the operation a running handle. That handle is
//! the only place child scopes come from, so a child borrows the handle it was
//! created from and cannot outlive the measured region of its parent. A pending
//! scope is single-use: [`run`](TimingScope::run) consumes it, starts its timer,
//! runs the operation and returns the unchanged `Result`.

use std::borrow::Cow;
use std::marker::PhantomData;

use crate::timer::recording::{Label, NodeId, Recording};
use crate::timer::report::{NodeOutcome, TimingReport};

/// Entry point for starting or disabling a recording.
///
/// Both operations preserve the returned `Result` and hand back a completed
/// [`TimingReport`]. Disabled recording performs no clock reads and creates no
/// nodes.
pub struct Timing;

/// Marker for a child scope whose timer has not started yet.
pub struct Pending;

/// Marker for the handle available to a running operation.
pub struct Active;

/// A handle to one node of a recording.
///
/// A [`Pending`] scope is created by
/// [`child`](TimingScope::child) and started by [`run`](TimingScope::run). The
/// [`Active`] handle is passed to the operation closure and is the only way to
/// create further children or attach a completed report. Handles are neither
/// `Send` nor `Sync`, and their lifetimes tie them to the recording that
/// created them.
pub struct TimingScope<'a, S = Pending> {
    target: Target<'a>,
    state: PhantomData<S>,
    thread: PhantomData<*const ()>,
}

enum Target<'a> {
    Disabled,
    Planned {
        recording: &'a Recording,
        parent: NodeId,
        label: Label,
    },
    Running {
        recording: &'a Recording,
        node: NodeId,
    },
}

impl<S> TimingScope<'_, S> {
    /// Returns true when this scope can still contribute a measured node.
    ///
    /// Adapters use it to request optional timing only when it will be recorded.
    /// It is false for disabled recording and for a child whose parent has no
    /// remaining node or depth budget.
    pub fn is_recording(&self) -> bool {
        match &self.target {
            Target::Disabled => false,
            Target::Running { .. } => true,
            Target::Planned {
                recording, parent, ..
            } => recording.child_fits(*parent),
        }
    }
}

impl<'a> TimingScope<'a, Active> {
    fn running(recording: &'a Recording, node: NodeId) -> Self {
        Self {
            target: Target::Running { recording, node },
            state: PhantomData,
            thread: PhantomData,
        }
    }

    fn disabled() -> Self {
        Self {
            target: Target::Disabled,
            state: PhantomData,
            thread: PhantomData,
        }
    }

    /// Creates a pending child scope below this running node.
    ///
    /// The child borrows this handle, so it cannot outlive the measured region
    /// it was created in. Nothing is recorded until the child is run.
    pub fn child(&self, name: impl Into<Cow<'static, str>>) -> TimingScope<'_, Pending> {
        let Target::Running { recording, node } = &self.target else {
            return TimingScope::disabled_pending();
        };
        TimingScope::planned(recording, *node, Label::bounded(name))
    }

    /// Attaches an owned completed report below this running node.
    ///
    /// Attachment consumes completed data, never a live scope. `None`, a
    /// disabled report, or a report that has to be clipped marks this node and
    /// its ancestors incomplete without changing the product result. On a
    /// disabled scope attachment is a no-op.
    pub fn attach(&self, report: Option<TimingReport>) {
        if let Target::Running { recording, node } = &self.target {
            recording.attach(*node, report);
        }
    }
}

impl<'a> TimingScope<'a, Pending> {
    fn planned(recording: &'a Recording, parent: NodeId, label: Label) -> Self {
        Self {
            target: Target::Planned {
                recording,
                parent,
                label,
            },
            state: PhantomData,
            thread: PhantomData,
        }
    }

    fn disabled_pending<'b>() -> TimingScope<'b, Pending> {
        TimingScope {
            target: Target::Disabled,
            state: PhantomData,
            thread: PhantomData,
        }
    }

    /// Starts this scope's timer, runs the operation and returns its `Result`.
    ///
    /// The original value or error is returned unchanged. Ordinary errors are
    /// recorded before they propagate, completed children are retained when `?`
    /// returns early, and a scope whose node could not be created runs the
    /// operation without measurement instead of failing.
    pub fn run<T, E, F>(self, operation: F) -> Result<T, E>
    where
        F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>,
    {
        let Target::Planned {
            recording,
            parent,
            label,
        } = self.target
        else {
            let handle = TimingScope::<Active>::disabled();
            return operation(&handle);
        };
        let Some(started) = recording.begin_child(parent, label) else {
            let handle = TimingScope::<Active>::disabled();
            return operation(&handle);
        };
        let id = started.id;
        let handle = TimingScope::running(recording, id);
        let result = operation(&handle);
        recording.finish(started, NodeOutcome::from_result(&result));
        result
    }
}

impl Timing {
    /// Starts a recording, runs the operation and returns both results.
    ///
    /// The root starts before `operation` is called and its elapsed time stays
    /// inclusive of everything the operation and its children did. The returned
    /// report owns the completed tree; the caller decides whether to render it,
    /// save it or drop it.
    pub fn record<T, E, F>(
        name: impl Into<Cow<'static, str>>,
        operation: F,
    ) -> (Result<T, E>, TimingReport)
    where
        F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>,
    {
        let recording = Recording::start(Label::bounded(name));
        let root = recording.root();
        let result = {
            let handle = TimingScope::running(&recording, root);
            operation(&handle)
        };
        let report = recording.complete(NodeOutcome::from_result(&result));
        (result, report)
    }

    /// Runs the operation without clocks, nodes or timing output.
    ///
    /// The name is accepted for call-site symmetry and then dropped. The
    /// operation still runs and its `Result` is returned unchanged, together
    /// with a report that has no root.
    pub fn disabled<T, E, F>(
        name: impl Into<Cow<'static, str>>,
        operation: F,
    ) -> (Result<T, E>, TimingReport)
    where
        F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>,
    {
        let _ = name;
        let handle = TimingScope::<Active>::disabled();
        let result = operation(&handle);
        (result, TimingReport::disabled())
    }
}
