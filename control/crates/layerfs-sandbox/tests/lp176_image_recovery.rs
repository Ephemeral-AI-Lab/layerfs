//! Image parts enumeration, lock- & InUse-safe prune, interrupted publish
//! recovery, moved/ambiguous ref handling, sparse tar expansion, hardlink to
//! dropped target refusal, declared-loss metadata/SHA verification, and
//! record-only variant/config (#176 I03).
#[allow(dead_code)]
#[path = "../../layerfs-sandbox-backend/tests/support/image.rs"]
mod fixture;
use fixture::{pax_record, Scratch, Tar};
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::names::MachineName;
use layerfs_machine::worker::{read_frame, read_frame_within, write_frame, MAX_HELLO, PROTOCOL};
use layerfs_sandbox::docker::Docker;
use layerfs_sandbox::image_artifact::{self as cache, architecture, Sidecar, CHUNK};
use layerfs_sandbox::image_load::{load, ImageLock, Loaded, Target};
use layerfs_sandbox::image_tar::{Filtered, TarReport, EXTENDED_TOTAL_MAX, SPARSE_MAX};
use layerfs_sandbox::images;
use layerfs_sandbox_backend::{rootfs, Image};
use layerfs_store::place::{self, Executor};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn sha256_hex(bytes: &[u8]) -> String {
    layerfs_bridge::sdk_values::hex(&Sha256::digest(bytes))
}

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn real_docker() -> Docker {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let program = std::env::split_paths(&path)
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.join("docker"))
        .find(|p| p.is_file());
    Docker::at(program, None)
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

/// Manages real-Docker test images/tags/containers and cleans up only its own
/// resources on Drop.
struct DockerGuard {
    tags: Vec<String>,
    homes: Vec<String>,
    scratch: Scratch,
}

impl DockerGuard {
    fn new(name: &str) -> Self {
        Self {
            tags: Vec::new(),
            homes: Vec::new(),
            scratch: Scratch::new(name),
        }
    }

    fn make_home(&mut self, name: &str) -> (PathBuf, String) {
        let home = self.scratch.base.join(name);
        fs::create_dir_all(&home).unwrap();
        let id = home::ensure(&home).unwrap().id;
        self.homes.push(id.clone());
        (home, id)
    }

    fn import_tar(&mut self, tag: &str, tar: &[u8]) -> String {
        use std::io::Write;
        let mut child = Command::new("docker")
            .args(["import", "-", tag])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn docker import");
        child.stdin.take().unwrap().write_all(tar).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "docker import failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        self.tags.push(tag.to_owned());
        self.inspect_id(tag)
    }

    fn build_dockerfile(&mut self, tag: &str, dockerfile: &str) -> String {
        let dir = self
            .scratch
            .base
            .join(format!("df-{}", SEQ.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Dockerfile"), dockerfile).unwrap();
        let out = Command::new("docker")
            .args([
                "build",
                "--pull=false",
                "-q",
                "-t",
                tag,
                dir.to_str().unwrap(),
            ])
            .output()
            .expect("docker build");
        assert!(
            out.status.success(),
            "docker build failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        self.tags.push(tag.to_owned());
        self.inspect_id(tag)
    }

    fn retag(&mut self, src: &str, dst: &str) {
        let out = Command::new("docker")
            .args(["tag", src, dst])
            .output()
            .expect("docker tag");
        assert!(
            out.status.success(),
            "docker tag failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if !self.tags.iter().any(|t| t == dst) {
            self.tags.push(dst.to_owned());
        }
    }

    fn inspect_id(&self, ref_or_id: &str) -> String {
        let inspect = Command::new("docker")
            .args(["image", "inspect", "--format", "{{.Id}}", ref_or_id])
            .output()
            .expect("docker image inspect");
        assert!(inspect.status.success());
        String::from_utf8_lossy(&inspect.stdout).trim().to_owned()
    }
}

impl Drop for DockerGuard {
    fn drop(&mut self) {
        let mut cnames = BTreeSet::new();
        for home_id in &self.homes {
            cnames.extend(containers_for_home(home_id));
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
        for tag in self.tags.iter().rev() {
            let _ = Command::new("docker").args(["rmi", "-f", tag]).output();
        }
    }
}

fn filtered_with_report(bytes: &[u8]) -> std::io::Result<(Vec<u8>, TarReport)> {
    let mut out = Vec::new();
    let mut reader = Filtered::new(bytes);
    reader.read_to_end(&mut out)?;
    Ok((out, reader.into_report()))
}

fn filtered_bytes(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    filtered_with_report(bytes).map(|(out, _)| out)
}

struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn new(child: std::process::Child) -> Self {
        Self(Some(child))
    }

    fn kill_and_wait(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill_and_wait();
    }
}

fn run_load(
    docker: &Docker,
    home: &Path,
    machine: &str,
    image: &str,
) -> Result<Loaded, layerfs_bridge::control::SdkRefusal> {
    let mut sink = |_: SdkValue| {};
    let mut events = Events::new(&mut sink);
    load(
        docker,
        &mut events,
        &mut Target::local(home, machine),
        None,
        image,
    )
}

fn native_name() -> MachineName {
    MachineName::parse("native").unwrap()
}

fn image_rows(home: &Path) -> Vec<SdkValue> {
    let out = images::list(home, &native_name(), false).unwrap();
    match out.get("images") {
        Some(SdkValue::List(rows)) => rows.clone(),
        other => panic!("unexpected images field: {other:?}"),
    }
}

fn step(home: &Path, args: SdkValue) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    cache::step(&layerfs_sandbox::deploy::Side::Home(home), &args)
}

/// Builds a GNU oldgnu sparse (`b'S'`) tar entry with arbitrary chunks (supports >4 chunks).
fn build_gnu_sparse_tar(
    path: &str,
    mode: u32,
    mtime: u64,
    realsize: u64,
    chunks: &[(u64, &[u8])],
) -> Vec<u8> {
    let stored_size: u64 = chunks.iter().map(|(_, d)| d.len() as u64).sum();
    let mut hdr = [0u8; 512];
    hdr[..path.len()].copy_from_slice(path.as_bytes());
    let oct = |f: &mut [u8], v: u64| {
        let w = f.len() - 1;
        let t = format!("{v:0w$o}\0");
        f.copy_from_slice(&t.as_bytes()[..f.len()]);
    };
    oct(&mut hdr[100..108], u64::from(mode));
    oct(&mut hdr[108..116], 0);
    oct(&mut hdr[116..124], 0);
    oct(&mut hdr[124..136], stored_size);
    oct(&mut hdr[136..148], mtime);
    hdr[156] = b'S';
    hdr[257..263].copy_from_slice(b"ustar ");
    hdr[263..265].copy_from_slice(b" \0");

    for (i, (off, data)) in chunks.iter().take(4).enumerate() {
        let base = 386 + i * 24;
        oct(&mut hdr[base..base + 12], *off);
        oct(&mut hdr[base + 12..base + 24], data.len() as u64);
    }
    let has_ext = chunks.len() > 4;
    hdr[482] = u8::from(has_ext);
    oct(&mut hdr[483..495], realsize);
    hdr[148..156].fill(b' ');
    let sum: u64 = hdr.iter().map(|b| u64::from(*b)).sum();
    hdr[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());

    let mut out = Vec::new();
    out.extend_from_slice(&hdr);

    let mut idx = 4;
    while idx < chunks.len() {
        let mut ext = [0u8; 512];
        for (slot, (off, data)) in chunks[idx..].iter().take(21).enumerate() {
            let base = slot * 24;
            oct(&mut ext[base..base + 12], *off);
            oct(&mut ext[base + 12..base + 24], data.len() as u64);
        }
        idx += 21;
        ext[504] = u8::from(idx < chunks.len());
        out.extend_from_slice(&ext);
    }

    for (_, data) in chunks {
        out.extend_from_slice(data);
    }
    let rem = out.len() % 512;
    if rem != 0 {
        out.resize(out.len() + (512 - rem), 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

#[test]
fn real_docker_tag_move_leaves_enumerable_part_and_clears_stale_ref_on_publish() {
    if !docker_available() {
        eprintln!("NOT_RUN: docker unavailable");
        return;
    }
    let mut guard = DockerGuard::new("lp176-rec-tagmove");
    let pid = std::process::id();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let repo_tag = format!("layerfs-lp176-rec-app-{pid}-{seq}:latest");
    let v1_base = format!("layerfs-lp176-rec-v1b-{pid}-{seq}:test");
    let v2_base = format!("layerfs-lp176-rec-v2b-{pid}-{seq}:test");
    let v2_tag = format!("layerfs-lp176-rec-v2-{pid}-{seq}:test");

    let tar1 = Tar::new()
        .at(1_700_000_000)
        .directory("etc/", 0o755)
        .file("etc/ver", 0o644, b"v1\n")
        .file("usr/bin/blob1", 0o755, &vec![0x11; CHUNK + 65_536])
        .finish();
    let v1_id = guard.import_tar(&v1_base, &tar1);
    let v1_hex = v1_id.strip_prefix("sha256:").unwrap().to_owned();
    guard.retag(&v1_base, &repo_tag);

    let tar2 = Tar::new()
        .at(1_700_000_000)
        .directory("etc/", 0o755)
        .file("etc/ver", 0o644, b"v2\n")
        .file("usr/bin/blob2", 0o755, &vec![0x22; CHUNK + 131_072])
        .finish();
    guard.import_tar(&v2_base, &tar2);
    let v2_id = guard.build_dockerfile(
        &v2_tag,
        &format!("FROM {v2_base}\nENV APP_MODE=prod\nWORKDIR /app\nUSER 1000:1000\n"),
    );
    let v2_hex = v2_id.strip_prefix("sha256:").unwrap().to_owned();

    let (home, _) = guard.make_home("home");
    let docker = real_docker();

    // 1. Load V1 under repo_tag.
    let loaded1 = run_load(&docker, &home, "native", &repo_tag).unwrap();
    assert_eq!(loaded1.id, v1_id);
    assert!(!loaded1.reused);

    // 2. Move repo_tag in Docker to V2, start loading repo_tag, and kill mid-send.
    guard.retag(&v2_tag, &repo_tag);
    let mut child = ChildGuard::new(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "helper_pause_load", "--nocapture"])
            .env("LP176_REC_PAUSE_HOME", &home)
            .env("LP176_REC_PAUSE_MACHINE", "native")
            .env("LP176_REC_PAUSE_IMAGE", &repo_tag)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for("child pauses with staged part", || {
        home.join("paused").exists()
    });
    child.kill_and_wait();

    // 3. machine.images must enumerate both V1 (ready) and V2's staged part,
    // which no live copy holds: broken, reason partial (v19)
    let rows = image_rows(&home);
    let v1_row = rows
        .iter()
        .find(|r| r.get_text("id") == Some(v1_id.as_str()))
        .expect("V1 published row");
    assert_eq!(v1_row.get_text("state"), Some("ready"));
    assert!(cache::texts(v1_row.get("refs")).contains(&repo_tag));

    let v2_part = rows
        .iter()
        .find(|r| r.get_text("id") == Some(v2_id.as_str()))
        .unwrap_or_else(|| panic!("missing staged part row for V2 in machine.images: {rows:?}"));
    let part_len = fs::symlink_metadata(cache::cache(&home).join(format!(".recv-{v2_hex}.part")))
        .unwrap()
        .len();
    assert_eq!(v2_part.get_int("staged_bytes"), Some(part_len as i64));
    assert_eq!(v2_part.get("size"), Some(&SdkValue::Null));
    assert_eq!(v2_part.get_text("state"), Some("broken"), "{v2_part:?}");
    assert_eq!(v2_part.get_text("reason"), Some("partial"));
    assert_eq!(
        v2_part.get_text("fix"),
        Some(format!("layerfs machine load-image native {v2_id}").as_str())
    );
    for gone in ["staged", "stranded", "resumable", "age"] {
        assert_eq!(v2_part.get(gone), None, "{gone} stays out of the item");
    }

    // 4. Finish loading repo_tag: V2 publishes, clears stale repo_tag from V1 sidecar, records config.
    std::thread::sleep(Duration::from_millis(2200));
    let loaded2 = run_load(&docker, &home, "native", &repo_tag).unwrap();
    assert_eq!(loaded2.id, v2_id);
    assert!(!loaded2.reused);

    let v1_sidecar = Sidecar::read(&cache::cache(&home), &v1_hex)
        .unwrap()
        .unwrap();
    assert!(
        !v1_sidecar.refs.contains(&repo_tag),
        "stale ref {repo_tag} must be removed from V1 sidecar: {v1_sidecar:?}"
    );
    let v2_sidecar = Sidecar::read(&cache::cache(&home), &v2_hex)
        .unwrap()
        .unwrap();
    assert!(v2_sidecar.refs.contains(&repo_tag));

    // Offline ref probe must resolve to V2, never V1.
    let probe_by_ref = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("ref", SdkValue::text(&repo_tag)),
        ]),
    )
    .unwrap();
    assert_eq!(probe_by_ref.get_text("id"), Some(v2_id.as_str()));

    let rows_after = image_rows(&home);
    assert!(rows_after
        .iter()
        .all(|r| r.get_text("state") == Some("ready")
            && r.get("staged_bytes") == Some(&SdkValue::Null)));
    let v2_row = rows_after
        .iter()
        .find(|r| r.get_text("id") == Some(v2_id.as_str()))
        .unwrap();
    let cfg = v2_row
        .get("config")
        .expect("config visible in machine.images");
    assert_eq!(cfg.get_text("WorkingDir"), Some("/app"));
    assert_eq!(cfg.get_text("User"), Some("1000:1000"));
}

#[test]
fn prune_reclaims_parts_temps_and_stranded_while_protecting_live_locks_and_in_use() {
    let scratch = Scratch::new("lp176-rec-prune");
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    home::ensure(&home).unwrap();
    let cache_dir = cache::cache(&home);
    fs::create_dir_all(&cache_dir).unwrap();

    let hex_unheld = "1".repeat(64);
    let hex_stranded = "2".repeat(64);
    let hex_dangling = "3".repeat(64);
    let hex_locked = "4".repeat(64);
    let id_locked = format!("sha256:{hex_locked}");
    let hex_inuse = "5".repeat(64);
    let id_inuse = format!("sha256:{hex_inuse}");
    let hex_held = "6".repeat(64);
    let id_held = format!("sha256:{hex_held}");

    // 1. Unheld part + temp dir.
    cache::write(&cache_dir, &hex_unheld, 0, b"partial-tar-bytes").unwrap();
    fs::create_dir_all(cache_dir.join(format!(".tmp-{hex_unheld}.2-ab12/usr"))).unwrap();

    // 2. Stranded unpacked dir (no .2.json) + dangling sidecar (no .2 dir).
    let flat_tar = scratch.base.join("flat.tar");
    let small_tar = filtered_bytes(&Tar::new().file("etc/a", 0o644, b"a").finish()).unwrap();
    fs::write(&flat_tar, &small_tar).unwrap();
    rootfs::ensure_flat(
        &flat_tar,
        &Image::parse(&format!("sha256:{hex_stranded}")).unwrap(),
        &cache_dir,
    )
    .unwrap();
    Sidecar {
        id: format!("sha256:{hex_dangling}"),
        refs: vec!["dangling:1".to_owned()],
        size: Some(10),
        architecture: Some(architecture().to_owned()),
        source: "imported".to_owned(),
        config: SdkValue::Null,
        last_used: None,
    }
    .write(&cache_dir)
    .unwrap();

    // 3. Live-locked part + temp dir (held by child process).
    cache::write(&cache_dir, &hex_locked, 0, b"locked-partial-bytes").unwrap();
    fs::create_dir_all(cache_dir.join(format!(".tmp-{hex_locked}.2-cd34/bin"))).unwrap();
    let mut lock_child = ChildGuard::new(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "helper_hold_image_lock", "--nocapture"])
            .env("LP176_REC_LOCK_HOME", &home)
            .env("LP176_REC_LOCK_MACHINE", "native")
            .env("LP176_REC_LOCK_IMAGE", &id_locked)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for("lock child holds ImageLock", || {
        home.join("lock-held").exists()
    });

