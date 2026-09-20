#![cfg(feature = "native")]
use layerfs_telemetry::{
    operation::OperationRecorder,
    output::{Output, OutputConfig, OutputMode},
};
use std::{fs::File, io::Write, path::PathBuf, time::Duration};
#[test]
#[ignore = "requires a dedicated disposable filesystem no larger than 64 MiB"]
fn actual_enospc_preserves_the_original_result() {
    let root = PathBuf::from(
        std::env::var_os("LAYERFS_TEST_DISK_ROOT").expect("dedicated filesystem required"),
    );
    let fs = nix::sys::statvfs::statvfs(&root).unwrap();
    let filesystem_bytes = u128::from(fs.blocks())
        .checked_mul(u128::from(fs.fragment_size()))
        .unwrap();
    assert!(
        (8 * 1024 * 1024..=64 * 1024 * 1024).contains(&filesystem_bytes),
        "refusing to fill a non-test filesystem"
    );
    let mut config = OutputConfig::forward();
    config.mode = OutputMode::Local;
    config.directory = Some(root.join("operational"));
    let output = Output::start(config).unwrap();
    let mut filler = File::create(root.join("owned-test-filler")).unwrap();
    let block = [0x5a; 4096];
    let mut written = 0;
    let error = loop {
        assert!(written <= 64 * 1024 * 1024);
        match filler.write_all(&block) {
            Ok(()) => written += block.len(),
            Err(e) => break e,
        }
    };
    assert_eq!(error.raw_os_error(), Some(nix::errno::Errno::ENOSPC as i32));
    let (result, _) = OperationRecorder::disabled().run(1, "known-result", |_| Err::<(), _>(42));
    output.submit(b"LFT1 {\"known_result\":42}\n".to_vec());
    output.shutdown(Duration::from_secs(2));
    assert_eq!(result, Err(42));
    assert_eq!(output.loss().failed, 1);
    println!("filesystem_bytes={filesystem_bytes} filler_bytes={} native_error=ENOSPC output_failures={} original_result=Err(42)",filler.metadata().unwrap().len(),output.loss().failed);
}
