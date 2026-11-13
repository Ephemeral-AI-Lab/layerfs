//! A Sandbox's control connection lent for one call, traced while the
//! call's thread is (moved from `common/trace.rs`, layerfs-project#103).
use crate::daemon::pool::{Controls, Lease};
use crate::internal::{self, Control};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::trace::{add_waited, collect, scope};
use std::ops::{Deref, DerefMut};

/// A Sandbox's control connection, traced while this thread is.
pub struct Traced<'a>(Lease<'a>);

/// Lends a control connection for one call's exchanges; the wait for it is
/// the call's queue time.
pub fn control(controls: &Controls) -> Result<Traced<'_>, SdkRefusal> {
    let (mut lease, waited) = controls.lend()?;
    add_waited(waited);
    internal::trace(&mut lease, scope());
    Ok(Traced(lease))
}

/// [`control`], waiting at most `wait` for admission and for an in-use
/// connection. A short-wait caller reports the daemon's exact refusal instead
/// of holding its operation intent for the whole admission wait (#540).
pub fn control_within(
    controls: &Controls,
    wait: std::time::Duration,
) -> Result<Traced<'_>, SdkRefusal> {
    let (mut lease, waited) = controls.lend_within(wait)?;
    add_waited(waited);
    internal::trace(&mut lease, scope());
    Ok(Traced(lease))
}
impl Drop for Traced<'_> {
    fn drop(&mut self) {
        let telemetry = internal::take_telemetry(&mut self.0);
        internal::trace(&mut self.0, None);
        telemetry.into_iter().for_each(collect);
    }
}
impl Deref for Traced<'_> {
    type Target = Control;
    fn deref(&self) -> &Control {
        &self.0
    }
}
impl DerefMut for Traced<'_> {
    fn deref_mut(&mut self) -> &mut Control {
        &mut self.0
    }
}
