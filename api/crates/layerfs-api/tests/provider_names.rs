//! The names LayerFS gives at a provider (layerfs-project#180), through the
//! machine calls and stand-in provider tools: no VM, container or volume
//! is made.
//!
//! - A new home's `docker` machine records `vm_name`
//!   `layerfs-<home id8>-docker`; its worker container and volume carry it.
//! - A `docker` record from before that (no `vm_name`) is read as it is
//!   and never rewritten; it reads `broken` `outdated`, and a start of it is
//!   refused `Failed` naming what made it, both with `machine remove docker`
//!   as the fix, before the provider is asked for anything: this build
//!   computes no older name. That remove takes its record alone, asking the
//!   provider nothing, and the next call records `docker` again with its
//!   2.0 name.
//! - `machine.create` refuses `Exists` a name whose VM name is another
//!   machine's recorded one (`a.b` then `a_b`), and refuses `InvalidInput`
//!   a name over 46 characters, both before the provider is asked for
//!   anything; a 46-character name
//!   reaches the provider as a 63-character VM name.
//! - A `lima` machine's name must also leave Lima's socket path
//!   `<Lima home>/<VM name>/ssh.sock.<16 digits>` shorter than 104
//!   characters: one character more is refused `InvalidInput` stating this
//!   host's limit, before the provider is asked for anything, and the
//!   longest name that fits reaches the provider.
//! - `sandbox.create` refuses a name ending in `.root`.
#![cfg(all(feature = "machine", target_os = "macos"))]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_protocol::provision::install::SdkValue;
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
    let (code, message) = call(home, "machine.start", &[("machine", t("docker"))]).unwrap_err();
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
    // only the stand-ins: no fallback folder after PATH
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    let bundle = tempdir("bundle");
    for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
        std::fs::write(bundle.join(name), b"").unwrap();
    }
    std::fs::write(bundle.join("bundle.json"), r#"{"hash":"abcd"}"#).unwrap();
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    std::env::set_var("XDG_CACHE_HOME", tempdir("cache"));
    // a Lima home short enough for a 63-character VM name; no such folder
    // exists and the stand-in makes none
    std::env::set_var("LIMA_HOME", "/lfs-no-lima");

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
    let native = read_record(&paths::machine_record(
        &fresh,
        layerfs_home::names::HOST_MACHINE,
    ))
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

    // --- a home from before: its `docker` record has no `vm_name`; it is
    // never rewritten, and a start of it is refused before any provider call
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
    let before = effects(&tools);
    let refused = layerfs_api::call(
        Some(&old),
        "machine.start",
        SdkValue::map([("machine", t("docker"))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refused.code, "Failed", "{}", refused.message);
    assert!(
        refused.message.contains("has no vm_name") && refused.message.contains("No call repairs"),
        "{}",
        refused.message
    );
    assert_eq!(
        refused.detail.get_text("fix"),
        Some("layerfs machine remove docker")
    );
    let docker = [("machine", t("docker")), ("detail", SdkValue::Bool(true))];
    let got = call(&old, "machine.get", &docker).unwrap();
    assert_eq!(got.get_text("state"), Some("broken"), "{got:?}");
    assert_eq!(got.get_text("reason"), Some("outdated"), "{got:?}");
    assert_eq!(got.get_text("fix"), Some("layerfs machine remove docker"));
    assert!(
        (got.get_text("message").unwrap_or("")).contains("has no vm_name"),
        "{got:?}"
    );
    // a live read keeps what its record says (UI pass 2, F1): nothing is
    // probed, and the long form, the short form and a live list all answer
    // `broken` `outdated` with its fix, never `unknown`
    let probes = std::fs::read_to_string(tools.join("calls.log")).unwrap_or_default();
    let live = SdkValue::List(vec![t("live")]);
    let mut asked = docker.to_vec();
    asked.push(("with", live.clone()));
    let got = call(&old, "machine.get", &asked).unwrap();
    assert_eq!(got.get_text("state"), Some("broken"), "{got:?}");
    assert_eq!(got.get_text("reason"), Some("outdated"), "{got:?}");
    assert_eq!(got.get_text("fix"), Some("layerfs machine remove docker"));
    assert!(
        (got.get_text("message").unwrap_or("")).contains("has no vm_name"),
        "{got:?}"
    );
    let short = [("machine", t("docker")), ("with", live.clone())];
    let short = call(&old, "machine.get", &short).unwrap();
    assert_eq!(short.get_text("reason"), Some("outdated"), "{short:?}");
    assert_eq!(short.get_text("fix"), Some("layerfs machine remove docker"));
    let list = call(&old, "machine.list", &[("with", live)]).unwrap();
    let Some(SdkValue::List(items)) = list.get("items") else {
        panic!("no items in {list:?}")
    };
    let item = (items.iter())
        .find(|item| item.get_text("machine") == Some("docker"))
        .unwrap();
    assert_eq!(item.get_text("state"), Some("broken"), "{item:?}");
    assert_eq!(item.get_text("reason"), Some("outdated"), "{item:?}");
    assert_eq!(item.get_text("fix"), Some("layerfs machine remove docker"));
    let after = std::fs::read_to_string(tools.join("calls.log")).unwrap_or_default();
    assert_eq!(
        after, probes,
        "a live read probed a machine with no vm_name"
    );
    let kept = read_record(&paths::machine_record(&old, "docker"))
        .unwrap()
        .unwrap();
    assert_eq!(kept, old_docker, "a live read rewrote the record");
    // the remove takes the record alone; the next call records it again
    let removed = call(&old, "machine.remove", &docker).unwrap();
    assert_eq!(removed.get_text("outcome"), Some("removed"), "{removed:?}");
    let said = removed.get_text("message").unwrap_or("");
    assert!(said.contains("its record has no vm_name"), "{said}");
    assert_eq!(effects(&tools), before, "the provider was asked");
    let again = call(&old, "machine.get", &docker).unwrap();
    assert_eq!(again.get_text("state"), Some("stopped"), "{again:?}");
    let made = read_record(&paths::machine_record(&old, "docker"))
        .unwrap()
        .unwrap();
    let name = format!("layerfs-{}-docker", &old_id[..8]);
    assert_eq!(made.get_text("vm_name"), Some(name.as_str()));

    // --- collisions: a VM name keeps no punctuation
    let home = fresh;
    // as this build's create records one, and a record with no `vm_name`
    // at all (which takes no name)
    lima(
        &home,
        "a.b",
        &[("vm_name", t(&format!("layerfs-{id8}-a-b")))],
    );
    lima(&home, "e.f", &[]);
    let made = effects(&tools);
    for (wanted, taken) in [("a_b", "a.b"), ("a-b", "a.b")] {
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

    // the record with no `vm_name`: a start is refused with `machine
    // remove` as its fix, and the remove takes its record alone, asking
    // the provider nothing
    let made = effects(&tools);
    let machine = [("machine", t("e.f"))];
    let (code, message) = call(&home, "machine.start", &machine).unwrap_err();
    assert_eq!(code, "Failed", "{message}");
    assert!(message.contains("has no vm_name"), "{message}");
    let start = SdkValue::map([("machine", t("e.f"))]);
    let refused = layerfs_api::call(Some(&home), "machine.start", start, &mut |_| {}).unwrap_err();
    assert_eq!(
        refused.detail.get_text("fix"),
        Some("layerfs machine remove e.f")
    );
    let detail = [("machine", t("e.f")), ("detail", SdkValue::Bool(true))];
    let removed = call(&home, "machine.remove", &detail).unwrap();
    assert_eq!(removed.get_text("outcome"), Some("removed"), "{removed:?}");
    let said = removed.get_text("message").unwrap_or("");
    assert!(said.contains("its record has no vm_name"), "{said}");
    assert!(!paths::machine_dir(&home, "e.f").exists());
    assert_eq!(effects(&tools), made, "the provider was asked");

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

    // --- Lima's own limit: `<Lima home>/<VM name>/ssh.sock.<16 digits>`
    // must stay under 104 characters. This home (32 characters, no such
    // folder) leaves a VM name 103 - (32 + 2 + 25) = 44 and a machine name 27.
    let lima_home = format!("/lfs-no-lima/{}", "h".repeat(19));
    std::env::set_var("LIMA_HOME", &lima_home);
    let made = effects(&tools);
    let long = "v".repeat(28);
    let (code, message) = create(&home, &long);
    assert_eq!(code, "InvalidInput", "{message}");
    let path = format!("`{lima_home}/<VM name>/ssh.sock.<16 digits>`");
    for part in [
        "28 characters",
        "on this host a lima machine's name is at most 27",
        path.as_str(),
        "shorter than 104",
        "takes 17",
    ] {
        assert!(message.contains(part), "{part}: {message}");
    }
    assert!(!paths::machine_dir(&home, &long).exists());
    assert_eq!(effects(&tools), made, "a refused create asked the provider");

    let longest = "w".repeat(27);
    let (code, message) = create(&home, &longest);
    assert_eq!(code, "Failed", "{message}");
    let vm = format!("layerfs-{id8}-{longest}");
    assert_eq!(
        format!("{lima_home}/{vm}/ssh.sock.1234567890123456").len(),
        103
    );
    let asked = effects(&tools);
    let line = format!("limactl create --name {vm} --tty=false -");
    assert!(asked.contains(&line), "{asked:?}");

    // a Lima home that exists counts with its links resolved, as Lima
    // counts it (`/var/...` is `/private/var/...`); a temporary folder's
    // path leaves no room for the 46-character name
    let real_home = tempdir("lima");
    std::env::set_var("LIMA_HOME", &real_home);
    let resolved = std::fs::canonicalize(&real_home).unwrap();
    let room = 103usize.saturating_sub(resolved.as_os_str().len() + 2 + 25 + 17);
    assert!(room < 46, "the temporary folder {resolved:?} is too short");
    let made = effects(&tools);
    let name = "z".repeat(46);
    let (code, message) = create(&home, &name);
    assert_eq!(code, "InvalidInput", "{message}");
    let fits = match room {
        0 => "no lima machine name fits on this host".to_owned(),
        room => format!("a lima machine's name is at most {room}:"),
    };
    let path = format!("`{}/<VM name>/", resolved.display());
    for part in [fits.as_str(), path.as_str()] {
        assert!(message.contains(part), "{part}: {message}");
    }
    assert!(!paths::machine_dir(&home, &name).exists());
    assert_eq!(effects(&tools), made, "a refused create asked the provider");

    std::env::remove_var("LIMA_HOME");
    std::env::remove_var("LAYERFS_BUNDLE");
    for dir in [home, old, tools, bundle, real_home] {
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
