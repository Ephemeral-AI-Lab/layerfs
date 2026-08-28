use std::cell::RefCell;
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Disposition {
    Pass,
    Revise,
    Fail,
}
impl Disposition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Revise => "REVISE",
            Self::Fail => "FAIL",
        }
    }
}
pub(crate) struct FailureContext {
    pub(crate) row_id: String,
    pub(crate) phase: &'static str,
    pub(crate) started: Option<Instant>,
}
impl Default for FailureContext {
    fn default() -> Self {
        Self {
            row_id: String::new(),
            phase: "admission",
            started: None,
        }
    }
}
thread_local! {
    static FAILURE_CONTEXT: RefCell<FailureContext> = RefCell::new(FailureContext::default());
}
pub(crate) fn begin_failure_context(row_id: &str, phase: &'static str) {
    FAILURE_CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        if context.row_id != row_id {
            context.row_id = row_id.to_owned();
            context.started = Some(Instant::now());
        }
        context.phase = phase;
    });
}
pub(crate) fn set_failure_phase(phase: &'static str) {
    FAILURE_CONTEXT.with(|context| context.borrow_mut().phase = phase);
}
pub(crate) fn failure_observation() -> (String, &'static str, u128) {
    FAILURE_CONTEXT.with(|context| {
        let context = context.borrow();
        (
            context.row_id.clone(),
            context.phase,
            context
                .started
                .as_ref()
                .map_or(Duration::ZERO, Instant::elapsed)
                .as_nanos(),
        )
    })
}
