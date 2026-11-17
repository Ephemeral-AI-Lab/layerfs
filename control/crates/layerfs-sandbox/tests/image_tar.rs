//! The flat image tar copied from `docker export`: the init layer's files and
//! device/FIFO entries are left out, everything else passes byte for byte.
#[allow(dead_code)]
#[path = "../../layerfs-sandbox-backend/tests/support/image.rs"]
mod fixture;
use fixture::{pax_record, Tar};
use layerfs_sandbox::image_tar::{dropped, Filtered};
use std::io::Read;

fn filtered(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // small reads exercise every buffer boundary
    let mut reader = Filtered::new(bytes);
    let mut chunk = [0u8; 7];
    loop {
        let n = reader.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
    }
    out
}

#[test]
fn the_init_layer_and_devices_are_dropped() {
    let export = Tar::new()
        .file(".dockerenv", 0o755, b"")
        .directory("bin/", 0o755)
        .file("bin/tool", 0o755, b"#!/bin/sh\necho hi\n")
        .directory("dev/", 0o755)
        .directory("dev/pts/", 0o755)
        .file("dev/console", 0o600, b"")
        .directory("etc/", 0o755)
        .file("etc/hosts", 0o644, b"")
        .file("etc/hostname", 0o644, b"")
        .file("etc/resolv.conf", 0o644, b"")
        .symlink("etc/mtab", "/proc/mounts")
        .file("etc/os-release", 0o644, b"ID=debian\n")
        .raw("null", b'3', 0o666, b"")
        .raw("fifo", b'6', 0o644, b"")
        .finish();
    let expected = Tar::new()
        .directory("bin/", 0o755)
        .file("bin/tool", 0o755, b"#!/bin/sh\necho hi\n")
        .directory("dev/", 0o755)
        .directory("etc/", 0o755)
        .file("etc/os-release", 0o644, b"ID=debian\n")
        .finish();
    assert_eq!(filtered(&export), expected);
}

#[test]
fn a_pax_named_entry_is_decided_by_its_long_name() {
    let long = format!("usr/share/{}", "x".repeat(120));
    let header = |name: &str| pax_record("path", name);
    let export = Tar::new()
        .raw("PaxHeaders/1", b'x', 0o644, &header(&long))
        .file("placeholder-1", 0o644, b"kept")
        .raw("PaxHeaders/2", b'x', 0o644, &header("./etc/hosts"))
        .file("placeholder-2", 0o644, b"")
        .finish();
    let expected = Tar::new()
        .raw("PaxHeaders/1", b'x', 0o644, &header(&long))
        .file("placeholder-1", 0o644, b"kept")
        .finish();
    assert_eq!(filtered(&export), expected);
}

#[test]
fn which_paths_are_dropped() {
    for path in [
        ".dockerenv",
        "./etc/hosts",
        "/etc/resolv.conf",
        "dev/shm",
        "dev/pts/0",
    ] {
        assert!(dropped(path, b'0'), "{path}");
    }
    for path in ["dev", "dev/", "etc/hosts.allow", "usr/dev/x", "etc"] {
        assert!(!dropped(path, b'5'), "{path}");
    }
    assert!(dropped("anything", b'4'));
}

#[test]
fn two_exports_differing_only_in_the_init_layer_filter_to_the_same_bytes() {
    let base = |hostname: &[u8]| {
        Tar::new()
            .file(".dockerenv", 0o755, b"")
            .file("etc/hostname", 0o644, hostname)
            .file("etc/os-release", 0o644, b"ID=debian\n")
            .finish()
    };
    assert_eq!(filtered(&base(b"abc\n")), filtered(&base(b"defg\n")));
}

