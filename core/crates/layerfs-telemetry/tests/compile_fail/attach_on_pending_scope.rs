//! Compile-fail fixture: attachment belongs to the running handle.

use layerfs_telemetry::timer::{TimingReport, TimingScope};

// The fixture is intentionally never called: the body is what must not compile.
#[allow(dead_code)]
fn construct(scope: TimingScope<'_>, report: TimingReport) {
    scope.attach(Some(report));
}

fn main() {}
