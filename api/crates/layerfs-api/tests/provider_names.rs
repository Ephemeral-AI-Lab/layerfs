//! The names LayerFS gives at a provider (layerfs-project#180), through the
//! machine calls and stand-in provider tools: no VM, container or volume
//! is made.
//!
//! - A new home's `docker` machine records `vm_name`
//!   `layerfs-<home id8>-docker`; its worker container and volume carry it.
//! - A home whose `docker` record predates that (no `vm_name`) keeps
//!   `lfs-<home id8>-docker` and the volume `lfs-<home id8>-docker-data`,
//!   where its Stores are.
//! - `machine.create` refuses `Exists` a name whose VM name is another
//!   machine's (`a.b` then `a_b`), whichever formula gave that machine its
//!   name, and refuses `InvalidInput` a name over 46 characters, both
//!   before the provider is asked for anything; a 46-character name
//!   reaches the provider as a 63-character VM name.
//! - `sandbox.create` refuses a name ending in `.root`.
#![cfg(all(feature = "machine", target_os = "macos"))]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// A stand-in `limactl` with no instance and no disk, which makes none:
/// `create` is refused, so a create that reached the provider is `Failed`.
const LIMACTL: &str = r#"#!/bin/sh
d="${0%/*}"
echo "limactl $*" >> "$d/calls.log"
case "$1" in
  --version) echo "limactl version 0.0-stand-in" ;;
  list|disk) ;;
  *) echo "the stand-in makes no VM" >&2; exit 9 ;;
esac
"#;

/// A stand-in `docker` with no container, which starts none.
const DOCKER: &str = r#"#!/bin/sh
d="${0%/*}"
echo "docker $*" >> "$d/calls.log"
case "$3" in
  version) echo 29.0 ;;
  inspect) echo "Error: No such object" >&2; exit 1 ;;
  *) echo "the stand-in starts no container" >&2; exit 9 ;;
esac
"#;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-pnames-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// `machine.create(name, lima)`'s refusal.
fn create(home: &Path, name: &str) -> (String, String) {
    let made = call(
        home,
        "machine.create",
        &[("name", t(name)), ("platform", t("lima"))],
    );
    made.expect_err("the stand-in makes no VM, so no create succeeds")
}

