use layerfs_bridge::contract::Source;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[test]
fn portable_source_handles_dynamic_input_deadline_and_cancellation() {
    let mut bytes = b"input".as_slice();
    let source: &mut dyn Source = &mut bytes;
    let mut out = [0; 5];
    assert_eq!(
        source
            .read(&mut out, Instant::now(), &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    assert_eq!(
        source
            .read(&mut out, deadline, &AtomicBool::new(true))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::Interrupted
    );
    assert_eq!(
        source
            .read(&mut out, deadline, &AtomicBool::new(false))
            .unwrap(),
        5
    );
    assert_eq!(&out, b"input");
}
