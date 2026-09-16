//! Compile-fail fixture: only a running handle can create child scopes.

use layerfs_telemetry::timer::TimingScope;

// The fixture is intentionally never called: the body is what must not compile.
#[allow(dead_code)]
fn construct(scope: TimingScope<'_>) {
    let _child = scope.child("child");
}

fn main() {}
