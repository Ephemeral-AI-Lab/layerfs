//! Compile-fail fixture: a pending scope is consumed by `run`.

use layerfs_telemetry::timer::TimingScope;

// The fixture is intentionally never called: the body is what must not compile.
#[allow(dead_code)]
fn construct(scope: TimingScope<'_>) -> Result<(), ()> {
    scope.run(|_| Ok(()))?;
    scope.run(|_| Ok(()))
}

fn main() {}
