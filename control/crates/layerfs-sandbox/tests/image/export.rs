//! Real-Docker proof for exclusive per-export containers, deterministic tar
//! exports (>=2 s apart for resume), anonymous volume cleanup on `VOLUME`
//! images, and home-scoped owned-container recovery under `ImageLock`
//! (#176 I02).
use crate::fixture;
use fixture::{pax_record, Scratch, Tar};
use layerfs_home::disk;
use layerfs_home::reply::events::Events;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::image::cache::{self as cache, CHUNK};
use layerfs_sandbox::image::copy::docker::Docker;
use layerfs_sandbox::image::copy::load::{
    cleanup_owned, load, owned_containers, Cleanup, ImageLock, Loaded, Target, EXPORT_ROLE,
};
use layerfs_sandbox::image::tar::Filtered;
use layerfs_sandbox::{rootfs, Image};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn container_exists(name: &str) -> bool {
    Command::new("docker")
        .args(["container", "inspect", "--format", "{{.Id}}", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn volume_exists(name: &str) -> bool {
    Command::new("docker")
        .args(["volume", "inspect", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn container_volumes(container: &str) -> Vec<String> {
    let Ok(out) = Command::new("docker")
        .args([
            "container",
            "inspect",
            "--format",
            "{{range .Mounts}}{{if eq .Type \"volume\"}}{{.Name}}{{\"\\n\"}}{{end}}{{end}}",
            container,
        ])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

fn containers_for_home(home_id: &str) -> Vec<String> {
    let Ok(out) = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label=layerfs.home={home_id}"),
            "--format",
            "{{.Names}}",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Builds a tiny unique Linux image with `dev/`, `etc/`, a >1 MiB blob (for
/// resume across `CHUNK`) and optionally `VOLUME /data/vol`. Tracks registered
/// homes so Drop removes any test-owned containers, anonymous volumes and the
/// test image even if a test panics.
struct TestImage {
    base_tag: String,
    tag: String,
    id: String,
    scratch: Scratch,
    homes: Vec<String>,
}

impl TestImage {
    fn build(name: &str, with_volume: bool) -> Self {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let scratch = Scratch::new(&format!("lp176-exp-{name}-{seq}"));
        let base_tag = format!("layerfs-lp176-base-{pid}-{seq}:test");
        let tag = format!("layerfs-lp176-img-{pid}-{seq}:test");
        let tar = Tar::new()
            .at(1_700_000_000)
            .directory("dev/", 0o755)
            .directory("etc/", 0o755)
            .file(
                "etc/app.conf",
                0o644,
                format!("mode=test-{pid}-{seq}\n").as_bytes(),
            )
            .directory("usr/", 0o755)
            .directory("usr/bin/", 0o755)
            .file("usr/bin/blob", 0o755, &vec![0x5a; CHUNK + 131_072])
            .finish();
        let mut child = Command::new("docker")
            .args(["import", "-", &base_tag])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn docker import");
        child.stdin.take().unwrap().write_all(&tar).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "docker import failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let final_tag = if with_volume {
            let build_dir = scratch.base.join("build");
            fs::create_dir_all(&build_dir).unwrap();
            fs::write(
                build_dir.join("Dockerfile"),
                format!("FROM {base_tag}\nVOLUME /data/vol\n"),
            )
            .unwrap();
            let out = Command::new("docker")
                .args([
                    "build",
                    "--pull=false",
                    "-q",
                    "-t",
                    &tag,
                    build_dir.to_str().unwrap(),
                ])
                .output()
                .expect("docker build");
            assert!(
                out.status.success(),
                "docker build failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            tag
        } else {
            base_tag.clone()
        };
        let inspect = Command::new("docker")
            .args(["image", "inspect", "--format", "{{.Id}}", &final_tag])
            .output()
            .expect("docker image inspect");
        assert!(inspect.status.success());
        let id = String::from_utf8_lossy(&inspect.stdout).trim().to_owned();
        Self {
            base_tag,
            tag: final_tag,
            id,
            scratch,
            homes: Vec::new(),
        }
    }

    fn hex(&self) -> &str {
        self.id.strip_prefix("sha256:").expect("sha256 image id")
    }

    fn make_home(&mut self, name: &str) -> (PathBuf, String) {
        let home = self.scratch.base.join(name);
        fs::create_dir_all(&home).unwrap();
        let id = disk::ensure(&home).unwrap().id;
        self.homes.push(id.clone());
        (home, id)
    }
}

impl Drop for TestImage {
    fn drop(&mut self) {
        let mut cnames = BTreeSet::new();
        for home_id in &self.homes {
            cnames.extend(containers_for_home(home_id));
        }
        if let Ok(out) = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("ancestor={}", self.id),
                "--format",
                "{{.Names}}",
            ])
            .output()
        {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let line = line.trim();
                    if line.starts_with("layerfs-export-") || line.starts_with("lp176-exp-") {
                        cnames.insert(line.to_owned());
                    }
                }
            }
        }
        for cname in cnames {
            let vols = container_volumes(&cname);
            let _ = Command::new("docker")
                .args(["rm", "-f", "-v", &cname])
                .output();
            for v in vols {
                let _ = Command::new("docker").args(["volume", "rm", &v]).output();
            }
        }
        if self.tag != self.base_tag {
            let _ = Command::new("docker")
                .args(["rmi", "-f", &self.tag])
                .output();
        }
        let _ = Command::new("docker")
            .args(["rmi", "-f", &self.base_tag])
            .output();
    }
}

fn real_docker() -> Docker {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let program = std::env::split_paths(&path)
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.join("docker"))
        .find(|p| p.is_file());
    Docker::at(program, None)
}

fn filtered_export(image_id: &str, cname: &str) -> (Vec<u8>, String) {
    let out = Command::new("docker")
        .args([
            "create",
            "--name",
            cname,
            "--network",
            "none",
            "--entrypoint",
            "/layerfs-export",
            image_id,
        ])
        .output()
        .expect("docker create");
    assert!(
        out.status.success(),
        "docker create {cname} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut child = Command::new("docker")
        .args(["export", cname])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("docker export");
    let mut filtered = Vec::new();
    Filtered::new(child.stdout.take().unwrap())
        .read_to_end(&mut filtered)
        .unwrap();
    let status = child.wait_with_output().unwrap();
    let _ = Command::new("docker")
        .args(["rm", "-f", "-v", cname])
        .output();
    assert!(status.status.success());
    let sha = layerfs_protocol::sdk::hex(&Sha256::digest(&filtered));
    (filtered, sha)
}

fn run_load(
    docker: &Docker,
    home: &Path,
    machine: &str,
    image: &str,
) -> (
    Result<Loaded, layerfs_protocol::sdk::SdkRefusal>,
    Vec<SdkValue>,
) {
    let mut phases = Vec::new();
    let mut sink = |event: SdkValue| {
        if let Some(p) = event.get("progress") {
            phases.push(p.clone());
        }
    };
    let mut events = Events::new(&mut sink);
    let out = load(
        docker,
        &mut events,
        &mut Target::local(home, machine),
        None,
        image,
    );
    (out, phases)
}

#[test]
fn pax_and_directory_mtimes_are_normalized_with_valid_checksums() {
    let mut pax_dir = pax_record("path", "var/cache/dir/");
    pax_dir.extend_from_slice(&pax_record("mtime", "1700000123.456789"));
    pax_dir.extend_from_slice(&pax_record("atime", "1700000123.111"));
    let pax_only_mtime = pax_record("mtime", "1700000999.5");
    let pax_file = pax_record("mtime", "1700000555.25");

    // the init layer's directories (`dev/`, `etc/`, `proc/`) differ between
    // exports; the image's own (`var/`, `var/cache/dir/`) keep their times
    let build = |dir_sec: u64| {
        Tar::new()
            .at(dir_sec)
            .directory("dev/", 0o755)
            .directory("etc/", 0o755)
            .raw("PaxHeaders/dir2", b'x', 0o644, &pax_only_mtime)
            .directory("proc/", 0o755)
            .at(1_650_000_000)
            .directory("var/", 0o755)
            .raw("PaxHeaders/dir1", b'x', 0o644, &pax_dir)
            .directory("placeholder-dir", 0o755)
            .raw("PaxHeaders/file1", b'x', 0o644, &pax_file)
            .file("etc/app.conf", 0o644, b"hello\n")
            .symlink("etc/link", "/etc/app.conf")
            .finish()
    };

    let mut out1 = Vec::new();
    Filtered::new(build(1_700_000_010).as_slice())
        .read_to_end(&mut out1)
        .unwrap();
    let mut out2 = Vec::new();
    Filtered::new(build(1_700_000_025).as_slice())
        .read_to_end(&mut out2)
        .unwrap();
    assert_eq!(out1, out2);

    // Unpack with the real rootfs tar reader to verify every recomputed header
    // checksum and directory/file mtime.
    let scratch = Scratch::new("lp176-tar-norm");
    let tar_path = scratch.base.join("flat.tar");
    fs::write(&tar_path, &out1).unwrap();
    let id = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (root, unpacked) =
        rootfs::ensure_flat(&tar_path, &Image::parse(id).unwrap(), &scratch.cache).unwrap();
    assert!(unpacked);
    assert_eq!(
        fs::read(root.directory().join("etc/app.conf")).unwrap(),
        b"hello\n"
    );
    assert!(root.directory().join("var/cache/dir").is_dir());
    let file_mtime = fs::symlink_metadata(root.directory().join("etc/app.conf"))
        .unwrap()
        .modified()
        .unwrap();
    let dir_mtime = fs::symlink_metadata(root.directory().join("etc"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        file_mtime
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        1_700_000_555
    );
    assert_eq!(
        dir_mtime
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        0
    );
    let secs = |path: &str| {
        fs::symlink_metadata(root.directory().join(path))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    };
    assert_eq!(secs("var"), 1_650_000_000, "the image's own directory");
    assert_eq!(
        secs("var/cache/dir"),
        1_700_000_123,
        "its pax mtime is kept"
    );
}

#[test]
fn real_docker_two_exports_over_2s_apart_match_sha_and_resume() {
    if !docker_available() {
        eprintln!("NOT_RUN: docker unavailable");
        return;
    }
    let mut img = TestImage::build("resume", false);
    let (bytes1, sha1) =
        filtered_export(&img.id, &format!("lp176-exp-probe1-{}", std::process::id()));
    assert!(bytes1.len() > CHUNK);
    std::thread::sleep(Duration::from_millis(2200));
    let (bytes2, sha2) =
        filtered_export(&img.id, &format!("lp176-exp-probe2-{}", std::process::id()));
    assert_eq!(
        sha1, sha2,
        "two exports >= 2s apart must yield identical filtered tar SHA-256"
    );
    assert_eq!(bytes1, bytes2);

    // Stage the first CHUNK from export 1 into a fresh home, wait >=2s, and
    // verify `load()` resumes from offset CHUNK rather than restarting at 0.
    let (home, _) = img.make_home("home-resume");
    cache::write(&cache::cache(&home), img.hex(), 0, &bytes1[..CHUNK]).unwrap();
    std::thread::sleep(Duration::from_millis(2200));

    let docker = real_docker();
    let (loaded, phases) = run_load(&docker, &home, layerfs_home::names::HOST_MACHINE, &img.tag);
    let loaded = loaded.unwrap();
    assert!(!loaded.reused);
    assert_eq!(loaded.size, Some(bytes1.len() as u64));
    let sends: Vec<_> = phases
        .iter()
        .filter(|p| p.get_text("phase") == Some("send"))
        .collect();
    let first_done = sends[0].get_int("done").unwrap();
    assert_eq!(
        first_done, CHUNK as i64,
        "resume must start sending from staged CHUNK={CHUNK}, got phases={sends:?}"
    );
    assert!(owned_containers(&docker, &home).unwrap().is_empty());
}

#[test]
fn real_docker_volume_image_leaves_no_anonymous_volumes() {
    if !docker_available() {
        eprintln!("NOT_RUN: docker unavailable");
        return;
    }
    let mut img = TestImage::build("volume", true);
    let (home, home_id) = img.make_home("home-vol");

    // Pause a real `load()` mid-send to inspect the exact anonymous volume ID
    // created for the container, then let `load()` finish normally and assert
    // both container and anonymous volume are gone.
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_load_with_pause", "--nocapture"])
        .env("LP176_PAUSE_HOME", &home)
        .env("LP176_PAUSE_MACHINE", layerfs_home::names::HOST_MACHINE)
        .env("LP176_PAUSE_IMAGE", &img.tag)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    wait_for("child load to pause in send", || {
        home.join("paused").exists()
    });
    let docker = real_docker();
    let owned = owned_containers(&docker, &home).unwrap();
    assert_eq!(owned.len(), 1, "expected 1 live owned container: {owned:?}");
    let cname = &owned[0].name;
    assert!(cname.starts_with(&format!("layerfs-export-{}-native-", &home_id[..8])));
    let vols = container_volumes(cname);
    assert_eq!(
        vols.len(),
        1,
        "VOLUME /data/vol must create 1 anonymous volume on {cname}"
    );
    let vol_id = vols[0].clone();
    assert!(volume_exists(&vol_id));

    fs::write(home.join("proceed"), b"").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "paused load child failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !container_exists(cname),
        "completed load left container {cname}"
    );
    assert!(
        !volume_exists(&vol_id),
        "completed load on VOLUME image leaked anonymous volume {vol_id}"
    );
    assert!(owned_containers(&docker, &home).unwrap().is_empty());
}

#[test]
fn real_docker_error_and_cancel_clean_container_without_leaks() {
    if !docker_available() {
        eprintln!("NOT_RUN: docker unavailable");
        return;
    }
    let mut img = TestImage::build("err-cancel", true);
    let (home, _) = img.make_home("home-err");
    let docker = real_docker();

    // Case A: error during `send` (`.recv-<hex>.part` is a directory, so
    // `target.step(write)` fails EISDIR after container creation and export).
    let cache_dir = cache::cache(&home);
    fs::create_dir_all(cache_dir.join(format!(".recv-{}.part", img.hex()))).unwrap();
    let (err, _) = run_load(&docker, &home, "docker", &img.tag);
    assert!(
        err.is_err(),
        "expected write failure when .part is a directory"
    );
    assert!(
        owned_containers(&docker, &home).unwrap().is_empty(),
        "errored load leaked owned container"
    );
    fs::remove_dir_all(cache_dir.join(format!(".recv-{}.part", img.hex()))).unwrap();

    // Case B: simulated cancellation via unwind inside the progress sink after
    // the export container has been created and `send` has begun.
    let (home_cancel, _) = img.make_home("home-cancel");
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut sink = |event: SdkValue| {
            if let Some(p) = event.get("progress") {
                if p.get_text("phase") == Some("send") {
                    panic!("simulated caller cancel");
                }
            }
        };
        let mut events = Events::new(&mut sink);
        let _ = load(
            &docker,
            &mut events,
            &mut Target::local(&home_cancel, "docker"),
            None,
            &img.tag,
        );
    }));
    assert!(panicked.is_err());
    assert!(
        owned_containers(&docker, &home_cancel).unwrap().is_empty(),
        "cancelled load leaked owned container"
    );
    assert!(
        ImageLock::try_take(&home_cancel, "docker", &img.id)
            .unwrap()
            .is_some(),
        "lock must be released after cancel unwind"
    );
}

#[test]
fn real_docker_killed_load_enumerates_and_recovers_under_lock() {
    if !docker_available() {
        eprintln!("NOT_RUN: docker unavailable");
        return;
    }
    let mut img = TestImage::build("kill", true);
    let (home, home_id) = img.make_home("home-kill");
    let docker = real_docker();

    // 1. Spawn child that creates container, stages >= CHUNK bytes, then pauses.
    let mut child1 = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_load_with_pause", "--nocapture"])
        .env("LP176_PAUSE_HOME", &home)
        .env("LP176_PAUSE_MACHINE", "docker")
        .env("LP176_PAUSE_IMAGE", &img.tag)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("first killed-to-be child pauses in send", || {
        home.join("paused").exists()
    });

    // While child1 is alive and holding ImageLock:
    // - owned_containers enumerates its container;
    // - ImageLock::try_take returns None;
    // - cleanup_owned leaves the active container untouched.
    let owned_live = owned_containers(&docker, &home).unwrap();
    assert_eq!(owned_live.len(), 1);
    let c1 = owned_live[0].clone();
    assert_eq!(c1.home_id, home_id);
    assert_eq!(c1.machine, "docker");
    assert_eq!(c1.image, img.id);
    assert!(!c1.nonce.is_empty());
    assert!(c1.name.starts_with(&format!(
        "layerfs-export-{}-docker-{}",
        &home_id[..8],
        c1.nonce
    )));
    let vols1 = container_volumes(&c1.name);
    assert_eq!(vols1.len(), 1);
    let v1 = vols1[0].clone();
    assert!(volume_exists(&v1));
    assert!(ImageLock::try_take(&home, "docker", &img.id)
        .unwrap()
        .is_none());
    let during_live = cleanup_owned(&docker, &home, None, None).unwrap();
    assert_eq!(
        during_live,
        Cleanup {
            removed: Vec::new(),
            active: vec![c1.name.clone()],
            failed: Vec::new(),
        }
    );
    assert!(container_exists(&c1.name));

    // Kill child1 (no Drop runs).
    child1.kill().unwrap();
    child1.wait().unwrap();

    // After SIGKILL: residual owned container is still enumerable, and
    // cleanup_owned removes both the container and its anonymous volume under I01's lock.
    assert_eq!(owned_containers(&docker, &home).unwrap(), vec![c1.clone()]);
    let cleaned = cleanup_owned(&docker, &home, None, None).unwrap();
    assert_eq!(
        cleaned,
        Cleanup {
            removed: vec![c1.name.clone()],
            active: Vec::new(),
            failed: Vec::new(),
        }
    );
    assert!(!container_exists(&c1.name));
    assert!(!volume_exists(&v1));
    assert!(owned_containers(&docker, &home).unwrap().is_empty());

    // 2. Kill a second child after staging CHUNK bytes; verify the next `load()`
    // recovers under I01's lock: removes the stale owned container + volume,
    // exports with a new nonce container, resumes from CHUNK, and cleans up.
    let _ = fs::remove_file(home.join("paused"));
    let mut child2 = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_load_with_pause", "--nocapture"])
        .env("LP176_PAUSE_HOME", &home)
        .env("LP176_PAUSE_MACHINE", "docker")
        .env("LP176_PAUSE_IMAGE", &img.tag)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("second killed-to-be child pauses in send", || {
        home.join("paused").exists()
    });
    let c2 = owned_containers(&docker, &home).unwrap().remove(0);
    assert_ne!(c2.nonce, c1.nonce);
    let v2 = container_volumes(&c2.name)[0].clone();
    child2.kill().unwrap();
    child2.wait().unwrap();

    std::thread::sleep(Duration::from_millis(2200));
    let (resumed, phases) = run_load(&docker, &home, "docker", &img.tag);
    let resumed = resumed.unwrap();
    assert!(!resumed.reused);
    let sends: Vec<_> = phases
        .iter()
        .filter(|p| p.get_text("phase") == Some("send"))
        .collect();
    assert!(
        sends[0].get_int("done").unwrap() >= CHUNK as i64,
        "resumed load after kill must resume from staged bytes: {sends:?}"
    );
    assert!(!container_exists(&c2.name));
    assert!(!volume_exists(&v2));
    assert!(owned_containers(&docker, &home).unwrap().is_empty());
}

#[test]
fn real_docker_different_homes_do_not_collide_or_delete_each_other() {
    if !docker_available() {
        eprintln!("NOT_RUN: docker unavailable");
        return;
    }
    let mut img = TestImage::build("two-homes", true);
    let (home1, id1) = img.make_home("home-1");
    let (home2, id2) = img.make_home("home-2");
    assert_ne!(id1, id2);
    let docker = real_docker();

    // Leave a residual container from home1 (via killed helper, plus an old
    // `layerfs.export.started=1` container owned by home1).
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_load_with_pause", "--nocapture"])
        .env("LP176_PAUSE_HOME", &home1)
        .env("LP176_PAUSE_MACHINE", "docker")
        .env("LP176_PAUSE_IMAGE", &img.tag)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("home1 child pauses in send", || {
        home1.join("paused").exists()
    });
    let owned1 = owned_containers(&docker, &home1).unwrap();
    assert_eq!(owned1.len(), 1);
    let c1 = owned1[0].name.clone();
    let v1 = container_volumes(&c1)[0].clone();
    child.kill().unwrap();
    child.wait().unwrap();

    // Also create an aged container labeled for home1 to ensure STALE_SECS
    // age-based sweeping is gone.
    let aged_c1 = format!("layerfs-export-{}-docker-0000000000000001", &id1[..8]);
    let out = Command::new("docker")
        .args([
            "create",
            "--name",
            &aged_c1,
            "--label",
            &format!("layerfs.role={EXPORT_ROLE}"),
            "--label",
            &format!("layerfs.home={id1}"),
            "--label",
            "layerfs.machine=docker",
            "--label",
            &format!("layerfs.image={}", img.id),
            "--label",
            "layerfs.export.nonce=0000000000000001",
            "--label",
            "layerfs.export.started=1",
            "--network",
            "none",
            "--entrypoint",
            "/layerfs-export",
            &img.id,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    // home2 neither sees nor deletes home1's containers.
    assert!(owned_containers(&docker, &home2).unwrap().is_empty());
    assert_eq!(
        cleanup_owned(&docker, &home2, None, None).unwrap(),
        Cleanup::default()
    );
    let (loaded2, _) = run_load(&docker, &home2, "docker", &img.tag);
    loaded2.unwrap();
    assert!(container_exists(&c1), "home2 must not delete home1's {c1}");
    assert!(
        container_exists(&aged_c1),
        "home2 must not delete aged container {aged_c1} of home1"
    );

    // home1's own cleanup enumerates and removes both of home1's containers
    // and their anonymous volumes.
    let cleaned = cleanup_owned(&docker, &home1, None, None).unwrap();
    assert_eq!(cleaned.removed.len(), 2);
    assert!(cleaned.failed.is_empty());
    assert!(!container_exists(&c1));
    assert!(!volume_exists(&v1));
    assert!(!container_exists(&aged_c1));
}

#[test]
fn real_docker_next_copy_sweeps_a_killed_copy_without_touching_active() {
    if !docker_available() {
        eprintln!("NOT_RUN: docker unavailable");
        return;
    }
    let mut img_killed = TestImage::build("m1-killed", true);
    let mut img_active = TestImage::build("m1-active", true);
    let mut img_next = TestImage::build("m1-next", false);
    let (home, _) = img_killed.make_home("home-m1");
    let home_id = disk::ensure(&home).unwrap().id;
    img_active.homes.push(home_id.clone());
    img_next.homes.push(home_id);
    let docker = real_docker();

    // 1. SIGKILL a load on (machine="docker", img_killed), leaving container + anonymous volume.
    let mut killed_child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_load_with_pause", "--nocapture"])
        .env("LP176_PAUSE_HOME", &home)
        .env("LP176_PAUSE_MACHINE", "docker")
        .env("LP176_PAUSE_IMAGE", &img_killed.tag)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for("killed-to-be load pauses in send", || {
        home.join("paused").exists()
    });
    let killed_owned = owned_containers(&docker, &home).unwrap();
    assert_eq!(killed_owned.len(), 1);
    let c_killed = killed_owned[0].clone();
    let v_killed = container_volumes(&c_killed.name)[0].clone();
    assert!(volume_exists(&v_killed));
    killed_child.kill().unwrap();
    killed_child.wait().unwrap();
    fs::remove_file(home.join("paused")).unwrap();

    // 2. Keep a second load actively paused on (machine="lima", img_active).
    let active_child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_load_with_pause", "--nocapture"])
        .env("LP176_PAUSE_HOME", &home)
        .env("LP176_PAUSE_MACHINE", "lima")
        .env("LP176_PAUSE_IMAGE", &img_active.tag)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for("active load pauses in send", || {
        home.join("paused").exists()
    });
    let c_active = owned_containers(&docker, &home)
        .unwrap()
        .into_iter()
        .find(|c| c.image == img_active.id)
        .expect("active container present");
    let v_active = container_volumes(&c_active.name)[0].clone();

    // 3. A normal load on a DIFFERENT image and machine ("native", img_next)
    //    sweeps every image whose copy lock is free (spec v20): c_killed
    //    goes with its anonymous volume, c_active (held) stays, and the
    //    removal is a line of the home's `log/sweep.jsonl`.
    // an unpack a killed copy left in native's image cache goes too
    let left_tmp = cache::cache(&home).join(format!(".tmp-{}.2-zz1", "cd".repeat(32)));
    fs::create_dir_all(left_tmp.join("usr")).unwrap();
    fs::write(left_tmp.join("usr/tool"), b"x").unwrap();
    let (loaded, _) = run_load(
        &docker,
        &home,
        layerfs_home::names::HOST_MACHINE,
        &img_next.tag,
    );
    assert!(!loaded.unwrap().reused);
    assert!(!left_tmp.exists(), "the copy sweeps a free image's unpack");
    assert!(
        !container_exists(&c_killed.name),
        "the next copy sweeps the killed copy's container"
    );
    assert!(
        !volume_exists(&v_killed),
        "the sweep removes the killed container's anonymous volume {v_killed}"
    );
    let log = fs::read_to_string(home.join("log").join("sweep.jsonl")).unwrap();
    assert!(
        log.lines().any(|line| line.contains(&c_killed.name)
            && line.contains("\"export_container\"")
            && line.contains("\"removed\"")),
        "{log}"
    );
    assert!(
        container_exists(&c_active.name),
        "a live copy's container stays"
    );
    assert!(volume_exists(&v_active));
    assert_eq!(
        owned_containers(&docker, &home).unwrap(),
        vec![c_active.clone()]
    );

    // 4. Let active_child finish and verify full cleanup.
    fs::write(home.join("proceed"), b"").unwrap();
    let out = active_child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "active load child failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!container_exists(&c_active.name));
    assert!(!volume_exists(&v_active));
    assert!(owned_containers(&docker, &home).unwrap().is_empty());
}

#[test]
fn cleanup_owned_reports_failed_when_removal_fails() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = Scratch::new("lp176-cleanup-fail");
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    let home_id = disk::ensure(&home).unwrap().id;
    let cname = format!("layerfs-export-{}-docker-deadbeef00000001", &home_id[..8]);
    let img_id = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    let vol_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    // Case 1: `rm -v` and `rm -f` both fail while `container inspect` confirms container remains.
    let bin_rm_fail = scratch.base.join("docker-rm-fail");
    let script_rm_fail = format!(
        "#!/bin/sh\n\
         case \"$1 $2\" in\n\
           \"ps -a\") printf '%s\\t%s\\tdocker\\t%s\\tdeadbeef00000001\\n' '{cname}' '{home_id}' '{img_id}'; exit 0 ;;\n\
           \"container inspect\")\n\
             case \"$*\" in\n\
               *Mounts*) exit 0 ;;\n\
               *) echo id; exit 0 ;;\n\
             esac ;;\n\
         esac\n\
         case \"$1\" in\n\
           rm) echo 'permission denied' >&2; exit 1 ;;\n\
           *) exit 2 ;;\n\
         esac\n"
    );
    fs::write(&bin_rm_fail, script_rm_fail).unwrap();
    fs::set_permissions(&bin_rm_fail, fs::Permissions::from_mode(0o755)).unwrap();
    let res1 = cleanup_owned(&Docker::at(Some(bin_rm_fail), None), &home, None, None).unwrap();
    assert_eq!(
        res1,
        Cleanup {
            removed: Vec::new(),
            active: Vec::new(),
            failed: vec![cname.clone()],
        }
    );

    // Case 2: `rm -v` fails, `rm -f` succeeds, but `volume rm` fails on an anonymous volume.
    let bin_vol_fail = scratch.base.join("docker-vol-fail");
    let script_vol_fail = format!(
        "#!/bin/sh\n\
         case \"$1 $2\" in\n\
           \"ps -a\") printf '%s\\t%s\\tdocker\\t%s\\tdeadbeef00000001\\n' '{cname}' '{home_id}' '{img_id}'; exit 0 ;;\n\
           \"container inspect\")\n\
             case \"$*\" in\n\
               *Mounts*) echo '{vol_id}'; exit 0 ;;\n\
               *) echo id; exit 0 ;;\n\
             esac ;;\n\
           \"volume rm\") echo 'volume in use' >&2; exit 1 ;;\n\
         esac\n\
         case \"$1 $2\" in\n\
           \"rm -v\") echo 'cannot remove volume' >&2; exit 1 ;;\n\
           \"rm -f\") exit 0 ;;\n\
         esac\n\
         exit 2\n"
    );
    fs::write(&bin_vol_fail, script_vol_fail).unwrap();
    fs::set_permissions(&bin_vol_fail, fs::Permissions::from_mode(0o755)).unwrap();
    let res2 = cleanup_owned(&Docker::at(Some(bin_vol_fail), None), &home, None, None).unwrap();
    assert_eq!(
        res2,
        Cleanup {
            removed: Vec::new(),
            active: Vec::new(),
            failed: vec![cname],
        }
    );
}

/// Helper process used by the kill/volume tests: runs real `load()`, signals
/// `<home>/paused` once `send` reaches `done >= CHUNK` (so `.part` has data
/// and the container + volume exist in Docker), and waits for `<home>/proceed`.
#[test]
fn helper_load_with_pause() {
    let Ok(home) = std::env::var("LP176_PAUSE_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let machine = std::env::var("LP176_PAUSE_MACHINE").unwrap();
    let image = std::env::var("LP176_PAUSE_IMAGE").unwrap();
    let docker = real_docker();
    let mut paused = false;
    let mut sink = |event: SdkValue| {
        if !paused {
            if let Some(p) = event.get("progress") {
                if p.get_text("phase") == Some("send")
                    && p.get_int("done").unwrap_or(0) >= CHUNK as i64
                {
                    paused = true;
                    fs::write(home.join("paused"), b"").unwrap();
                    while !home.join("proceed").exists() {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        }
    };
    let mut events = Events::new(&mut sink);
    load(
        &docker,
        &mut events,
        &mut Target::local(&home, &machine),
        None,
        &image,
    )
    .unwrap();
}
