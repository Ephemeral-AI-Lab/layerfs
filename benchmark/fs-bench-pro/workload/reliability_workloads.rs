use super::workspace_common::{self as common, Content, Entry, EntryKind, Receipt};
use super::{workspace_reliability as family, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

fn file(path: &str, content: &Content, exclusive: bool) -> Result<u64> {
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(exclusive)
        .truncate(true)
        .open(path)?;
    content.write_to(&mut out)?;
    out.sync_all()?;
    Ok(content.len())
}
fn normalize(entries: &[Entry], times_only: bool) -> Result<()> {
    let mut entries = entries.iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.path
            .matches('/')
            .count()
            .cmp(&a.path.matches('/').count())
            .then_with(|| b.path.cmp(&a.path))
    });
    for entry in entries {
        if entry.path.starts_with("sentinels/") || entry.path == "links/alias.dat" {
            continue;
        }
        if times_only {
            common::set_mtime_nofollow(
                Path::new(&entry.path),
                entry.mtime_seconds,
                entry.mtime_nanoseconds,
            )?;
        } else {
            common::set_metadata(Path::new(&entry.path), entry)?;
        }
        if !matches!(entry.kind, EntryKind::Symlink(_)) {
            File::open(&entry.path)?.sync_all()?;
        }
    }
    Ok(())
}
fn checkpoint(case: &super::workspace_common::Case, state: &str) -> Result<()> {
    let expected = family::expected(case, state, 1)?;
    normalize(&expected, false)?;
    if let Some(entry) = expected.iter().find(|e| {
        e.path
            == if case.kind == "dirty-net-zero" {
                "sentinels/f0002.dat"
            } else {
                "sentinels/f0000.dat"
            }
    }) {
        common::set_metadata(Path::new(&entry.path), entry)?;
    }
    let receipt = common::verify_native(Path::new("."), &expected)?;
    println!("checkpoint_{state}={receipt:?}");
    Ok(())
}
fn require_handle_bytes(handle: &mut (impl Read + Seek), expected: &[u8]) -> Result<usize> {
    handle.seek(SeekFrom::Start(0))?;
    let mut actual = Vec::with_capacity(expected.len() + 1);
    handle
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut actual)?;
    if actual != expected {
        return Err("open-unlinked descriptor content differs from independent oracle".into());
    }
    Ok(actual.len())
}
fn require_modes(paths: &[&str], expected: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for path in paths {
        let actual = fs::metadata(path)?.permissions().mode() & 0o7777;
        if actual != expected {
            return Err(
                format!("mode at {path}: expected {expected:o}, observed {actual:o}").into(),
            );
        }
    }
    println!("observed_modes_{expected:o}={paths:?}");
    Ok(())
}
fn require_mtimes(paths: &[&str], seconds: i64, nanoseconds: i64) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    for path in paths {
        let metadata = fs::metadata(path)?;
        if (metadata.mtime(), metadata.mtime_nsec()) != (seconds, nanoseconds) {
            return Err(format!(
                "mtime at {path}: expected {seconds}.{nanoseconds:09}, observed {}.{:09}",
                metadata.mtime(),
                metadata.mtime_nsec()
            )
            .into());
        }
    }
    println!("observed_mtime={seconds}.{nanoseconds:09} paths={paths:?}");
    Ok(())
}

