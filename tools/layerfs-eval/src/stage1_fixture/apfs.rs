use super::contract::EvalResult;
use super::error::{display_error, io_error};
use super::selector::read_selector;
use super::tree::collect_paths;
use std::fs;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

pub fn assert_apfs(path: &Path) -> EvalResult<String> {
    if !cfg!(target_os = "macos") {
        return Err("strict Stage One reset requires macOS APFS".to_owned());
    }
    let path = path.canonicalize().map_err(io_error)?;
    let df = Command::new("/bin/df")
        .arg("-P")
        .arg(&path)
        .output()
        .map_err(io_error)?;
    if !df.status.success() {
        return Err("df could not identify fixture volume".to_owned());
    }
    let df = String::from_utf8(df.stdout).map_err(display_error)?;
    let mut rows = df.lines().skip(1).filter(|line| !line.trim().is_empty());
    let device = rows
        .next()
        .and_then(|line| line.split_whitespace().next())
        .filter(|device| device.starts_with("/dev/"))
        .ok_or_else(|| "df did not return a local fixture volume device".to_owned())?;
    if rows.next().is_some() {
        return Err("df returned multiple fixture volume devices".to_owned());
    }
    let output = Command::new("/usr/sbin/diskutil")
        .arg("info")
        .arg(device)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("diskutil could not identify fixture volume".to_owned());
    }
    let text = String::from_utf8(output.stdout).map_err(display_error)?;
    apfs_identity(device, &text)
}

pub(super) fn apfs_identity(device: &str, text: &str) -> EvalResult<String> {
    fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        text.lines().find_map(|line| {
            let (candidate, value) = line.split_once(':')?;
            (candidate.trim() == key)
                .then_some(value.trim())
                .filter(|value| !value.is_empty())
        })
    }

    let node = value(text, "Device Node")
        .ok_or_else(|| "diskutil did not report the fixture device node".to_owned())?;
    let identifier = value(text, "Device Identifier")
        .ok_or_else(|| "diskutil did not report the fixture device identifier".to_owned())?;
    let personality = value(text, "File System Personality")
        .ok_or_else(|| "diskutil did not report the fixture filesystem".to_owned())?;
    let bundle = value(text, "Type (Bundle)")
        .ok_or_else(|| "diskutil did not report the fixture filesystem bundle".to_owned())?;
    let mount = value(text, "Mount Point")
        .ok_or_else(|| "diskutil did not report the fixture mount point".to_owned())?;
    if node != device
        || !personality.eq_ignore_ascii_case("apfs")
        || !bundle.eq_ignore_ascii_case("apfs")
    {
        return Err("fixture volume is not identified as APFS".to_owned());
    }
    let volume_uuid = value(text, "Volume UUID")
        .ok_or_else(|| "diskutil did not report the fixture volume UUID".to_owned())?;
    let partition_uuid = value(text, "Disk / Partition UUID")
        .ok_or_else(|| "diskutil did not report the fixture partition UUID".to_owned())?;
    Ok(format!(
        "device_identifier={identifier};device_node={node};volume_uuid={volume_uuid};partition_uuid={partition_uuid};personality=apfs;type=apfs;mount_point={mount}"
    ))
}

pub(crate) fn clone_directory(source: &Path, destination: &Path) -> EvalResult<()> {
    if destination.exists() {
        return Err(format!("refusing to overwrite {}", destination.display()));
    }
    let status = Command::new("/bin/cp")
        .arg("-cR")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(io_error)?;
    if !status.success() {
        return Err(format!("APFS fixture clone failed with {status}"));
    }
    prove_distinct_inodes(source, destination)?;
    strict_clone_id(source, destination)?;
    Ok(())
}

pub(super) fn strict_clone_id(source: &Path, destination: &Path) -> EvalResult<u64> {
    let selector = read_selector(source)?;
    let file = format!("generation-{:016x}.sqlite", selector.generation);
    let source_id = clone_id(&source.join(&file))?;
    let destination_id = clone_id(&destination.join(file))?;
    if source_id == 0 || source_id != destination_id {
        return Err(format!(
            "strict APFS clone proof failed: source clone ID {source_id}, destination clone ID {destination_id}"
        ));
    }
    Ok(source_id)
}

