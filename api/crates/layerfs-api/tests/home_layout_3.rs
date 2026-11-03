//! The conversion of a layout 2 home (2.0) into layout 3 (#536 §5, H10):
//! one with closed Sandboxes converts and answers every call as before; one
//! with anything live is refused `InUse`, naming each, and left as it was.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, read_record, write_record, Layout, LAYOUT_VERSION};
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use layerfs_sandbox::local::LocalProcess;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-l3-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir_at(&dir);
    dir
}

fn dir_at(path: &Path) {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .unwrap();
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, SdkValue> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| {
        SdkValue::map([
            ("code", t(&r.code)),
            ("message", t(&r.message)),
            ("detail", r.detail),
        ])
    })
}

/// A process that has ended (and been reaped).
fn dead_pid() -> i64 {
    let mut child = Command::new("true").spawn().unwrap();
    child.wait().unwrap();
    i64::from(child.id())
}

/// A live process in its own group, killed when the test ends with it.
struct Live(Child);

impl Live {
    fn new() -> Self {
        Self(
            Command::new("sleep")
                .arg("100")
                .process_group(0)
                .spawn()
                .unwrap(),
        )
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }

    fn end(mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A home with Store `shop`, project `shop/app`, machine record `dev`
/// (lima) and the Sandbox records `extra` adds, at layout 3.
fn home_with(tag: &str, extra: &[(&str, SdkValue)]) -> PathBuf {
    let home = tempdir(tag);
    home::open(&home).unwrap();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    write_record(
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([("name", t("app")), ("created", SdkValue::Int(1))]),
    )
    .unwrap();
    write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([
            ("name", t("dev")),
            ("platform", t("lima")),
            ("state", t("stopped")),
        ]),
    )
    .unwrap();
    for (name, record) in extra {
        write_record(&paths::sandbox_record(&home, name), record).unwrap();
    }
    home
}

/// A Sandbox record as layout 2 kept it.
fn sandbox(name: &str, machine: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    let mut all = vec![
        ("name", t(name)),
        ("project", t("shop/app")),
        ("machine", t(machine)),
        ("created", SdkValue::Int(5)),
    ];
    all.extend(fields.iter().cloned());
    SdkValue::Map(all.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

fn files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        // the failure log of a refused conversion
        if path.ends_with("log/migration-failures.jsonl") {
            continue;
        }
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// Turns layout 3 home `home` into what layout 2 kept: `layerfs.json` at
/// layout 2, no `"v"` in its records, each Sandbox's record under
/// `machine/<m>/sandbox/<name>/`, empty numbered folders and the old local
/// root with an image cache.
fn downgrade(home: &Path) {
    let file = home::detect(home).unwrap();
    let Layout::Current(file) = file else {
        panic!("{file:?}")
    };
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([("layout", SdkValue::Int(2)), ("id", t(&file.id))]),
    )
    .unwrap();
    let mut found = Vec::new();
    files(home, &mut found);
    for path in found.iter().filter(|p| p.ends_with("record.json")) {
        let Some(SdkValue::Map(fields)) = read_record(path).unwrap() else {
            continue;
        };
        let fields = fields.into_iter().filter(|(n, _)| n != "v").collect();
        write_record(path, &SdkValue::Map(fields)).unwrap();
    }
    for name in paths::names(&home.join("sandbox")).unwrap() {
        let record = read_record(&paths::sandbox_record(home, &name))
            .unwrap()
            .unwrap();
        let machine = record.get_text("machine").unwrap().to_owned();
        let to = paths::machine_dir(home, &machine)
            .join("sandbox")
            .join(&name);
        dir_at(to.parent().unwrap());
        std::fs::rename(paths::sandbox_dir(home, &name), to).unwrap();
    }
    std::fs::remove_dir(home.join("sandbox")).unwrap();
    dir_at(&paths::machine_dir(home, "native").join("sandbox"));
    dir_at(&paths::machine_dir(home, "dev").join("sandbox"));
    dir_at(&paths::project_dir(home, "shop", "app").join("sandboxes"));
    let old = paths::machine_dir(home, "native").join("sandbox-root");
    dir_at(&old.join("images"));
    std::fs::write(old.join("images/x"), b"image").unwrap();
}

/// A layout 2 numbered native Sandbox `1` of `shop/app` whose daemon is
/// `process`, as its executing side recorded it.
fn numbered(home: &Path, process: LocalProcess) {
    let name = "lfs-x-shop.app-1";
    let directory = paths::machine_dir(home, "native")
        .join("sandbox-root")
        .join(name);
    dir_at(&directory);
    let record = SdkValue::map([
        ("runtime", t("native")),
        ("container", t(&"ab".repeat(32))),
        ("directory", t(directory.to_str().unwrap())),
        ("name", t(name)),
        ("pid", SdkValue::Int(i64::from(process.pid))),
        ("start", t(&process.start.to_string())),
        ("boot", t(&format!("{:032x}", process.boot.unwrap()))),
        ("port", SdkValue::Int(1)),
        ("labels", SdkValue::List(Vec::new())),
    ]);
    let folder = paths::project_dir(home, "shop", "app").join("sandboxes/1");
    dir_at(&folder);
    let bytes = Answer {
        id: 1,
        reply: Reply::Sdk(record),
    }
    .encode()
    .unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(folder.join("sandbox.record"))
        .unwrap();
    std::io::Write::write_all(&mut file, &bytes).unwrap();
}

/// What the calls answer about the home's Stores, projects and Sandboxes.
fn answers(home: &Path, sandboxes: &[&str]) -> Vec<Result<SdkValue, SdkValue>> {
    let mut all = vec![
        call(home, "store.list", &[]),
        call(home, "store.get", &[("name", t("shop"))]),
        call(home, "project.list", &[]),
        call(home, "project.get", &[("project", t("shop/app"))]),
        call(home, "sandbox.list", &[]),
    ];
    for name in sandboxes {
        all.push(call(home, "sandbox.get", &[("name", t(name))]));
    }
    all
}

/// The `sandbox.list` item of Sandbox `name`.
fn item(listed: &Result<SdkValue, SdkValue>, name: &str) -> SdkValue {
    match listed.as_ref().unwrap().get("items") {
        Some(SdkValue::List(items)) => (items.iter())
            .find(|i| i.get_text("name") == Some(name))
            .cloned()
            .unwrap(),
        other => panic!("{other:?}"),
    }
}

/// No Sandbox record or numbered Sandbox state outside `sandbox/<name>/`.
fn only_layout_3(home: &Path) {
    let mut found = Vec::new();
    files(home, &mut found);
    for path in &found {
        let relative = path.strip_prefix(home).unwrap();
        let parts: Vec<_> = relative.iter().map(|p| p.to_str().unwrap()).collect();
        assert!(
            !parts.contains(&"sandboxes") && !path.ends_with("sandbox.record"),
            "layout 2 Sandbox state left: {}",
            relative.display()
        );
        if parts.len() >= 3 && parts[parts.len() - 3] == "sandbox" {
            assert_eq!(
                parts[0], "sandbox",
                "a Sandbox file outside sandbox/: {parts:?}"
            );
            assert_eq!(parts.len(), 3, "{parts:?}");
        }
    }
}

#[test]
fn a_closed_layout_2_home_converts_and_answers_as_before() {
    let closed = sandbox(
        "old",
        "native",
        &[("state", t("creating")), ("pid", SdkValue::Int(dead_pid()))],
    );
    let unknown = sandbox("lost", "dev", &[("state", t("unknown"))]);
    let home = home_with("same", &[("old", closed), ("lost", unknown)]);
    // (`sandbox.get` of a native Sandbox asks its daemon, which no bundle
    // provides here: its refusal is compared too)
    let before = answers(&home, &["old", "lost"]);
    assert!(before[..5].iter().all(Result::is_ok), "{before:?}");
    downgrade(&home);
    // an ended numbered Sandbox and the record that names it
    let daemon = Live::new();
    let process = LocalProcess::observe(daemon.pid()).unwrap();
    numbered(&home, process);
    daemon.end();
    let ended = sandbox("nat", "native", &[("id", t("1")), ("state", t("running"))]);
    let folder = paths::machine_dir(&home, "native").join("sandbox/nat");
    dir_at(&folder);
    write_record(&folder.join("record.json"), &ended).unwrap();
    assert!(matches!(home::detect(&home).unwrap(), Layout::Previous(_)));

    // the first call converts it; the ended Sandbox now reads `gone`
    let after = answers(&home, &["old", "lost"]);
    let gone = item(&after[4], "nat");
    assert_eq!(gone.get_text("state"), Some("gone"));
    assert_eq!(gone.get("id"), Some(&SdkValue::Null));
    let listed = |answer: &Result<SdkValue, SdkValue>| -> Vec<SdkValue> {
        match answer.as_ref().unwrap().get("items") {
            Some(SdkValue::List(items)) => items
                .iter()
                .filter(|i| i.get_text("name") != Some("nat"))
                .cloned()
                .collect(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(after[..4], before[..4]);
    assert_eq!(listed(&after[4]), listed(&before[4]));
    assert_eq!(after[5..], before[5..]);

    let Layout::Current(file) = home::detect(&home).unwrap() else {
        panic!("not converted")
    };
    assert_eq!(file.layout, LAYOUT_VERSION);
    let report = read_record(&paths::log_dir(&home).join("migration-2.json"))
        .unwrap()
        .unwrap();
    assert_eq!(report.get_int("from"), Some(2));
    assert_eq!(
        report.get("reclaimed"),
        Some(&SdkValue::List(vec![t("shop/app#1")]))
    );
    assert!(!paths::log_dir(&home).join("layout-3.pending").exists());
    for record in [
        paths::store_record(&home, "shop"),
        paths::project_record(&home, "shop", "app"),
        paths::machine_record(&home, "dev"),
        paths::sandbox_record(&home, "old"),
        paths::sandbox_record(&home, "nat"),
    ] {
        let value = read_record(&record).unwrap().unwrap();
        assert_eq!(value.get_int("v"), Some(1), "{}", record.display());
    }
    let run = paths::run_root(&home);
    assert_eq!(std::fs::read(run.join("images/x")).unwrap(), b"image");
    assert!(
        !run.join("lfs-x-shop.app-1").exists(),
        "the ended daemon's folder is deleted"
    );
    assert!(!paths::machine_dir(&home, "native")
        .join("sandbox-root")
        .exists());
    only_layout_3(&home);

    // the ended Sandbox closes as gone; a second open changes nothing
    let closed = call(&home, "sandbox.close", &[("name", t("nat"))]).unwrap();
    assert_eq!(closed.get_text("was"), Some("gone"));
    home::open(&home).unwrap();
    let again = answers(&home, &["old", "lost"]);
    assert_eq!(again[..4], after[..4]);
    assert_eq!(listed(&again[4]), listed(&after[4]));
    assert_eq!(again[5..], after[5..]);
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn a_layout_2_home_with_anything_live_is_refused_and_left() {
    let creating = sandbox(
        "making",
        "native",
        &[
            ("state", t("creating")),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ],
    );
    let vm = sandbox("vm", "dev", &[("id", t("vm")), ("state", t("running"))]);
    let home = home_with("live", &[("making", creating), ("vm", vm)]);
    downgrade(&home);
    let daemon = Live::new();
    numbered(&home, LocalProcess::observe(daemon.pid()).unwrap());
    let folder = paths::machine_dir(&home, "native").join("sandbox/nat");
    dir_at(&folder);
    write_record(
        &folder.join("record.json"),
        &sandbox("nat", "native", &[("id", t("1")), ("state", t("running"))]),
    )
    .unwrap();
    let intent = paths::project_dir(&home, "shop", "app").join("create.intent");
    std::fs::write(&intent, b"").unwrap();
    let holder = Live::new();
    let leases = paths::lease_dir(&home, "shop");
    dir_at(&leases);
    std::fs::write(leases.join(holder.pid().to_string()), b"1\n").unwrap();
    // layout 2's record of a move by a live process
    let store = paths::store_record(&home, "shop");
    let moving = SdkValue::map([
        ("op", t("move")),
        ("pid", SdkValue::Int(i64::from(holder.pid()))),
    ]);
    let record = home::with_field(read_record(&store).unwrap().unwrap(), "in_progress", moving);
    write_record(&store, &record).unwrap();

    let mut before = Vec::new();
    files(&home, &mut before);
    before.sort();
    let refusal = call(&home, "store.list", &[]).unwrap_err();
    assert_eq!(refusal.get_text("code"), Some("InUse"), "{refusal:?}");
    let message = refusal.get_text("message").unwrap();
    for named in [
        "sandbox making",
        "sandbox vm",
        "sandbox nat",
        "intent shop/app",
        "lease shop",
        "store shop",
    ] {
        assert!(message.contains(named), "{named} not in {message}");
    }
    assert!(message.contains("layerfs sandbox close vm"), "{message}");
    let Some(SdkValue::List(related)) = refusal.get("detail").and_then(|d| d.get("related")) else {
        panic!("{refusal:?}")
    };
    let kinds: Vec<_> = related
        .iter()
        .map(|r| r.get_text("kind").unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["intent", "sandbox", "sandbox", "sandbox", "lease", "store"]
    );
    let mut after = Vec::new();
    files(&home, &mut after);
    after.sort();
    assert_eq!(after, before, "a refused conversion changes nothing");
    assert!(matches!(home::detect(&home).unwrap(), Layout::Previous(_)));

    // the VM Sandbox alone, still recorded with an id, keeps it refused
    daemon.end();
    holder.end();
    std::fs::remove_file(&intent).unwrap();
    let making = paths::machine_dir(&home, "native").join("sandbox/making/record.json");
    let record = read_record(&making).unwrap().unwrap();
    let record = home::with_field(record, "pid", SdkValue::Int(dead_pid()));
    write_record(&making, &record).unwrap();
    let refusal = call(&home, "store.list", &[]).unwrap_err();
    let message = refusal.get_text("message").unwrap();
    assert!(
        message.contains("sandbox vm") && !message.contains("sandbox nat"),
        "{message}"
    );

    // once it is closed (its record gone), the home converts
    std::fs::remove_dir_all(paths::machine_dir(&home, "dev").join("sandbox/vm")).unwrap();
    call(&home, "store.list", &[]).unwrap();
    assert!(matches!(home::detect(&home).unwrap(), Layout::Current(_)));
    let listed = call(&home, "sandbox.list", &[]);
    assert_eq!(item(&listed, "nat").get_text("state"), Some("gone"));
    // the ended move's `in_progress` is not carried into layout 3
    let record = read_record(&store).unwrap().unwrap();
    assert_eq!(record.get("in_progress"), None, "{record:?}");
    assert_eq!(record.get("op"), None, "{record:?}");
    only_layout_3(&home);
    std::fs::remove_dir_all(&home).unwrap();
}