    // 4. InUse image via helper.spec lower= (with an extra part + temp dir too) and ImageHold.
    rootfs::ensure_flat(&flat_tar, &Image::parse(&id_inuse).unwrap(), &cache_dir).unwrap();
    Sidecar {
        id: id_inuse.clone(),
        refs: vec!["inuse:1".to_owned()],
        size: Some(small_tar.len() as u64),
        architecture: Some(architecture().to_owned()),
        source: "imported".to_owned(),
        config: SdkValue::Null,
        last_used: None,
    }
    .write(&cache_dir)
    .unwrap();
    cache::write(&cache_dir, &hex_inuse, 0, b"inuse-part").unwrap();
    let spec_dir = cache_dir.parent().unwrap().join("sb-active/config");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("helper.spec"),
        format!(
            "root=/x\nlower={}\n",
            cache_dir.join(format!("{hex_inuse}.2")).display()
        ),
    )
    .unwrap();
    rootfs::ensure_flat(&flat_tar, &Image::parse(&id_held).unwrap(), &cache_dir).unwrap();
    Sidecar {
        id: id_held.clone(),
        refs: vec!["held:1".to_owned()],
        size: Some(small_tar.len() as u64),
        architecture: Some(architecture().to_owned()),
        source: "imported".to_owned(),
        config: SdkValue::Null,
        last_used: None,
    }
    .write(&cache_dir)
    .unwrap();
    let (_, hold) =
        rootfs::ensure_held(&flat_tar, &Image::parse(&id_held).unwrap(), &cache_dir).unwrap();

    // machine.images (v19): the unheld part is broken/partial, the part a
    // live copy holds is busy with op load_image, the in-use image stays
    // ready with its stale part as staged_bytes, one item per image.
    let rows = image_rows(&home);
    let row = |id: &str| {
        let mut found = rows.iter().filter(|r| r.get_text("id") == Some(id));
        let first = found
            .next()
            .unwrap_or_else(|| panic!("{id} listed: {rows:?}"));
        assert!(found.next().is_none(), "one item per image: {rows:?}");
        first.clone()
    };
    let r_unheld = row(&format!("sha256:{hex_unheld}"));
    assert_eq!(r_unheld.get_text("state"), Some("broken"));
    assert_eq!(r_unheld.get_text("reason"), Some("partial"));
    let r_locked = row(&id_locked);
    assert_eq!(
        r_locked.get_text("state"),
        Some("busy"),
        "live-locked part is busy: {r_locked:?}"
    );
    let op = r_locked.get("op").expect("busy carries op");
    assert_eq!(op.get_text("name"), Some("load_image"));
    assert_eq!(op.get_text("phase"), Some("lock"));
    let r_inuse = row(&id_inuse);
    assert_eq!(r_inuse.get_text("state"), Some("ready"));
    assert_eq!(
        r_inuse.get_int("staged_bytes"),
        Some(b"inuse-part".len() as i64)
    );
    let r_stranded = row(&format!("sha256:{hex_stranded}"));
    assert_eq!(r_stranded.get_text("reason"), Some("partial"));
    assert_eq!(r_stranded.get("staged_bytes"), Some(&SdkValue::Null));

    // Targeted prune: a live copy's image is Busy with its op, an image a
    // Sandbox uses is InUse with sandbox_count.
    let busy = images::prune(&home, &native_name(), Some(&id_locked), false).unwrap_err();
    assert_eq!(busy.code, "Busy");
    assert_eq!(
        (busy.detail.get("next"))
            .and_then(|next| next.get("op"))
            .and_then(|op| op.get_text("name")),
        Some("load_image"),
        "{busy:?}"
    );
    let inuse = images::prune(&home, &native_name(), Some(&id_inuse), false).unwrap_err();
    assert_eq!(inuse.code, "InUse");
    assert_eq!(inuse.detail.get_int("sandbox_count"), Some(1));

    // Unused prune must clean unheld part, unheld temp, stranded image, dangling sidecar
    // and the in-use image's part (v19), while leaving live-locked and InUse images untouched.
    let swept = images::prune(&home, &native_name(), None, true).unwrap();
    assert!(
        cache::texts(swept.get("removed")).contains(&id_inuse),
        "the in-use image's part is removed: {swept:?}"
    );

    assert!(!cache_dir.join(format!(".recv-{hex_unheld}.part")).exists());
    assert!(!cache_dir.join(format!(".tmp-{hex_unheld}.2-ab12")).exists());
    assert!(!cache_dir.join(format!("{hex_stranded}.2")).exists());
    assert!(!cache_dir.join(format!("{hex_dangling}.2.json")).exists());

    assert!(cache_dir.join(format!(".recv-{hex_locked}.part")).exists());
    assert!(cache_dir.join(format!(".tmp-{hex_locked}.2-cd34")).exists());
    assert!(cache_dir.join(format!("{hex_inuse}.2")).exists());
    assert!(cache_dir.join(format!("{hex_inuse}.2.json")).exists());
    assert!(!cache_dir.join(format!(".recv-{hex_inuse}.part")).exists());
    assert!(cache_dir.join(format!("{hex_held}.2")).exists());

    // Release locks and InUse, then prune locked part explicitly by target id before unused sweep.
    lock_child.kill_and_wait();
    drop(hold);
    fs::remove_dir_all(cache_dir.parent().unwrap().join("sb-active")).unwrap();

    // Targeted prune can remove a part-only digest and then reports NotFound once gone.
    let single = images::prune(&home, &native_name(), Some(&id_locked), false).unwrap();
    assert_eq!(
        single.get("removed"),
        Some(&SdkValue::List(vec![SdkValue::text(&id_locked)]))
    );
    assert_eq!(
        images::prune(&home, &native_name(), Some(&id_locked), false)
            .unwrap_err()
            .code,
        "NotFound"
    );

    images::prune(&home, &native_name(), None, true).unwrap();
    let leftovers: Vec<String> = fs::read_dir(&cache_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.ends_with(".lock"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "expected clean cache after prune, left: {leftovers:?}"
    );
}

#[test]
fn interrupted_publish_with_renamed_dir_and_missing_sidecar_recovers_and_prunes() {
    let scratch = Scratch::new("lp176-rec-publish");
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    home::ensure(&home).unwrap();
    let cache_dir = cache::cache(&home);
    fs::create_dir_all(&cache_dir).unwrap();

    let hex = "7".repeat(64);
    let id = format!("sha256:{hex}");
    let tar_bytes = filtered_bytes(
        &Tar::new()
            .directory("etc/", 0o755)
            .file("etc/hello", 0o644, b"world\n")
            .finish(),
    )
    .unwrap();

    // Stage part and simulate crash right after ensure_flat renamed `<hex>.2` before Sidecar::write.
    cache::write(&cache_dir, &hex, 0, &tar_bytes).unwrap();
    let part_path = cache_dir.join(format!(".recv-{hex}.part"));
    rootfs::ensure_flat(&part_path, &Image::parse(&id).unwrap(), &cache_dir).unwrap();
    assert!(cache_dir.join(format!("{hex}.2")).is_dir());
    assert!(!cache_dir.join(format!("{hex}.2.json")).exists());

    // probe must NOT claim an unpublished directory without sidecar is present.
    let probed = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("id", SdkValue::text(&id)),
        ]),
    )
    .unwrap();
    assert!(
        !probed.get_bool("present"),
        "dir without sidecar must not be treated as published: {probed:?}"
    );
    assert!(probed.get_bool("stranded"));
    assert_eq!(probed.get_int("offset"), Some(tar_bytes.len() as i64));
    let deploy = layerfs_sandbox::host::Deploy {
        sandbox: Some("namespaces".to_owned()),
        local_dir: cache::cache(&home)
            .parent()
            .unwrap()
            .to_str()
            .map(str::to_owned),
        container_prefix: None,
        daemon: None,
        image_archive: None,
        image: Some(id.clone()),
        platform: None,
    };
    assert!(!cache::unpacked(&deploy, &Image::parse(&id).unwrap()));
    // v19: the unrecorded unpack and its part are one broken/partial item
    let rows_before = image_rows(&home);
    let stranded = rows_before
        .iter()
        .find(|r| r.get_text("id") == Some(id.as_str()))
        .unwrap_or_else(|| panic!("{id} listed: {rows_before:?}"));
    assert_eq!(stranded.get_text("state"), Some("broken"));
    assert_eq!(stranded.get_text("reason"), Some("partial"));
    assert_eq!(
        stranded.get_int("staged_bytes"),
        Some(tar_bytes.len() as i64)
    );

    // Stand-in Docker recovery load completes publish, writes sidecar, deletes .part.
    let standin_dir = scratch.base.join("standin");
    fs::create_dir_all(standin_dir.join("images")).unwrap();
    let script = r#"#!/bin/sh
D="$(dirname "$0")"
key() { printf '%s' "$1" | tr '/:' '__'; }
case "$1 $2" in
  "image inspect")
    shift 4
    cat "$D/images/$(key "$1").json"; echo; exit 0 ;;
  "container inspect")
    if [ "$3" = "--size" ]; then echo 0; exit 0; fi
    if [ -f "$D/containers/$5" ]; then echo id; exit 0; fi
    echo "No such object" >&2; exit 1 ;;
esac
case "$1" in
  create) mkdir -p "$D/containers"; while [ $# -gt 0 ]; do case "$1" in --name) n="$2"; shift 2;; *) shift;; esac; done; : > "$D/containers/$n"; echo id ;;
  export) cat "$D/export.tar" ;;
  history) echo 0 ;;
  rm) shift; [ "$1" = "-f" ] && shift; [ "$1" = "-v" ] && shift; rm -f "$D/containers/$1" ;;
  ps) ;;
esac
"#;
    let dbin = standin_dir.join("docker");
    fs::write(&dbin, script).unwrap();
    fs::set_permissions(&dbin, fs::Permissions::from_mode(0o755)).unwrap();
    let json = format!(
        r#"{{"Id":"{id}","RepoTags":["rec/app:1"],"Os":"linux","Architecture":"{}","Size":100,"Config":{{"Env":["A=1"],"WorkingDir":"/srv","User":"app"}}}}"#,
        architecture()
    );
    for k in ["rec/app:1", &id] {
        fs::write(
            standin_dir
                .join("images")
                .join(format!("{}.json", k.replace(['/', ':'], "_"))),
            &json,
        )
        .unwrap();
    }
    fs::write(standin_dir.join("export.tar"), &tar_bytes).unwrap();

    let docker = Docker::at(Some(dbin), None);
    let loaded = run_load(&docker, &home, "native", "rec/app:1").unwrap();
    assert_eq!(loaded.id, id);
    assert!(!loaded.reused);
    assert!(Sidecar::read(&cache_dir, &hex).unwrap().is_some());
    assert!(!part_path.exists());

    // Subsequent probe sees present=true, and cache::unpacked sees true.
    assert!(cache::unpacked(&deploy, &Image::parse(&id).unwrap()));
    let probed2 = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("id", SdkValue::text(&id)),
        ]),
    )
    .unwrap();
    assert!(probed2.get_bool("present"));
    assert!(!probed2.get_bool("stranded"));
}

