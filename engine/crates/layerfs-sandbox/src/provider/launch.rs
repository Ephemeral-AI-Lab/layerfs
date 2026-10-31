//! One-attempt launch failure: acknowledged partial ownership and the last phase reached.
use crate::backend::Sandbox;
use crate::RuntimeError;
use std::fmt;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchPhase {
    Validate,
    Create,
    Start,
    Listener,
}
/// The caller retains the exact resource even when launch fails after creating it.
pub struct LaunchFailure {
    pub phase: LaunchPhase,
    pub instance: Option<Box<Sandbox>>,
    pub cause: RuntimeError,
}
impl fmt::Debug for LaunchFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchFailure")
            .field("phase", &self.phase)
            .field("identity", &self.instance.as_ref().map(|i| i.identity()))
            .field("cause", &self.cause)
            .finish()
    }
}