#[cfg(target_os = "macos")]
pub(super) fn clone_id(path: &Path) -> EvalResult<u64> {
    use std::ffi::{c_char, c_int, c_ulong, c_void, CString};

    const FSOPT_ATTR_CMN_EXTENDED: c_ulong = 0x0000_0020;

    #[repr(C)]
    struct AttrList {
        bitmap_count: u16,
        reserved: u16,
        common: u32,
        volume: u32,
        directory: u32,
        file: u32,
        fork: u32,
    }

    unsafe extern "C" {
        fn getattrlist(
            path: *const c_char,
            attributes: *mut c_void,
            buffer: *mut c_void,
            size: usize,
            options: c_ulong,
        ) -> c_int;
    }

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "clone-ID path contains NUL".to_owned())?;
    let mut attributes = AttrList {
        bitmap_count: 5,
        reserved: 0,
        common: 0,
        volume: 0,
        directory: 0,
        file: 0,
        fork: 0x0000_0100,
    };
    let mut buffer = [0_u8; 12];
    // SAFETY: all pointers refer to live, correctly sized C-compatible values for
    // the duration of the call; getattrlist writes at most buffer.len() bytes.
    let result = unsafe {
        getattrlist(
            path.as_ptr(),
            (&mut attributes as *mut AttrList).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            FSOPT_ATTR_CMN_EXTENDED,
        )
    };
    if result != 0 || u32::from_ne_bytes(buffer[..4].try_into().unwrap()) != 12 {
        return Err(format!(
            "ATTR_CMNEXT_CLONEID unavailable for {}: {}",
            path.to_string_lossy(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(u64::from_ne_bytes(buffer[4..12].try_into().unwrap()))
}

#[cfg(not(target_os = "macos"))]
pub(super) fn clone_id(_path: &Path) -> EvalResult<u64> {
    Err("ATTR_CMNEXT_CLONEID requires macOS".to_owned())
}

pub(super) fn prove_distinct_inodes(source: &Path, destination: &Path) -> EvalResult<u64> {
    let mut files = Vec::new();
    collect_paths(source, source, &mut files)?;
    let mut count = 0_u64;
    for relative in files {
        let source_metadata = fs::symlink_metadata(source.join(&relative)).map_err(io_error)?;
        let destination_metadata =
            fs::symlink_metadata(destination.join(&relative)).map_err(io_error)?;
        if source_metadata.file_type().is_symlink() || destination_metadata.file_type().is_symlink()
        {
            return Err("fixture/reset may not contain symlinks".to_owned());
        }
        if source_metadata.is_file() {
            if !destination_metadata.is_file()
                || source_metadata.len() != destination_metadata.len()
                || (source_metadata.dev(), source_metadata.ino())
                    == (destination_metadata.dev(), destination_metadata.ino())
            {
                return Err(format!(
                    "clone inode/size proof failed for {}",
                    relative.display()
                ));
            }
            count += 1;
        }
    }
    if count == 0 {
        return Err("clone proof found no regular files".to_owned());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::clone_id;
    use super::{apfs_identity, assert_apfs};
    use std::fs;
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::MetadataExt;
    #[cfg(target_os = "macos")]
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "layerfs-eval-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clone_id_proves_apfs_copy_on_write_pair() {
        let root = test_directory("clone-id");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::write(&source, vec![0x42; 8_192]).unwrap();
        assert!(Command::new("/bin/cp")
            .arg("-c")
            .arg(&source)
            .arg(&destination)
            .status()
            .unwrap()
            .success());
        let source_metadata = fs::metadata(&source).unwrap();
        let destination_metadata = fs::metadata(&destination).unwrap();
        let source_id = clone_id(&source).unwrap();
        assert_ne!(source_id, 0);
        assert_eq!(source_id, clone_id(&destination).unwrap());
        assert_ne!(
            (source_metadata.dev(), source_metadata.ino()),
            (destination_metadata.dev(), destination_metadata.ino())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apfs_preflight_resolves_the_containing_volume() {
        let root = test_directory("apfs-preflight");
        let nested = root.join("directory with spaces");
        fs::create_dir(&nested).unwrap();
        let identity = assert_apfs(&root).unwrap();
        assert!(identity.contains("personality=apfs;type=apfs"));
        assert_eq!(identity, assert_apfs(&nested).unwrap());
        let allocation = nested.join("temporary allocation");
        fs::write(&allocation, vec![0x42; 1024 * 1024]).unwrap();
        assert_eq!(identity, assert_apfs(&nested).unwrap());
        fs::remove_file(allocation).unwrap();
        assert_eq!(identity, assert_apfs(&nested).unwrap());
        assert!(assert_apfs(&root.join("missing")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apfs_parser_rejects_deceptive_personality_text() {
        let text = concat!(
            "Device Node: /dev/disk-test\n",
            "Device Identifier: disk-test\n",
            "Mount Point: /fixture\n",
            "File System Personality: Not APFS\n",
            "Type (Bundle): apfs\n",
            "Volume UUID: volume\n",
            "Disk / Partition UUID: partition\n",
        );
        assert!(apfs_identity("/dev/disk-test", text).is_err());
    }
}
