//! The six doctors (call surface 2.0) against temporary homes, records
//! only: each answers the one report in its row's shape, short and long; a
//! clean home is `{"ok":true}` alone; a stopped machine blocks what is on it
//! without trying it; a thing that is not there is refused; no doctor makes
//! or converts a home; and one progress event goes out per finished thing.
use layerfs_api::home::{paths, write_entity};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-doctor-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn args(pairs: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        (pairs.iter())
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

/// One call: its answer or `code: message`, and its progress events.
fn call(
    home: &Path,
    name: &str,
    pairs: &[(&str, SdkValue)],
) -> (Result<SdkValue, String>, Vec<SdkValue>) {
    let mut events = Vec::new();
    let answer = layerfs_api::call(Some(home), name, args(pairs), &mut |event| {
        events.push(event)
    })
    .map_err(|refusal| format!("{}: {}", refusal.code, refusal.message));
    (answer, events)
}

fn short(home: &Path, name: &str, pairs: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, pairs)
        .0
        .unwrap_or_else(|why| panic!("{name}: {why}"))
}

fn long(home: &Path, name: &str, pairs: &[(&str, SdkValue)]) -> SdkValue {
    let mut pairs = pairs.to_vec();
    pairs.push(("detail", SdkValue::Bool(true)));
    short(home, name, &pairs)
}

