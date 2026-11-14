//! Copying an image from (a stand-in) Docker into a machine's image cache,
//! on the local side: resolve, refuse, export, send, unpack once, reuse,
//! resume; then prune.
#[allow(dead_code)]
#[path = "../../layerfs-sandbox-backend/tests/support/image.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/standin.rs"]
mod standin;
use fixture::{Scratch, Tar};
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_sandbox::deploy::Side;
use layerfs_sandbox::image_artifact::{self as cache, architecture, Sidecar};
use layerfs_sandbox::image_load::{load, Loaded, Target};
use layerfs_sandbox::image_tar::Filtered;
use layerfs_sandbox::images;
use layerfs_sandbox_backend::{rootfs, Image};
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
) -> (Result<Loaded, layerfs_bridge::control::SdkRefusal>, Run) {
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
        &mut Target::Local(home),
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
        &mut Target::Local(&home),
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
    assert_eq!(error.detail.get_text("reason"), Some("arch"));
    assert_eq!(error.detail.get_text("value"), Some(other));
    let error = run(&standin, &home, "demo/none:1").0.unwrap_err();
    assert_eq!(error.code, "InvalidInput", "{}", error.message);
    let none = layerfs_sandbox::docker::Docker::at(None, None);
    let mut sink = |_| {};
    let error = load(
        &none,
        &mut Events::new(&mut sink),
        &mut Target::Local(&home),
        None,
        "demo/pg:17",
    )
    .unwrap_err();
    assert_eq!(error.code, "Unavailable");
    assert_eq!(error.detail.get_text("what"), Some("docker"));
    // larger than twice what the machine has free
    standin.set_rootfs_size(u64::MAX / 8);
    let error = run(&standin, &home, "demo/pg:17").0.unwrap_err();
    assert_eq!(error.code, "NoSpace");
    let need = error.detail.get_int("need").unwrap();
    let free = error.detail.get_int("free").unwrap();
    assert!(need > free, "{need} {free}");
    assert!(!cache::cache(&home).join(format!("{HEX}.2")).exists());
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
        // left by a copy that stopped a minute ago, not one still sending
        let part = cache::cache(&home).join(format!(".recv-{HEX}.part"));
        fs::File::options()
            .write(true)
            .open(&part)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(60))
            .unwrap();
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
    assert!(probe.get_bool("receiving"));
    assert!(!probe.get_bool("present"));
    let wrong = step(SdkValue::map([
        ("step", SdkValue::text("publish")),
        ("id", SdkValue::text(ID)),
        ("size", SdkValue::Int(3)),
        ("sha256", SdkValue::text("00")),
    ]));
    assert_eq!(wrong.unwrap_err().code, "InvalidInput");
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
) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
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
    assert_eq!(error.detail.get_int("count"), Some(1));
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