/// A lima machine's record as an earlier create left it, with `more`.
fn lima(home: &Path, name: &str, more: &[(&str, SdkValue)]) {
    let mut fields = vec![
        ("name".to_owned(), t(name)),
        ("platform".to_owned(), t("lima")),
        ("state".to_owned(), t("stopped")),
    ];
    fields.extend(more.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    write_record(&paths::machine_record(home, name), &SdkValue::Map(fields)).unwrap();
}

/// The stand-ins' calls so far that made or started something (every
/// call but the probes and reads).
fn effects(tools: &Path) -> Vec<String> {
    let log = std::fs::read_to_string(tools.join("calls.log")).unwrap_or_default();
    log.lines()
        .filter(|line| {
            !["--version", "list", "disk list", "inspect", "version"]
                .iter()
                .any(|read| line.contains(&format!(" {read}")))
        })
        .map(str::to_owned)
        .collect()
}

/// The docker stand-in's `run` line of a `machine.start docker`, which the
/// stand-in refuses.
fn docker_run(home: &Path, tools: &Path) -> String {
    let before = effects(tools).len();
    let (code, message) = call(home, "machine.start", &[("name", t("docker"))]).unwrap_err();
    assert_eq!(code, "Failed", "{message}");
    let after = effects(tools);
    let runs: Vec<_> = after[before..]
        .iter()
        .filter(|line| line.contains(" run "))
        .collect();
    assert_eq!(runs.len(), 1, "{after:?}");
    runs[0].clone()
}

// One test owns PATH and the bundle setting for every case.
#[test]
fn provider_names_and_their_refusals() {
    let tools = tempdir("tools");
    for (name, script) in [("limactl", LIMACTL), ("docker", DOCKER)] {
        let file = tools.join(name);
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("PATH", &tools);
    let bundle = tempdir("bundle");
    for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
        std::fs::write(bundle.join(name), b"").unwrap();
    }
    std::fs::write(bundle.join("bundle.json"), r#"{"hash":"abcd"}"#).unwrap();
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    std::env::set_var("XDG_CACHE_HOME", tempdir("cache"));

    // --- a new home: `docker` records its worker's name, with `layerfs-`
    let fresh = tempdir("new");
    let id = home::open(&fresh).unwrap().id;
    let id8 = &id[..8];
    call(&fresh, "machine.list", &[]).unwrap();
    let docker = read_record(&paths::machine_record(&fresh, "docker"))
        .unwrap()
        .unwrap();
    let worker = format!("layerfs-{id8}-docker");
    assert_eq!(docker.get_text("vm_name"), Some(worker.as_str()));
    let native = read_record(&paths::machine_record(&fresh, "native"))
        .unwrap()
        .unwrap();
    assert_eq!(native.get("vm_name"), None, "native has no provider name");
    write_record(
        &paths::machine_record(&fresh, "docker"),
        &home::with_field(docker, "image", t("test-base")),
    )
    .unwrap();
    let run = docker_run(&fresh, &tools);
    assert!(run.contains(&format!(" --name {worker} ")), "{run}");
    assert!(
        run.contains(&format!(" -v {worker}-data:/layerfs/data ")),
        "{run}"
    );

    // --- a home from before: its `docker` record has no `vm_name`, and
    // keeps the container and volume names it always had
    let old = tempdir("old");
    let old_id = home::open(&old).unwrap().id;
    let old_docker = SdkValue::map([
        ("v", SdkValue::Int(1)),
        ("name", t("docker")),
        ("platform", t("docker")),
        ("created", SdkValue::Int(1)),
        ("image", t("test-base")),
    ]);
    write_record(&paths::machine_record(&old, "docker"), &old_docker).unwrap();
    call(&old, "machine.list", &[]).unwrap();
    let kept = read_record(&paths::machine_record(&old, "docker"))
        .unwrap()
        .unwrap();
    assert_eq!(
        kept, old_docker,
        "the first call rewrote an existing record"
    );
    let worker = format!("lfs-{}-docker", &old_id[..8]);
    let run = docker_run(&old, &tools);
    assert!(run.contains(&format!(" --name {worker} ")), "{run}");
    assert!(
        run.contains(&format!(" -v {worker}-data:/layerfs/data ")),
        "{run}"
    );
    assert!(!run.contains("layerfs-"), "{run}");

    // --- collisions: a VM name keeps no punctuation
    let home = fresh;
    // as this build's create records one, as a build before #180 recorded
    // one, and a record with no `vm_name` at all (the earlier formula)
    lima(
        &home,
        "a.b",
        &[("vm_name", t(&format!("layerfs-{id8}-a-b")))],
    );
    lima(&home, "c.d", &[("vm_name", t(&format!("lfs-{id8}-c-d")))]);
    lima(&home, "e.f", &[]);
    let made = effects(&tools);
    for (wanted, taken) in [
        ("a_b", "a.b"),
        ("a-b", "a.b"),
        ("c_d", "c.d"),
        ("e_f", "e.f"),
        ("e-f", "e.f"),
    ] {
        let (code, message) = create(&home, wanted);
        assert_eq!(code, "Exists", "{wanted}: {message}");
        assert!(message.contains(&format!("machine {taken} ")), "{message}");
        assert!(!paths::machine_dir(&home, wanted).exists(), "{wanted}");
    }
    assert_eq!(effects(&tools), made, "a refused create asked the provider");
    // the same name again is the folder's own refusal, as before
    let (code, message) = create(&home, "a.b");
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("Exists", "a machine named a.b exists")
    );

    // a name that collides with none reaches the provider, as `layerfs-`
    let (code, message) = create(&home, "ab");
    assert_eq!(code, "Failed", "{message}");
    let asked = effects(&tools);
    let line = format!("limactl create --name layerfs-{id8}-ab --tty=false -");
    assert!(asked.contains(&line), "{asked:?}");
    assert!(!paths::machine_dir(&home, "ab").exists());

    // --- length: 46 characters is the longest new name
    let made = effects(&tools);
    let long = "x".repeat(47);
    let (code, message) = create(&home, &long);
    assert_eq!(code, "InvalidInput", "{message}");
    for part in ["47 characters", "at most 46", "at most 63", "takes 17"] {
        assert!(message.contains(part), "{part}: {message}");
    }
    assert!(!paths::machine_dir(&home, &long).exists());
    assert_eq!(effects(&tools), made, "a refused create asked the provider");

    let longest = "y".repeat(46);
    let (code, message) = create(&home, &longest);
    assert_eq!(code, "Failed", "{message}");
    let vm = format!("layerfs-{id8}-{longest}");
    assert_eq!(vm.len(), 63);
    let asked = effects(&tools);
    let line = format!("limactl create --name {vm} --tty=false -");
    assert!(asked.contains(&line), "{asked:?}");

    std::env::remove_var("LAYERFS_BUNDLE");
    for dir in [home, old, tools, bundle] {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_sandbox_name_ending_in_root_is_refused() {
    let home = tempdir("root");
    home::open(&home).unwrap();
    for name in ["x.root", "a.b.root"] {
        let (code, message) = call(
            &home,
            "sandbox.create",
            &[("project", t("shop/api")), ("name", t(name))],
        )
        .unwrap_err();
        assert_eq!(code, "InvalidInput", "{name}: {message}");
        assert!(message.contains("`.root`"), "{message}");
        assert!(!paths::sandbox_dir(&home, name).exists(), "{name}");
    }
    // a name that only contains it is an ordinary name: refused for its
    // project, which this home does not have, not for its name
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[("project", t("shop/api")), ("name", t("x.rooted"))],
    )
    .unwrap_err();
    assert_ne!(code, "InvalidInput", "{message}");
    std::fs::remove_dir_all(home).unwrap();
}
