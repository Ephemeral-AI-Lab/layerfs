use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

pub(super) fn command_ok(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    if command.status()?.success() {
        Ok(())
    } else {
        Err("native metadata command failed".into())
    }
}

pub(super) fn assert_apple_metadata(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)?;
    let finder = Command::new("xattr")
        .args(["-px", "com.apple.FinderInfo"])
        .arg(path)
        .output()?;
    let finder = String::from_utf8_lossy(&finder.stdout)
        .split_whitespace()
        .collect::<String>();
    if metadata.mtime() != -2
        || !String::from_utf8_lossy(&Command::new("ls").arg("-le").arg(path).output()?.stdout)
            .contains("everyone allow read")
        || !String::from_utf8_lossy(
            &Command::new("stat")
                .args(["-f", "%Sf"])
                .arg(path)
                .output()?
                .stdout,
        )
        .contains("hidden")
        || Command::new("xattr")
            .args(["-p", "com.apple.ResourceFork"])
            .arg(path)
            .output()?
            .stdout
            != b"resource-fork\n"
        || finder != "0000000000000000000000000000000000000000000000000000000000000001"
    {
        return Err("Apple metadata oracle mismatch".into());
    }
    Ok(())
}

pub(super) fn assert_managed_root(
    root: &Path,
    stage: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected = vec![0x6d; 1024 * 1024];
    expected[4096..8192].fill(0xa5);
    if stage >= 4 {
        expected.splice(8192..8192, std::iter::repeat_n(0x3c, 8192));
    }
    if stage >= 5 {
        expected.drain(16_384..20_480);
    }
    if stage >= 6 {
        expected.truncate(1_048_576);
    }
    let link = if stage >= 6 {
        "managed-link"
    } else {
        "relative-link"
    };
    if fs::read(root.join("nested/managed.bin"))? != expected
        || fs::read_link(root.join(link))? != Path::new("nested/large.bin")
    {
        return Err(format!("retained S{stage} oracle mismatch").into());
    }
    Ok(())
}