#[test]
fn ambiguous_ref_is_refused_and_moved_ref_never_selects_old_image() {
    let scratch = Scratch::new("lp176-rec-refs");
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    let cache_dir = cache::cache(&home);
    fs::create_dir_all(&cache_dir).unwrap();
    let tar_bytes = filtered_bytes(&Tar::new().file("f", 0o644, b"x").finish()).unwrap();
    let sha = sha256_hex(&tar_bytes);

    let hex1 = "8".repeat(64);
    let id1 = format!("sha256:{hex1}");
    let hex2 = "9".repeat(64);
    let id2 = format!("sha256:{hex2}");

    // Two images both claiming "shared/app:latest" -> ambiguous ref must be refused naming the ref.
    cache::write(&cache_dir, &hex1, 0, &tar_bytes).unwrap();
    step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("publish")),
            ("id", SdkValue::text(&id1)),
            ("size", SdkValue::Int(tar_bytes.len() as i64)),
            ("sha256", SdkValue::text(&sha)),
            (
                "refs",
                SdkValue::List(vec![SdkValue::text("shared/app:latest")]),
            ),
        ]),
    )
    .unwrap();

    cache::write(&cache_dir, &hex2, 0, &tar_bytes).unwrap();
    let flat2 = scratch.base.join("flat2.tar");
    fs::write(&flat2, &tar_bytes).unwrap();
    rootfs::ensure_flat(&flat2, &Image::parse(&id2).unwrap(), &cache_dir).unwrap();
    Sidecar {
        id: id2.clone(),
        refs: vec!["shared/app:latest".to_owned()],
        size: Some(tar_bytes.len() as u64),
        architecture: Some(architecture().to_owned()),
        source: "imported".to_owned(),
        config: SdkValue::Null,
        last_used: None,
    }
    .write(&cache_dir)
    .unwrap();

    let err = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("ref", SdkValue::text("shared/app:latest")),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "InvalidInput", "{err:?}");
    assert!(
        err.message.contains("shared/app:latest") && err.message.contains("machine.prune"),
        "ambiguous ref error must name the ref and suggest machine.prune: {}",
        err.message
    );

    let offline = Docker::at(None, None);
    let load_err = run_load(&offline, &home, "native", "shared/app:latest").unwrap_err();
    assert!(
        load_err.message.contains("shared/app:latest"),
        "{load_err:?}"
    );

    // Publish id2 with "moved/app:latest" after id1 had ["moved/app:latest", "moved/app:v1"]:
    // id1 must lose "moved/app:latest" and keep "moved/app:v1".
    let hex_old = "a".repeat(64);
    let id_old = format!("sha256:{hex_old}");
    let hex_new = "b".repeat(64);
    let id_new = format!("sha256:{hex_new}");
    cache::write(&cache_dir, &hex_old, 0, &tar_bytes).unwrap();
    step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("publish")),
            ("id", SdkValue::text(&id_old)),
            ("size", SdkValue::Int(tar_bytes.len() as i64)),
            ("sha256", SdkValue::text(&sha)),
            (
                "refs",
                SdkValue::List(vec![
                    SdkValue::text("moved/app:latest"),
                    SdkValue::text("moved/app:v1"),
                ]),
            ),
        ]),
    )
    .unwrap();

    cache::write(&cache_dir, &hex_new, 0, &tar_bytes).unwrap();
    step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("publish")),
            ("id", SdkValue::text(&id_new)),
            ("size", SdkValue::Int(tar_bytes.len() as i64)),
            ("sha256", SdkValue::text(&sha)),
            (
                "refs",
                SdkValue::List(vec![SdkValue::text("moved/app:latest")]),
            ),
        ]),
    )
    .unwrap();

    let old_sidecar = Sidecar::read(&cache_dir, &hex_old).unwrap().unwrap();
    assert_eq!(old_sidecar.refs, vec!["moved/app:v1".to_owned()]);
    let p_latest = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("ref", SdkValue::text("moved/app:latest")),
        ]),
    )
    .unwrap();
    assert_eq!(p_latest.get_text("id"), Some(id_new.as_str()));
    let p_v1 = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("ref", SdkValue::text("moved/app:v1")),
        ]),
    )
    .unwrap();
    assert_eq!(p_v1.get_text("id"), Some(id_old.as_str()));
}

#[test]
fn sparse_files_expand_with_exact_bytes_and_hardlink_to_dropped_target_fails() {
    let scratch = Scratch::new("lp176-rec-sparse-hl");

    // 1. GNU oldgnu sparse file with 5 chunks (exercises extended sparse header block) + trailing hole.
    let mut expected_gnu = vec![0u8; 32 * 1024];
    expected_gnu[0..4].copy_from_slice(b"C000");
    expected_gnu[4096..4100].copy_from_slice(b"C001");
    expected_gnu[8192..8196].copy_from_slice(b"C002");
    expected_gnu[16384..16388].copy_from_slice(b"C003");
    expected_gnu[24576..24581].copy_from_slice(b"C004!");
    let gnu_tar = build_gnu_sparse_tar(
        "var/big.bin",
        0o640,
        1_700_000_222,
        expected_gnu.len() as u64,
        &[
            (0, b"C000"),
            (4096, b"C001"),
            (8192, b"C002"),
            (16384, b"C003"),
            (24576, b"C004!"),
        ],
    );
    let (filtered_gnu, report_gnu) = filtered_with_report(&gnu_tar).expect("filter GNU sparse tar");
    assert_eq!(report_gnu.sparse_files.len(), 1);
    assert_eq!(report_gnu.sparse_files[0].path, "var/big.bin");
    assert_eq!(report_gnu.sparse_files[0].expanded_bytes, 32 * 1024);
    assert_eq!(report_gnu.sparse_files[0].stored_bytes, 21);
    assert_eq!(report_gnu.sparse_expanded_bytes, 32 * 1024);
    let gnu_path = scratch.base.join("gnu.tar");
    fs::write(&gnu_path, &filtered_gnu).unwrap();
    let (root1, _) = rootfs::ensure_flat(
        &gnu_path,
        &Image::parse(&format!("sha256:{}", "c".repeat(64))).unwrap(),
        &scratch.cache,
    )
    .unwrap();
    let unpacked_gnu = fs::read(root1.directory().join("var/big.bin")).unwrap();
    assert_eq!(sha256_hex(&unpacked_gnu), sha256_hex(&expected_gnu));
    let meta_gnu = fs::symlink_metadata(root1.directory().join("var/big.bin")).unwrap();
    assert_eq!(meta_gnu.permissions().mode() & 0o777, 0o640);
    assert_eq!(
        meta_gnu
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        1_700_000_222
    );

    // 2. PAX 1.0 sparse file (real GNU tar --format=posix --sparse).
    let pax_src = scratch.base.join("pax_src");
    fs::create_dir_all(&pax_src).unwrap();
    let sp_file = pax_src.join("pax.bin");
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut f = fs::File::create(&sp_file).unwrap();
        f.write_all(b"HEAD").unwrap();
        f.seek(SeekFrom::Start(65_536)).unwrap();
        f.write_all(b"TAIL").unwrap();
        f.set_len(131_072).unwrap();
    }
    let expected_pax = fs::read(&sp_file).unwrap();
    let pax_tar = Command::new("tar")
        .args(["--format=posix", "--sparse", "-cf", "-", "-C"])
        .arg(&pax_src)
        .arg("pax.bin")
        .output()
        .unwrap();
    assert!(pax_tar.status.success());
    let (filtered_pax, report_pax) =
        filtered_with_report(&pax_tar.stdout).expect("filter PAX sparse tar");
    assert_eq!(report_pax.sparse_files.len(), 1);
    assert_eq!(report_pax.sparse_files[0].path, "pax.bin");
    assert_eq!(report_pax.sparse_files[0].expanded_bytes, 131_072);
    assert_eq!(report_pax.sparse_expanded_bytes, 131_072);
    let pax_path = scratch.base.join("pax.tar");
    fs::write(&pax_path, &filtered_pax).unwrap();
    let (root2, _) = rootfs::ensure_flat(
        &pax_path,
        &Image::parse(&format!("sha256:{}", "d".repeat(64))).unwrap(),
        &scratch.cache,
    )
    .unwrap();
    let unpacked_pax = fs::read(root2.directory().join("pax.bin")).unwrap();
    assert_eq!(sha256_hex(&unpacked_pax), sha256_hex(&expected_pax));

    // 2b. PAX 0.1 sparse entry (`GNU.sparse.map`) and PAX 0.0 (`offset`/`numbytes`).
    let mut pax01_meta = pax_record("GNU.sparse.major", "0");
    pax01_meta.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    pax01_meta.extend_from_slice(&pax_record("GNU.sparse.name", "opt/sp01.bin"));
    pax01_meta.extend_from_slice(&pax_record("GNU.sparse.realsize", "16384"));
    pax01_meta.extend_from_slice(&pax_record("GNU.sparse.map", "0,4,8192,4"));
    let mut pax00_meta = pax_record("GNU.sparse.major", "0");
    pax00_meta.extend_from_slice(&pax_record("GNU.sparse.minor", "0"));
    pax00_meta.extend_from_slice(&pax_record("GNU.sparse.name", "opt/sp00.bin"));
    pax00_meta.extend_from_slice(&pax_record("GNU.sparse.realsize", "8192"));
    pax00_meta.extend_from_slice(&pax_record("GNU.sparse.offset", "4096"));
    pax00_meta.extend_from_slice(&pax_record("GNU.sparse.numbytes", "5"));
    let pax0x_tar = Tar::new()
        .directory("opt/", 0o755)
        .raw("PaxHeaders/sp01", b'x', 0o644, &pax01_meta)
        .file("opt/sp01.bin", 0o644, b"ABCDWXYZ")
        .raw("PaxHeaders/sp00", b'x', 0o644, &pax00_meta)
        .file("opt/sp00.bin", 0o644, b"12345")
        .finish();
    let (filtered_0x, report_0x) =
        filtered_with_report(&pax0x_tar).expect("filter PAX 0.x sparse tar");
    assert_eq!(report_0x.sparse_files.len(), 2);
    assert_eq!(report_0x.sparse_expanded_bytes, 16_384 + 8_192);
    let pax0x_path = scratch.base.join("pax0x.tar");
    fs::write(&pax0x_path, &filtered_0x).unwrap();
    let (root2b, _) = rootfs::ensure_flat(
        &pax0x_path,
        &Image::parse(&format!("sha256:{}", "0".repeat(64))).unwrap(),
        &scratch.cache,
    )
    .unwrap();
    let mut expected_01 = vec![0u8; 16_384];
    expected_01[..4].copy_from_slice(b"ABCD");
    expected_01[8192..8196].copy_from_slice(b"WXYZ");
    assert_eq!(
        fs::read(root2b.directory().join("opt/sp01.bin")).unwrap(),
        expected_01
    );
    let mut expected_00 = vec![0u8; 8_192];
    expected_00[4096..4101].copy_from_slice(b"12345");
    assert_eq!(
        fs::read(root2b.directory().join("opt/sp00.bin")).unwrap(),
        expected_00
    );

    // 3. Hardlinks pointing to dropped targets must produce a clear error.
    let bad_init = Tar::new()
        .file("etc/hosts", 0o644, b"127.0.0.1 localhost\n")
        .hard_link("app/hosts-alias", "etc/hosts")
        .finish();
    let err_init = filtered_bytes(&bad_init).unwrap_err();
    let msg_init = err_init.to_string();
    assert!(
        msg_init.contains("etc/hosts") && msg_init.contains("app/hosts-alias"),
        "hardlink->dropped init error unclear: {msg_init}"
    );

    let bad_fifo = Tar::new()
        .raw("var/run/pipe", b'6', 0o644, b"")
        .hard_link("app/pipe-alias", "var/run/pipe")
        .finish();
    let err_fifo = filtered_bytes(&bad_fifo).unwrap_err();
    let msg_fifo = err_fifo.to_string();
    assert!(
        msg_fifo.contains("var/run/pipe") && msg_fifo.contains("app/pipe-alias"),
        "hardlink->dropped fifo error unclear: {msg_fifo}"
    );

    let bad_dev = Tar::new()
        .raw("tmp/null", b'3', 0o666, b"")
        .hard_link("app/null-alias", "tmp/null")
        .finish();
    let err_dev = filtered_bytes(&bad_dev).unwrap_err();
    let msg_dev = err_dev.to_string();
    assert!(
        msg_dev.contains("tmp/null") && msg_dev.contains("app/null-alias"),
        "hardlink->dropped device error unclear: {msg_dev}"
    );

    // 4. Hardlink to a kept file succeeds and preserves shared inode.
    let good_hl = Tar::new()
        .directory("usr/", 0o755)
        .file("usr/orig", 0o644, b"shared-bytes\n")
        .hard_link("usr/alias", "usr/orig")
        .finish();
    let filtered_hl = filtered_bytes(&good_hl).unwrap();
    let hl_path = scratch.base.join("hl.tar");
    fs::write(&hl_path, &filtered_hl).unwrap();
    let (root3, _) = rootfs::ensure_flat(
        &hl_path,
        &Image::parse(&format!("sha256:{}", "e".repeat(64))).unwrap(),
        &scratch.cache,
    )
    .unwrap();
    assert_eq!(
        fs::symlink_metadata(root3.directory().join("usr/orig"))
            .unwrap()
            .ino(),
        fs::symlink_metadata(root3.directory().join("usr/alias"))
            .unwrap()
            .ino()
    );
}

