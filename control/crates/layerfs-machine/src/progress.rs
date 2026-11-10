//! Machine lifecycle stages use the caller's existing progress stream.
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use std::time::Instant;

pub(crate) fn stage<T, E>(
    events: &mut Events<'_>,
    name: &str,
    run: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    events.progress(SdkValue::map([
        ("stage", SdkValue::text(name)),
        ("state", SdkValue::text("started")),
    ]));
    let started = Instant::now();
    let result = run();
    events.progress(SdkValue::map([
        ("stage", SdkValue::text(name)),
        (
            "state",
            SdkValue::text(if result.is_ok() {
                "completed"
            } else {
                "failed"
            }),
        ),
        (
            "elapsed_ms",
            SdkValue::Int(started.elapsed().as_millis().min(i64::MAX as u128) as i64),
        ),
    ]));
    result
}
