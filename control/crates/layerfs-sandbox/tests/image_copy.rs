//! Copying an image from (a stand-in) Docker into a machine's image cache,
//! on the local side: resolve, refuse, export, send, unpack once, reuse,
//! resume; then prune.
#[allow(dead_code)]
#[path = "support/image.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/standin.rs"]
mod standin;
use fixture::{Scratch, Tar};
use layerfs_home::events::Events;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::deploy::Side;
use layerfs_sandbox::image_artifact::{self as cache, architecture, Sidecar};
use layerfs_sandbox::image_load::{load, Loaded, Target};
use layerfs_sandbox::image_tar::Filtered;
use layerfs_sandbox::images;
use layerfs_sandbox::{rootfs, Image};
use standin::Standin;
use std::fs;
use std::io::Read;
use std::path::Path;

const ID: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const HEX: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn export() -> Vec<u8> {
    Tar::new()
        .file(".dockerenv", 0o755, b"")
        .directory("etc/", 0o755)
        .file("etc/hostname", 0o644, b"abc\n")
        .file("etc/os-release", 0o644, b"ID=debian\n")
        .directory("usr/", 0o755)
        .directory("usr/bin/", 0o755)
        .file("usr/bin/psql", 0o755, &vec![7u8; 3000])
        .finish()
}

fn filtered(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    Filtered::new(bytes).read_to_end(&mut out).unwrap();
    out
}

struct Run {
    phases: Vec<SdkValue>,
}

fn run(
    standin: &Standin,
    home: &Path,
    image: &str,
) -> (Result<Loaded, layerfs_protocol::sdk::SdkRefusal>, Run) {
    let mut phases = Vec::new();
    let mut sink = |event: SdkValue| {
        if let Some(p) = event.get("progress") {
            phases.push(p.clone());
        }
    };
    let mut events = Events::new(&mut sink);
    let out = load(
        &standin.docker(),
        &mut events,
        &mut Target::local(home, layerfs_home::names::HOST_MACHINE),
        None,
        image,
    );
    (out, Run { phases })
}

fn named(run: &Run, phase: &str) -> Vec<SdkValue> {
    run.phases
        .iter()
        .filter(|p| p.get_text("phase") == Some(phase))
        .cloned()
        .collect()
}

fn setup(name: &str) -> (Scratch, Standin, std::path::PathBuf) {
    let scratch = Scratch::new(name);
    let standin = Standin::new(&scratch.base.join("standin"));
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    // the home is opened, as every call opens it before a copy: a folder
    // holding a lock record and no `layerfs.json` is not a 2.0 home
    layerfs_home::home::open(&home).unwrap();
    standin.image(ID, &["demo/pg:17"], ("linux", architecture()), 4000);
    standin.export(&export());
    (scratch, standin, home)
}

#[test]
fn a_reference_is_copied_once_then_reused_with_nothing_sent() {
    let (_scratch, standin, home) = setup("copy-once");
    let (first, run1) = run(&standin, &home, "demo/pg:17");
    let first = first.unwrap();
    assert_eq!(first.id, ID);
    assert!(!first.reused);
    assert_eq!(first.size, Some(filtered(&export()).len() as u64));
    // what the copy left out is in its answer
    let report = first.report.as_ref().expect("a sent copy reports");
    assert_eq!(report.init, vec![".dockerenv", "etc/hostname"]);
    let answer = first.to_value();
    let dropped = answer.get("dropped").expect("dropped");
    assert_eq!(
        cache::texts(dropped.get("init")),
        vec![".dockerenv", "etc/hostname"]
    );
    assert!(cache::texts(dropped.get("fifos")).is_empty());
    assert_eq!(
        answer
            .get("sparse")
            .and_then(|s| s.get_int("expanded_bytes")),
        Some(0)
    );
    let sends = named(&run1, "send");
    let last = sends.last().expect("send events");
    assert_eq!(
        last.get_int("done"),
        first.size.map(|n| n as i64),
        "the last send is exact"
    );
    if let Some(total) = last.get_int("total") {
        assert_eq!(Some(total), last.get_int("done"));
    }
    assert_eq!(named(&run1, "unpack").len(), 1);
    let dir = cache::cache(&home);
    let root = dir.join(format!("{HEX}.2"));
    assert_eq!(
        fs::read(root.join("etc/os-release")).unwrap(),
        b"ID=debian\n"
    );
    assert!(!root.join(".dockerenv").exists());
    assert!(!root.join("etc/hostname").exists());
    assert!(
        !dir.join(format!(".recv-{HEX}.part")).exists(),
        "the staged tar is deleted"
    );
    let sidecar = Sidecar::read(&dir, HEX).unwrap().unwrap();
    assert_eq!(sidecar.refs, vec!["demo/pg:17".to_owned()]);
    assert_eq!(sidecar.source, "imported");
    assert!(sidecar.config.get("Env").is_some());
    assert_eq!(standin.exports(), 1);
    // again by reference, and by Id: nothing exported or sent
    for image in ["demo/pg:17", ID] {
        let (again, run) = run(&standin, &home, image);
        let again = again.unwrap();
        assert!(again.reused, "{image}");
        assert!(again.report.is_none() && again.to_value().get("dropped").is_none());
        assert!(named(&run, "send").is_empty(), "{image}");
        assert_eq!(
            again.size, first.size,
            "a reused load answers the sidecar's size: {image}"
        );
    }
    assert_eq!(standin.exports(), 1);
    // without Docker, a cached image is still found by its ref
    let mut phases = Vec::new();
    let mut sink = |e: SdkValue| phases.push(e);
    let none = layerfs_sandbox::docker::Docker::at(None, None);
    let found = load(
        &none,
        &mut Events::new(&mut sink),
        &mut Target::local(&home, layerfs_home::names::HOST_MACHINE),
        None,
        "demo/pg:17",
    )
    .unwrap();
    assert!(found.reused);
    assert_eq!(found.id, ID);
    assert_eq!(found.size, first.size);
}

