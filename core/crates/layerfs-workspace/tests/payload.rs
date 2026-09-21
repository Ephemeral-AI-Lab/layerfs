//! Private payload input through the public Workspace/Source API and real Linux I/O.
//! The immutable root is supplied by a read-only fixture delivery capability;
//! this test does not claim a service save, mount, namespace edit or Commit.
use layerfs_bridge::contract::{Code, Inspect, Operation, Response};
use layerfs_workspace::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

const MIB: u64 = 1024 * 1024;
#[cfg(target_os = "linux")]
fn deadline() -> std::time::Instant {
    std::time::Instant::now() + std::time::Duration::from_secs(10)
}

fn config(path: &Path, quota: Option<u64>) -> WorkspaceConfig {
    WorkspaceConfig {
        root: path.to_owned(),
        max_count: 2,
        memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
        disk_budget_bytes: quota,
    }
}
fn delivery() -> OperationDelivery {
    Arc::new(|request, _, _, _| match &request.operation {
        Operation::Inspect {
            query: Inspect::Attributes { path },
            ..
        } if path.is_empty() => Ok(Response::Attributes {
            serial: 7,
            kind: 2,
            references: 0,
            content: [7; 32],
            metadata: [1; 32],
            mode: 0o755,
            mtime: -1,
            nanoseconds: 123,
            size: 0,
        }),
        _ => Err(Code::Unsupported.into()),
    })
}
fn private(path: &Path) {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).unwrap();
}
fn temporary(parent: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = parent.canonicalize().unwrap().join(format!(
        "payload-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    private(&path);
    path
}

#[cfg(not(target_os = "linux"))]
#[test]
fn private_payload_profile_explicitly_requires_linux() {
    let path = temporary(&std::env::temp_dir());
    let result = WorkspaceHost::new(config(&path, Some(128 * MIB)), delivery());
    match result {
        Err(WorkspaceError::Unsupported) => {}
        Err(WorkspaceError::Backing(failure))
            if failure.kind == std::io::ErrorKind::Unsupported => {}
        _ => panic!("unsupported host must explicitly refuse the backing capability"),
    }
    fs::remove_dir_all(path).unwrap();
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use layerfs_bridge::contract::{Source, MAX_FILE};
    use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt};
    use std::{io, sync::atomic::AtomicBool, time::Instant};

    fn options(path: &Path, id: &str, incarnation: u8) -> AttachOptions {
        let metadata = fs::metadata(path).unwrap();
        AttachOptions {
            id: id.into(),
            incarnation: [incarnation; 32],
            store: 1,
            base: Base::Root([1; 32]),
            owner_uid: metadata.uid(),
            owner_gid: metadata.gid(),
        }
    }

    struct Fixture {
        path: PathBuf,
        host: WorkspaceHost,
        workspace: Workspace,
    }
    impl Fixture {
        fn new(parent: &Path, quota: u64) -> Self {
            let path = temporary(parent);
            let host = WorkspaceHost::new(config(&path, Some(quota)), delivery()).unwrap();
            let workspace = host.attach(options(&path, "input", 9), deadline()).unwrap();
            Self {
                path,
                host,
                workspace,
            }
        }
        fn backing(&self) -> PathBuf {
            self.path.join("private-backing/input")
        }
        fn files(&self) -> Vec<PathBuf> {
            let mut files: Vec<_> = fs::read_dir(self.backing())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            files.sort();
            files
        }
        fn clean(self) {
            self.workspace.reclaim_payloads(deadline()).unwrap();
            let status = self.workspace.backing_status().unwrap();
            assert_eq!(
                (
                    status.payloads,
                    status.allocated_bytes,
                    status.reserved_bytes
                ),
                (0, 0, 0)
            );
            self.workspace.close_clean_until(deadline()).unwrap();
            assert!(!self.backing().exists());
            drop(self.workspace);
            drop(self.host);
            fs::remove_dir_all(self.path).unwrap();
        }
    }

    struct Generated {
        left: u64,
        position: u64,
        workspace: Option<Workspace>,
        peak: usize,
        max_fds: usize,
    }
    impl Generated {
        fn new(length: u64) -> Self {
            Self {
                left: length,
                position: 0,
                workspace: None,
                peak: 0,
                max_fds: 0,
            }
        }
    }
    impl Source for Generated {
        fn read(
            &mut self,
            buffer: &mut [u8],
            end: Instant,
            cancel: &AtomicBool,
        ) -> io::Result<usize> {
            if cancel.load(Ordering::Acquire) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if Instant::now() >= end {
                return Err(io::ErrorKind::TimedOut.into());
            }
            if let Some(workspace) = &self.workspace {
                self.peak = self.peak.max(workspace.status().unwrap().accounted_bytes);
                self.max_fds = self
                    .max_fds
                    .max(fs::read_dir("/proc/self/fd").unwrap().count());
            }
            let n = buffer.len().min(self.left as usize);
            for (index, byte) in buffer[..n].iter_mut().enumerate() {
                *byte = ((self.position + index as u64) % 251) as u8;
            }
            self.position += n as u64;
            self.left -= n as u64;
            Ok(n)
        }
    }
    fn verify(reader: &mut PayloadReader, start: u64, length: u64) {
        let cancel = AtomicBool::new(false);
        let mut buffer = [0; 17003];
        let mut position = start;
        let end = deadline();
        while position < start + length {
            let n = reader.read(&mut buffer, end, &cancel).unwrap();
            assert!(n > 0 && n as u64 <= start + length - position);
            for (index, byte) in buffer[..n].iter().enumerate() {
                assert_eq!(*byte, ((position + index as u64) % 251) as u8);
            }
            position += n as u64;
        }
        assert_eq!(reader.read(&mut buffer, end, &cancel).unwrap(), 0);
    }
    fn pass(name: &str) {
        println!("PAYLOAD_CHECK {name} PASS");
    }

    #[test]
    #[ignore = "requires owned ext4 mounts; run tests/payload_route.py"]
    fn linux_owned_payload_contract() {
        let parent = PathBuf::from(
            std::env::var_os("LAYERFS_PAYLOAD_TEST_ROOT").expect("explicit owned ext4 root"),
        );
        let small = PathBuf::from(
            std::env::var_os("LAYERFS_PAYLOAD_ENOSPC_ROOT").expect("explicit small ext4 root"),
        );
        let f = Fixture::new(&parent, 128 * MIB);
        let original = f.workspace.root();
        let bytes = 2 * MIB + 17;
        let payload = f
            .workspace
            .own_payload(bytes, &mut Generated::new(bytes), deadline())
            .unwrap();
        assert_eq!(payload.len(), bytes);
        assert!(!payload.is_empty());
        assert_eq!(f.files().len(), 3);
        let expected_disk = 3 * 4096 + 2 * MIB + 4096;
        assert_eq!(
            f.files()
                .iter()
                .map(|p| fs::metadata(p).unwrap().blocks() * 512)
                .sum::<u64>(),
            expected_disk
        );
        let status = f.workspace.backing_status().unwrap();
        assert!(status.accounting_complete);
        assert_eq!(
            (status.allocated_bytes, status.reserved_bytes),
            (expected_disk, 0)
        );
        assert_eq!(f.workspace.root(), original);
        for (start, length) in [
            (0, 11),
            (4095, 9001),
            (MIB - 13, 30001),
            (bytes - 17, 17),
            (bytes, 0),
        ] {
            let mut reader = payload.reader(start..start + length).unwrap();
            verify(&mut reader, start, length);
        }
        assert!(payload.reader(bytes..bytes + 1).is_err());
        pass("unaligned-segment-ranges");

        let mut first = payload.reader(0..bytes).unwrap();
        let second = payload.reader(0..1).unwrap();
        assert!(payload.reader(0..1).is_err());
        let clone = payload.clone();
        drop(payload);
        drop(clone);
        assert!(matches!(
            f.workspace.close_clean_until(deadline()),
            Err(WorkspaceError::Busy)
        ));
        verify(&mut first, 0, bytes);
        drop(first);
        drop(second);
        let cleanup = f.workspace.reclaim_payloads(deadline()).unwrap();
        assert_eq!(
            (
                cleanup.payloads_released,
                cleanup.segments_released,
                cleanup.bytes_released
            ),
            (1, 3, expected_disk)
        );
        assert!(f.files().is_empty());
        pass("reader-pins-and-admission");

        let empty = f
            .workspace
            .own_payload(0, &mut &[][..], deadline())
            .unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(f.files().is_empty());
        drop(empty);
        f.workspace.reclaim_payloads(deadline()).unwrap();
        let cancelled = f.workspace.own_payload(1, &mut &[1][..], Instant::now());
        assert!(matches!(cancelled, Err(WorkspaceError::Deadline)));
        struct Unread;
        impl Source for Unread {
            fn read(&mut self, _: &mut [u8], _: Instant, _: &AtomicBool) -> io::Result<usize> {
                panic!("refused input was consumed")
            }
        }
        assert!(f
            .workspace
            .own_payload(MAX_FILE + 1, &mut Unread, deadline())
            .is_err());
        assert!(f
            .workspace
            .own_payload(128 * MIB, &mut Unread, deadline())
            .is_err());
        pass("empty-deadline-and-quota");

        struct Broken {
            sent: bool,
        }
        impl Source for Broken {
            fn read(&mut self, b: &mut [u8], _: Instant, _: &AtomicBool) -> io::Result<usize> {
                if self.sent {
                    return Err(io::Error::other("external source failure"));
                }
                self.sent = true;
                b[..17].fill(3);
                Ok(17)
            }
        }
        for case in 0..3 {
            let error = match case {
                0 => f
                    .workspace
                    .own_payload(MIB + 1, &mut Broken { sent: false }, deadline()),
                1 => f.workspace.own_payload(2, &mut &[1][..], deadline()),
                _ => f.workspace.own_payload(1, &mut &[1, 2][..], deadline()),
            }
            .err()
            .expect("incomplete or extra input must not produce a token");
            assert!(matches!(error, WorkspaceError::Backing(_)), "{error:?}");
            let status = f.workspace.backing_status().unwrap();
            assert_eq!(status.failed_payloads, 1);
            assert!(status.allocated_bytes > 0);
            assert!(status.accounting_complete);
            assert_eq!(status.reserved_bytes, 0);
            assert_eq!(f.workspace.root(), original);
            f.workspace.reclaim_payloads(deadline()).unwrap();
            assert!(f.files().is_empty());
        }
        pass("failed-input-retention");

        fn file_limit(value: &str) {
            let status = std::process::Command::new("prlimit")
                .arg("--pid")
                .arg(std::process::id().to_string())
                .arg(format!("--fsize={value}:"))
                .status()
                .unwrap();
            assert!(status.success());
        }
        struct ShortWrite {
            generated: Generated,
            limited: bool,
        }
        impl Source for ShortWrite {
            fn read(
                &mut self,
                b: &mut [u8],
                end: Instant,
                cancel: &AtomicBool,
            ) -> io::Result<usize> {
                if !self.limited {
                    // Preallocation/header already completed. The following
                    // actual pwrite crosses the soft limit and writes 64KiB.
                    file_limit("69632");
                    self.limited = true;
                }
                self.generated.read(b, end, cancel)
            }
        }
        impl Drop for ShortWrite {
            fn drop(&mut self) {
                if self.limited {
                    file_limit("unlimited");
                }
            }
        }
        let limits = fs::read_to_string("/proc/self/limits").unwrap();
        assert!(limits.lines().any(
            |line| line.starts_with("Max file size") && line.matches("unlimited").count() == 2
        ));
        let mut source = ShortWrite {
            generated: Generated::new(MIB),
            limited: false,
        };
        let result = f.workspace.own_payload(MIB, &mut source, deadline());
        drop(source); // Restore the exact initially unlimited soft limit.
        let WorkspaceError::Backing(failure) =
            result.err().expect("short write must not publish input")
        else {
            panic!("typed short write failure")
        };
        assert_eq!(failure.phase, BackingPhase::Write);
        assert_eq!(failure.kind, io::ErrorKind::WriteZero);
        assert_eq!(failure.allocated_bytes, MIB + 4096);
        assert_eq!(failure.reserved_bytes, 0);
        let file = fs::File::open(f.files().pop().unwrap()).unwrap();
        let mut prefix = vec![0; 65536 + 4096];
        file.read_exact_at(&mut prefix, 4096).unwrap();
        for (index, byte) in prefix[..65536].iter().enumerate() {
            assert_eq!(*byte, (index % 251) as u8);
        }
        assert!(prefix[65536..].iter().all(|byte| *byte == 0));
        drop(file);
        println!("PAYLOAD_SHORT_WRITE {failure:?} observed_written_prefix=65536");
        f.workspace.reclaim_payloads(deadline()).unwrap();
        assert!(f.files().is_empty());
        pass("native-short-write-retention");

        let payload = f
            .workspace
            .own_payload(5000, &mut Generated::new(5000), deadline())
            .unwrap();
        let file = f.files().pop().unwrap();
        let raw = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file)
            .unwrap();
        let mut saved = [0];
        raw.read_exact_at(&mut saved, 0).unwrap();
        raw.write_all_at(&[0], 0).unwrap();
        let mut reader = payload.reader(0..1).unwrap();
        assert!(reader
            .read(&mut [0], deadline(), &AtomicBool::new(false))
            .is_err());
        drop(reader);
        drop(payload);
        let before = f.workspace.backing_status().unwrap().allocated_bytes;
        assert!(f.workspace.reclaim_payloads(deadline()).is_err());
        assert_eq!(
            f.workspace.backing_status().unwrap().allocated_bytes,
            before
        );
        assert!(file.exists());
        raw.write_all_at(&saved, 0).unwrap();
        drop(raw);
        f.workspace.reclaim_payloads(deadline()).unwrap();
        assert!(!file.exists());
        pass("corruption-and-explicit-cleanup");

        // Keep both read windows occupied while an acquisition and cleanup run.
        let old = f
            .workspace
            .own_payload(17, &mut Generated::new(17), deadline())
            .unwrap();
        let first = old.reader(0..17).unwrap();
        let second = old.reader(0..17).unwrap();
        drop(
            f.workspace
                .own_payload(0, &mut &[][..], deadline())
                .unwrap(),
        );
        struct Progress {
            workspace: Workspace,
            first: PayloadReader,
            second: PayloadReader,
            checked: bool,
        }
        impl Source for Progress {
            fn read(&mut self, b: &mut [u8], _: Instant, _: &AtomicBool) -> io::Result<usize> {
                if self.checked {
                    return Ok(0);
                }
                self.checked = true;
                assert!(matches!(
                    self.workspace.own_payload(0, &mut Unread, deadline()),
                    Err(WorkspaceError::Busy)
                ));
                assert!(self.workspace.status().unwrap().active_operations > 0);
                verify(&mut self.first, 0, 17);
                verify(&mut self.second, 0, 17);
                assert_eq!(
                    self.workspace
                        .reclaim_payloads(deadline())
                        .unwrap()
                        .payloads_released,
                    1
                );
                b[0] = 0;
                Ok(1)
            }
        }
        let mut source = Progress {
            workspace: f.workspace.clone(),
            first,
            second,
            checked: false,
        };
        let new = f.workspace.own_payload(1, &mut source, deadline()).unwrap();
        assert!(source.checked);
        drop(source);
        drop(old);
        drop(new);
        f.workspace.reclaim_payloads(deadline()).unwrap();
        pass("local-progress-and-window-headroom");

        let count = 64 * MIB;
        let baseline_fds = fs::read_dir("/proc/self/fd").unwrap().count();
        let mut source = Generated::new(count);
        source.workspace = Some(f.workspace.clone());
        let payload = f
            .workspace
            .own_payload(count, &mut source, deadline())
            .unwrap();
        assert!(source.peak <= DEFAULT_MEMORY_BUDGET_BYTES);
        assert!(source.max_fds <= baseline_fds + 16);
        assert_eq!(payload.len(), count);
        let observer = PathBuf::from(std::env::var_os("LAYERFS_PAYLOAD_RESIDENCY_ORACLE").unwrap());
        let output = std::process::Command::new("python3")
            .arg(&observer)
            .arg(f.backing())
            .arg("after-acquire")
            .output()
            .unwrap();
        println!(
            "PAYLOAD_RESIDENCY {}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut reader = payload.reader(0..count).unwrap();
        verify(&mut reader, 0, count);
        let output = std::process::Command::new("python3")
            .arg(&observer)
            .arg(f.backing())
            .arg("after-read")
            .output()
            .unwrap();
        println!(
            "PAYLOAD_RESIDENCY {}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cancel = AtomicBool::new(true);
        assert!(reader.read(&mut [0], deadline(), &cancel).is_err());
        drop(reader);
        drop(payload);
        println!("PAYLOAD_RESOURCE input_bytes={count} accounted_peak={} open_fds_peak={} baseline_fds={baseline_fds}", source.peak, source.max_fds);
        drop(source);
        f.workspace.reclaim_payloads(deadline()).unwrap();
        pass("large-stream-and-residency");
        f.clean();
        pass("clean-close");

        let exact = Fixture::new(&parent, 8192);
        let payload = exact
            .workspace
            .own_payload(1, &mut &[1][..], deadline())
            .unwrap();
        assert_eq!(
            exact.workspace.backing_status().unwrap().allocated_bytes,
            8192
        );
        assert!(exact
            .workspace
            .own_payload(1, &mut Unread, deadline())
            .is_err());
        drop(payload);
        exact.workspace.reclaim_payloads(deadline()).unwrap();
        drop(
            exact
                .workspace
                .own_payload(1, &mut &[2][..], deadline())
                .unwrap(),
        );
        exact.clean();
        pass("exact-block-quota-reuse");

        let seed = Fixture::new(&parent, 128 * MIB);
        let token = seed
            .workspace
            .own_payload(1, &mut &[7][..], deadline())
            .unwrap();
        let valid_segment = fs::read(seed.files().pop().unwrap()).unwrap();
        drop(token);
        seed.clean();
        let collision = Fixture::new(&parent, 128 * MIB);
        let path = collision.backing().join("p-0000000000000001-00000000");
        fs::write(&path, &valid_segment).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(collision
            .workspace
            .own_payload(1, &mut Unread, deadline())
            .is_err());
        let status = collision.workspace.backing_status().unwrap();
        assert!(status.admission_stopped && !status.accounting_complete);
        assert!(collision.workspace.reclaim_payloads(deadline()).is_err());
        assert_eq!(
            fs::read(&path).unwrap(),
            valid_segment,
            "even a matching header grants no ownership"
        );
        fs::remove_file(path).unwrap();
        collision.workspace.reclaim_payloads(deadline()).unwrap();
        assert!(
            !collision
                .workspace
                .backing_status()
                .unwrap()
                .admission_stopped
        );
        drop(
            collision
                .workspace
                .own_payload(1, &mut &[8][..], deadline())
                .unwrap(),
        );
        collision.clean();
        pass("unowned-collider-retention");

        let stale = temporary(&parent);
        private(&stale.join("private-backing"));
        private(&stale.join("private-backing/input"));
        fs::write(stale.join("private-backing/input/sentinel"), b"unowned").unwrap();
        let host = WorkspaceHost::new(config(&stale, Some(128 * MIB)), delivery()).unwrap();
        assert!(host
            .attach(options(&stale, "input", 9), deadline())
            .is_err());
        assert!(!stale.join("workspace/input").exists());
        assert_eq!(
            fs::read(stale.join("private-backing/input/sentinel")).unwrap(),
            b"unowned"
        );
        host.attach(options(&stale, "other", 8), deadline())
            .unwrap()
            .close_clean_until(deadline())
            .unwrap();
        drop(host);
        fs::remove_dir_all(stale).unwrap();
        let unsupported = temporary(Path::new("/dev/shm"));
        let host = WorkspaceHost::new(config(&unsupported, Some(128 * MIB)), delivery()).unwrap();
        let error = host
            .attach(options(&unsupported, "input", 9), deadline())
            .err()
            .expect("tmpfs is unsupported");
        assert!(
            matches!(error, WorkspaceError::Unsupported)
                || matches!(error, WorkspaceError::Backing(ref value) if value.kind == io::ErrorKind::Unsupported),
            "{error:?}"
        );
        assert!(!unsupported.join("workspace/input").exists());
        drop(host);
        fs::remove_dir_all(unsupported).unwrap();
        pass("stale-path-and-filesystem-refusal");

        let full = Fixture::new(&small, 128 * MIB);
        let error = full
            .workspace
            .own_payload(64 * MIB, &mut Generated::new(64 * MIB), deadline())
            .err()
            .expect("real filesystem must fill before the declared input ends");
        let WorkspaceError::Backing(failure) = error else {
            panic!("typed disk-full ownership: {error:?}")
        };
        assert_eq!(failure.kind, io::ErrorKind::StorageFull);
        assert!(failure.created_segments > 0);
        let status = full.workspace.backing_status().unwrap();
        assert_eq!(status.failed_payloads, 1);
        assert!(status.allocated_bytes > 0 && status.allocated_bytes < 64 * MIB);
        assert!(status.accounting_complete);
        assert_eq!(status.reserved_bytes, 0);
        println!("PAYLOAD_ENOSPC {failure:?}");
        full.clean();
        pass("native-enospc-retention");
    }
}
