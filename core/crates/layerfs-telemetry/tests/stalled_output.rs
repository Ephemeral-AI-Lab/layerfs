#![cfg(feature = "native")]
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn blocked_native_stderr_preserves_result_and_process_exit() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "stalled_writer_child",
            "--nocapture",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("stalled diagnostic writer prevented child exit");
        }
        std::thread::park_timeout(Duration::from_millis(10));
    }
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("product=Err(42), failed=1"));
}
#[test]
#[ignore = "subprocess helper; invoked by blocked_native_stderr_preserves_result_and_process_exit"]
fn stalled_writer_child() {
    use layerfs_telemetry::output::{Output, OutputConfig};
    let mut config = OutputConfig::forward();
    config.rate = 1024 * 1024;
    config.burst = 1024 * 1024;
    let output = Output::start(config).unwrap();
    let original = Err::<(), _>(42);
    for _ in 0..256 {
        output.submit(vec![b'x'; 4096]);
    }
    output.shutdown(Duration::from_secs(2));
    assert_eq!(original, Err(42));
    assert_eq!(output.loss().failed, 1);
    println!(
        "product=Err(42), failed=1, dropped={}",
        output.loss().dropped
    );
}