#[test]
fn refusals_carry_their_detail() {
    let (_scratch, standin, home) = setup("copy-refusals");
    let other = if architecture() == "arm64" {
        "amd64"
    } else {
        "arm64"
    };
    let wrong = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
    standin.image(wrong, &["demo/x86:1"], ("linux", other), 10);
    let error = run(&standin, &home, "demo/x86:1").0.unwrap_err();
    assert_eq!(error.code, "Unsupported");
    assert_eq!(error.detail.get_text("architecture"), Some(other));
    let error = run(&standin, &home, "demo/none:1").0.unwrap_err();
    assert_eq!(error.code, "InvalidInput", "{}", error.message);
    let none = layerfs_sandbox::docker::Docker::at(None, None);
    let mut sink = |_| {};
    let error = load(
        &none,
        &mut Events::new(&mut sink),
        &mut Target::local(&home, layerfs_home::names::HOST_MACHINE),
        None,
        "demo/pg:17",
    )
    .unwrap_err();
    assert_eq!(error.code, "Unavailable");
    assert_eq!(error.detail.get_text("what"), Some("docker"));
    // larger than twice what the machine has free: refused before anything
    // exists, and the container's root is not what decides it
    standin.image(ID, &["demo/pg:17"], ("linux", architecture()), u64::MAX / 8);
    let error = run(&standin, &home, "demo/pg:17").0.unwrap_err();
    assert_eq!(error.code, "Capacity");
    let needed = error.detail.get_int("needed_bytes").unwrap();
    let free = error.detail.get_int("free_bytes").unwrap();
    assert!(needed > free, "{needed} {free}");
    assert_eq!(
        needed as u64,
        layerfs_sandbox::docker::need(u64::MAX / 8),
        "a fresh copy needs the whole unpacked size"
    );
    assert_eq!(
        error.detail.get_text("path"),
        Some(cache::cache(&home).to_string_lossy().as_ref()),
        "the refusal names the filesystem it measured"
    );
    assert_eq!(
        error.detail.get_text("fix"),
        Some("layerfs machine prune native --unused"),
        "the refusal says how to free room"
    );
    assert!(
        !standin.dir.join("containers").exists(),
        "no export container is created"
    );
    assert_eq!(standin.exports(), 0, "no byte is exported");
    assert!(!cache::cache(&home)
        .join(format!(".recv-{HEX}.part"))
        .exists());
    assert!(!cache::cache(&home).join(format!("{HEX}.2")).exists());
}

#[test]
fn need_counts_what_the_layers_unpack_to() {
    // containerd's image store: inspect's Size (4000) is the compressed
    // content; the layers unpack to far more than the machine has free
    let (_scratch, standin, home) = setup("copy-need-layers");
    let layers = [u64::MAX / 32, 0, u64::MAX / 32];
    standin.layers(ID, &layers);
    let error = run(&standin, &home, "demo/pg:17").0.unwrap_err();
    assert_eq!(error.code, "Capacity", "{}", error.message);
    let needed = layerfs_sandbox::docker::need(layers.iter().sum());
    assert_eq!(error.detail.get_int("needed_bytes"), Some(needed as i64));
}

