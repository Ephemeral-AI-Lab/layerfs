//! The recorded Branch merges of a replay Store, merged again (#355 (d)).
//!
//! Set `LAYERFS_355_REPLAY_STORE` to a closed Store file and
//! `LAYERFS_355_REPLAY_BINDING` to its history binding key (hex), then run
//! with `--ignored`. The Store is copied first; the copy is opened writable
//! and every Commit made by a Branch merge (`via` 3) has its two parents
//! forked and merged with `branch.merge`, oldest first. Each prints one
//! `REPLAY_MERGE` line with the recorded root and the merged one. Merges are
//! deterministic on equal copies, so two builds' lines compare exactly; the
//! recorded root differs only where the merge took fresh serials, whose
//! allocator has moved since.
//!
//! With `LAYERFS_352_REPLAY_UNITS=1` each merge passes the F-057 units
//! `node_modules` and `build` (`packages/*/*/lib`, `**/tsconfig.tsbuildinfo`)
//! and prints one `REPLAY_UNITS` line: the units step's time (classify and
//! revert), each decided unit, and the merge Commit's stored bytes.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/view.rs"]
#[allow(dead_code)]
mod view;

use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_store_service::{bootstrap::open_store_observed, Owner, OwnerConfig, StoreService};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        text.push_str(&format!("{byte:02x}"));
        text
    })
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// A recorded merge: its Commit, both parents and its root.
type Recorded = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

fn call(
    service: &StoreService,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    let mut fields: Vec<(String, SdkValue)> = fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    fields.push(("project".into(), SdkValue::text("dsh")));
    let call = SdkCall {
        method: method.into(),
        args: SdkValue::Map(fields),
    };
    layerfs_api::call(service, &call)
}

