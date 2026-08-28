use std::io::Write;
use std::process::Command;
#[cfg(target_os = "macos")]
pub(super) fn valid_json(document: &str) {
    use std::process::Stdio;
    let mut child = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "/dev/null", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(document.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        document,
        String::from_utf8_lossy(&output.stderr)
    );
}