#[test]
fn the_last_send_is_done_equals_total_whatever_the_estimate() {
    // an export of several 1 MiB frames and a partial one
    let big = Tar::new()
        .directory("usr/", 0o755)
        .file("usr/lib.so", 0o644, &vec![5u8; 5 * 1024 * 1024 / 2])
        .file("usr/tail", 0o644, &vec![6u8; 777])
        .finish();
    let sent = filtered(&big).len() as u64;
    assert!(sent > cache::CHUNK as u64 * 2);
    // (root size, layer sizes, inspect's Size): an estimate over, under,
    // from the layers, from inspect alone, and none at all
    let cases: [(&str, u64, Option<u64>, u64); 5] = [
        ("over", sent * 3, None, 4000),
        ("under", sent / 2, None, 4000),
        ("layers", 0, Some(sent - 4096), 4000),
        ("inspect", 0, None, 4000),
        ("none", 0, None, 0),
    ];
    for (name, rootfs, layers, size) in cases {
        let (_scratch, standin, home) = setup(&format!("copy-total-{name}"));
        standin.image(ID, &["demo/pg:17"], ("linux", architecture()), size);
        standin.export(&big);
        standin.set_rootfs_size(rootfs);
        if let Some(n) = layers {
            standin.layers(ID, &[n, 0]);
        }
        let (out, run) = run(&standin, &home, "demo/pg:17");
        let loaded = out.unwrap();
        assert_eq!(loaded.size, Some(sent), "{name}");
        let sends = named(&run, "send");
        for event in &sends {
            let (done, total) = (event.get_int("done"), event.get_int("total"));
            assert!(total.is_none_or(|t| Some(t) >= done), "{name}: {event:?}");
        }
        let last = sends.last().expect("send events");
        assert_eq!(last.get_int("done"), Some(sent as i64), "{name}");
        assert_eq!(last.get_int("total"), Some(sent as i64), "{name}");
    }
}

#[test]
fn a_staged_prefix_is_resumed_and_a_mismatched_one_restarted() {
    for (name, matching) in [("copy-resume", true), ("copy-restart", false)] {
        let (_scratch, standin, home) = setup(name);
        let bytes = filtered(&export());
        let staged = if matching {
            bytes[..1024].to_vec()
        } else {
            vec![9u8; 1024]
        };
        cache::write(&cache::cache(&home), HEX, 0, &staged).unwrap();
        // left by a copy that is not running: no lock holds it, so its bytes
        // are resumed whatever their age
        let (out, run) = run(&standin, &home, "demo/pg:17");
        out.unwrap();
        let first = named(&run, "send")[0].get_int("done").unwrap();
        assert_eq!(first, if matching { 1024 } else { 0 }, "{name}");
        let root = cache::cache(&home).join(format!("{HEX}.2"));
        assert_eq!(
            fs::read(root.join("usr/bin/psql")).unwrap(),
            vec![7u8; 3000]
        );
        assert_eq!(standin.exports(), if matching { 1 } else { 2 }, "{name}");
    }
}