fn keys(value: &SdkValue) -> Vec<&str> {
    match value {
        SdkValue::Map(fields) => fields.iter().map(|(key, _)| key.as_str()).collect(),
        _ => Vec::new(),
    }
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

const LONG: [&str; 7] = [
    "ok",
    "findings",
    "not_checked",
    "status",
    "counts",
    "observed_at",
    "tree",
];

/// Checks a doctor's pair of answers against the one report: the long form
/// has its seven keys, the short form only `ok` alone or `findings` and
/// `not_checked` (each present only when it has entries), its entries
/// without the long form's `evidence` and nulls, and `counts` counts every
/// thing in the tree once.
fn check_pair(short: &SdkValue, long: &SdkValue) {
    assert_eq!(keys(long), LONG, "{long:?}");
    let clean = list(long, "findings").is_empty() && list(long, "not_checked").is_empty();
    assert_eq!(long.get("ok"), Some(&SdkValue::Bool(clean)));
    if clean {
        assert_eq!(json::print(short), r#"{"ok":true}"#);
    } else {
        assert!(short.get("ok").is_none(), "{short:?}");
        for key in keys(short) {
            assert!(["findings", "not_checked"].contains(&key), "{short:?}");
        }
    }
    for (short, long) in list(short, "findings").iter().zip(list(long, "findings")) {
        assert_eq!(
            &keys(long)[1..],
            ["check", "severity", "message", "fix", "from", "evidence"]
        );
        assert_eq!(keys(short)[0], keys(long)[0], "the thing's own word");
        assert!(!keys(short).contains(&"evidence"));
    }
    for (short, long) in list(short, "not_checked")
        .iter()
        .zip(list(long, "not_checked"))
    {
        assert_eq!(&keys(long)[1..], ["check", "blocked_by", "reason"]);
        let said = (
            short.get("blocked_by").is_some(),
            short.get("reason").is_some(),
        );
        assert!(said == (true, false) || said == (false, true), "{short:?}");
    }
    let tree = long.get("tree").unwrap();
    let mut things = 1;
    for (key, value) in match tree {
        SdkValue::Map(fields) => fields.as_slice(),
        _ => &[],
    } {
        if let SdkValue::List(nodes) = value {
            if key != "checks" {
                things += nodes.len();
            }
        }
    }
    let counted: i64 = match long.get("counts") {
        Some(SdkValue::Map(fields)) => fields
            .iter()
            .map(|(_, n)| match n {
                SdkValue::Int(n) => *n,
                _ => 0,
            })
            .sum(),
        _ => 0,
    };
    assert_eq!(counted, things as i64, "{long:?}");
}

/// A home with its native daemon recorded as an executable file, so the
/// home's daemon check has something to read.
#[cfg(feature = "machine")]
fn initialized(tag: &str) -> PathBuf {
    use layerfs_api::home::{read_record, with_field};
    let home = tempdir(tag);
    short(&home, "home.init", &[]);
    let daemon = home.join("layerfs-daemon");
    std::fs::write(&daemon, "#!/bin/sh\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = paths::machine_record(&home, "native");
    let native = read_record(&path)
        .unwrap()
        .unwrap_or_else(|| SdkValue::map([("name", t("native")), ("platform", t("native"))]));
    let native = with_field(native, "daemon", t(&daemon.display().to_string()));
    write_entity(&path, &native).unwrap();
    home
}

#[test]
fn a_home_that_does_not_exist_is_reported_and_stays_absent() {
    let home = tempdir("absent");
    let (answer, events) = call(&home, "home.doctor", &[("detail", SdkValue::Bool(true))]);
    let long = answer.unwrap();
    assert!(!home.exists(), "the doctor made the home");
    assert_eq!(keys(&long), LONG);
    // nothing to check under it, and no layout or records to read
    let tree = long.get("tree").unwrap();
    assert_eq!(
        keys(tree),
        [
            "home",
            "status",
            "coverage",
            "checks",
            "blocked_by",
            "reason",
            "machines",
            "stores",
            "buckets"
        ]
    );
    assert!(list(tree, "machines").is_empty());
    let ran: Vec<&str> = (list(tree, "checks").iter())
        .filter_map(|row| row.get_text("check"))
        .collect();
    assert!(
        !ran.contains(&"host.layout") && !ran.contains(&"host.records"),
        "{ran:?}"
    );
    assert_eq!(events.len(), 1, "one thing, one event");
    let short = short(&home, "home.doctor", &[]);
    check_pair(&short, &long);
    assert!(!home.exists());
}

#[test]
#[cfg(feature = "machine")]
fn a_clean_home_is_ok_alone_and_its_long_form_is_the_one_report() {
    let home = initialized("clean");
    let before = layerfs_api::home::read_record(&home.join("layerfs.json")).unwrap();
    let (answer, events) = call(&home, "home.doctor", &[]);
    let short = answer.unwrap();
    let long = long(&home, "home.doctor", &[]);
    check_pair(&short, &long);
    // this build's own environment decides two of the home's checks
    let environment = |finding: &SdkValue| {
        matches!(
            finding.get_text("check"),
            Some("host.prerequisites" | "host.bundle")
        )
    };
    if cfg!(target_os = "macos") {
        assert_eq!(json::print(&short), r#"{"ok":true}"#, "{long:?}");
    } else {
        assert!(list(&long, "findings").iter().all(environment), "{long:?}");
    }
    assert_eq!(list(&long, "not_checked").len(), 0, "{long:?}");
    // the home, then each machine shallowly: a stopped one by its records
    let tree = long.get("tree").unwrap();
    assert_eq!(tree.get_text("coverage"), Some("shallow"));
    let machines = list(tree, "machines");
    let names: Vec<&str> = machines
        .iter()
        .filter_map(|m| m.get_text("machine"))
        .collect();
    assert_eq!(names, ["docker", "native"]);
    for machine in machines {
        assert_eq!(
            keys(machine),
            [
                "machine",
                "status",
                "coverage",
                "checks",
                "blocked_by",
                "reason"
            ],
            "a shallow child has no lists of its own"
        );
    }
    let docker: Vec<&str> = (list(&machines[0], "checks").iter())
        .filter_map(|row| row.get_text("check"))
        .collect();
    assert_eq!(docker, ["machine.record", "machine.operation"]);
    // one event per finished thing: the home, then its machines
    let phases: Vec<String> = (events.iter())
        .map(|event| {
            let event = event.get("progress").unwrap();
            assert_eq!(keys(event), ["phase", "done", "total", "node"]);
            format!(
                "{} {}/{}",
                event.get_text("phase").unwrap(),
                event.get_int("done").unwrap(),
                event.get_int("total").unwrap()
            )
        })
        .collect();
    assert_eq!(phases, ["self 1/3", "machines 2/3", "machines 3/3"]);
    let after = layerfs_api::home::read_record(&home.join("layerfs.json")).unwrap();
    assert_eq!(before, after);
    let _ = std::fs::remove_dir_all(home);
}

/// A sqlite Store record `name` on `machine`, with a made-up identity.
fn sqlite_store(home: &Path, name: &str, machine: &str) {
    write_entity(
        &paths::store_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("kind", t("sqlite")),
            ("machine", t(machine)),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ]),
    )
    .unwrap();
}

/// A sqlite Store `cache` and a Sandbox `sb` on the stopped built-in
/// machine `docker`.
#[cfg(feature = "machine")]
fn on_docker(home: &Path) {
    sqlite_store(home, "cache", "docker");
    write_entity(
        &paths::project_record(home, "cache", "site"),
        &SdkValue::map([("name", t("site"))]),
    )
    .unwrap();
    write_entity(
        &paths::sandbox_record(home, "sb"),
        &SdkValue::map([
            ("name", t("sb")),
            ("project", t("cache/site")),
            ("machine", t("docker")),
            ("id", t("sb")),
        ]),
    )
    .unwrap();
}

#[test]
#[cfg(feature = "machine")]
fn a_stopped_machine_blocks_what_is_on_it_without_trying_it() {
    let home = initialized("stopped");
    on_docker(&home);
    let stopped = r#"{"machine":"docker","reason":"stopped"}"#;
    // the machine: its records checks, then each thing on it not tried
    let short_form = short(&home, "machine.doctor", &[("machine", t("docker"))]);
    let long_form = long(&home, "machine.doctor", &[("machine", t("docker"))]);
    check_pair(&short_form, &long_form);
    assert_eq!(
        json::print(&short_form),
        format!(
            r#"{{"not_checked":[{{"store":"cache","blocked_by":{stopped}}},{{"sandbox":"sb","blocked_by":{stopped}}}]}}"#
        )
    );
    let tree = long_form.get("tree").unwrap();
    assert_eq!(keys(tree)[6..], ["stores", "sandboxes"]);
    let store = &list(tree, "stores")[0];
    assert_eq!(store.get_text("status"), Some("not_checked"));
    assert_eq!(store.get_text("coverage"), Some("none"));
    assert!(list(store, "checks").is_empty());
    assert_eq!(long_form.get_text("status"), Some("not_checked"));
    // the Store: its records checks, and its projects wait on the machine
    let store = short(&home, "store.doctor", &[("store", t("cache"))]);
    assert_eq!(
        json::print(&store),
        format!(r#"{{"not_checked":[{{"project":"cache/site","blocked_by":{stopped}}}]}}"#)
    );
    let long_store = long(&home, "store.doctor", &[("store", t("cache"))]);
    check_pair(&store, &long_store);
    let checks: Vec<&str> = (list(long_store.get("tree").unwrap(), "checks").iter())
        .filter_map(|row| row.get_text("check"))
        .collect();
    assert_eq!(checks, ["store.record"]);
    // the project and the Sandbox: nothing tried
    for (name, word, thing) in [
        ("project.doctor", "project", "cache/site"),
        ("sandbox.doctor", "sandbox", "sb"),
    ] {
        let answer = short(&home, name, &[(word, t(thing))]);
        assert_eq!(
            json::print(&answer),
            format!(r#"{{"not_checked":[{{"{word}":"{thing}","blocked_by":{stopped}}}]}}"#)
        );
        let long_answer = long(&home, name, &[(word, t(thing))]);
        check_pair(&answer, &long_answer);
        assert_eq!(
            long_answer.get("tree").unwrap().get_text("coverage"),
            Some("none")
        );
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_project_s_record_is_checked_against_its_store() {
    let home = tempdir("project");
    short(&home, "home.init", &[]);
    sqlite_store(&home, "shop", "native");
    write_entity(
        &paths::project_record(&home, "shop", "old"),
        &SdkValue::map([("name", t("old"))]),
    )
    .unwrap();
    let (answer, events) = call(
        &home,
        "project.doctor",
        &[("project", t("shop/old")), ("detail", SdkValue::Bool(true))],
    );
    let report = answer.unwrap();
    let tree = report.get("tree").unwrap();
    assert_eq!(
        keys(tree),
        [
            "project",
            "status",
            "coverage",
            "checks",
            "blocked_by",
            "reason"
        ]
    );
    let rows = list(tree, "checks");
    assert_eq!(rows[0].get_text("check"), Some("project.record"));
    assert_eq!(rows[0].get_text("outcome"), Some("pass"));
    assert_eq!(rows[1].get_text("check"), Some("project.listed"));
    // the Store's file is not there: the listing waits on the Store
    assert_eq!(
        json::print(&list(&report, "not_checked")[0]),
        r#"{"project":"shop/old","check":"project.listed","blocked_by":{"store":"shop","reason":"missing"},"reason":null}"#,
        "{report:?}"
    );
    assert_eq!(events.len(), 1);
    // `--full` is not built yet
    let (refused, _) = call(
        &home,
        "project.doctor",
        &[("project", t("shop/old")), ("full", SdkValue::Bool(true))],
    );
    let refused = refused.unwrap_err();
    assert!(
        refused.starts_with("Unsupported: project doctor --full"),
        "{refused}"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_bucket_s_record_is_its_first_check() {
    let home = tempdir("bucket");
    short(&home, "home.init", &[]);
    write_entity(
        &paths::bucket_record(&home, "b1"),
        &SdkValue::map([("name", t("b1"))]),
    )
    .unwrap();
    let (answer, events) = call(&home, "backup.doctor", &[("bucket", t("b1"))]);
    let short_form = answer.unwrap();
    let long_form = long(&home, "backup.doctor", &[("bucket", t("b1"))]);
    check_pair(&short_form, &long_form);
    let found = &list(&short_form, "findings")[0];
    assert_eq!(found.get_text("bucket"), Some("b1"));
    assert_eq!(found.get_text("check"), Some("bucket.configuration"));
    assert_eq!(found.get_text("severity"), Some("error"));
    assert!(events.is_empty(), "backup doctor sends no progress");
    assert_eq!(long_form.get("ok"), Some(&SdkValue::Bool(false)));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn what_is_not_there_is_refused_and_no_home_is_made() {
    let home = tempdir("missing");
    for (name, pairs) in [
        ("machine.doctor", vec![("machine", t("dev"))]),
        ("store.doctor", vec![("store", t("shop"))]),
        ("project.doctor", vec![("project", t("shop/api"))]),
        ("sandbox.doctor", vec![("sandbox", t("sb"))]),
        ("backup.doctor", vec![("bucket", t("r2"))]),
    ] {
        let refused = call(&home, name, &pairs).0.unwrap_err();
        assert!(refused.starts_with("NotFound: "), "{name}: {refused}");
    }
    assert!(!home.exists(), "a doctor made the home");
    // `--machine` probes a PostgreSQL Store; a sqlite Store is refused
    short(&home, "home.init", &[]);
    sqlite_store(&home, "shop", "native");
    let refused = call(
        &home,
        "store.doctor",
        &[("store", t("shop")), ("machine", t("lab"))],
    )
    .0;
    assert!(refused.unwrap_err().starts_with("InvalidInput: "));
    // the old names and options are gone
    for gone in ["doctor.run", "home.check"] {
        assert!(call(&home, gone, &[]).0.is_err());
    }
    let refused = call(&home, "home.doctor", &[("timeout", t("1s"))])
        .0
        .unwrap_err();
    assert!(refused.starts_with("InvalidInput: "), "{refused}");
    let _ = std::fs::remove_dir_all(home);
}