#[test]
fn real_docker_and_small_tar_preserve_mtime_mode_sha_and_record_config() {
    let scratch = Scratch::new("lp176-rec-meta");
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    let cache_dir = cache::cache(&home);
    fs::create_dir_all(&cache_dir).unwrap();

    // Verify small tar mtime, mode, sha, symlink, hardlink vs declared drops (FIFO, dev, init, dir mtime=0).
    let mut pax_link = pax_record("mtime", "1700000777.25");
    pax_link.extend_from_slice(&pax_record("linkpath", "/etc/app.conf"));
    let input = Tar::new()
        .at(1_700_000_111)
        .file(".dockerenv", 0o755, b"")
        .directory("dev/", 0o755)
        .file("dev/console", 0o600, b"")
        .directory("etc/", 0o750)
        .file("etc/hosts", 0o644, b"127.0.0.1\n")
        .file("etc/app.conf", 0o640, b"secret-config\n")
        .raw("PaxHeaders/etc/link", b'x', 0o644, &pax_link)
        .symlink("etc/link", "")
        .at(1_700_000_333)
        .file("etc/exec.sh", 0o755, b"#!/bin/sh\n")
        .hard_link("etc/exec_hl.sh", "etc/exec.sh")
        .raw("var/fifo", b'6', 0o644, b"")
        .raw("var/chr", b'3', 0o660, b"")
        .finish();
    let (filtered, report) = filtered_with_report(&input).unwrap();
    assert_eq!(report.fifos, vec!["var/fifo"]);
    assert_eq!(report.devices, vec!["var/chr"]);
    assert_eq!(report.init, vec![".dockerenv", "dev/console", "etc/hosts"]);
    let tar_path = scratch.base.join("meta.tar");
    fs::write(&tar_path, &filtered).unwrap();
    let hex = "f".repeat(64);
    let id = format!("sha256:{hex}");
    let (root, _) =
        rootfs::ensure_flat(&tar_path, &Image::parse(&id).unwrap(), &cache_dir).unwrap();
    let r = root.directory();

    // Declared losses: init, dev/*, FIFO, device are dropped; directory mtime is normalized to 0.
    assert!(!r.join(".dockerenv").exists());
    assert!(!r.join("dev/console").exists());
    assert!(!r.join("etc/hosts").exists());
    assert!(!r.join("var/fifo").exists());
    assert!(!r.join("var/chr").exists());
    assert!(r.join("dev").is_dir());
    assert_eq!(
        fs::symlink_metadata(r.join("etc"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        0
    );

    // Non-lost attributes must be preserved.
    assert_eq!(
        sha256_hex(&fs::read(r.join("etc/app.conf")).unwrap()),
        sha256_hex(b"secret-config\n")
    );
    assert_eq!(
        fs::symlink_metadata(r.join("etc"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o750
    );
    assert_eq!(
        fs::symlink_metadata(r.join("etc/app.conf"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::symlink_metadata(r.join("etc/exec.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::symlink_metadata(r.join("etc/app.conf"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        1_700_000_111
    );
    assert_eq!(
        fs::symlink_metadata(r.join("etc/exec.sh"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        1_700_000_333
    );
    assert_eq!(
        fs::symlink_metadata(r.join("etc/link"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        1_700_000_777
    );
    assert_eq!(
        fs::symlink_metadata(r.join("etc/exec.sh")).unwrap().ino(),
        fs::symlink_metadata(r.join("etc/exec_hl.sh"))
            .unwrap()
            .ino()
    );

    // Publish with variant + config (Env, WorkingDir, User) and verify publish/probe/touch/machine.images
    // preserve them without applying them.
    let sha = sha256_hex(&filtered);
    cache::write(&cache_dir, &hex, 0, &filtered).unwrap();
    let cfg = SdkValue::map([
        (
            "Env",
            SdkValue::List(vec![SdkValue::text("LP176_TEST_VAR=recorded_only")]),
        ),
        ("WorkingDir", SdkValue::text("/srv/work")),
        ("User", SdkValue::text("nobody")),
    ]);
    let pub_out = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("publish")),
            ("id", SdkValue::text(&id)),
            ("size", SdkValue::Int(filtered.len() as i64)),
            ("sha256", SdkValue::text(&sha)),
            ("refs", SdkValue::List(vec![SdkValue::text("meta/app:1")])),
            ("architecture", SdkValue::text(architecture())),
            ("variant", SdkValue::text("v8")),
            ("config", cfg.clone()),
        ]),
    )
    .unwrap();
    assert_eq!(pub_out.get_text("variant"), Some("v8"));
    assert_eq!(pub_out.get("config"), Some(&cfg));
    assert!(std::env::var_os("LP176_TEST_VAR").is_none());
    let probed = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("probe")),
            ("id", SdkValue::text(&id)),
        ]),
    )
    .unwrap();
    assert_eq!(probed.get_text("variant"), Some("v8"));
    assert_eq!(probed.get("config"), Some(&cfg));
    cache::touch(&layerfs_sandbox::deploy::Side::Home(&home), None, &id).unwrap();
    let rows = image_rows(&home);
    let row = rows
        .iter()
        .find(|r| r.get_text("id") == Some(id.as_str()))
        .unwrap();
    assert_eq!(row.get_text("variant"), Some("v8"));
    // machine.images shows the Env names, never their values; the sidecar
    // on the machine keeps the whole record
    let listed = SdkValue::map([
        (
            "Env",
            SdkValue::List(vec![SdkValue::text("LP176_TEST_VAR")]),
        ),
        ("WorkingDir", SdkValue::text("/srv/work")),
        ("User", SdkValue::text("nobody")),
    ]);
    assert_eq!(
        row.get("config"),
        Some(&listed),
        "config must be visible in machine.images without Env values: {row:?}"
    );
    assert!(!format!("{row:?}").contains("recorded_only"), "{row:?}");
    let kept = cache::SidecarInfo::read(&cache_dir, &hex).unwrap().unwrap();
    assert_eq!(kept.sidecar.config, cfg);

    // Also test real Docker image with ENV/WORKDIR/USER/FIFO/mode/mtime.
    if docker_available() {
        let mut guard = DockerGuard::new("lp176-rec-docker-meta");
        let pid = std::process::id();
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let base_tag = format!("layerfs-lp176-rec-mbase-{pid}-{seq}:test");
        let final_tag = format!("layerfs-lp176-rec-mfinal-{pid}-{seq}:test");
        let dtar = Tar::new()
            .at(1_700_000_500)
            .directory("app/", 0o755)
            .file("app/run.sh", 0o750, b"#!/bin/sh\necho ok\n")
            .symlink("app/run.link", "/app/run.sh")
            .raw("app/p.fifo", b'6', 0o644, b"")
            .file("app/blob", 0o644, &vec![0x33; CHUNK + 1024])
            .finish();
        guard.import_tar(&base_tag, &dtar);
        let did = guard.build_dockerfile(
            &final_tag,
            &format!("FROM {base_tag}\nENV LP176_DOCKER_ENV=yes\nWORKDIR /app\nUSER 1001:1001\n"),
        );
        let dhex = did.strip_prefix("sha256:").unwrap().to_owned();
        let (dhome, _) = guard.make_home("dhome");
        let docker = real_docker();
        run_load(&docker, &dhome, "native", &final_tag).unwrap();
        let droot = cache::cache(&dhome).join(format!("{dhex}.2"));
        assert!(!droot.join("app/p.fifo").exists());
        assert_eq!(
            fs::symlink_metadata(droot.join("app/run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o750
        );
        assert_eq!(
            sha256_hex(&fs::read(droot.join("app/run.sh")).unwrap()),
            sha256_hex(b"#!/bin/sh\necho ok\n")
        );
        let drows = image_rows(&dhome);
        let drow = drows
            .iter()
            .find(|r| r.get_text("id") == Some(did.as_str()))
            .unwrap();
        let dcfg = drow
            .get("config")
            .expect("real Docker config visible in machine.images");
        assert_eq!(dcfg.get_text("WorkingDir"), Some("/app"));
        assert_eq!(dcfg.get_text("User"), Some("1001:1001"));
    }
}

struct ModeGuard(PathBuf, u32);
impl Drop for ModeGuard {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(self.1));
    }
}

#[test]
fn prune_skips_unwritable_lock_dir_and_bad_lock_symlinks_while_reclaiming_others() {
    use std::os::unix::fs::symlink;

    // 1. Unwritable lock dir (simulates ENOSPC / EACCES for record/file creation):
    //    prune's gate is a lock on the folder itself and creates no file, so prune does not
    //    abort and reclaims both the hex with a pre-existing `.lock` and one without.
    let scratch1 = Scratch::new("lp176-r1-lock-ro");
    let home1 = scratch1.base.join("home");
    fs::create_dir_all(&home1).unwrap();
    home::ensure(&home1).unwrap();
    let cache1 = cache::cache(&home1);
    fs::create_dir_all(&cache1).unwrap();

    let hex_existing = "1".repeat(64);
    let id_existing = format!("sha256:{hex_existing}");
    let hex_new = "2".repeat(64);
    let id_new = format!("sha256:{hex_new}");

    cache::write(&cache1, &hex_existing, 0, b"part-existing").unwrap();
    cache::write(&cache1, &hex_new, 0, b"part-new").unwrap();
    drop(
        ImageLock::try_take(&home1, "native", &id_existing)
            .unwrap()
            .unwrap(),
    );
    assert!(ImageLock::path(&home1, "native", &hex_existing).is_file());

    let lock_dir1 = ImageLock::dir(&home1, "native");
    let _mode_guard = ModeGuard(lock_dir1.clone(), 0o700);
    fs::set_permissions(&lock_dir1, fs::Permissions::from_mode(0o555)).unwrap();

    let out1 = images::prune(&home1, &native_name(), None, true).unwrap();
    let removed1 = cache::texts(out1.get("removed"));
    assert_eq!(
        removed1,
        vec![id_existing.clone(), id_new.clone()],
        "{out1:?}"
    );
    assert!(!cache1.join(format!(".recv-{hex_existing}.part")).exists());
    assert!(!cache1.join(format!(".recv-{hex_new}.part")).exists());
    assert!(out1.get("skipped").is_none(), "{out1:?}");
    assert!(
        !ImageLock::path(&home1, "native", &hex_new).exists(),
        "prune creates no lock file"
    );
    drop(_mode_guard);

    // 2. Bad `.lock` symlinks (in ImageLock dir and in cache `.2.lock`) must not abort
    //    `prune --unused` or targeted prune of unrelated hexes, while targeted prune on the
    //    bad-lock hex returns an error.
    let scratch2 = Scratch::new("lp176-r1-lock-symlink");
    let home2 = scratch2.base.join("home");
    fs::create_dir_all(&home2).unwrap();
    home::ensure(&home2).unwrap();
    let cache2 = cache::cache(&home2);
    fs::create_dir_all(&cache2).unwrap();
    let lock_dir2 = ImageLock::dir(&home2, "native");
    fs::create_dir_all(&lock_dir2).unwrap();

    let hex_good_unused = "3".repeat(64);
    let id_good_unused = format!("sha256:{hex_good_unused}");
    let hex_good_target = "4".repeat(64);
    let id_good_target = format!("sha256:{hex_good_target}");
    let hex_bad_lock = "5".repeat(64);
    let id_bad_lock = format!("sha256:{hex_bad_lock}");
    let hex_bad_hold = "6".repeat(64);
    let id_bad_hold = format!("sha256:{hex_bad_hold}");

    cache::write(&cache2, &hex_good_unused, 0, b"good-unused").unwrap();
    cache::write(&cache2, &hex_good_target, 0, b"good-target").unwrap();
    cache::write(&cache2, &hex_bad_lock, 0, b"bad-lock").unwrap();
    cache::write(&cache2, &hex_bad_hold, 0, b"bad-hold").unwrap();

    let dummy_target = scratch2.base.join("dummy");
    fs::write(&dummy_target, b"x").unwrap();
    symlink(
        &dummy_target,
        ImageLock::path(&home2, "native", &hex_bad_lock),
    )
    .unwrap();
    symlink(&dummy_target, cache2.join(format!("{hex_bad_hold}.2.lock"))).unwrap();

    // Targeted prune of an unrelated healthy hex succeeds despite bad sibling locks.
    let targeted = images::prune(&home2, &native_name(), Some(&id_good_target), false).unwrap();
    assert_eq!(
        cache::texts(targeted.get("removed")),
        vec![id_good_target.clone()]
    );
    assert!(!cache2
        .join(format!(".recv-{hex_good_target}.part"))
        .exists());

    // Targeted prune of the bad-lock hexes returns an error.
    assert!(images::prune(&home2, &native_name(), Some(&id_bad_lock), false).is_err());
    assert!(images::prune(&home2, &native_name(), Some(&id_bad_hold), false).is_err());

    // Unused prune reclaims healthy hex and records skip reasons for both bad-lock hexes.
    let out2 = images::prune(&home2, &native_name(), None, true).unwrap();
    assert_eq!(
        cache::texts(out2.get("removed")),
        vec![id_good_unused.clone()],
        "{out2:?}"
    );
    assert!(!cache2
        .join(format!(".recv-{hex_good_unused}.part"))
        .exists());
    assert!(cache2.join(format!(".recv-{hex_bad_lock}.part")).exists());
    assert!(cache2.join(format!(".recv-{hex_bad_hold}.part")).exists());
    let skipped2 = match out2.get("skipped") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("expected skipped entries for bad symlinks: {other:?}"),
    };
    for bad_id in [&id_bad_lock, &id_bad_hold] {
        assert!(
            skipped2.iter().any(|s| s.get_text("id") == Some(bad_id)
                && s.get_text("reason").is_some_and(|r| !r.is_empty())),
            "missing skip reason for {bad_id}: {skipped2:?}"
        );
    }
}

#[test]
fn publish_skips_corrupt_unrelated_sidecar() {
    let scratch = Scratch::new("lp176-r1-corrupt-sidecar");
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    let cache_dir = cache::cache(&home);
    fs::create_dir_all(&cache_dir).unwrap();

    let hex_bad = "7".repeat(64);
    fs::create_dir_all(cache_dir.join(format!("{hex_bad}.2"))).unwrap();
    let bad_sidecar_path = cache::sidecar_path(&cache_dir, &hex_bad);
    fs::write(&bad_sidecar_path, b"{not valid json").unwrap();

    let hex_good = "8".repeat(64);
    let id_good = format!("sha256:{hex_good}");
    let tar_bytes = filtered_bytes(&Tar::new().file("etc/ok", 0o644, b"ok\n").finish()).unwrap();
    let sha = sha256_hex(&tar_bytes);
    cache::write(&cache_dir, &hex_good, 0, &tar_bytes).unwrap();

    let out = step(
        &home,
        SdkValue::map([
            ("step", SdkValue::text("publish")),
            ("id", SdkValue::text(&id_good)),
            ("size", SdkValue::Int(tar_bytes.len() as i64)),
            ("sha256", SdkValue::text(&sha)),
            (
                "refs",
                SdkValue::List(vec![SdkValue::text("r1/app:latest")]),
            ),
        ]),
    )
    .unwrap();
    assert!(out.get_bool("unpacked"));
    let good_sidecar = Sidecar::read(&cache_dir, &hex_good).unwrap().unwrap();
    assert_eq!(good_sidecar.refs, vec!["r1/app:latest".to_owned()]);
    assert_eq!(fs::read(&bad_sidecar_path).unwrap(), b"{not valid json");
}

#[test]
fn sparse_safety_rejects_oversized_realsize_excess_chunks_size_mismatch_and_malformed_pax() {
    let assert_invalid_data = |label: &str, tar_bytes: Vec<u8>| {
        let err = filtered_bytes(&tar_bytes).expect_err(label);
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::InvalidData,
            "{label}: got {err:?}"
        );
    };

    // 1. N5: realsize exceeding SPARSE_MAX (oldgnu and PAX 0.1).
    let huge_gnu = build_gnu_sparse_tar("big.bin", 0o644, 0, SPARSE_MAX + 1, &[(0, b"A")]);
    assert_invalid_data("oldgnu realsize > SPARSE_MAX", huge_gnu);

    let mut huge_pax = pax_record("GNU.sparse.major", "0");
    huge_pax.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    huge_pax.extend_from_slice(&pax_record("GNU.sparse.name", "big_pax.bin"));
    huge_pax.extend_from_slice(&pax_record(
        "GNU.sparse.realsize",
        &(SPARSE_MAX + 1).to_string(),
    ));
    huge_pax.extend_from_slice(&pax_record("GNU.sparse.map", "0,1"));
    let huge_pax_tar = Tar::new()
        .raw("PaxHeaders/big", b'x', 0o644, &huge_pax)
        .file("big_pax.bin", 0o644, b"A")
        .finish();
    assert_invalid_data("PAX 0.1 realsize > SPARSE_MAX", huge_pax_tar);

    // 2. N6: oldgnu extended block chain exceeding entry `size` (aborts before EOF/extra blocks).
    let mut gnu_hdr = [0u8; 512];
    gnu_hdr[..7].copy_from_slice(b"bad.bin");
    let oct = |f: &mut [u8], v: u64| {
        let w = f.len() - 1;
        let t = format!("{v:0w$o}\0");
        f.copy_from_slice(&t.as_bytes()[..f.len()]);
    };
    oct(&mut gnu_hdr[100..108], 0o644);
    oct(&mut gnu_hdr[124..136], 3); // entry size = 3 => at most 3 chunks
    gnu_hdr[156] = b'S';
    gnu_hdr[257..263].copy_from_slice(b"ustar ");
    gnu_hdr[263..265].copy_from_slice(b" \0");
    for i in 0..4 {
        let base = 386 + i * 24;
        oct(&mut gnu_hdr[base..base + 12], (i as u64) * 4096);
        oct(&mut gnu_hdr[base + 12..base + 24], 1); // 4th chunk already exceeds size=3!
    }
    gnu_hdr[482] = 1; // claims an extended block follows (intentionally absent!)
    oct(&mut gnu_hdr[483..495], 65_536);
    gnu_hdr[148..156].fill(b' ');
    let sum: u64 = gnu_hdr.iter().map(|b| u64::from(*b)).sum();
    gnu_hdr[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    assert_invalid_data(
        "oldgnu chunk count > size before extended block",
        gnu_hdr.to_vec(),
    );

    // 3. N6: PAX 0.x GNU.sparse.map with chunk count > entry `size` (both PAX `size` and ustar `size`).
    let mut pax_chunks_over_pax_size = pax_record("GNU.sparse.major", "0");
    pax_chunks_over_pax_size.extend_from_slice(&pax_record("size", "2"));
    pax_chunks_over_pax_size.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    pax_chunks_over_pax_size.extend_from_slice(&pax_record("GNU.sparse.name", "m.bin"));
    pax_chunks_over_pax_size.extend_from_slice(&pax_record("GNU.sparse.realsize", "16384"));
    pax_chunks_over_pax_size.extend_from_slice(&pax_record("GNU.sparse.map", "0,1,4096,1,8192,1"));
    let tar_over_pax_size = Tar::new()
        .raw("PaxHeaders/m", b'x', 0o644, &pax_chunks_over_pax_size)
        .file("m.bin", 0o644, b"AB")
        .finish();
    assert_invalid_data("PAX 0.1 chunk count > PAX size", tar_over_pax_size);

    let mut pax_chunks_over_hdr_size = pax_record("GNU.sparse.major", "0");
    pax_chunks_over_hdr_size.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    pax_chunks_over_hdr_size.extend_from_slice(&pax_record("GNU.sparse.name", "m2.bin"));
    pax_chunks_over_hdr_size.extend_from_slice(&pax_record("GNU.sparse.realsize", "16384"));
    pax_chunks_over_hdr_size.extend_from_slice(&pax_record("GNU.sparse.map", "0,1,4096,1,8192,1"));
    let tar_over_hdr_size = Tar::new()
        .raw("PaxHeaders/m2", b'x', 0o644, &pax_chunks_over_hdr_size)
        .file("m2.bin", 0o644, b"AB")
        .finish();
    assert_invalid_data("PAX 0.1 chunk count > ustar size", tar_over_hdr_size);

    // 4. Item 5a: payload_needed != size (extra trailing bytes inside tar entry) for oldgnu, PAX 0.1, PAX 1.0.
    let mut bad_size_gnu = build_gnu_sparse_tar("pad.bin", 0o644, 0, 8192, &[(0, b"ABCD")]);
    oct(&mut bad_size_gnu[124..136], 100); // stored=4 != size=100
    bad_size_gnu[148..156].fill(b' ');
    let sum2: u64 = bad_size_gnu[..512].iter().map(|b| u64::from(*b)).sum();
    bad_size_gnu[148..156].copy_from_slice(format!("{sum2:06o}\0 ").as_bytes());
    assert_invalid_data("oldgnu payload_needed != size", bad_size_gnu);

    let mut pax01_mismatch = pax_record("GNU.sparse.major", "0");
    pax01_mismatch.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    pax01_mismatch.extend_from_slice(&pax_record("GNU.sparse.name", "s01.bin"));
    pax01_mismatch.extend_from_slice(&pax_record("GNU.sparse.realsize", "8192"));
    pax01_mismatch.extend_from_slice(&pax_record("GNU.sparse.map", "0,4"));
    let pax01_mismatch_tar = Tar::new()
        .raw("PaxHeaders/s01", b'x', 0o644, &pax01_mismatch)
        .file("s01.bin", 0o644, b"ABCDEXTRA") // stored=4 != size=9
        .finish();
    assert_invalid_data("PAX 0.1 payload_needed != size", pax01_mismatch_tar);

    let mut pax10_mismatch = pax_record("GNU.sparse.major", "1");
    pax10_mismatch.extend_from_slice(&pax_record("GNU.sparse.minor", "0"));
    pax10_mismatch.extend_from_slice(&pax_record("GNU.sparse.name", "s10.bin"));
    pax10_mismatch.extend_from_slice(&pax_record("GNU.sparse.realsize", "8192"));
    let mut pax10_payload = b"1\n0\n4\n".to_vec();
    pax10_payload.resize(512, 0); // 1-block sparse map
    pax10_payload.extend_from_slice(b"ABCDEXTRA"); // 512 + 4 != 512 + 9
    let pax10_mismatch_tar = Tar::new()
        .raw("PaxHeaders/s10", b'x', 0o644, &pax10_mismatch)
        .file("s10.bin", 0o644, &pax10_payload)
        .finish();
    assert_invalid_data("PAX 1.0 payload_needed != size", pax10_mismatch_tar);

    // 5. Item 5b: malformed PAX record after GNU.sparse.* key must return InvalidData.
    let mut malformed_after_sparse = pax_record("GNU.sparse.major", "0");
    malformed_after_sparse.extend_from_slice(b"999 bad=record\n");
    let malformed_pax_tar = Tar::new()
        .raw("PaxHeaders/bad", b'x', 0o644, &malformed_after_sparse)
        .file("plain.bin", 0o644, b"RAW")
        .finish();
    assert_invalid_data("malformed PAX after GNU.sparse.*", malformed_pax_tar);
}

#[test]
fn r2_pax_size_overflow_plain_bad_size_and_order_independent_chunk_cap() {
    let assert_invalid = |label: &str, tar_bytes: &[u8], needle: &str| {
        let err = filtered_bytes(tar_bytes).expect_err(label);
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::InvalidData,
            "{label}: got {err:?}"
        );
        assert!(
            err.to_string().contains(needle),
            "{label}: expected `{needle}` in `{err}`"
        );
    };

    // 1. R2-M1: PAX size = u64::MAX on both plain and sparse entries returns InvalidData without panic.
    let huge_pax_size = pax_record("size", &u64::MAX.to_string());
    let plain_u64_max = Tar::new()
        .raw("PaxHeaders/huge-plain", b'x', 0o644, &huge_pax_size)
        .file("plain.bin", 0o644, b"RAW")
        .finish();
    assert_invalid("plain PAX size=u64::MAX", &plain_u64_max, "overflow");

    let mut sparse_u64_max_pax = pax_record("GNU.sparse.major", "0");
    sparse_u64_max_pax.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    sparse_u64_max_pax.extend_from_slice(&pax_record("GNU.sparse.realsize", "8192"));
    sparse_u64_max_pax.extend_from_slice(&pax_record("GNU.sparse.map", "0,1"));
    sparse_u64_max_pax.extend_from_slice(&huge_pax_size);
    let sparse_u64_max = Tar::new()
        .raw("PaxHeaders/huge-sparse", b'x', 0o644, &sparse_u64_max_pax)
        .file("sparse.bin", 0o644, b"A")
        .finish();
    assert_invalid("sparse PAX size=u64::MAX", &sparse_u64_max, "overflow");

    // Ustar/base-256 header size = u64::MAX also returns InvalidData without panic.
    let mut b256_hdr = [0u8; 512];
    b256_hdr[..7].copy_from_slice(b"big.bin");
    b256_hdr[100..108].copy_from_slice(b"0000644\0");
    b256_hdr[124] = 0x80;
    b256_hdr[128..136].copy_from_slice(&u64::MAX.to_be_bytes());
    b256_hdr[156] = b'0';
    b256_hdr[257..263].copy_from_slice(b"ustar\0");
    b256_hdr[263..265].copy_from_slice(b"00");
    b256_hdr[148..156].fill(b' ');
    let sum: u64 = b256_hdr.iter().map(|b| u64::from(*b)).sum();
    b256_hdr[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    assert_invalid("ustar base-256 size=u64::MAX", &b256_hdr, "overflow");

    // 2. R2-M1: Plain file with `12 size=bad\n` and no GNU.sparse.* passes through intact (a5616c8c7 parity).
    let bad_pax_size = pax_record("size", "bad");
    assert_eq!(bad_pax_size, b"12 size=bad\n");
    let plain_bad_size = Tar::new()
        .raw("PaxHeaders/bad-size", b'x', 0o644, &bad_pax_size)
        .file("plain.bin", 0o644, b"RAW")
        .finish();
    assert_eq!(filtered_bytes(&plain_bad_size).unwrap(), plain_bad_size);

    let mut sparse_bad_size_pax = pax_record("GNU.sparse.major", "0");
    sparse_bad_size_pax.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    sparse_bad_size_pax.extend_from_slice(&pax_record("GNU.sparse.realsize", "8192"));
    sparse_bad_size_pax.extend_from_slice(&pax_record("GNU.sparse.map", "0,1"));
    sparse_bad_size_pax.extend_from_slice(&bad_pax_size);
    let sparse_bad_size = Tar::new()
        .raw("PaxHeaders/sparse-bad", b'x', 0o644, &sparse_bad_size_pax)
        .file("sparse.bin", 0o644, b"A")
        .finish();
    assert_invalid("sparse PAX size=bad", &sparse_bad_size, "invalid pax size");

    // 3. R2-M2 + R3: Order-independent PAX chunk count cap aborts in Stage 2 before
    //    `validate_sparse_chunks` would hit `"sparse chunk starts past realsize"`, while
    //    Stage-1 syntax errors (like `,bad,bad`) abort even earlier while scanning `x`.
    let map_with_trailing_past_realsize = "0,1,4096,1,8192,1,32768,1";
    let chunk_err = "PAX sparse chunk count exceeds entry size or limit";

    // 3a. PAX `size` BEFORE `GNU.sparse.map`.
    let mut size_before_map = pax_record("GNU.sparse.major", "0");
    size_before_map.extend_from_slice(&pax_record("size", "2"));
    size_before_map.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    size_before_map.extend_from_slice(&pax_record("GNU.sparse.realsize", "16384"));
    size_before_map.extend_from_slice(&pax_record(
        "GNU.sparse.map",
        map_with_trailing_past_realsize,
    ));
    let tar_before = Tar::new()
        .raw("PaxHeaders/before", b'x', 0o644, &size_before_map)
        .file("m1.bin", 0o644, b"AB")
        .finish();
    assert_invalid("PAX size before map", &tar_before, chunk_err);

    // 3b. PAX `size` AFTER `GNU.sparse.map`.
    let mut size_after_map = pax_record("GNU.sparse.major", "0");
    size_after_map.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    size_after_map.extend_from_slice(&pax_record("GNU.sparse.realsize", "16384"));
    size_after_map.extend_from_slice(&pax_record(
        "GNU.sparse.map",
        map_with_trailing_past_realsize,
    ));
    size_after_map.extend_from_slice(&pax_record("size", "2"));
    let tar_after = Tar::new()
        .raw("PaxHeaders/after", b'x', 0o644, &size_after_map)
        .file("m2.bin", 0o644, b"AB")
        .finish();
    assert_invalid("PAX size after map", &tar_after, chunk_err);

    // 3c. No PAX `size` (falls back to ustar header size = 2) still aborts before
    //     `validate_sparse_chunks` sees `(32768, 1)` past `realsize=16384`.
    let mut no_pax_size = pax_record("GNU.sparse.major", "0");
    no_pax_size.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    no_pax_size.extend_from_slice(&pax_record("GNU.sparse.realsize", "16384"));
    no_pax_size.extend_from_slice(&pax_record(
        "GNU.sparse.map",
        map_with_trailing_past_realsize,
    ));
    let tar_no_pax_size = Tar::new()
        .raw("PaxHeaders/nosize", b'x', 0o644, &no_pax_size)
        .file("m3.bin", 0o644, b"AB")
        .finish();
    assert_invalid("PAX no size (ustar fallback)", &tar_no_pax_size, chunk_err);

    // 3d. PAX 0.0 offset/numbytes pairs with `size` AFTER pairs and trailing chunk past realsize.
    let mut pax00_size_after = pax_record("GNU.sparse.major", "0");
    pax00_size_after.extend_from_slice(&pax_record("GNU.sparse.minor", "0"));
    pax00_size_after.extend_from_slice(&pax_record("GNU.sparse.realsize", "16384"));
    for off in [0u64, 4096, 8192, 32768] {
        pax00_size_after.extend_from_slice(&pax_record("GNU.sparse.offset", &off.to_string()));
        pax00_size_after.extend_from_slice(&pax_record("GNU.sparse.numbytes", "1"));
    }
    pax00_size_after.extend_from_slice(&pax_record("size", "2"));
    let tar_pax00 = Tar::new()
        .raw("PaxHeaders/pax00", b'x', 0o644, &pax00_size_after)
        .file("m4.bin", 0o644, b"AB")
        .finish();
    assert_invalid("PAX 0.0 size after chunks", &tar_pax00, chunk_err);

    // 3e. Stage-1 syntax/numeric validation rejects malformed sparse map/offset immediately.
    let mut stage1_bad_map = pax_record("GNU.sparse.major", "0");
    stage1_bad_map.extend_from_slice(&pax_record("GNU.sparse.map", "0,1,4096,1,8192,1,bad,bad"));
    let tar_stage1_map = Tar::new()
        .raw("PaxHeaders/s1-map", b'x', 0o644, &stage1_bad_map)
        .file("m5.bin", 0o644, b"AB")
        .finish();
    assert_invalid(
        "Stage 1 bad sparse map syntax",
        &tar_stage1_map,
        "invalid sparse map offset",
    );

    let mut stage1_bad_off = pax_record("GNU.sparse.major", "0");
    stage1_bad_off.extend_from_slice(&pax_record("GNU.sparse.offset", "bad"));
    let tar_stage1_off = Tar::new()
        .raw("PaxHeaders/s1-off", b'x', 0o644, &stage1_bad_off)
        .file("m6.bin", 0o644, b"AB")
        .finish();
    assert_invalid(
        "Stage 1 bad sparse offset",
        &tar_stage1_off,
        "invalid pax GNU.sparse.offset",
    );
}

#[test]
fn sparse_expanded_bytes_are_counted_in_load_capacity_check() {
    use std::net::TcpListener;

    let scratch = Scratch::new("lp176-r1-sparse-cap");
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    home::ensure(&home).unwrap();

    // Stand-in Docker reports small inspected.size (64 KiB) so initial capacity() passes,
    // but streams a sparse file expanding to 512 KiB (docker::need(512 KiB) > 512 KiB free).
    let standin = scratch.base.join("standin");
    fs::create_dir_all(standin.join("images")).unwrap();
    let id = format!("sha256:{}", "9".repeat(64));
    let sparse_tar = build_gnu_sparse_tar("big.bin", 0o644, 0, 512 * 1024, &[(0, b"HEAD")]);
    fs::write(standin.join("export.tar"), &sparse_tar).unwrap();
    let img_json = format!(
        r#"{{"Id":"{id}","RepoTags":["cap/sparse:1"],"Os":"linux","Architecture":"{}","Size":65536,"Config":{{}}}}"#,
        architecture()
    );
    fs::write(standin.join("images/cap_sparse_1.json"), &img_json).unwrap();
    let dbin = standin.join("docker");
    fs::write(
        &dbin,
        r#"#!/bin/sh
D="$(dirname "$0")"
case "$1 $2" in
  "image inspect") cat "$D/images/cap_sparse_1.json"; echo; exit 0 ;;
  "container inspect")
    if [ "$3" = "--size" ]; then echo 65536; exit 0; fi
    echo "No such object" >&2; exit 1 ;;
esac
case "$1" in
  create) echo id ;;
  export) cat "$D/export.tar" ;;
  history) echo 65536 ;;
  rm|ps) ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&dbin, fs::Permissions::from_mode(0o755)).unwrap();

    // Mock worker reporting 512 KiB free so initial 64 KiB check passes,
    // then expanded streaming bytes hit Capacity during send().
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    write_record(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([
            ("name", SdkValue::text("vm1")),
            ("platform", SdkValue::text("lima")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "vm1"),
        &SdkValue::map([
            ("address", SdkValue::text(addr.to_string())),
            ("secret", SdkValue::text("k")),
        ]),
    )
    .unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut writes = 0usize;
        while let Ok((mut stream, _)) = listener.accept() {
            if done_rx.try_recv().is_ok() {
                break;
            }
            if read_frame_within(&mut stream, MAX_HELLO)
                .ok()
                .flatten()
                .is_none()
            {
                continue;
            }
            write_frame(
                &mut stream,
                &SdkValue::map([
                    ("ready", SdkValue::Bool(true)),
                    ("protocol", SdkValue::Int(PROTOCOL)),
                ]),
            )
            .unwrap();
            while let Ok(Some(req)) = read_frame(&mut stream) {
                let args = req.get("args").cloned().unwrap_or(SdkValue::Null);
                let step = args.get_text("step").unwrap_or("");
                if step == "write" {
                    writes += 1;
                }
                let value = match step {
                    "probe" => SdkValue::map([
                        ("architecture", SdkValue::text(architecture())),
                        ("free", SdkValue::Int(512 * 1024)),
                        (
                            "root",
                            SdkValue::text("/layerfs/data/machine/native/run/images"),
                        ),
                        ("present", SdkValue::Bool(false)),
                    ]),
                    _ => SdkValue::Null,
                };
                write_frame(&mut stream, &SdkValue::map([("value", value)])).unwrap();
            }
            if done_rx.try_recv().is_ok() {
                break;
            }
        }
        writes
    });

    let Executor::Worker(mut worker) = place::on_machine(&home, "vm1").unwrap() else {
        panic!("expected worker executor");
    };
    let docker = Docker::at(Some(dbin), None);
    let mut sink = |_: SdkValue| {};
    let mut events = Events::new(&mut sink);
    let err = load(
        &docker,
        &mut events,
        &mut Target::worker(&home, "vm1", &mut worker),
        None,
        "cap/sparse:1",
    )
    .unwrap_err();
    assert_eq!(err.code, "Capacity", "{err:?}");
    let _ = done_tx.send(());
    let _ = std::net::TcpStream::connect(addr);
    let writes = server.join().unwrap();
    assert_eq!(
        writes, 0,
        "capacity check must reject expanded frame before writing it"
    );
}