/// Plan Part 3 fix 4: a probe hashes the staged bytes only when the copy
/// asks (`hash`), which a load does once, on the probe it resumes from.
#[test]
fn a_probe_hashes_the_staged_bytes_only_when_asked() {
    let scratch = Scratch::new("copy-probe-hash");
    let home = scratch.base.join("home");
    cache::write(&cache::cache(&home), HEX, 0, b"abc").unwrap();
    let probe = |hash: bool| {
        cache::step(
            &Side::Home(&home),
            &SdkValue::map([
                ("step", SdkValue::text("probe")),
                ("id", SdkValue::text(ID)),
                ("hash", SdkValue::Bool(hash)),
            ]),
        )
        .unwrap()
    };
    let plain = probe(false);
    assert_eq!(plain.get_int("offset"), Some(3));
    assert_eq!(plain.get("prefix_sha256"), None, "not hashed unasked");
    let hashed = probe(true);
    assert_eq!(hashed.get_int("offset"), Some(3));
    assert_eq!(
        hashed.get_text("prefix_sha256"),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

/// Plan Part 3 fix 4 and v19: a listing says who holds a copy's lock from
/// the lock's owner record (`ImageLock::holder`, which the item's `busy`
/// state and `op` phase come from) and never takes or tests the kernel
/// lock, so it cannot make a named prune's guard refuse. A guard that
/// writes no record (a prune's) is not seen; a live copy's record is, with
/// the phase the copy last wrote. The worker's raw row of a part carries no
/// `resumable` flag any more.
#[test]
fn a_listing_reads_the_copy_lock_record_and_takes_no_lock() {
    use layerfs_sandbox::image_load::ImageLock;
    let scratch = Scratch::new("copy-list-lock");
    let home = scratch.base.join("home");
    cache::write(&cache::cache(&home), HEX, 0, b"abc").unwrap();
    let part = |home: &Path| {
        let listed =
            images::in_worker(home, SdkValue::map([("machine", SdkValue::text("docker"))]));
        let listed = listed.unwrap();
        let Some(SdkValue::List(rows)) = listed.get("images") else {
            panic!("no images in {listed:?}");
        };
        rows.iter()
            .find(|row| row.get_text("id") == Some(ID) && row.get_bool("staged"))
            .cloned()
            .expect("the staged part is listed")
    };
    let guard = ImageLock::try_hold(&home, "docker", ID).unwrap().unwrap();
    let row = part(&home);
    assert_eq!(row.get_int("size"), Some(3));
    assert_eq!(row.get("resumable"), None, "the flag is gone in v19");
    assert!(
        ImageLock::holder(&home, "docker", HEX).is_none(),
        "a guard with no record is not seen"
    );
    drop(guard);
    let copy = ImageLock::try_take(&home, "docker", ID).unwrap().unwrap();
    part(&home);
    let holder = ImageLock::holder(&home, "docker", HEX).expect("a live copy's record is seen");
    assert_eq!(
        holder.get("phase"),
        None,
        "no phase before the copy names one"
    );
    assert!(holder.get_int("since_ms").is_some());
    copy.at("send");
    let holder = ImageLock::holder(&home, "docker", HEX).unwrap();
    assert_eq!(holder.get_text("phase"), Some("send"));
    assert!(
        ImageLock::try_hold(&home, "docker", ID).unwrap().is_none(),
        "the copy still holds its lock"
    );
    drop(copy);
    part(&home);
    assert!(ImageLock::holder(&home, "docker", HEX).is_none());
}

#[test]
fn frames_out_of_order_are_refused_and_publish_checks_the_bytes() {
    let scratch = Scratch::new("copy-frames");
    let home = scratch.base.join("home");
    let dir = cache::cache(&home);
    cache::write(&dir, HEX, 0, b"abc").unwrap();
    assert!(cache::write(&dir, HEX, 5, b"x").is_err());
    let step = |args: SdkValue| cache::step(&Side::Home(&home), &args);
    let probe = step(SdkValue::map([
        ("step", SdkValue::text("probe")),
        ("id", SdkValue::text(ID)),
    ]))
    .unwrap();
    assert_eq!(probe.get_int("offset"), Some(3));
    assert_eq!(
        probe.get_text("root"),
        Some(cache::cache(&home).to_string_lossy().as_ref()),
        "a probe says which filesystem its free bytes are"
    );
    assert!(!probe.get_bool("present"));
    let wrong = step(SdkValue::map([
        ("step", SdkValue::text("publish")),
        ("id", SdkValue::text(ID)),
        ("size", SdkValue::Int(3)),
        ("sha256", SdkValue::text("00")),
    ]));
    assert_eq!(wrong.unwrap_err().code, "Failed");
    assert!(
        dir.join(format!(".recv-{HEX}.part")).exists(),
        "kept for a resume"
    );
}

#[test]
fn two_concurrent_unpacks_of_one_image_unpack_once() {
    let scratch = Scratch::new("copy-concurrent");
    let tar = scratch.base.join("flat.tar");
    fs::write(&tar, filtered(&export())).unwrap();
    let cache = scratch.base.join("images");
    let image = Image::parse(ID).unwrap();
    let results: Vec<bool> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| rootfs::ensure_flat(&tar, &image, &cache).unwrap().1))
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|u| **u).count(), 1, "{results:?}");
}

