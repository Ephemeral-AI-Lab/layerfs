//! #818 probe: time, client round trips and statements of every `fs.*`
//! Store call and of Namespace Init, through the SDK's own call route
//! (`open_store_observed` with the SDK's reader capacity and cache, then
//! `methods::call`), in the arm `FS818_ARM` names:
//!
//! - `sqlite`: a sealed SQLite Store (Normal profile) in a temporary dir;
//! - `pg`: PostgreSQL at `LAYERFS_TEST_POSTGRES_URL`;
//! - `pg2`, `pg10`: the same server with 2 or 10 ms of `netem` delay added
//!   inside the network namespace of the container named by
//!   `FS818_NETEM_CONTAINER` (never on the host network), after setup and
//!   before the measured calls (Init: just before Init), removed afterwards.
//!
//! `FS818_GROUP` selects `reads`, `history`, `writes` or `w0`; each group
//! runs in its own process from the same setup (Init of the tree, a `work`
//! Branch, 100 `fs.write` Commits), so each command stays under two minutes.
//! `FS818_CASES` (comma-separated case names) optionally runs only those
//! cases of the group, in group order: a skipped case changes what the
//! caches hold for the later ones, so such a run is labelled.
//!
//! The `reads`, `history` and `writes` groups run their cases one after
//! another on the Store open that wrote the history, as the 056e920166
//! baseline did (cases appended since then come last, so the baseline cases
//! see the same state). The `w0` group (writes.md W0) attributes each write
//! call's SQL work to the phases of its route, on a new Store open per case
//! (`cold`) and then once more on that open (`warm`); see [`w0`].
//!
//! A measurement tool, not a gate. Each measured case prints one line
//! `FS_RT <json>` (`FS_W0 <json>` for the attribution): `client` is every
//! session together, as at baseline; `roles` splits it into the writer, the
//! control reader and the Workspace readers; `server` is the database's
//! top-level `pg_stat_statements` calls (support `round_trips.rs`). One
//! sample per case. Run with `--test-threads=1 --nocapture`.
use crate::rt;

use layerfs_persistence::SqlWork;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::telemetry::{attribute_sql, mark_phases, phase_sql, Recording};
use rt::{t, Opened, Server, Statements, Store};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// A delay inside the PostgreSQL container's network namespace.
#[derive(Clone)]
struct Netem {
    container: String,
    ms: u32,
}
impl Netem {
    /// Adds (`on`) or removes the delay with `tc` in a throwaway container
    /// sharing the PostgreSQL container's network namespace (never the
    /// host network), then prints the observer's `SELECT 1` latency.
    fn set(&self, on: bool, server: Option<&mut Server>) {
        // Removal succeeds only when no netem qdisc is left, so a failed
        // `apk` fetch cannot leave the delay on unnoticed.
        let tc = if on {
            format!("tc qdisc add dev eth0 root netem delay {}ms", self.ms)
        } else {
            "{ tc qdisc del dev eth0 root 2>/dev/null; ! tc qdisc show dev eth0 | grep -q netem; }"
                .to_owned()
        };
        let status = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--net",
                &format!("container:{}", self.container),
                "--cap-add",
                "NET_ADMIN",
                "alpine:3",
                "sh",
                "-c",
                &format!("apk add --no-cache -q iproute2-tc >/dev/null 2>&1 && {tc}"),
            ])
            .status()
            .expect("docker");
        assert!(status.success(), "netem: {tc}");
        if let Some(server) = server {
            let (min, median, p90) = server.calibrate();
            println!(
                "FS_RT {{\"case\":\"calibration.select1\",\"delay_ms\":{},\"min_us\":{min},\"median_us\":{median},\"p90_us\":{p90}}}",
                if on { self.ms } else { 0 }
            );
        }
    }
}