fn make_raw_header(name: &str, flag: u8, size: u64) -> [u8; 512] {
    let mut hdr = [0u8; 512];
    hdr[..name.len()].copy_from_slice(name.as_bytes());
    let oct = |f: &mut [u8], v: u64| {
        let w = f.len() - 1;
        let t = format!("{v:0w$o}\0");
        f.copy_from_slice(&t.as_bytes()[..f.len()]);
    };
    oct(&mut hdr[100..108], 0o644);
    oct(&mut hdr[108..116], 0);
    oct(&mut hdr[116..124], 0);
    oct(&mut hdr[124..136], size);
    oct(&mut hdr[136..148], 0);
    hdr[156] = flag;
    hdr[257..263].copy_from_slice(b"ustar\0");
    hdr[263..265].copy_from_slice(b"00");
    hdr[148..156].fill(b' ');
    let sum: u64 = hdr.iter().map(|b| u64::from(*b)).sum();
    hdr[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    hdr
}

fn append_raw_entry(buf: &mut Vec<u8>, name: &str, flag: u8, body: &[u8]) {
    buf.extend_from_slice(&make_raw_header(name, flag, body.len() as u64));
    buf.extend_from_slice(body);
    let pad = (512 - body.len() % 512) % 512;
    buf.resize(buf.len() + pad, 0);
}

#[test]
fn r3_bad_sparse_eof_cross_header_caps_and_xattr_headroom() {
    assert_eq!(EXTENDED_TOTAL_MAX, 48 << 20);

    let assert_invalid = |label: &str, input: Vec<u8>, needle: &str| {
        let err = filtered_bytes(&input).expect_err(label);
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::InvalidData,
            "{label}: got {err:?}"
        );
        assert!(
            err.to_string().contains(needle),
            "{label}: expected `{needle}` in `{err}`"
        );
    };

    // 1. Bad sparse PAX metadata followed by zero end blocks OR clean EOF must fail immediately.
    let mut bad_after_sparse = pax_record("GNU.sparse.major", "0");
    bad_after_sparse.extend_from_slice(b"999 bad=record\n");
    let mut bad_sparse_size = pax_record("GNU.sparse.major", "0");
    bad_sparse_size.extend_from_slice(&pax_record("size", "bad"));

    let bad_sparse_cases: [(&str, Vec<u8>, &str); 8] = [
        (
            "bad major",
            pax_record("GNU.sparse.major", "bad"),
            "invalid pax GNU.sparse.major",
        ),
        (
            "bad realsize",
            pax_record("GNU.sparse.realsize", "bad"),
            "invalid pax GNU.sparse.realsize",
        ),
        (
            "bad map item",
            pax_record("GNU.sparse.map", "0,1,bad,1"),
            "invalid sparse map offset",
        ),
        (
            "odd map item",
            pax_record("GNU.sparse.map", "0,1,4096"),
            "odd item count in GNU.sparse.map",
        ),
        (
            "unpaired offset",
            pax_record("GNU.sparse.offset", "0"),
            "GNU.sparse.offset without numbytes",
        ),
        (
            "unpaired numbytes",
            pax_record("GNU.sparse.numbytes", "4"),
            "GNU.sparse.numbytes without offset",
        ),
        (
            "malformed after sparse",
            bad_after_sparse,
            "malformed PAX record after GNU.sparse.*",
        ),
        (
            "bad pax size with sparse",
            bad_sparse_size,
            "invalid pax size",
        ),
    ];

    for (label, body, needle) in bad_sparse_cases {
        let mut with_end_blocks = Vec::new();
        append_raw_entry(&mut with_end_blocks, "PaxHeaders/bad", b'x', &body);
        with_end_blocks.resize(with_end_blocks.len() + 1024, 0);
        assert_invalid(&format!("{label} + zero blocks"), with_end_blocks, needle);

        let mut clean_eof = Vec::new();
        append_raw_entry(&mut clean_eof, "PaxHeaders/bad", b'x', &body);
        assert_invalid(&format!("{label} + clean EOF"), clean_eof, needle);
    }

    // Plain trailing PAX headers followed by end blocks or clean EOF stay tolerated (a5616c8c7 parity).
    for (label, body) in [
        ("plain mtime", pax_record("mtime", "1700000000")),
        ("plain bad size", pax_record("size", "bad")),
    ] {
        let mut with_end_blocks = Vec::new();
        append_raw_entry(&mut with_end_blocks, "PaxHeaders/plain", b'x', &body);
        with_end_blocks.resize(with_end_blocks.len() + 1024, 0);
        assert_eq!(
            filtered_bytes(&with_end_blocks).expect(label),
            vec![0u8; 1024],
            "{label} + zero blocks"
        );

        let mut clean_eof = Vec::new();
        append_raw_entry(&mut clean_eof, "PaxHeaders/plain", b'x', &body);
        assert_eq!(
            filtered_bytes(&clean_eof).expect(label),
            vec![0u8; 1024],
            "{label} + clean EOF"
        );
    }

    // 2. On-demand Reader: consecutive `x` headers exceeding SPARSE_CHUNKS_MAX (1,048,576)
    //    must fail before reading a trailing malformed header (distinct error).
    enum StreamKind {
        SparseChunks,
        ExtendedTotal,
        BadSparseThenMore,
    }

    struct OnDemandFloodReader {
        kind: StreamKind,
        index: usize,
        cur: Vec<u8>,
        pos: usize,
        headers_emitted: usize,
        illegal_tail_read: bool,
    }

    impl OnDemandFloodReader {
        fn new(kind: StreamKind) -> Self {
            Self {
                kind,
                index: 0,
                cur: Vec::new(),
                pos: 0,
                headers_emitted: 0,
                illegal_tail_read: false,
            }
        }

        fn refill(&mut self) -> bool {
            use std::cmp::Ordering;
            self.cur.clear();
            self.pos = 0;
            match self.kind {
                StreamKind::SparseChunks => {
                    // 17 headers * 65,536 chunks = 1,114,112 > SPARSE_CHUNKS_MAX (1,048,576),
                    // with ~600 KiB per header (< 1 MiB) and ~10.3 MiB total (< 16 MiB).
                    match self.index.cmp(&17) {
                        Ordering::Less => {
                            let mut map = String::new();
                            let base = (self.index as u64) * 65_536;
                            for j in 0..65_536u64 {
                                if j > 0 {
                                    map.push(',');
                                }
                                use std::fmt::Write as _;
                                let _ = write!(map, "{},1", (base + j) * 2);
                            }
                            let mut body = pax_record("GNU.sparse.major", "0");
                            body.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
                            body.extend_from_slice(&pax_record("GNU.sparse.map", &map));
                            append_raw_entry(&mut self.cur, "PaxHeaders/flood", b'x', &body);
                            self.index += 1;
                            self.headers_emitted += 1;
                            true
                        }
                        Ordering::Equal => {
                            self.illegal_tail_read = true;
                            let mut bad = [0u8; 512];
                            bad[..8].copy_from_slice(b"tail.bin");
                            bad[124..136].copy_from_slice(b"zzzzzzzzzzz\0");
                            bad[156] = b'0';
                            self.cur.extend_from_slice(&bad);
                            self.index += 1;
                            true
                        }
                        Ordering::Greater => false,
                    }
                }
                StreamKind::ExtendedTotal => {
                    // 64 headers * (512 + 786,944) = 50,397,184 > 48 MiB (50,331,648);
                    // 63 headers = 49,609,728 <= 48 MiB.
                    match self.index.cmp(&64) {
                        Ordering::Less => {
                            let blob = "x".repeat(768 * 1024);
                            let body = pax_record("SCHILY.xattr.user.pad", &blob);
                            append_raw_entry(&mut self.cur, "PaxHeaders/xattr-flood", b'x', &body);
                            self.index += 1;
                            self.headers_emitted += 1;
                            true
                        }
                        Ordering::Equal => {
                            self.illegal_tail_read = true;
                            let mut bad = [0u8; 512];
                            bad[..8].copy_from_slice(b"tail.bin");
                            bad[124..136].copy_from_slice(b"zzzzzzzzzzz\0");
                            bad[156] = b'0';
                            self.cur.extend_from_slice(&bad);
                            self.index += 1;
                            true
                        }
                        Ordering::Greater => false,
                    }
                }
                StreamKind::BadSparseThenMore => {
                    if self.index == 0 {
                        let body = pax_record("GNU.sparse.major", "bad");
                        append_raw_entry(&mut self.cur, "PaxHeaders/bad", b'x', &body);
                        self.index += 1;
                        self.headers_emitted += 1;
                        true
                    } else if self.index < 10 {
                        self.illegal_tail_read = true;
                        let body = pax_record("mtime", "1700000000");
                        append_raw_entry(&mut self.cur, "PaxHeaders/next", b'x', &body);
                        self.index += 1;
                        self.headers_emitted += 1;
                        true
                    } else {
                        false
                    }
                }
            }
        }
    }

    impl Read for OnDemandFloodReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.cur.len() && !self.refill() {
                return Ok(0);
            }
            let n = (self.cur.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.cur[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    let mut chunk_flood_reader = OnDemandFloodReader::new(StreamKind::SparseChunks);
    let mut chunk_flood_filtered = Filtered::new(&mut chunk_flood_reader);
    let err = chunk_flood_filtered
        .read_to_end(&mut Vec::new())
        .expect_err("consecutive x headers exceeding SPARSE_CHUNKS_MAX must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string()
            .contains("PAX sparse chunk count exceeds entry size or limit"),
        "expected hard chunk cap error, got: {err}"
    );
    assert!(
        !chunk_flood_reader.illegal_tail_read,
        "must hit SPARSE_CHUNKS_MAX before reading trailing illegal header"
    );
    assert_eq!(chunk_flood_reader.headers_emitted, 17);

    let mut bad_first_reader = OnDemandFloodReader::new(StreamKind::BadSparseThenMore);
    let mut bad_first_filtered = Filtered::new(&mut bad_first_reader);
    let err = bad_first_filtered
        .read_to_end(&mut Vec::new())
        .expect_err("bad sparse x header must abort before buffering trailing headers");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("invalid pax GNU.sparse.major"));
    assert!(
        !bad_first_reader.illegal_tail_read,
        "must reject on first bad sparse x header without reading second header"
    );

    // 3. Per-entry extended header total byte cap (48 MiB) catches both multi-header xattr floods
    //    and zero-length empty `x` header floods without unbounded buffering.
    let mut ext_flood_reader = OnDemandFloodReader::new(StreamKind::ExtendedTotal);
    let mut ext_flood_filtered = Filtered::new(&mut ext_flood_reader);
    let err = ext_flood_filtered
        .read_to_end(&mut Vec::new())
        .expect_err("extended headers exceeding 48 MiB must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("extended headers exceed 48 MiB"),
        "unexpected error for extended total cap: {err}"
    );
    assert!(
        !ext_flood_reader.illegal_tail_read,
        "must hit extended total cap before reading trailing illegal header"
    );
    assert_eq!(ext_flood_reader.headers_emitted, 64);

    struct EmptyHeaderFlood {
        emitted: usize,
        offset: usize,
    }
    impl Read for EmptyHeaderFlood {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            // Each zero-length `x` header adds 512 bytes to total extended budget ->
            // 98,304 headers == 48 MiB; the 98,305th exceeds 48 MiB.
            let hdr = make_raw_header("PaxHeaders/empty", b'x', 0);
            let mut n = 0;
            while n < buf.len() && self.emitted < 105_000 {
                let take = (512 - self.offset).min(buf.len() - n);
                buf[n..n + take].copy_from_slice(&hdr[self.offset..self.offset + take]);
                n += take;
                self.offset += take;
                if self.offset == 512 {
                    self.offset = 0;
                    self.emitted += 1;
                }
            }
            Ok(n)
        }
    }
    let mut empty_flood = EmptyHeaderFlood {
        emitted: 0,
        offset: 0,
    };
    let err = Filtered::new(&mut empty_flood)
        .read_to_end(&mut Vec::new())
        .expect_err("empty extended header flood must hit total cap");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("extended headers exceed 48 MiB"));
    assert_eq!(empty_flood.emitted, ((48 << 20) / 512) + 1);

    // 4. Realistic container entry with long path/linkpath and heavy SCHILY.xattr.* set passes
    //    both filtering and rootfs::ensure_flat without being rejected.
    let long_file_path = format!(
        "var/lib/containers/storage/overlay/{}/{}/diff/usr/libexec/app",
        "p".repeat(130),
        "q".repeat(130)
    );
    let long_link_path = format!(
        "var/lib/containers/storage/overlay/{}/{}/diff/usr/bin/app-link",
        "l".repeat(130),
        "m".repeat(130)
    );
    let long_link_target = format!("../libexec/{}/{}/app", "t".repeat(120), "u".repeat(120));
    let mut xattrs_a = pax_record("path", &long_file_path);
    xattrs_a.extend_from_slice(&pax_record("mtime", "1700000123.5"));
    xattrs_a.extend_from_slice(&pax_record(
        "SCHILY.xattr.security.capability",
        "cap_net_bind",
    ));
    xattrs_a.extend_from_slice(&pax_record(
        "SCHILY.xattr.security.selinux",
        "system_u:object_r:container_file_t:s0:c123,c456",
    ));
    let mut xattrs_b = Vec::new();
    let xattr_val = "v".repeat(600);
    for i in 0..200 {
        xattrs_b.extend_from_slice(&pax_record(
            &format!("SCHILY.xattr.user.attr_{i:03}"),
            &xattr_val,
        ));
    }
    assert!(xattrs_a.len() + xattrs_b.len() > 120_000);
    let g_hdr = pax_record("comment", "oci-export");
    let mut x_sym = pax_record("path", &long_link_path);
    x_sym.extend_from_slice(&pax_record("linkpath", &long_link_target));

    let realistic_tar = Tar::new()
        .raw("PaxHeaders/global", b'g', 0o644, &g_hdr)
        .raw("PaxHeaders/x1", b'x', 0o644, &xattrs_a)
        .raw("PaxHeaders/x2", b'x', 0o644, &xattrs_b)
        .file("placeholder-bin", 0o755, b"ELF-mock-binary\n")
        .raw("PaxHeaders/sym", b'x', 0o644, &x_sym)
        .symlink("placeholder-sym", "target")
        .finish();
    let filtered_realistic = filtered_bytes(&realistic_tar).expect("realistic xattr entry allowed");
    assert_eq!(filtered_realistic, realistic_tar);

    let scratch = Scratch::new("lp176-r3-xattr-headroom");
    let tar_path = scratch.base.join("realistic.tar");
    fs::write(&tar_path, &filtered_realistic).unwrap();
    let (root, _) = rootfs::ensure_flat(
        &tar_path,
        &Image::parse(&format!("sha256:{}", "e".repeat(64))).unwrap(),
        &scratch.cache,
    )
    .unwrap();
    assert_eq!(
        fs::read(root.directory().join(&long_file_path)).unwrap(),
        b"ELF-mock-binary\n"
    );
    assert_eq!(
        fs::read_link(root.directory().join(&long_link_path))
            .unwrap()
            .to_string_lossy(),
        long_link_target
    );
}