#[test]
fn sidecars_round_trip() {
    let scratch = Scratch::new("copy-sidecar");
    let sidecar = Sidecar {
        id: ID.to_owned(),
        refs: vec!["a:1".into(), "b:2".into()],
        size: Some(42),
        architecture: Some("arm64".into()),
        source: "imported".into(),
        config: SdkValue::map([("User", SdkValue::text("postgres"))]),
        last_used: Some(cache::utc(0)),
    };
    sidecar.write(&scratch.base).unwrap();
    assert_eq!(Sidecar::read(&scratch.base, HEX).unwrap(), Some(sidecar));
    assert_eq!(cache::utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(cache::utc(1_794_649_800_000), "2026-11-14T09:50:00Z");
}

fn prune(
    root: &Path,
    target: Option<&str>,
    unused: bool,
    default: Option<&str>,
) -> Result<SdkValue, layerfs_protocol::sdk::SdkRefusal> {
    images::prune_in_worker(
        root,
        SdkValue::map([
            ("machine", SdkValue::text("docker")),
            ("target", target.map_or(SdkValue::Null, SdkValue::text)),
            ("unused", SdkValue::Bool(unused)),
            ("image", default.map_or(SdkValue::Null, SdkValue::text)),
        ]),
    )
}

#[test]
fn prune_refuses_an_image_in_use_and_keeps_the_default() {
    let (_scratch, standin, root) = setup("copy-prune");
    run(&standin, &root, "demo/pg:17").0.unwrap();
    let second = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
    standin.image(second, &["demo/b:1"], ("linux", architecture()), 10);
    run(&standin, &root, "demo/b:1").0.unwrap();
    let dir = cache::cache(&root);
    // a Sandbox built on the first image
    let sandbox = dir.parent().unwrap().join("lfs-test-sb1/config");
    fs::create_dir_all(&sandbox).unwrap();
    fs::write(
        sandbox.join("helper.spec"),
        format!(
            "root=/x\nlower={}\n",
            dir.join(format!("{HEX}.2")).display()
        ),
    )
    .unwrap();
    let error = prune(&root, Some(ID), false, None).unwrap_err();
    assert_eq!(error.code, "InUse");
    assert_eq!(error.detail.get_int("sandbox_count"), Some(1));
    // a create between its unpack and its spec holds the image
    let image = Image::parse(second).unwrap();
    let tar = root.join("flat.tar");
    fs::write(&tar, filtered(&export())).unwrap();
    let (_, hold) = rootfs::ensure_held(&tar, &image, &dir).unwrap();
    assert_eq!(
        prune(&root, Some(second), false, None).unwrap_err().code,
        "InUse"
    );
    let out = prune(&root, None, true, Some(second)).unwrap();
    assert_eq!(out.get("removed"), Some(&SdkValue::List(Vec::new())));
    drop(hold);
    // --unused keeps the default and the image in use
    let out = prune(&root, None, true, Some(second)).unwrap();
    assert_eq!(out.get("removed"), Some(&SdkValue::List(Vec::new())));
    // naming it removes it, sidecar too
    let out = prune(&root, Some(second), false, Some(second)).unwrap();
    assert_eq!(
        out.get("removed"),
        Some(&SdkValue::List(vec![SdkValue::text(second)]))
    );
    assert!(!dir
        .join("3333333333333333333333333333333333333333333333333333333333333333.2")
        .exists());
    assert!(Sidecar::read(
        &dir,
        "3333333333333333333333333333333333333333333333333333333333333333"
    )
    .unwrap()
    .is_none());
    assert_eq!(
        prune(&root, Some(second), false, None).unwrap_err().code,
        "NotFound"
    );
    // once no Sandbox uses it, --unused removes the first
    fs::remove_dir_all(dir.parent().unwrap().join("lfs-test-sb1")).unwrap();
    let out = prune(&root, None, true, None).unwrap();
    assert_eq!(
        out.get("removed"),
        Some(&SdkValue::List(vec![SdkValue::text(ID)]))
    );
}

/// v19: `machine.capacity`'s `reclaimable_image_bytes` is what `machine
/// prune --unused` frees from a listing: a ready image no Sandbox uses but
/// the default (with any stale part beside it), the stale part of an image
/// in use, a part no copy holds; never a busy copy's part, the default or
/// an image a Sandbox uses.
#[test]
fn reclaimable_counts_what_an_unused_prune_frees() {
    let item = |id: &str, state: &str, size: Option<i64>, staged: Option<i64>, users: usize| {
        SdkValue::map([
            ("id", SdkValue::text(id)),
            ("state", SdkValue::text(state)),
            ("size", size.map_or(SdkValue::Null, SdkValue::Int)),
            ("staged_bytes", staged.map_or(SdkValue::Null, SdkValue::Int)),
            (
                "sandboxes",
                SdkValue::List(
                    (0..users)
                        .map(|n| SdkValue::text(format!("s{n}")))
                        .collect(),
                ),
            ),
        ])
    };
    let listed = SdkValue::map([
        ("default", SdkValue::text("sha256:d")),
        (
            "images",
            SdkValue::List(vec![
                item("sha256:d", "ready", Some(1000), None, 0),
                item("sha256:u", "ready", Some(100), Some(7), 0),
                item("sha256:s", "ready", Some(2000), Some(20), 1),
                item("sha256:p", "broken", None, Some(3), 0),
                item("sha256:b", "busy", None, Some(5000), 0),
            ]),
        ),
    ]);
    assert_eq!(images::reclaimable(&listed), 100 + 7 + 20 + 3);
    assert_eq!(images::reclaimable(&SdkValue::map([])), 0);
}