/// One arm: its Store and, for a delay arm, the delay.
struct Arm {
    name: &'static str,
    store: Store,
    netem: Option<Netem>,
}
impl Arm {
    /// The arm `FS818_ARM` names, or `None` with a NOT_RUN note.
    fn from_env(label: &str) -> Option<Self> {
        let (name, delay) = match std::env::var("FS818_ARM").as_deref() {
            Ok("sqlite") => ("sqlite", None),
            Ok("pg") => ("pg", None),
            Ok("pg2") => ("pg2", Some(2)),
            Ok("pg10") => ("pg10", Some(10)),
            _ => {
                eprintln!("NOT_RUN: FS818_ARM must be sqlite, pg, pg2 or pg10");
                return None;
            }
        };
        let store = if name == "sqlite" {
            Store::sqlite(label)
        } else {
            let store = Store::postgresql(label)?;
            if store.server.is_none() {
                eprintln!("NOT_RUN: pg_stat_statements unavailable");
                return None;
            }
            store
        };
        let netem = match delay {
            None => None,
            Some(ms) => {
                let Ok(container) = std::env::var("FS818_NETEM_CONTAINER") else {
                    eprintln!("NOT_RUN: FS818_NETEM_CONTAINER is unset");
                    return None;
                };
                Some(Netem { container, ms })
            }
        };
        Some(Self { name, store, netem })
    }
    fn delay(&mut self, on: bool) {
        if let Some(netem) = &self.netem {
            netem.set(on, self.store.server.as_mut());
        }
    }
}

fn wanted(case: &str) -> bool {
    std::env::var("FS818_CASES")
        .map(|cases| cases.split(',').any(|c| c.trim() == case))
        .unwrap_or(true)
}

/// One measured call; a refusal is printed, not fatal.
fn measure(
    arm: &mut Arm,
    opened: &Opened,
    case: &str,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
) -> Option<SdkValue> {
    if !wanted(case) {
        return None;
    }
    let before = opened.work();
    let server_before = arm.store.snapshot();
    let started = Instant::now();
    let reply = opened.call(method, fields);
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let work = rt::since(&opened.work(), &before);
    let server = arm
        .store
        .server_delta(server_before)
        .map_or("null".to_owned(), |s| s.json(6));
    let outcome = match &reply {
        Ok(value) => value.get_text("kind").unwrap_or("ok").to_owned(),
        Err(refusal) => format!("refused {refusal}").replace('"', "'"),
    };
    println!(
        "FS_RT {{\"arm\":\"{}\",\"case\":\"{case}\",\"method\":\"{method}\",\"outcome\":\"{outcome}\",\"ms\":{ms:.2},\"client\":{},\"roles\":{},\"server\":{server},\"load1\":\"{}\"}}",
        arm.name,
        rt::work_json(&rt::total(&work)),
        rt::roles_json(&work),
        rt::load1()
    );
    reply.ok()
}

fn path(p: &str) -> (&'static str, SdkValue) {
    ("path", t(p))
}
fn branch() -> (&'static str, SdkValue) {
    ("branch", t(rt::BRANCH))
}
fn bytes(text: &str) -> (&'static str, SdkValue) {
    ("bytes", SdkValue::Bytes(text.as_bytes().to_vec()))
}
fn apply(dir: &str, prefix: &str) -> Vec<(&'static str, SdkValue)> {
    let changes: Vec<SdkValue> = (0..10)
        .map(|n| {
            SdkValue::map([
                ("op", t("write")),
                ("path", t(&format!("{dir}/{prefix}{n}.txt"))),
                (
                    "bytes",
                    SdkValue::Bytes(format!("{prefix}{n}\n").into_bytes()),
                ),
            ])
        })
        .collect();
    vec![branch(), ("changes", SdkValue::List(changes))]
}