#[test]
#[ignore = "needs a replay Store: LAYERFS_355_REPLAY_STORE and LAYERFS_355_REPLAY_BINDING"]
fn recorded_branch_merges_merge_again() {
    let source = std::env::var("LAYERFS_355_REPLAY_STORE").expect("replay Store");
    let binding = unhex(&std::env::var("LAYERFS_355_REPLAY_BINDING").expect("binding key"));
    let directory = std::env::temp_dir().join(format!("layerfs-355-replay-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("store.sqlite");
    std::fs::copy(&source, &path).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
    let merges: Vec<Recorded> = {
        let sql = rusqlite::Connection::open(&path).unwrap();
        let mut statement = sql
            .prepare(
                "SELECT commit_id, parent_commit_id, second_parent_id, root_id FROM \"commit\"
                 WHERE via=3 ORDER BY created_ms, commit_id",
            )
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap();
        let only = std::env::var("LAYERFS_355_REPLAY_ONLY").unwrap_or_default();
        rows.map(Result::unwrap)
            .filter(|(commit, ..): &Recorded| {
                only.is_empty()
                    || only
                        .split(',')
                        .any(|prefix| hex(commit).starts_with(&prefix.to_lowercase()))
            })
            .collect()
    };
    let config =
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable);
    let owner_config = OwnerConfig::default();
    let opened = open_store_observed(
        config,
        &binding,
        installed::CURSOR,
        owner_config.namespaces,
        64 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        owner_config,
    )
    .unwrap();
    let service = StoreService::new(opened, &owner);
    // With `LAYERFS_355_VIEW_DIR`, each merge's decoded view (or refusal) is
    // written to `<index>.txt` there, for comparing two builds (#355 M1).
    let views = std::env::var_os("LAYERFS_355_VIEW_DIR").map(std::path::PathBuf::from);
    let viewer = views
        .as_ref()
        .map(|_| view::Viewer::open(&path, &binding, installed::CURSOR));
    let units = std::env::var_os("LAYERFS_352_REPLAY_UNITS").map(|_| {
        let group = |name: &str, patterns: &[&str]| {
            SdkValue::map([
                ("name", SdkValue::text(name)),
                (
                    "patterns",
                    SdkValue::List(patterns.iter().map(|p| SdkValue::text(*p)).collect()),
                ),
            ])
        };
        SdkValue::List(vec![
            group("node_modules", &["node_modules"]),
            group("build", &["packages/*/*/lib", "**/tsconfig.tsbuildinfo"]),
        ])
    });
    let (mut same, mut merged_count) = (0, 0);
    for (index, (commit, parent, second, recorded)) in merges.iter().enumerate() {
        let (left, right) = (format!("replay-l{index}"), format!("replay-r{index}"));
        for (name, from) in [(&left, parent), (&right, second)] {
            call(
                &service,
                "branch.fork",
                vec![
                    ("name", SdkValue::text(name)),
                    ("from", SdkValue::text(hex(from))),
                ],
            )
            .unwrap();
        }
        let before = service.opened.diagnostics().unwrap().writer;
        let mut fields = vec![
            ("branch", SdkValue::text(&left)),
            ("with", SdkValue::text(&right)),
            ("name", SdkValue::text(format!("replay-m{index}"))),
        ];
        if let Some(units) = &units {
            fields.push(("units", units.clone()));
        }
        let recording = layerfs_store_service::telemetry::Recording::start();
        let started = std::time::Instant::now();
        let merged = call(&service, "branch.merge", fields);
        let ms = started.elapsed().as_secs_f64() * 1e3;
        let reports = recording.finish();
        let root_ms = |name: &str| {
            reports
                .iter()
                .filter_map(|report| report.root())
                .filter(|node| node.name() == name)
                .map(|node| node.elapsed().as_secs_f64() * 1e3)
                .sum::<f64>()
        };
        let stored = merged
            .as_ref()
            .ok()
            .filter(|reply| reply.get("fastForward") == Some(&SdkValue::Bool(false)))
            .map(|_| {
                let commits = call(
                    &service,
                    "branch.list_commits",
                    vec![("branch", SdkValue::text(format!("replay-m{index}")))],
                )
                .unwrap();
                let Some(SdkValue::List(items)) = commits.get("items") else {
                    return "none".to_owned();
                };
                let int = |name| match items[0].get(name) {
                    Some(SdkValue::Int(n)) => *n,
                    _ => -1,
                };
                format!(
                    "{}/{}/{}",
                    int("newCanonical"),
                    int("newStored"),
                    int("newMetadata")
                )
            })
            .unwrap_or_else(|| "none".to_owned());
        let decided = match &merged {
            Ok(reply) => reply.get("units").cloned(),
            Err(refusal) => refusal.detail.get("units").cloned(),
        };
        let step_ms = |name: &str| {
            reports
                .iter()
                .filter_map(|report| report.root())
                .flat_map(|node| node.children())
                .filter(|node| node.name() == name)
                .map(|node| node.elapsed().as_secs_f64() * 1e3)
                .sum::<f64>()
        };
        println!(
            "REPLAY_UNITS {index} units_ms={:.1} classify_ms={:.1} revert_ms={:.1} merge_ms={:.1} stored={stored} decided={decided:?}",
            root_ms("branch.merge.units"),
            step_ms("units.classify"),
            step_ms("units.revert"),
            root_ms("branch.merge"),
        );
        let after = service.opened.diagnostics().unwrap().writer;
        println!(
            "REPLAY_WORK {index} ms={ms:.1} statements={} vm_steps={} fullscan_steps={} sorts={}",
            after.statements - before.statements,
            after.vm_steps - before.vm_steps,
            after.fullscan_steps - before.fullscan_steps,
            after.sorts - before.sorts
        );
        let result = match &merged {
            Ok(reply) => {
                merged_count += 1;
                let root = reply.get_text("root").unwrap_or("none").to_owned();
                if root == hex(recorded) {
                    same += 1;
                }
                format!("kind={} root={root}", reply.get_text("kind").unwrap_or("?"))
            }
            Err(refusal) => format!("refused={}:{}", refusal.code, refusal.message),
        };
        if let (Some(views), Some(viewer)) = (&views, &viewer) {
            let lines = match &merged {
                Ok(reply) => viewer.view(reply.get_text("root").unwrap(), true),
                Err(refusal) => vec![format!("refused {} {:?}", refusal.code, refusal.detail)],
            };
            std::fs::write(views.join(format!("{index}.txt")), lines.join("\n")).unwrap();
            // The merge's inputs, for telling one side's change from both.
            let sql = rusqlite::Connection::open(&path).unwrap();
            let root_of = |node: &[u8]| -> Vec<u8> {
                sql.query_row(
                    "SELECT root_id FROM \"commit\" WHERE commit_id=?1
                     UNION ALL SELECT root_id FROM layer WHERE layer_id=?1",
                    [node],
                    |row| row.get(0),
                )
                .unwrap()
            };
            let base = merged
                .as_ref()
                .ok()
                .and_then(|reply| reply.get_text("mergeBase"))
                .map(unhex);
            for (label, node) in [
                ("ours", Some(parent.clone())),
                ("theirs", Some(second.clone())),
                ("base", base),
            ] {
                if let Some(node) = node {
                    let lines = viewer.view(&hex(&root_of(&node)), true);
                    std::fs::write(views.join(format!("{index}.{label}.txt")), lines.join("\n"))
                        .unwrap();
                }
            }
        }
        println!(
            "REPLAY_MERGE {index} commit={} recorded={} {result}",
            hex(commit),
            hex(recorded)
        );
    }
    println!(
        "REPLAY_SUMMARY merges={} merged={merged_count} equal_to_recorded={same}",
        merges.len()
    );
    drop(service);
    owner.stop().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
