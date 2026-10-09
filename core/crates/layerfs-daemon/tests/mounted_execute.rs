//! Real kernel mounts: a program stored in the Workspace is executed by the
//! kernel, and after an in-place rewrite the next execution is the rewrite.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use mounted::COMMAND;
use mutating::{direct, errno, harness, shell, unmounted};
use nix::errno::Errno;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    process::Command,
};

/// One dynamically linked ELF program of the image, as bytes.
fn program(name: &str) -> Vec<u8> {
    let bytes = fs::read(Path::new("/bin").join(name)).unwrap();
    assert_eq!(&bytes[..4], b"\x7fELF", "/bin/{name} is an ELF program");
    bytes
}
/// The exit status of the mounted program, executed by this unregistered
/// process: the kernel reads its header and maps its pages from the mount.
fn run(path: &Path) -> i32 {
    Command::new(path).status().unwrap().code().unwrap()
}

#[test]
fn a_stored_program_executes_and_its_in_place_rewrite_executes_next() {
    let (f, h) = harness("-execute");
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);
    let succeeds = program("true");
    let fails = program("false");
    assert_eq!(succeeds.len(), fails.len(), "an equal-length rewrite");
    assert_ne!(succeeds, fails);

    // A program written through ordinary WRITE requests.
    let tool = root.join("tool");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&tool)
        .unwrap();
    file.write_all(&succeeds).unwrap();

    // Without an execute bit nothing runs it, the mount owner included, while
    // its bytes stay readable.
    assert_eq!(
        errno(&Command::new(&tool).status().unwrap_err()),
        Errno::EACCES,
        "mode 644"
    );
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(
        errno(&Command::new(&tool).status().unwrap_err()),
        Errno::EACCES,
        "mode 000"
    );
    assert_eq!(fs::read(&tool).unwrap(), succeeds, "mode 000 read by root");
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(fs::metadata(&tool).unwrap().mode() & 0o7777, 0o755);

    // Executed while a writable descriptor is open: refused by the kernel,
    // and the refusal changes nothing.
    assert_eq!(
        errno(&Command::new(&tool).status().unwrap_err()),
        Errno::ETXTBSY
    );
    drop(file);
    assert_eq!(run(&tool), 0, "the stored program");
    assert!(
        shell(COMMAND, &tool.display().to_string()).status.success(),
        "the command identity executes it too"
    );

    // Rewritten in place, same length, with its pages cached by the runs
    // above: the next execution is the rewrite and not a cached page.
    let file = OpenOptions::new().write(true).open(&tool).unwrap();
    file.write_all_at(&fails, 0).unwrap();
    drop(file);
    assert_eq!(fs::read(&tool).unwrap(), fails, "cached read");
    assert_eq!(direct(&tool), fails, "daemon read");
    assert_eq!(run(&tool), 1, "the rewrite");
    assert_eq!(
        shell(COMMAND, &tool.display().to_string()).status.code(),
        Some(1)
    );

    // Replaced by rename: the name runs the new inode at once.
    let next = root.join("tool.next");
    fs::write(&next, &succeeds).unwrap();
    fs::set_permissions(&next, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(&next, &tool).unwrap();
    assert_eq!(run(&tool), 0, "the renamed replacement");

    // An interpreter script: the kernel reads the first line from the mount
    // and the interpreter reads the rest.
    let script = root.join("script");
    fs::write(&script, b"#!/bin/sh\necho first\nexit 3\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o750)).unwrap();
    let output = Command::new(&script).output().unwrap();
    assert_eq!(
        (output.status.code(), &output.stdout[..]),
        (Some(3), &b"first\n"[..])
    );
    fs::write(&script, b"#!/bin/sh\necho second\nexit 4\n").unwrap();
    let output = Command::new(&script).output().unwrap();
    assert_eq!(
        (output.status.code(), &output.stdout[..]),
        (Some(4), &b"second\n"[..]),
        "the rewritten script keeps its mode and runs as rewritten"
    );
    // Mode 750 with every inode owned by the command identity: the owner
    // runs it and another identity, which does run the mode 755 program,
    // does not.
    assert_eq!(
        shell(COMMAND, &script.display().to_string()).status.code(),
        Some(4)
    );
    assert!(shell(COMMAND + 1, &tool.display().to_string())
        .status
        .success());
    assert!(!shell(COMMAND + 1, &script.display().to_string())
        .status
        .success());

    println!(
        "MOUNTED_EXECUTE program_bytes={} rewrite=in-place rename=replaced script=rewritten",
        succeeds.len()
    );
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}