#[test]
fn fs_calls() {
    let Some(mut arm) = Arm::from_env("fs") else {
        return;
    };
    let group = std::env::var("FS818_GROUP").unwrap_or_default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let source = arm.store.dir.join("source");
        rt::build_tree(&source);
        let init = arm.store.init(&source, &mut |_, _| {});
        print_init(&arm, "setup.init_nested_1k", &init);
        if group == "w0" {
            w0(&mut arm);
            return;
        }
        let opened = arm.store.open("fs");
        rt::fork_work(&opened);
        let (before, at) = (opened.work(), Instant::now());
        let ids = rt::write_history(&opened);
        println!(
            "FS_RT {{\"arm\":\"{}\",\"case\":\"setup.history_100_writes_total\",\"ms\":{:.1},\"client\":{}}}",
            arm.name,
            at.elapsed().as_secs_f64() * 1e3,
            rt::work_json(&rt::total(&rt::since(&opened.work(), &before)))
        );
        arm.delay(true);
        let o = &opened;
        let mut go = |case: &str, method: &str, fields: Vec<(&str, SdkValue)>| {
            measure(&mut arm, o, case, method, fields)
        };
        match group.as_str() {
            "reads" => {
                go(
                    "stat.depth3",
                    "fs.stat",
                    vec![branch(), path("/src/d05/f050.txt")],
                );
                go(
                    "stat.depth7",
                    "fs.stat",
                    vec![branch(), path("/deep/a/b/c/d/e/leaf.txt")],
                );
                go("stat.root", "fs.stat", vec![branch(), path("/")]);
                go(
                    "read.small_depth3",
                    "fs.read",
                    vec![branch(), path("/src/d05/f050.txt")],
                );
                go(
                    "read.2000_lines",
                    "fs.read",
                    vec![branch(), path("/text/big.txt")],
                );
                go(
                    "read.lines_100_200",
                    "fs.read",
                    vec![
                        branch(),
                        path("/text/big.txt"),
                        ("startLine", SdkValue::Int(100)),
                        ("endLine", SdkValue::Int(200)),
                    ],
                );
                go(
                    "list.dir_100",
                    "fs.list",
                    vec![branch(), path("/src/d05"), ("pageSize", SdkValue::Int(100))],
                );
                go("list.root", "fs.list", vec![branch(), path("/")]);
                go(
                    "glob.dir_100",
                    "fs.glob",
                    vec![branch(), path("/src/d05"), ("pattern", t("f0*"))],
                );
                go(
                    "glob.tree",
                    "fs.glob",
                    vec![branch(), path("/"), ("pattern", t("*"))],
                );
                go(
                    "grep.one_file_dir",
                    "fs.grep",
                    vec![branch(), path("/text"), ("pattern", t("needle"))],
                );
                go(
                    "grep.tree_1k",
                    "fs.grep",
                    vec![branch(), path("/"), ("pattern", t("needle"))],
                );
                let selector =
                    |id: &str| SdkValue::map([("commit", t(id)), ("path", t("/hist/a.txt"))]);
                go(
                    "diff_files.h10_vs_h99",
                    "fs.diff_files",
                    vec![("left", selector(&ids[9])), ("right", selector(&ids[98]))],
                );
            }
            "history" => {
                let at = |n: usize| ("commit", t(&ids[n - 1]));
                go(
                    "blame.h10",
                    "fs.blame",
                    vec![branch(), path("/hist/a.txt"), at(10)],
                );
                go(
                    "blame.h100",
                    "fs.blame",
                    vec![branch(), path("/hist/a.txt")],
                );
                go(
                    "revision.h100",
                    "fs.revision",
                    vec![branch(), path("/hist/a.txt")],
                );
                // Appended at P0 (smaller shapes for the delay arms, D14):
                // measured after the baseline cases on the same open.
                go(
                    "blame.h20",
                    "fs.blame",
                    vec![branch(), path("/hist/a.txt"), at(20)],
                );
                go(
                    "revision.h10",
                    "fs.revision",
                    vec![branch(), path("/hist/a.txt"), at(10)],
                );
                go(
                    "revision.h20",
                    "fs.revision",
                    vec![branch(), path("/hist/a.txt"), at(20)],
                );
            }
            "writes" => {
                go(
                    "write.new_file",
                    "fs.write",
                    vec![branch(), path("/src/d05/new.txt"), bytes("new\n")],
                );
                go(
                    "write.overwrite",
                    "fs.write",
                    vec![branch(), path("/src/d05/f050.txt"), bytes("changed\n")],
                );
                go(
                    "edit.2000_lines",
                    "fs.edit",
                    vec![
                        branch(),
                        path("/text/big.txt"),
                        ("oldText", t("line 1000\n")),
                        ("newText", t("LINE 1000\n")),
                    ],
                );
                go("mkdir", "fs.mkdir", vec![branch(), path("/newdir")]);
                go(
                    "cp.file",
                    "fs.cp",
                    vec![
                        branch(),
                        ("from", t("/src/d05/f051.txt")),
                        ("to", t("/newdir/copy.txt")),
                    ],
                );
                go(
                    "mv.file",
                    "fs.mv",
                    vec![
                        branch(),
                        ("from", t("/newdir/copy.txt")),
                        ("to", t("/newdir/moved.txt")),
                    ],
                );
                go(
                    "rm.file",
                    "fs.rm",
                    vec![branch(), path("/newdir/moved.txt")],
                );
                go("apply.10_writes", "fs.apply", apply("/src/d06", "a"));
                go(
                    "rm.dir_100_recursive",
                    "fs.rm",
                    vec![
                        branch(),
                        path("/src/d07"),
                        ("recursive", SdkValue::Bool(true)),
                    ],
                );
            }
            other => panic!("FS818_GROUP must be reads, history, writes or w0, not {other:?}"),
        }
        opened.stop();
    }));
    arm.delay(false);
    drop(arm);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