#[test]
fn pax_time_stripping_handles_empty_malformed_xattr_and_all_time_blocks_byte_for_byte() {
    // 1. Empty PAX `x` block: kept intact (header mtime zeroed on directories, untouched on files).
    let empty_pax_dir = Tar::new()
        .at(1_700_000_123)
        .raw("PaxHeaders/empty", b'x', 0o644, b"")
        .directory("dir/", 0o755)
        .raw("PaxHeaders/emptyf", b'x', 0o644, b"")
        .file("file.txt", 0o644, b"ok")
        .finish();
    let expected_empty_pax = Tar::new()
        .at(0)
        .raw("PaxHeaders/empty", b'x', 0o644, b"")
        .directory("dir/", 0o755)
        .at(1_700_000_123)
        .raw("PaxHeaders/emptyf", b'x', 0o644, b"")
        .file("file.txt", 0o644, b"ok")
        .finish();
    assert_eq!(filtered(&empty_pax_dir), expected_empty_pax);

    // 2. Malformed PAX length fields: missing space, non-numeric, length <= space + 1, length > rest.len().
    for malformed in [
        b"mtime=1700000000\n".as_slice(),
        b"abc mtime=1700000000\n".as_slice(),
        b"2 mtime=1\n".as_slice(),
        b"0 a=b\n".as_slice(),
        b"999 mtime=1700000000\n".as_slice(),
    ] {
        let input = Tar::new()
            .at(1_700_000_456)
            .raw("PaxHeaders/bad", b'x', 0o644, malformed)
            .directory("dir/", 0o755)
            .finish();
        let expected = Tar::new()
            .at(0)
            .raw("PaxHeaders/bad", b'x', 0o644, malformed)
            .directory("dir/", 0o755)
            .finish();
        assert_eq!(filtered(&input), expected);
    }

    // 3. PAX `x` block with only time records (mtime, atime, ctime) becomes empty -> entire `x` block dropped.
    let mut only_times = pax_record("mtime", "1700000001.1");
    only_times.extend_from_slice(&pax_record("atime", "1700000002.2"));
    only_times.extend_from_slice(&pax_record("ctime", "1700000003.3"));
    let input_only_times = Tar::new()
        .at(1_700_000_789)
        .raw("PaxHeaders/times", b'x', 0o644, &only_times)
        .directory("dir/", 0o755)
        .finish();
    let expected_only_times = Tar::new().at(0).directory("dir/", 0o755).finish();
    assert_eq!(filtered(&input_only_times), expected_only_times);

    // 4. PAX `x` block mixing times with SCHILY.xattr.* (and path): only time records are stripped on
    //    directories while SCHILY.xattr.* stays intact; regular files keep all PAX records untouched.
    let xattr_cap = pax_record("SCHILY.xattr.security.capability", "cap_net_bind");
    let xattr_user = pax_record("SCHILY.xattr.user.note", "kept-value");
    let dir_path = pax_record("path", "var/with-xattr/");

    let mut mixed_dir_pax = Vec::new();
    mixed_dir_pax.extend_from_slice(&dir_path);
    mixed_dir_pax.extend_from_slice(&pax_record("mtime", "1700000100.5"));
    mixed_dir_pax.extend_from_slice(&xattr_cap);
    mixed_dir_pax.extend_from_slice(&pax_record("atime", "1700000200.5"));
    mixed_dir_pax.extend_from_slice(&pax_record("ctime", "1700000300.5"));
    mixed_dir_pax.extend_from_slice(&xattr_user);

    let mut expected_dir_pax = Vec::new();
    expected_dir_pax.extend_from_slice(&dir_path);
    expected_dir_pax.extend_from_slice(&xattr_cap);
    expected_dir_pax.extend_from_slice(&xattr_user);

    let mut mixed_file_pax = pax_record("mtime", "1700000555.25");
    mixed_file_pax.extend_from_slice(&xattr_user);

    let input_mixed = Tar::new()
        .at(1_700_000_111)
        .raw("PaxHeaders/mixed_dir", b'x', 0o644, &mixed_dir_pax)
        .directory("placeholder-dir", 0o755)
        .at(1_700_000_222)
        .raw("PaxHeaders/mixed_file", b'x', 0o644, &mixed_file_pax)
        .file("usr/bin/app", 0o755, b"elf")
        .finish();
    let expected_mixed = Tar::new()
        .at(0)
        .raw("PaxHeaders/mixed_dir", b'x', 0o644, &expected_dir_pax)
        .directory("placeholder-dir", 0o755)
        .at(1_700_000_222)
        .raw("PaxHeaders/mixed_file", b'x', 0o644, &mixed_file_pax)
        .file("usr/bin/app", 0o755, b"elf")
        .finish();
    assert_eq!(filtered(&input_mixed), expected_mixed);
}
