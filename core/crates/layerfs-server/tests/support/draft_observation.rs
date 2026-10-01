//! External bounded queries against this test executable's actual selected SQLite.
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
pub fn selected_library() -> PathBuf {
    #[cfg(target_os = "macos")]
    let libraries = {
        let output = Command::new("/usr/bin/otool")
            .arg("-L")
            .arg(std::env::current_exe().unwrap())
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter(|line| line.contains("libsqlite3."))
            .map(|line| PathBuf::from(line.split_whitespace().next().unwrap()))
            .collect::<BTreeSet<_>>()
    };
    #[cfg(target_os = "linux")]
    let libraries = {
        let mut maps = String::new();
        std::fs::File::open("/proc/self/maps")
            .unwrap()
            .take(1_048_577)
            .read_to_string(&mut maps)
            .unwrap();
        assert!(maps.len() <= 1_048_576);
        maps.lines()
            .filter_map(|line| line.split_whitespace().last())
            .filter(|path| path.starts_with('/') && path.contains("libsqlite3.so"))
            .map(PathBuf::from)
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        libraries.len(),
        1,
        "one exact dynamically selected library, with no provider substitute"
    );
    libraries.into_iter().next().unwrap()
}

fn helper(library: &Path, mode: &str, database: &Path, sql: Option<&str>) -> String {
    let metadata = std::fs::symlink_metadata(database).unwrap();
    assert!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "the owned fixture database itself must be a regular selected file"
    );
    let database = std::fs::canonicalize(database).unwrap();
    let mut command = Command::new("python3");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/claim_phase_assertion.py"))
        .arg(library)
        .arg(mode)
        .arg(database);
    if let Some(sql) = sql {
        command.arg(sql);
    }
    let output = command.output().unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "selected-library observation failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

pub fn number(library: &Path, database: &Path, sql: &str) -> u64 {
    helper(library, "number", database, Some(sql))
        .trim()
        .parse()
        .unwrap()
}

pub fn scratch_files(parent: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(parent).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_name().to_string_lossy().starts_with(".lfcs-") {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path()).unwrap();
        assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
        for file in std::fs::read_dir(entry.path()).unwrap() {
            let file = file.unwrap().path();
            let metadata = std::fs::symlink_metadata(&file).unwrap();
            assert!(metadata.is_file() && !metadata.file_type().is_symlink());
            assert_eq!(file.extension().unwrap(), "sqlite");
            files.push(file);
        }
    }
    files.sort();
    files
}

/// One external acknowledged read-lock owner on the exact selected provider.
pub struct ReaderLock {
    child: Option<std::process::Child>,
    control: std::os::unix::net::UnixStream,
    directory: PathBuf,
}
impl ReaderLock {
    pub fn acquire(database: &Path, library: &Path) -> Self {
        use nix::poll::{poll, PollFd, PollFlags};
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::{
            io::{Read, Write},
            os::{fd::AsFd, unix::net::UnixListener},
            process::Stdio,
            time::Duration,
        };
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let directory = std::env::temp_dir().join(format!(
            "lfcd-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let socket = directory.join("r");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let child = Command::new("python3")
            .arg("-u")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/scratch_reader.py"))
            .arg(library)
            .arg("read")
            .arg(std::fs::canonicalize(database).unwrap())
            .arg(socket)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut events = [PollFd::new(listener.as_fd(), PollFlags::POLLIN)];
        assert_eq!(poll(&mut events, 5000u16).unwrap(), 1);
        let mut control = listener.accept().unwrap().0;
        control.set_nonblocking(false).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut ready = [0];
        control.read_exact(&mut ready).unwrap();
        assert_eq!(ready, [1]);
        let mut length = [0; 4];
        control.read_exact(&mut length).unwrap();
        let length = u32::from_be_bytes(length) as usize;
        assert!(length > 0 && length <= 8192);
        let mut info = vec![0; length];
        control.read_exact(&mut info).unwrap();
        let info = String::from_utf8(info).unwrap();
        assert!(
            info.contains("\"mode\": \"read\"")
                && info.contains("\"source\":")
                && info.contains("SHARED held")
        );
        eprintln!("DIAGNOSTIC acknowledged actual selected read barrier: {info}");
        // No SQL/path access to the retained scratch occurs after Unknown.
        control.flush().unwrap();
        Self {
            child: Some(child),
            control,
            directory,
        }
    }
    pub fn release(&mut self) {
        use std::io::Write;
        self.control.write_all(&[2]).unwrap();
        let mut ack = [0];
        self.control.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [3]);
        let output = self.child.take().unwrap().wait_with_output().unwrap();
        assert!(output.stdout.len() <= 16384 && output.stderr.len() <= 16384);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!(
            "DIAGNOSTIC read barrier completion: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
impl Drop for ReaderLock {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