fn print_init(arm: &Arm, case: &str, init: &rt::InitOutcome) {
    println!(
        "FS_RT {{\"arm\":\"{}\",\"case\":\"{case}\",\"method\":\"project.init\",\"entries\":{},\"ms\":{:.1},\"client_storage\":{},\"server\":{},\"logical\":{},\"walked\":{},\"stored\":{},\"canonical\":{},\"load1\":\"{}\"}}",
        arm.name,
        init.entries,
        init.ms,
        rt::work_json(&init.work),
        init.server.as_ref().map_or("null".to_owned(), |s| s.json(6)),
        init.logical,
        init.walked,
        init.statistics.stored,
        init.statistics.canonical,
        rt::load1()
    );
}

// ---------------------------------------------------------------------------
// W0: where each write call's SQL work goes

/// One phase boundary: the phase, start (`false`) or end, every role's
/// totals and the server's statement counters at that moment.
type Event = (&'static str, bool, [SqlWork; 3], Option<Statements>);
/// One phase label's work by role and its server statements
/// (`(top-level calls, text)`).
type Labelled = (String, [SqlWork; 3], Vec<(i64, String)>);

/// writes.md W0: each write call's SQL work by phase and role, on a new
/// Store open (`cold`: first use of every session and statement, and the
/// Store's first serial reservation for a create), then once more on the
/// same open (`warm`: the same kind of change at another path). The cases,
/// their order and their arguments are those of `sdk_write_round_trips.rs`,
/// so each call's totals can be checked against its pin: attribution must
/// not change the counts.
///
/// The SDK write route is `files.resolve`, `files.bind` (the `store.bind`
/// phase), `files.change`, the `commit` root with its phases, then
/// `files.close`. Only phases are marked (`telemetry::mark_phases`), not
/// roots, so the work between phases is labelled by position: before
/// `store.bind` is `files.resolve`; from `store.bind`'s end to the first
/// Commit phase is `files.change` (the Commit root reads nothing before its
/// first phase); between Commit phases `commit.between`; after the last
/// phase `files.close`. Inside a phase the work goes to the innermost open
/// phase (self time), so every interval is counted once and the intervals
/// sum to the call. `telemetry::phase_sql` reports each phase's inclusive
/// totals beside them (`phase_sql`).
fn w0(arm: &mut Arm) {
    let setup = arm.store.open("w0-setup");
    rt::fork_work(&setup);
    rt::write_history(&setup);
    setup.stop();
    arm.delay(true);
    type Fields = Vec<(&'static str, SdkValue)>;
    let cases: Vec<(&str, &str, Fields, Fields)> = vec![
        (
            "write.new_file",
            "fs.write",
            vec![branch(), path("/src/d05/new.txt"), bytes("new\n")],
            vec![branch(), path("/src/d05/new2.txt"), bytes("new2\n")],
        ),
        (
            "write.overwrite",
            "fs.write",
            vec![branch(), path("/src/d05/f050.txt"), bytes("changed\n")],
            vec![
                branch(),
                path("/src/d05/f050.txt"),
                bytes("changed again\n"),
            ],
        ),
        (
            "edit.2000_lines",
            "fs.edit",
            vec![
                branch(),
                path("/text/big.txt"),
                ("oldText", t("line 1000\n")),
                ("newText", t("LINE 1000\n")),
            ],
            vec![
                branch(),
                path("/text/big.txt"),
                ("oldText", t("line 1001\n")),
                ("newText", t("LINE 1001\n")),
            ],
        ),
        (
            "mkdir",
            "fs.mkdir",
            vec![branch(), path("/newdir")],
            vec![branch(), path("/newdir2")],
        ),
        (
            "cp.file",
            "fs.cp",
            vec![
                branch(),
                ("from", t("/src/d05/f051.txt")),
                ("to", t("/newdir/copy.txt")),
            ],
            vec![
                branch(),
                ("from", t("/src/d05/f052.txt")),
                ("to", t("/newdir/copy2.txt")),
            ],
        ),
        (
            "mv.file",
            "fs.mv",
            vec![
                branch(),
                ("from", t("/newdir/copy.txt")),
                ("to", t("/newdir/moved.txt")),
            ],
            vec![
                branch(),
                ("from", t("/newdir/copy2.txt")),
                ("to", t("/newdir/moved2.txt")),
            ],
        ),
        (
            "rm.file",
            "fs.rm",
            vec![branch(), path("/newdir/moved.txt")],
            vec![branch(), path("/newdir/moved2.txt")],
        ),
        (
            "apply.10_writes",
            "fs.apply",
            apply("/src/d06", "a"),
            apply("/src/d06", "b"),
        ),
        (
            "rm.dir_100_recursive",
            "fs.rm",
            vec![
                branch(),
                path("/src/d07"),
                ("recursive", SdkValue::Bool(true)),
            ],
            vec![
                branch(),
                path("/src/d08"),
                ("recursive", SdkValue::Bool(true)),
            ],
        ),
    ];
    for (case, method, cold, warm) in cases {
        if !wanted(case) {
            continue;
        }
        let opened = Rc::new(arm.store.open(&format!("w0-{case}")));
        attributed(arm, &opened, case, "cold", method, cold);
        attributed(arm, &opened, case, "warm", method, warm);
        Rc::try_unwrap(opened).ok().unwrap().stop();
    }
}

/// One write call with its phases marked; prints `FS_W0 <json>`.
fn attributed(
    arm: &mut Arm,
    opened: &Rc<Opened>,
    case: &str,
    cache: &str,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
) {
    let server: Option<Rc<RefCell<Server>>> =
        arm.store.server.take().map(|s| Rc::new(RefCell::new(s)));
    let events: Rc<RefCell<Vec<Event>>> = Rc::default();
    let snap = {
        let (opened, server) = (opened.clone(), server.clone());
        move || {
            (
                opened.work(),
                server.as_ref().map(|s| s.borrow_mut().snapshot()),
            )
        }
    };
    let recording = Recording::start();
    let probe = opened.clone();
    attribute_sql(Some(Box::new(move || rt::total(&probe.work()))));
    let (start_work, start_server) = snap();
    events
        .borrow_mut()
        .push(("call", false, start_work, start_server));
    {
        let (events, snap) = (events.clone(), snap.clone());
        mark_phases(Some(Box::new(move |name, end| {
            let (work, server) = snap();
            events.borrow_mut().push((name, end, work, server));
        })));
    }
    let started = Instant::now();
    let reply = opened.call(method, fields);
    let ms = started.elapsed().as_secs_f64() * 1e3;
    mark_phases(None);
    let (end_work, end_server) = snap();
    events
        .borrow_mut()
        .push(("call", true, end_work, end_server));
    let inclusive = phase_sql();
    attribute_sql(None);
    drop(recording.finish());
    drop(snap);
    if let Some(server) = server {
        arm.store.server = Some(Rc::try_unwrap(server).ok().unwrap().into_inner());
    }
    let outcome = match &reply {
        Ok(value) => value.get_text("kind").unwrap_or("ok").to_owned(),
        Err(refusal) => format!("refused {refusal}").replace('"', "'"),
    };
    let events = events.borrow();
    // Intervals between consecutive boundaries, labelled as `w0` says.
    let mut open: Vec<&str> = Vec::new();
    let mut last_closed: Option<&str> = None;
    let mut labelled: Vec<(String, [SqlWork; 3], Option<rt::ServerDelta>)> = Vec::new();
    for pair in events.windows(2) {
        let (name, end, work, server) = &pair[0];
        let (next, next_end, next_work, next_server) = &pair[1];
        if *name != "call" {
            if *end {
                if let Some(at) = open.iter().rposition(|n| n == name) {
                    open.truncate(at);
                }
                last_closed = Some(name);
            } else {
                open.push(name);
            }
        }
        let label = match (open.last(), last_closed) {
            (Some(phase), _) => (*phase).to_owned(),
            (None, None) => "files.resolve".to_owned(),
            (None, Some("store.bind")) => "files.change".to_owned(),
            (None, Some(_)) if *next == "call" && *next_end => "files.close".to_owned(),
            (None, Some(_)) => "commit.between".to_owned(),
        };
        let _ = next_end;
        let delta = rt::since(next_work, work);
        let server = match (server, next_server) {
            (Some(a), Some(b)) => Some(rt::ServerDelta::between(a, b)),
            _ => None,
        };
        labelled.push((label, delta, server));
    }
    // By label, in first-appearance order.
    let mut by_label: Vec<Labelled> = Vec::new();
    for (label, work, server) in &labelled {
        let slot = match by_label.iter().position(|(l, _, _)| l == label) {
            Some(at) => at,
            None => {
                by_label.push((label.clone(), Default::default(), Vec::new()));
                by_label.len() - 1
            }
        };
        let entry = &mut by_label[slot];
        for (total, work) in entry.1.iter_mut().zip(work) {
            total.accumulate(work);
        }
        if let Some(server) = server {
            for (top, _, text) in server.templates.iter().filter(|t| t.0 > 0) {
                let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let text: String = text.chars().take(100).collect();
                match entry.2.iter_mut().find(|(_, t)| *t == text) {
                    Some(found) => found.0 += top,
                    None => entry.2.push((*top, text)),
                }
            }
        }
    }
    let phases: Vec<String> = by_label
        .iter()
        .filter(|(_, work, _)| work.iter().any(|w| w.statements + w.round_trips + w.connects > 0))
        .map(|(label, work, templates)| {
            let mut templates = templates.clone();
            templates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let templates: Vec<String> = templates
                .iter()
                .map(|(n, text)| {
                    format!(
                        "[{n},\"{}\"]",
                        text.replace('\\', "\\\\").replace('"', "\\\"")
                    )
                })
                .collect();
            format!(
                "{{\"phase\":\"{label}\",\"writer\":{:?},\"control\":{:?},\"workspace\":{:?},\"prepares\":{},\"server\":[{}]}}",
                rt::counts(&work[0]),
                rt::counts(&work[1]),
                rt::counts(&work[2]),
                work[0].prepares + work[1].prepares + work[2].prepares,
                templates.join(",")
            )
        })
        .collect();
    let inclusive: Vec<String> = inclusive
        .iter()
        .map(|(name, work)| format!("[\"{name}\",{}]", rt::work_json(work)))
        .collect();
    let total = rt::since(&events.last().unwrap().2, &events[0].2);
    println!(
        "FS_W0 {{\"arm\":\"{}\",\"case\":\"{case}\",\"cache\":\"{cache}\",\"method\":\"{method}\",\"outcome\":\"{outcome}\",\"ms\":{ms:.2},\"roles\":{},\"phases\":[{}],\"phase_sql\":[{}],\"load1\":\"{}\"}}",
        arm.name,
        rt::roles_json(&total),
        phases.join(","),
        inclusive.join(","),
        rt::load1()
    );
}

// ---------------------------------------------------------------------------
// Init

/// Init of `dirs` directories of `per` small files each. A delay arm adds
/// its delay just before the counted Init (after the Store's creation) and
/// removes it right after.
fn init_case(case: &str, dirs: usize, per: usize) {
    let Some(mut arm) = Arm::from_env("init") else {
        return;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let source = arm.store.dir.join("source");
        for d in 0..dirs {
            let sub = source.join(format!("d{d:03}"));
            std::fs::create_dir_all(&sub).unwrap();
            for f in 0..per {
                std::fs::write(sub.join(format!("f{f:05}.txt")), format!("file {d} {f}\n"))
                    .unwrap();
            }
        }
        let netem = arm.netem.clone();
        let init = arm.store.init(&source, &mut |on, server| {
            if let Some(netem) = &netem {
                netem.set(on, server);
            }
        });
        assert_eq!(
            init.logical, init.walked,
            "Init logical equals the Store walk"
        );
        print_init(&arm, case, &init);
    }));
    arm.delay(false);
    drop(arm);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn init_nested_1k() {
    init_case("init.nested_1k", 10, 100);
}

#[test]
fn init_flat_1k() {
    init_case("init.flat_1k", 1, 1000);
}

#[test]
fn init_nested_10k() {
    init_case("init.nested_10k", 100, 100);
}

#[test]
fn init_flat_10k() {
    init_case("init.flat_10k", 1, 10_000);
}
