//! Compile-fail fixture: scope handles are not `Send`.

use layerfs_telemetry::timer::{Active, Timing, TimingScope};

fn main() {
    let (_result, _report) = Timing::record(
        "op",
        |root: &TimingScope<'_, Active>| -> Result<(), ()> {
            let child = root.child("child");
            let recorded = std::thread::spawn(move || child.is_recording())
                .join()
                .expect("thread");
            assert!(!recorded);
            Ok(())
        },
    );
}
