//! The SDK trace route. While a traced call runs, every control exchange it
//! makes travels as a traced observation under the call's scope, and the
//! telemetry of the replies is collected for its trace line. An untraced call
//! sends exactly what it did before. Telemetry never changes a result.
#[cfg(unix)]
use crate::pool::{Controls, Lease};
#[cfg(unix)]
use layerfs_api::internal;
#[cfg(unix)]
use layerfs_api::internal::Control;
#[cfg(unix)]
use layerfs_bridge::control::SdkRefusal;
use layerfs_bridge::control::SdkValue;
use std::cell::{Cell, RefCell};
#[cfg(unix)]
use std::ops::{Deref, DerefMut};
use std::time::Duration;

/// Most telemetry values one call keeps; later ones are dropped.
const TELEMETRY: usize = 64;

thread_local! {
    static SCOPE: Cell<Option<[u8; 32]>> = const { Cell::new(None) };
    static COLLECTED: RefCell<Vec<SdkValue>> = const { RefCell::new(Vec::new()) };
    /// Time this thread's call waited for a free control connection.
    static WAITED: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// The connection wait of the call this thread runs, and resets it.
pub(crate) fn take_waited() -> Duration {
    WAITED.replace(Duration::ZERO)
}

/// Takes the trace scope a call's `observe` argument names out of its
/// arguments; an absent or malformed one leaves the call untraced.
pub(crate) fn take_scope(args: &mut SdkValue) -> Option<[u8; 32]> {
    let SdkValue::Map(fields) = args else {
        return None;
    };
    let at = fields.iter().position(|(name, _)| name == "observe")?;
    let SdkValue::Text(hex) = fields.remove(at).1 else {
        return None;
    };
    let scope: [u8; 32] = layerfs_bridge::sdk_values::unhex(&hex)?.try_into().ok()?;
    (scope != [0; 32]).then_some(scope)
}

/// Runs `work` with this thread traced under `scope` and returns the
/// telemetry its exchanges received. The scope is cleared on every exit.
pub(crate) fn run<T>(scope: Option<[u8; 32]>, work: impl FnOnce() -> T) -> (T, Vec<SdkValue>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SCOPE.set(None);
            COLLECTED.with_borrow_mut(Vec::clear);
        }
    }
    let _reset = Reset;
    COLLECTED.with_borrow_mut(Vec::clear);
    WAITED.set(Duration::ZERO);
    SCOPE.set(scope);
    let value = work();
    (value, COLLECTED.with_borrow_mut(std::mem::take))
}

/// True while this thread runs a traced call.
pub(crate) fn traced() -> bool {
    SCOPE.get().is_some()
}

/// Keeps one telemetry value of an in-process call for its trace line.
pub(crate) fn collect(value: SdkValue) {
    COLLECTED.with_borrow_mut(|collected| {
        if collected.len() < TELEMETRY {
            collected.push(value);
        }
    });
}

/// A Sandbox's control connection, traced while this thread is.
#[cfg(unix)]
pub(crate) struct Traced<'a>(Lease<'a>);

/// Lends a control connection for one call's exchanges; the wait for it is
/// the call's queue time.
#[cfg(unix)]
pub(crate) fn control(controls: &Controls) -> Result<Traced<'_>, SdkRefusal> {
    let (mut lease, waited) = controls.lend()?;
    WAITED.set(WAITED.get() + waited);
    internal::trace(&mut lease, SCOPE.get());
    Ok(Traced(lease))
}
#[cfg(unix)]
impl Drop for Traced<'_> {
    fn drop(&mut self) {
        let telemetry = internal::take_telemetry(&mut self.0);
        internal::trace(&mut self.0, None);
        COLLECTED.with_borrow_mut(|collected| {
            let room = TELEMETRY.saturating_sub(collected.len());
            collected.extend(telemetry.into_iter().take(room));
        });
    }
}
#[cfg(unix)]
impl Deref for Traced<'_> {
    type Target = Control;
    fn deref(&self) -> &Control {
        &self.0
    }
}
#[cfg(unix)]
impl DerefMut for Traced<'_> {
    fn deref_mut(&mut self) -> &mut Control {
        &mut self.0
    }
}