fn sustained_handoff(
    outgoing: &std::sync::mpsc::Sender<()>,
    incoming: &std::sync::mpsc::Receiver<()>,
    deadline: Instant,
) -> Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("sustained 30-second progress gate".into());
    }
    outgoing
        .send(())
        .map_err(|_| "sustained peer disconnected during handoff")?;
    incoming
        .recv_timeout(remaining)
        .map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => "sustained 30-second progress gate",
            std::sync::mpsc::RecvTimeoutError::Disconnected => {
                "sustained peer disconnected during handoff"
            }
        })?;
    Ok(())
}
fn wait(path: &Path, exists: bool) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(120);
    while path.exists() != exists {
        if Instant::now() >= end {
            return Err("barrier timeout".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
fn barrier(action: &str) -> (PathBuf, PathBuf) {
    (
        PathBuf::from(format!("/tmp/layerfs-{action}-ready")),
        PathBuf::from(format!("/tmp/layerfs-{action}-release")),
    )
}
fn write_tag(
    case: &super::workspace_common::Case,
    path: &str,
    tag: &str,
    len: u64,
    exclusive: bool,
) -> Result<u64> {
    file(path, &family::data(case, tag, len)?, exclusive)
}
fn emit(receipt: Receipt) {
    for (k, v) in receipt {
        println!("{k}={v}");
    }
}
fn operations(case: &super::workspace_common::Case, action: &str, ordinal: u64) -> Result<Receipt> {
    let started = Instant::now();
    let mut writes = 0;
    let mut operations = 0;
    let mut final_ordinal = ordinal;
    match action {
        "prior" => {
            writes += write_tag(case, "work/a/prior.dat", "prior", 4096, true)?;
            operations = 1;
        }
        "large-dirty" => {
            for i in 0..17 {
                let path = format!(
                    "work/{}/dirty-{i:02}.dat",
                    if i % 2 == 0 { "a" } else { "b" }
                );
                writes += write_tag(
                    case,
                    &path,
                    &format!("dirty-{i}"),
                    if i < 15 { 1048576 } else { 65536 },
                    true,
                )?;
                operations += 1;
            }
        }
        "candidate-dirty" => {
            writes += write_tag(case, "work/a/one.dat", "one", 4096, true)?;
            writes += write_tag(case, "work/b/two.dat", "two", 4096, true)?;
            std::os::unix::fs::symlink("../a/one.dat", "work/b/link")?;
            fs::rename("work/dir", "dest/dir")?;
            operations = 4;
        }
        "published-dirty" => {
            writes += write_tag(case, "work/a/published.dat", "published", 4096, true)?;
            operations = 1;
        }
        "write-A" | "write-B" => {
            writes += write_tag(
                case,
                "work/a/result.dat",
                &action[6..],
                4096,
                action == "write-A",
            )?;
            operations = 1;
        }
        "netzero" => {
            let mut f = OpenOptions::new()
                .read(true)
                .write(true)
                .open("sentinels/f0002.dat")?;
            let mut saved = [0u8; 64];
            f.read_exact(&mut saved)?;
            f.seek(SeekFrom::Start(0))?;
            f.write_all(&[0x5a; 64])?;
            f.sync_all()?;
            checkpoint(case, "net-overwrite")?;
            f.seek(SeekFrom::End(0))?;
            f.write_all(&[0x33; 64])?;
            f.sync_all()?;
            checkpoint(case, "net-append")?;
            f.set_len(32768)?;
            f.sync_all()?;
            checkpoint(case, "net-truncate")?;
            write_tag(case, "work/a/temp.dat", "temp", 64, true)?;
            checkpoint(case, "net-temp")?;
            fs::rename("work/a/temp.dat", "work/b/temp.dat")?;
            checkpoint(case, "net-moved")?;
            fs::remove_file("work/b/temp.dat")?;
            f.seek(SeekFrom::Start(0))?;
            f.write_all(&saved)?;
            f.sync_all()?;
            operations = 7;
            writes = 192;
        }
        "prepare-failure" => {
            writes += write_tag(case, "work/a/prior.dat", "prior", 4096, true)?;
            File::create("work/b/fail.dat")?.sync_all()?;
            operations = 2;
        }
        "fail-write" => {
            let error = (|| -> Result<()> {
                let mut file = OpenOptions::new().write(true).open("work/b/fail.dat")?;
                if let Err(error) = family::data(case, "failure", 4096)?.write_to(&mut file) {
                    println!("error_boundary=write\nwrite_acknowledged_bytes=0");
                    return Err(error);
                }
                println!("write_acknowledged_bytes=4096");
                if let Err(error) = file.sync_all() {
                    println!("error_boundary=fsync");
                    return Err(error.into());
                }
                Ok(())
            })();
            let expected_errno = if case.kind == "short-spool-write" {
                5
            } else {
                28
            };
            let Err(error) = error else {
                return Err("faulted write unexpectedly succeeded".into());
            };
            let io = error
                .downcast_ref::<std::io::Error>()
                .ok_or("write error not native I/O")?;
            if io.raw_os_error() != Some(expected_errno) {
                return Err(format!("faulted write errno {:?}", io.raw_os_error()).into());
            }
            println!("observed_errno={expected_errno}");
            operations = 1;
        }
        "normalize" => {
            operations = 0;
        }
        "invalid-namespace" => {
            for (from, to, errno) in [
                ("work/dir", "work/dir/child/nested", 22),
                ("work/a/prior.dat", "work/b", 21),
            ] {
                let error =
                    fs::rename(from, to).expect_err("invalid namespace unexpectedly succeeded");
                if error.raw_os_error() != Some(errno) {
                    return Err(
                        format!("invalid namespace errno {:?}", error.raw_os_error()).into(),
                    );
                }
                let preserved =
                    common::verify_native(Path::new("."), &family::expected(case, "prior", 0)?)?;
                println!("rejection_errno_{errno}_preserved={preserved:?}");
            }
            operations = 2;
        }
        "read-corrupt" => {
            let mut data = Vec::new();
            let result =
                File::open("sentinels/f0003.dat").and_then(|mut file| file.read_to_end(&mut data));
            match result {
                Err(error) if error.raw_os_error() == Some(5) && data.is_empty() => {
                    println!("observed_errno=5")
                }
                other => return Err(format!("integrity read expected EIO: {other:?}").into()),
            }
            operations = 1;
        }
        "hold-writer" => {
            let (ready, release) = barrier(action);
            let _ = fs::remove_file(&ready);
            let _ = fs::remove_file(&release);
            let mut f = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open("work/a/writer.dat")?;
            family::data(case, "writer", 4096)?.write_to(&mut f)?;
            f.sync_all()?;
            normalize(&family::expected(case, "done", 1)?, false)?;
            File::create(&ready)?;
            wait(&release, true)?;
            drop(f);
            let _ = fs::remove_file(&ready);
            let _ = fs::remove_file(&release);
            writes = 4096;
            operations = 1;
        }
        "hold-execution" | "hold-cancel" | "hold-disconnect" => {
            let (ready, release) = barrier(action);
            let _ = fs::remove_file(&ready);
            let _ = fs::remove_file(&release);
            writes += write_tag(case, "work/prefix.dat", "prefix", 4096, true)?;
            normalize(&family::expected(case, "done", 1)?, false)?;
            if action == "hold-cancel" || action == "hold-disconnect" {
                let child = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg("while :; do :; done")
                    .spawn()?;
                fs::write(
                    format!("/tmp/layerfs-{action}-child"),
                    child.id().to_string(),
                )?;
                std::mem::forget(child);
            }
            File::create(&ready)?;
            wait(&release, true)?;
            operations = 1;
        }
        "parallel" => {
            for w in 0..4 {
                fs::create_dir(format!("work/w{w}"))?;
            }
            let gate = Arc::new(Barrier::new(4));
            std::thread::scope(|scope| -> Result<()> {
                let mut joins = Vec::new();
                for w in 0..4 {
                    let gate = gate.clone();
                    joins.push(scope.spawn(move || -> std::result::Result<u64, String> {
                        (|| -> Result<u64> {
                            let mut total = 0;
                            for cycle in 0..16 {
                                let mut input = [0u8; 64];
                                File::open(format!("sentinels/f{w:04}.dat"))?
                                    .read_exact(&mut input)?;
                                total += write_tag(
                                    case,
                                    &format!("work/w{w}/cycle.dat"),
                                    &format!("worker-{w}-cycle-{cycle}"),
                                    4096,
                                    true,
                                )?;
                                gate.wait();
                                let source = (w + 3) % 4;
                                let temp = format!("work/w{source}/cycle.dat");
                                let actual = fs::read(&temp)?;
                                let mut expected = Vec::new();
                                family::data(
                                    case,
                                    &format!("worker-{source}-cycle-{cycle}"),
                                    4096,
                                )?
                                .write_to(&mut expected)?;
                                if actual != expected {
                                    return Err("parallel handoff bytes".into());
                                }
                                fs::rename(&temp, format!("work/w{source}/final.dat"))?;
                                gate.wait();
                            }
                            Ok(total)
                        })()
                        .map_err(|e| e.to_string())
                    }));
                }
                for j in joins {
                    writes += j.join().map_err(|_| "parallel worker panic")??;
                }
                Ok(())
            })?;
            operations = 64;
        }
        "contention" => {
            use std::sync::atomic::{AtomicBool, Ordering};
            let gate = Arc::new(Barrier::new(2));
            let stop = AtomicBool::new(false);
            let mut allowed = Vec::new();
            for w in 0..2 {
                for generation in 0..16 {
                    let mut bytes = Vec::new();
                    family::data(case, &format!("writer-{w}-generation-{generation}"), 4096)?
                        .write_to(&mut bytes)?;
                    allowed.push(bytes);
                }
            }
            let winners = std::thread::scope(|scope| -> Result<Vec<bool>> {
                let reader = scope.spawn(|| -> std::result::Result<u64, String> {
                    (|| -> Result<u64> {
                        let mut count = 0;
                        while !stop.load(Ordering::Acquire) {
                            match File::open("work/shared.dat") {
                                Ok(mut file) => {
                                    let mut bytes = Vec::new();
                                    file.read_to_end(&mut bytes)?;
                                    if !allowed.contains(&bytes) {
                                        return Err("torn atomic replacement generation".into());
                                    }
                                    count += 1;
                                }
                                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                                Err(e) => return Err(e.into()),
                            }
                        }
                        Ok(count)
                    })()
                    .map_err(|e| e.to_string())
                });
                let mut joins = Vec::new();
                for w in 0..2 {
                    let gate = gate.clone();
                    joins.push(scope.spawn(move || -> std::result::Result<bool, String> {
                        (|| -> Result<bool> {
                            gate.wait();
                            let won = match OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open("work/claim.dat")
                            {
                                Ok(_) => true,
                                Err(e) if e.raw_os_error() == Some(17) => false,
                                Err(e) => return Err(e.into()),
                            };
                            for generation in 0..16 {
                                let temp = format!("work/.shared-{w}-{generation}");
                                write_tag(
                                    case,
                                    &temp,
                                    &format!("writer-{w}-generation-{generation}"),
                                    4096,
                                    true,
                                )?;
                                fs::rename(temp, "work/shared.dat")?;
                            }
                            Ok(won)
                        })()
                        .map_err(|e| e.to_string())
                    }));
                }
                let results = joins
                    .into_iter()
                    .map(|j| j.join().map_err(|_| "contention writer panic")?)
                    .collect::<std::result::Result<Vec<_>, String>>();
                stop.store(true, Ordering::Release);
                let reads = reader.join().map_err(|_| "contention reader panic")??;
                if reads == 0 {
                    return Err("contention reader observed no published generation".into());
                }
                println!("allowed_generation_read_count={reads}");
                Ok(results?)
            })?;
            if winners.iter().filter(|&&w| w).count() != 1 {
                return Err("exclusive creation outcome".into());
            }
            writes += write_tag(case, "work/shared.dat", "final", 4096, false)?;
            operations = 34;
        }
        "hardlink" => {
            fs::hard_link("sentinels/f0000.dat", "work/a/alias")?;
            checkpoint(case, "alias-created")?;
            let mut f = OpenOptions::new().write(true).open("work/a/alias")?;
            family::data(case, "alias-change", 64)?.write_to(&mut f)?;
            f.sync_all()?;
            drop(f);
            checkpoint(case, "alias-written")?;
            fs::rename("work/a", "dest/a")?;
            checkpoint(case, "alias-moved")?;
            fs::remove_file("links/alias.dat")?;
            checkpoint(case, "alias-unlinked")?;
            let temp = "dest/a/new";
            write_tag(case, temp, "replacement", 4096, true)?;
            fs::rename(temp, "dest/a/alias")?;
            operations = 5;
            writes = 4160;
        }
        "symlink" => {
            if fs::read_link("links/relative")?.as_os_str() != "../sentinels/f0001.dat" {
                return Err("relative readlink".into());
            }
            fs::rename("links", "work/links")?;
            checkpoint(case, "links-moved")?;
            match File::open("work/links/relative") {
                Err(e) if e.raw_os_error() == Some(2) => (),
                other => return Err(format!("moved symlink expected ENOENT: {other:?}").into()),
            }
            fs::rename("work/links", "links")?;
            if fs::read("links/relative")? != fs::read("sentinels/f0001.dat")? {
                return Err("restored relative symlink bytes".into());
            }
            let e = File::open("links/dangling").unwrap_err();
            if e.raw_os_error() != Some(2) {
                return Err("dangling errno".into());
            }
            let e = File::open("links/cycle-a").unwrap_err();
            if e.raw_os_error() != Some(40) {
                return Err("cycle errno".into());
            }
            writes += write_tag(case, "work/marker", "marker", 1, true)?;
            operations = 7;
        }
        "open-handles" => {
            writes += write_tag(case, "work/a/held.dat", "held", 4096, true)?;
            let mut held = OpenOptions::new()
                .read(true)
                .write(true)
                .open("work/a/held.dat")?;
            fs::rename("work/a/held.dat", "work/a/moved.dat")?;
            fs::remove_file("work/a/moved.dat")?;
            if Path::new("work/a/held.dat").exists() || Path::new("work/a/moved.dat").exists() {
                return Err("open unlinked name still exists".into());
            }
            let mut expected = Vec::new();
            family::data(case, "held", 4096)?.write_to(&mut expected)?;
            let read_bytes = require_handle_bytes(&mut held, &expected)?;
            println!("open_unlinked_before_write_bytes={read_bytes}");
            expected[0] ^= 1;
            held.seek(SeekFrom::Start(0))?;
            held.write_all(&expected[..1])?;
            writes += 1;
            held.sync_all()?;
            let read_bytes = require_handle_bytes(&mut held, &expected)?;
            println!("open_unlinked_after_write_bytes={read_bytes}");
            drop(held);
            writes += write_tag(case, "work/a/target.dat", "old", 4096, true)?;
            let mut old = File::open("work/a/target.dat")?;
            writes += write_tag(case, "work/a/new.dat", "new", 4096, true)?;
            fs::rename("work/a/new.dat", "work/a/target.dat")?;
            let mut check = Vec::new();
            old.read_to_end(&mut check)?;
            let mut expected = Vec::new();
            family::data(case, "old", 4096)?.write_to(&mut expected)?;
            if check != expected {
                return Err("old descriptor did not retain replaced inode bytes".into());
            }
            let mut expected = Vec::new();
            family::data(case, "new", 4096)?.write_to(&mut expected)?;
            if fs::read("work/a/target.dat")? != expected {
                return Err("new open did not observe replacement inode".into());
            }
            operations = 8;
        }
        "chmod" => {
            fs::set_permissions(
                "sentinels/f0000.dat",
                std::os::unix::fs::PermissionsExt::from_mode(0o600),
            )?;
            require_modes(&["sentinels/f0000.dat", "links/alias.dat"], 0o600)?;
            fs::set_permissions(
                "sentinels/f0000.dat",
                std::os::unix::fs::PermissionsExt::from_mode(0o640),
            )?;
            require_modes(&["sentinels/f0000.dat", "links/alias.dat"], 0o640)?;
            fs::set_permissions(
                "sentinels/f0001.dat",
                std::os::unix::fs::PermissionsExt::from_mode(0o600),
            )?;
            require_modes(&["sentinels/f0001.dat"], 0o600)?;
            fs::set_permissions(
                "work/a",
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            )?;
            require_modes(&["work/a"], 0o700)?;
            operations = 4;
        }
        "mtime" => {
            common::set_mtime_nofollow(Path::new("sentinels/f0000.dat"), 1700000013, 123456789)?;
            require_mtimes(
                &["sentinels/f0000.dat", "links/alias.dat"],
                1700000013,
                123456789,
            )?;
            common::set_mtime_nofollow(Path::new("work/a"), 1700000013, 123456789)?;
            require_mtimes(&["work/a"], 1700000013, 123456789)?;
            operations = 2;
        }
        "xattr" => {
            #[cfg(target_os = "linux")]
            unsafe {
                unsafe extern "C" {
                    fn setxattr(
                        path: *const std::ffi::c_char,
                        name: *const std::ffi::c_char,
                        value: *const std::ffi::c_void,
                        size: usize,
                        flags: i32,
                    ) -> i32;
                }
                let p = std::ffi::CString::new("sentinels/f0000.dat")?;
                let n = std::ffi::CString::new("user.layerfs-v013")?;
                let v = b"mixed-proof";
                if setxattr(p.as_ptr(), n.as_ptr(), v.as_ptr().cast(), v.len(), 0) == 0 {
                    return Err("xattr unexpectedly supported".into());
                }
                let errno = std::io::Error::last_os_error().raw_os_error();
                if errno != Some(95) {
                    return Err(format!("xattr errno {errno:?}").into());
                }
            }
            println!("observed_errno=95");
            operations = 1;
        }
        "exec-one" => {
            let mut input = [0u8; 64];
            File::open("sentinels/f0001.dat")?.read_exact(&mut input)?;
            writes += write_tag(
                case,
                "work/result.dat",
                &format!("exec-{ordinal}"),
                4096,
                false,
            )?;
            println!("{ordinal}");
            operations = 1;
        }
        "stage" => {
            for p in ["work/a/one", "work/b/two"] {
                writes += write_tag(case, p, &format!("stage-{ordinal}"), 4096, false)?;
            }
            operations = 2;
        }
        "sustained" => {
            use std::sync::atomic::{AtomicBool, Ordering};
            let start = std::sync::OnceLock::<Instant>::new();
            let (send_zero, receive_zero) = std::sync::mpsc::channel();
            let (send_one, receive_one) = std::sync::mpsc::channel();
            let peers = [(send_one, receive_zero), (send_zero, receive_one)];
            let stop = AtomicBool::new(false);
            let cycles = std::thread::scope(|scope| -> Result<u64> {
                let mut joins = Vec::new();
                for (w, (outgoing, incoming)) in peers.into_iter().enumerate() {
                    let stop = &stop;
                    let start = &start;
                    joins.push(scope.spawn(move || -> std::result::Result<u64, String> {
                        (|| -> Result<u64> {
                            let mut cycles = 0;
                            let mut last_report = Instant::now();
                            loop {
                                let cycle_start = Instant::now();
                                let cycle_deadline = cycle_start + Duration::from_secs(30);
                                let mut immutable = [0u8; 64];
                                let active_start = *start.get_or_init(Instant::now);
                                File::open("sentinels/f0001.dat")?.read_exact(&mut immutable)?;
                                let temp = format!("work/.active{w}.tmp");
                                write_tag(
                                    case,
                                    &temp,
                                    &format!("active-{w}-{cycles}"),
                                    4096,
                                    true,
                                )?;
                                fs::rename(&temp, format!("work/active{w}"))?;
                                sustained_handoff(&outgoing, &incoming, cycle_deadline)?;
                                let peer = 1 - w;
                                let actual = fs::read(format!("work/active{peer}"))?;
                                let mut expected = Vec::new();
                                family::data(case, &format!("active-{peer}-{cycles}"), 4096)?
                                    .write_to(&mut expected)?;
                                if actual != expected {
                                    return Err("sustained handoff content".into());
                                }
                                let scratch = format!("work/.scratch{w}");
                                write_tag(case, &scratch, "scratch", 64, true)?;
                                fs::remove_file(scratch)?;
                                sustained_handoff(&outgoing, &incoming, cycle_deadline)?;
                                cycles += 1;
                                if w == 0 {
                                    let mut result =
                                        format!("completed-cycles={cycles}\n").into_bytes();
                                    result.resize(64, 0);
                                    file("work/cycle-result", &Content::Literal(result), false)?;
                                    stop.store(
                                        active_start.elapsed() >= Duration::from_secs(600),
                                        Ordering::Release,
                                    );
                                    if last_report.elapsed() >= Duration::from_secs(1) {
                                        println!("progress_cycles={cycles}");
                                        last_report = Instant::now();
                                    }
                                }
                                sustained_handoff(&outgoing, &incoming, cycle_deadline)?;
                                if cycle_start.elapsed() >= Duration::from_secs(30) {
                                    return Err("sustained 30-second progress gate".into());
                                }
                                if stop.load(Ordering::Acquire) {
                                    break;
                                }
                            }
                            Ok(cycles)
                        })()
                        .map_err(|error| {
                            eprintln!("sustained_worker={w} failure={error}");
                            error.to_string()
                        })
                    }));
                }
                let mut counts = Vec::new();
                for j in joins {
                    counts.push(j.join().map_err(|_| "sustained worker panic")??);
                }
                if counts[0] != counts[1] || counts[0] == 0 {
                    return Err("sustained cycle accounting".into());
                }
                Ok(counts[0])
            })?;
            println!(
                "completed_cycles={cycles}\nactive_elapsed_ns={}",
                start
                    .get()
                    .ok_or("sustained activity never started")?
                    .elapsed()
                    .as_nanos()
            );
            final_ordinal = cycles;
            writes = cycles * (8320 + 64);
            operations = cycles * 9;
        }
        _ => return Err("unknown reliability workload action".into()),
    }
    if !matches!(
        action,
        "hold-writer"
            | "hold-execution"
            | "hold-cancel"
            | "hold-disconnect"
            | "xattr"
            | "mtime"
            | "read-corrupt"
            | "fail-write"
    ) {
        let state = if action == "prior" {
            "prior"
        } else if action == "write-A" {
            "published"
        } else {
            "done"
        };
        let expected = family::expected(case, state, final_ordinal)?;
        normalize(&expected, action == "chmod")?;
        if matches!(case.kind, "hardlink-alias" | "dirty-net-zero") {
            let target = if case.kind == "hardlink-alias" {
                "sentinels/f0000.dat"
            } else {
                "sentinels/f0002.dat"
            };
            let e = expected
                .iter()
                .find(|e| e.path == target)
                .ok_or("metadata restore target")?;
            common::set_metadata(Path::new(target), e)?;
        }
    }
    if action == "exec-one" {
        return Ok(Receipt::new());
    }
    let mut r = Receipt::new();
    for (k, v) in [
        ("inner_workload_ns", started.elapsed().as_nanos()),
        ("attempted_operations", operations as u128),
        (
            "completed_operations",
            if matches!(
                action,
                "fail-write" | "read-corrupt" | "xattr" | "invalid-namespace"
            ) {
                0
            } else {
                operations as u128
            },
        ),
        ("successful_write_bytes", writes as u128),
    ] {
        r.insert(k.into(), v.to_string());
    }
    Ok(r)
}
pub(crate) fn dispatch(args: &[String]) -> Result<()> {
    let [id, action, ordinal] = args else {
        return Err("workspace reliability workload: ID ACTION ORDINAL".into());
    };
    let case = family::resolve(id)?;
    let receipt = operations(&case, action, ordinal.parse()?)?;
    emit(receipt);
    Ok(())
}

#[cfg(test)]
#[test]
fn sustained_handoff_detects_peer_exit_and_progress_timeout() {
    let (send_zero, receive_zero) = std::sync::mpsc::channel();
    let (send_one, receive_one) = std::sync::mpsc::channel();
    let deadline = Instant::now() + Duration::from_secs(30);
    let peer = std::thread::spawn(move || {
        sustained_handoff(&send_zero, &receive_one, deadline).unwrap();
        // A worker failing between handoffs drops both endpoints.
    });
    sustained_handoff(&send_one, &receive_zero, deadline).unwrap();
    let error = sustained_handoff(&send_one, &receive_zero, deadline).unwrap_err();
    assert_eq!(
        error.to_string(),
        "sustained peer disconnected during handoff"
    );
    peer.join().unwrap();

    let (outgoing, _peer_receiver) = std::sync::mpsc::channel();
    let (_silent_peer, incoming) = std::sync::mpsc::channel();
    let error = sustained_handoff(
        &outgoing,
        &incoming,
        Instant::now() + Duration::from_millis(20),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "sustained 30-second progress gate");
    assert!(sustained_handoff(&outgoing, &incoming, Instant::now()).is_err());
}

#[cfg(test)]
#[test]
fn open_handle_oracle_rejects_wrong_or_extra_bytes() {
    use std::io::Cursor;
    assert_eq!(
        require_handle_bytes(&mut Cursor::new(b"abcd"), b"abcd").unwrap(),
        4
    );
    assert!(require_handle_bytes(&mut Cursor::new(b"abcd"), b"abxd").is_err());
    assert!(require_handle_bytes(&mut Cursor::new(b"abcde"), b"abcd").is_err());
    assert!(require_handle_bytes(&mut Cursor::new(b"abc"), b"abcd").is_err());
}