#[test]
fn r4_m1_cross_header_pax_size_override_and_final_validation() {
    let scratch = Scratch::new("lp176-r4-m1");
    let mut base_meta = pax_record("GNU.sparse.major", "0");
    base_meta.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
    base_meta.extend_from_slice(&pax_record("GNU.sparse.realsize", "8192"));
    base_meta.extend_from_slice(&pax_record("GNU.sparse.map", "0,1"));

    let mut single_meta = base_meta.clone();
    single_meta.extend_from_slice(&pax_record("size", "1"));

    // 1. Success cases: bad or overflowing PAX `size` in first `x`, overridden by `size=1` in second `x`.
    for (idx, (label, first_size, name, byte)) in [
        ("overflow then 1", u64::MAX.to_string(), "sp1.bin", b'A'),
        ("bad then 1", "bad".to_owned(), "sp2.bin", b'B'),
    ]
    .into_iter()
    .enumerate()
    {
        let mut x1 = base_meta.clone();
        x1.extend_from_slice(&pax_record("size", &first_size));
        let x2 = pax_record("size", "1");
        let input = Tar::new()
            .raw("PaxHeaders/x1", b'x', 0o644, &x1)
            .raw("PaxHeaders/x2", b'x', 0o644, &x2)
            .file(name, 0o644, &[byte])
            .finish();
        let single = Tar::new()
            .raw("PaxHeaders/single", b'x', 0o644, &single_meta)
            .file(name, 0o644, &[byte])
            .finish();

        let (filtered, report) = filtered_with_report(&input).expect(label);
        let expected_single = filtered_bytes(&single).expect("single header baseline");
        assert_eq!(filtered, expected_single, "{label}: single-header parity");
        assert_eq!(report.sparse_files.len(), 1, "{label}");
        assert_eq!(report.sparse_files[0].expanded_bytes, 8192, "{label}");
        assert_eq!(report.sparse_files[0].stored_bytes, 1, "{label}");

        let tar_path = scratch.base.join(format!("ok-{idx}.tar"));
        fs::write(&tar_path, &filtered).unwrap();
        let (root, _) = rootfs::ensure_flat(
            &tar_path,
            &Image::parse(&format!("sha256:{}", format!("{idx}").repeat(64))).unwrap(),
            &scratch.cache,
        )
        .unwrap();
        let mut expected = vec![0u8; 8192];
        expected[0] = byte;
        assert_eq!(fs::read(root.directory().join(name)).unwrap(), expected);

        // Trailing sparse `x` pair ending in `size=1` followed by zero end-blocks or clean EOF succeeds.
        let mut with_end = Vec::new();
        append_raw_entry(&mut with_end, "PaxHeaders/x1", b'x', &x1);
        append_raw_entry(&mut with_end, "PaxHeaders/x2", b'x', &x2);
        with_end.resize(with_end.len() + 1024, 0);
        assert_eq!(
            filtered_bytes(&with_end).expect("overridden size + end blocks"),
            vec![0u8; 1024]
        );

        let mut clean_eof = Vec::new();
        append_raw_entry(&mut clean_eof, "PaxHeaders/x1", b'x', &x1);
        append_raw_entry(&mut clean_eof, "PaxHeaders/x2", b'x', &x2);
        assert_eq!(
            filtered_bytes(&clean_eof).expect("overridden size + clean EOF"),
            vec![0u8; 1024]
        );
    }

    // 2. Rejection cases: valid `size=1` in first `x`, bad or overflowing `size` in second `x`
    //    must fail both before a real entry header and at zero end-blocks / clean EOF.
    let assert_invalid = |label: &str, input: Vec<u8>, needle: &str| {
        let err = filtered_bytes(&input).expect_err(label);
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::InvalidData,
            "{label}: got {err:?}"
        );
        assert!(
            err.to_string().contains(needle),
            "{label}: expected `{needle}` in `{err}`"
        );
    };

    for (label, second_size, needle) in [
        ("1 then bad", "bad".to_owned(), "invalid pax size"),
        ("1 then overflow", u64::MAX.to_string(), "overflow"),
    ] {
        let x2 = pax_record("size", &second_size);

        let with_entry = Tar::new()
            .raw("PaxHeaders/x1", b'x', 0o644, &single_meta)
            .raw("PaxHeaders/x2", b'x', 0o644, &x2)
            .file("sp.bin", 0o644, b"A")
            .finish();
        assert_invalid(&format!("{label} + entry"), with_entry, needle);

        let mut with_end = Vec::new();
        append_raw_entry(&mut with_end, "PaxHeaders/x1", b'x', &single_meta);
        append_raw_entry(&mut with_end, "PaxHeaders/x2", b'x', &x2);
        with_end.resize(with_end.len() + 1024, 0);
        assert_invalid(&format!("{label} + end blocks"), with_end, needle);

        let mut clean_eof = Vec::new();
        append_raw_entry(&mut clean_eof, "PaxHeaders/x1", b'x', &single_meta);
        append_raw_entry(&mut clean_eof, "PaxHeaders/x2", b'x', &x2);
        assert_invalid(&format!("{label} + clean EOF"), clean_eof, needle);
    }
}

