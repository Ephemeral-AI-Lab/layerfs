//! Compile-fail fixture: a child scope must not escape its measured region.

use layerfs_telemetry::timer::{Active, Timing, TimingScope};

fn main() {
    let mut escaped = Vec::new();
    let (_result, _report) = Timing::record(
        "op",
        |root: &TimingScope<'_, Active>| -> Result<(), ()> {
            escaped.push(root.child("child"));
            Ok(())
        },
    );
    let _ = escaped.len();
}