#[test]
fn r4_m2_long_digit_sparse_map_above_16mib_succeeds() {
    fn push_u64(dst: &mut Vec<u8>, mut n: u64) {
        if n == 0 {
            dst.push(b'0');
            return;
        }
        let mut digits = [0u8; 20];
        let mut i = digits.len();
        while n > 0 {
            i -= 1;
            digits[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        dst.extend_from_slice(&digits[i..]);
    }

    fn push_u64_padded11(dst: &mut Vec<u8>, mut n: u64) {
        let mut digits = [b'0'; 11];
        for slot in digits.iter_mut().rev() {
            *slot = b'0' + (n % 10) as u8;
            n /= 10;
        }
        dst.extend_from_slice(&digits);
    }

    fn pax_record_bytes(key: &str, value: &[u8]) -> Vec<u8> {
        let body_len = 1 + key.len() + 1 + value.len() + 1;
        let mut digits = 1usize;
        let total = loop {
            let candidate = digits + body_len;
            let d = candidate.to_string().len();
            if d == digits {
                break candidate;
            }
            digits = d;
        };
        let mut out = Vec::with_capacity(total);
        push_u64(&mut out, total as u64);
        out.push(b' ');
        out.extend_from_slice(key.as_bytes());
        out.push(b'=');
        out.extend_from_slice(value);
        out.push(b'\n');
        out
    }

    enum MapShape {
        Review4HugeRealsize,
        CompactEof,
    }

    struct LongDigitSparseTarReader {
        shape: MapShape,
        headers: usize,
        per_hdr: usize,
        payload_bytes: u64,
        real_size: u64,
        stage: usize,
        cur: Vec<u8>,
        pos: usize,
        payload_left: u64,
        pad_left: usize,
        tail_left: usize,
        extended_bytes: u64,
    }

    impl LongDigitSparseTarReader {
        fn new(shape: MapShape) -> Self {
            let (payload_bytes, real_size) = match shape {
                MapShape::Review4HugeRealsize => (1_000_000_000, 12_000_000_000),
                MapShape::CompactEof => (1_000_000, 2_000_000),
            };
            let pad_left = ((512 - (payload_bytes % 512)) % 512) as usize;
            Self {
                shape,
                headers: 32,
                per_hdr: 31_250,
                payload_bytes,
                real_size,
                stage: 0,
                cur: Vec::new(),
                pos: 0,
                payload_left: payload_bytes,
                pad_left,
                tail_left: 1024,
                extended_bytes: 0,
            }
        }

        fn build_next(&mut self) -> bool {
            self.cur.clear();
            self.pos = 0;
            match self.stage.cmp(&self.headers) {
                std::cmp::Ordering::Less => {
                    let mut map = Vec::with_capacity(self.per_hdr * 17);
                    let base = (self.stage as u64) * (self.per_hdr as u64);
                    for j in 0..self.per_hdr as u64 {
                        if j > 0 {
                            map.push(b',');
                        }
                        match self.shape {
                            MapShape::Review4HugeRealsize => {
                                push_u64(&mut map, 10_000_000_000u64 + 2_000u64 * (base + j));
                                map.extend_from_slice(b",1000");
                            }
                            MapShape::CompactEof => {
                                push_u64_padded11(&mut map, 2u64 * (base + j));
                                map.extend_from_slice(b",0001");
                            }
                        }
                    }
                    let mut body = Vec::with_capacity(map.len() + 256);
                    if self.stage == 0 {
                        body.extend_from_slice(&pax_record("GNU.sparse.major", "0"));
                        body.extend_from_slice(&pax_record("GNU.sparse.minor", "1"));
                        body.extend_from_slice(&pax_record("GNU.sparse.name", "big-sparse.bin"));
                        body.extend_from_slice(&pax_record(
                            "GNU.sparse.realsize",
                            &self.real_size.to_string(),
                        ));
                        body.extend_from_slice(&pax_record(
                            "size",
                            &self.payload_bytes.to_string(),
                        ));
                    }
                    body.extend_from_slice(&pax_record_bytes("GNU.sparse.map", &map));
                    append_raw_entry(&mut self.cur, "PaxHeaders/long-map", b'x', &body);
                    self.extended_bytes += self.cur.len() as u64;
                    self.stage += 1;
                    true
                }
                std::cmp::Ordering::Equal => {
                    let hdr = make_raw_header("placeholder", b'0', self.payload_bytes);
                    self.cur.extend_from_slice(&hdr);
                    self.stage += 1;
                    true
                }
                std::cmp::Ordering::Greater => false,
            }
        }
    }

    impl Read for LongDigitSparseTarReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos < self.cur.len() {
                let n = (self.cur.len() - self.pos).min(buf.len());
                buf[..n].copy_from_slice(&self.cur[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if self.stage <= self.headers && self.build_next() {
                let n = self.cur.len().min(buf.len());
                buf[..n].copy_from_slice(&self.cur[..n]);
                self.pos = n;
                return Ok(n);
            }
            if self.payload_left > 0 {
                let n = (buf.len() as u64).min(self.payload_left) as usize;
                let fill = match self.shape {
                    MapShape::Review4HugeRealsize => b'A',
                    MapShape::CompactEof => b'Z',
                };
                buf[..n].fill(fill);
                self.payload_left -= n as u64;
                return Ok(n);
            }
            if self.pad_left > 0 {
                let n = buf.len().min(self.pad_left);
                buf[..n].fill(0);
                self.pad_left -= n;
                return Ok(n);
            }
            if self.tail_left > 0 {
                let n = buf.len().min(self.tail_left);
                buf[..n].fill(0);
                self.tail_left -= n;
                return Ok(n);
            }
            Ok(0)
        }
    }

    // Part A: Exact Review-4 counterexample (1,000,000 11-digit/4-digit items across 32 `x`
    // headers, ~17.0 MB > 16 MiB, realsize=12 GB, stored=1 GB). Reading the first 64 KiB
    // exercises all Stage-1 & Stage-2 validation over all 32 extended headers and 1M chunks.
    let mut source_a = LongDigitSparseTarReader::new(MapShape::Review4HugeRealsize);
    let mut filtered_a = Filtered::new(&mut source_a);
    let mut head = Vec::with_capacity(64 * 1024);
    let mut scratch_buf = [0u8; 16 * 1024];
    while head.len() < 64 * 1024 {
        let n = filtered_a
            .read(&mut scratch_buf)
            .expect("17 MB PAX map must pass Stage 1 & Stage 2");
        assert!(n > 0);
        head.extend_from_slice(&scratch_buf[..n]);
    }
    assert_eq!(head[0..512][156], b'x');
    assert_eq!(head[1024..1536][156], b'0');
    assert_eq!(head[1024 + 124], 0x80);
    assert_eq!(
        u64::from_be_bytes(head[1024 + 128..1024 + 136].try_into().unwrap()),
        12_000_000_000
    );
    assert!(head[1536..].iter().all(|b| *b == 0));
    let report_a = filtered_a.report();
    assert_eq!(report_a.sparse_files.len(), 1);
    assert_eq!(report_a.sparse_files[0].path, "big-sparse.bin");
    assert_eq!(report_a.sparse_files[0].expanded_bytes, 12_000_000_000);
    assert_eq!(report_a.sparse_files[0].stored_bytes, 1_000_000_000);
    assert_eq!(report_a.sparse_expanded_bytes, 12_000_000_000);
    drop(filtered_a);
    assert!(source_a.extended_bytes > (16 << 20));
    assert!(source_a.extended_bytes < (48 << 20));

    // Part B: Complete start-to-EOF streaming on a >16 MiB, 1M-descriptor long-digit PAX 0.1
    // archive (11-digit offsets, 4-digit lengths, compact 2 MB realsize) verifying end-to-end
    // payload bytes, padding, end-blocks, and report totals.
    let mut source_b = LongDigitSparseTarReader::new(MapShape::CompactEof);
    let mut filtered_b = Filtered::new(&mut source_b);
    let mut out_b = Vec::new();
    filtered_b
        .read_to_end(&mut out_b)
        .expect("complete 17 MB PAX 0.1 stream must succeed to EOF");
    let out_pad = 512 - (2_000_000usize % 512);
    assert_eq!(out_b.len(), 1536 + 2_000_000 + out_pad + 1024);
    assert_eq!(out_b[1024 + 156], b'0');
    for (i, &byte) in out_b[1536..1536 + 2_000_000].iter().enumerate() {
        let expected = if i % 2 == 0 { b'Z' } else { 0 };
        if byte != expected {
            panic!("mismatch at expanded offset {i}: got {byte}, expected {expected}");
        }
    }
    assert!(out_b[1536 + 2_000_000..].iter().all(|b| *b == 0));
    let report_b = filtered_b.into_report();
    assert_eq!(report_b.sparse_files.len(), 1);
    assert_eq!(report_b.sparse_files[0].path, "big-sparse.bin");
    assert_eq!(report_b.sparse_files[0].expanded_bytes, 2_000_000);
    assert_eq!(report_b.sparse_files[0].stored_bytes, 1_000_000);
    assert!(source_b.extended_bytes > (16 << 20));
    assert!(source_b.extended_bytes < (48 << 20));
}

#[test]
fn helper_pause_load() {
    let Ok(home) = std::env::var("LP176_REC_PAUSE_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let machine = std::env::var("LP176_REC_PAUSE_MACHINE").unwrap();
    let image = std::env::var("LP176_REC_PAUSE_IMAGE").unwrap();
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
    let _ = load(
        &docker,
        &mut events,
        &mut Target::local(&home, &machine),
        None,
        &image,
    );
}

#[test]
fn helper_hold_image_lock() {
    let Ok(home) = std::env::var("LP176_REC_LOCK_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let machine = std::env::var("LP176_REC_LOCK_MACHINE").unwrap();
    let image = std::env::var("LP176_REC_LOCK_IMAGE").unwrap();
    let mut sink = |_: SdkValue| {};
    let _held = ImageLock::take(&home, &machine, &image, &mut Events::new(&mut sink)).unwrap();
    fs::write(home.join("lock-held"), b"").unwrap();
    for _ in 0..120 {
        std::thread::sleep(Duration::from_secs(1));
    }
}
