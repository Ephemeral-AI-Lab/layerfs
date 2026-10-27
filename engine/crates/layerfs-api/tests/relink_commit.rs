//! A clean relink's Commit on a replay Store (#364 items 2 and 3).
//!
//! Set `LAYERFS_364_RELINK_STORE` to a closed Store file,
//! `LAYERFS_364_RELINK_BINDING` to its history binding key (hex) and
//! `LAYERFS_364_RELINK_FROM` to the Commit to start from (hex), then run with
//! `--ignored`. The Store is copied first. A private Workspace on a Branch
//! forked from that Commit unlinks every regular file under a `node_modules`
//! directory and creates it again with the same bytes, mode and mtime, as pnpm's
//! copy import does after `rm -rf node_modules`: new inodes, identical bytes.
//! Only the Commit is timed; it prints one `RELINK_COMMIT` line per round
//! and one `RELINK_HEAD` line with each new Commit's identity.
//! `LAYERFS_364_RELINK_LIMIT` relinks only that many files.
//! `LAYERFS_364_RELINK_ROUNDS=2` relinks again over the first relink's
//! Commit and then pages `branch.diff_commits` between the two (range 16's
//! relink-over-relink verify), printing one `RELINK_DIFF` line and
//! `RELINK_DIFF_PAGE` lines for the first, every 25th and the last page.
//! `LAYERFS_364_RELINK_TIMES=now` leaves each new file its write time, as
//! pnpm's copy does; by default the original times are set again, so the
//! root depends only on the build.
//! `LAYERFS_364_RELINK_SAVE=<path>` keeps the Store after the run there;
//! `LAYERFS_364_RELINK_RESUME=1` starts from such a Store on its existing
//! `relink` Branch, whose head is the first side of the diff. Together they
//! split a two-round run too long for one process.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;

use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::PathName;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_history::{CommitVia, WorkspaceId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_store_service::write::mutation::Mutation;
use layerfs_store_service::{bootstrap::open_store_observed, Owner, OwnerConfig, StoreService};
use layerfs_workspace::{Operation, Time};

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn call(service: &StoreService, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields: Vec<(String, SdkValue)> = fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    fields.push(("project".into(), SdkValue::text("dsh")));
    let call = SdkCall {
        method: method.into(),
        args: SdkValue::Map(fields),
    };
    layerfs_api::call(service, &call).unwrap()
}

/// Every regular file under a `node_modules` directory: path, serial, length
/// and metadata.
fn files(m: &Mutation<'_>) -> Vec<(String, u64, u64, PortableMetadata)> {
    let mut out = Vec::new();
    let mut stack = vec![("".to_owned(), m.resolve("/").unwrap().serial, false)];
    while let Some((path, serial, inside)) = stack.pop() {
        let mut after: Option<Vec<u8>> = None;
        loop {
            let page = m
                .view
                .list(m.op.overlay(), serial, after.as_deref())
                .unwrap();
            for (name, child) in page.entries {
                let name = String::from_utf8(name).unwrap();
                if path.is_empty() && name == ".git" {
                    continue;
                }
                let child_path = format!("{path}/{name}");
                let stat = m.view.stat(m.op.overlay(), child).unwrap();
                match stat.kind {
                    InodeKind::Directory => {
                        stack.push((child_path, child, inside || name == "node_modules"))
                    }
                    InodeKind::RegularFile if inside && stat.namespace_refs == 1 => {
                        out.push((child_path, child, stat.logical_len, stat.metadata))
                    }
                    _ => {}
                }
            }
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)],
                DIGITS[usize::from(byte & 15)],
            ]
        })
        .map(char::from)
        .collect()
}

/// Statements of the writer and every reader session so far.
fn statements(service: &StoreService) -> (u64, u64) {
    let all = service.opened.diagnostics().unwrap();
    let readers = all.readers.iter().map(|work| work.statements).sum::<u64>();
    (all.writer.statements, readers)
}

/// One clean relink of the bound Workspace and its timed Commit; the new
/// Commit's identity text.
fn relink(
    service: &StoreService,
    workspace: &layerfs_store_service::store::BoundWorkspace,
    round: usize,
    keep_times: bool,
) -> String {
    let started = std::time::Instant::now();
    let mut m = Mutation::new(workspace).unwrap();
    let mut list = files(&m);
    if let Ok(limit) = std::env::var("LAYERFS_364_RELINK_LIMIT") {
        list.truncate(limit.parse().unwrap());
    }
    let mut bytes_total = 0u64;
    let mut touched = std::collections::BTreeMap::new();
    for (file, serial, length, metadata) in &list {
        let mut bytes = Vec::with_capacity(*length as usize);
        while (bytes.len() as u64) < *length {
            let want = (*length - bytes.len() as u64).min(128 << 10) as u32;
            let piece = m.read(*serial, bytes.len() as u64, want).unwrap();
            assert!(!piece.is_empty());
            bytes.extend_from_slice(&piece);
        }
        bytes_total += length;
        let (parent, name) = file.rsplit_once('/').unwrap();
        let parent = m.resolve(parent).unwrap();
        touched.entry(parent.serial).or_insert(parent.metadata);
        let parent = parent.serial;
        m.apply(Operation::Unlink {
            parent,
            name: PathName::new(name).unwrap(),
        })
        .unwrap();
        m.write(file, &bytes, None).unwrap();
        let created = m.resolve(file).unwrap().serial;
        assert_ne!(created, *serial, "a new inode");
        m.apply(Operation::SetAttributes {
            serial: created,
            mode: Some(metadata.mode),
            // The original time keeps the root comparable across builds.
            mtime: keep_times.then_some(Time {
                seconds: metadata.mtime_seconds,
                nanoseconds: metadata.mtime_nanoseconds,
            }),
            size: None,
        })
        .unwrap();
    }
    // Directories keep their times too, so the root depends only on the build.
    if keep_times {
        for (serial, metadata) in touched {
            m.apply(Operation::SetAttributes {
                serial,
                mode: None,
                mtime: Some(Time {
                    seconds: metadata.mtime_seconds,
                    nanoseconds: metadata.mtime_nanoseconds,
                }),
                size: None,
            })
            .unwrap();
        }
    }
    m.finish().unwrap();
    println!(
        "RELINK_SETUP round={round} files={} bytes={bytes_total} ms={:.0}",
        list.len(),
        started.elapsed().as_secs_f64() * 1e3
    );
    let before = service.opened.diagnostics().unwrap().writer;
    let started = std::time::Instant::now();
    let outcome = workspace
        .commit_captured_via(
            &layerfs_store_service::store::CommitOptions {
                force: false,
                message: Some("relink".to_owned()),
            },
            CommitVia::Files,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let after = service.opened.diagnostics().unwrap().writer;
    let (root, id) = match &outcome.history {
        Some(layerfs_history::CommitOutcome::Committed { record, .. }) => (
            format!("{:?}", record.root),
            hex(layerfs_history::NodeId::Commit(record.id).as_slice()),
        ),
        other => panic!("{other:?}"),
    };
    println!(
        "RELINK_COMMIT round={round} files={} ms={ms:.1} statements={} vm_steps={} fullscan_steps={} inserted={} reused={} stored_bytes={} root={root}",
        list.len(),
        after.statements - before.statements,
        after.vm_steps - before.vm_steps,
        after.fullscan_steps - before.fullscan_steps,
        outcome.saved.inserted,
        outcome.saved.reused,
        outcome.saved.stored_bytes,
    );
    if let Some(work) = outcome.namespace.as_ref().and_then(|held| held.work) {
        println!("RELINK_WORK round={round} {work:?}");
    }
    id
}

#[test]
#[ignore = "needs a replay Store: LAYERFS_364_RELINK_STORE, _BINDING and _FROM"]
fn clean_relink_commit() {
    let source = std::env::var("LAYERFS_364_RELINK_STORE").expect("replay Store");
    let binding = unhex(&std::env::var("LAYERFS_364_RELINK_BINDING").expect("binding key"));
    let from = std::env::var("LAYERFS_364_RELINK_FROM").expect("Commit to start from");
    let rounds: usize = std::env::var("LAYERFS_364_RELINK_ROUNDS")
        .map(|rounds| rounds.parse().unwrap())
        .unwrap_or(1);
    let keep_times = std::env::var("LAYERFS_364_RELINK_TIMES").as_deref() != Ok("now");
    let directory = std::env::temp_dir().join(format!("layerfs-364-relink-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("store.sqlite");
    std::fs::copy(&source, &path).unwrap();
    let wal = |path: &std::path::Path| path.with_extension("sqlite-wal");
    if wal(std::path::Path::new(&source)).exists() {
        std::fs::copy(wal(std::path::Path::new(&source)), wal(&path)).unwrap();
    }
    let resume = std::env::var("LAYERFS_364_RELINK_RESUME").is_ok();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
    let opened = open_store_observed(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &binding,
        installed::CURSOR,
        OwnerConfig::default().namespaces,
        64 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let service = StoreService::new(opened, &owner);
    if !resume {
        call(
            &service,
            "branch.fork",
            vec![
                ("name", SdkValue::text("relink")),
                ("from", SdkValue::text(&from)),
            ],
        );
    }
    let store = &service.store;
    let stack = layerfs_store_service::history::lookup::project(store, "dsh").unwrap();
    let branch = layerfs_store_service::history::lookup::branch(store, &stack, "relink").unwrap();
    let workspace = layerfs_store_service::telemetry::timed("bind", |timing| {
        service.bind(
            WorkspaceId::from_authority([64; 32]).unwrap(),
            branch.id,
            timing,
        )
    })
    .unwrap();
    let head = branch
        .head_commit
        .filter(|_| resume)
        .map(|id| hex(layerfs_history::NodeId::Commit(id).as_slice()));
    let commits: Vec<String> = head
        .into_iter()
        .chain((1..=rounds).map(|round| relink(&service, &workspace, round, keep_times)))
        .collect();
    for commit in &commits {
        println!("RELINK_HEAD {commit}");
    }
    drop(workspace);
    if let [first, second] = commits.as_slice() {
        // Range 16's verify: every page of the diff between two relinks.
        let (writer, readers) = statements(&service);
        let started = std::time::Instant::now();
        let (mut pages, mut entries, mut cursor) = (0u64, 0usize, SdkValue::Null);
        let mut each = Vec::new();
        loop {
            let (page_started, page_statements) = (std::time::Instant::now(), statements(&service));
            let page = call(
                &service,
                "branch.diff_commits",
                vec![
                    ("from", SdkValue::text(first)),
                    ("to", SdkValue::text(second)),
                    ("pageSize", SdkValue::Int(200)),
                    ("cursor", cursor.clone()),
                ],
            );
            pages += 1;
            let listed = match page.get("items") {
                Some(SdkValue::List(items)) => items.len(),
                _ => 0,
            };
            entries += listed;
            let after = statements(&service);
            let spent = after.0 + after.1 - page_statements.0 - page_statements.1;
            each.push((page_started.elapsed().as_secs_f64() * 1e3, spent, listed));
            cursor = page.get("nextCursor").cloned().unwrap_or(SdkValue::Null);
            if matches!(cursor, SdkValue::Null) {
                break;
            }
        }
        let ms = started.elapsed().as_secs_f64() * 1e3;
        // Each page's own cost, at the first, every 25th and the last page:
        // whether it follows the page's position in the difference.
        for (index, (page_ms, spent, listed)) in each.iter().enumerate() {
            if index % 25 == 0 || index + 1 == each.len() {
                println!(
                    "RELINK_DIFF_PAGE page={} ms={page_ms:.1} statements={spent} entries={listed}",
                    index + 1
                );
            }
        }
        let (writer_after, readers_after) = statements(&service);
        let read = readers_after - readers + (writer_after - writer);
        println!(
            "RELINK_DIFF pages={pages} entries={entries} ms={ms:.1} statements={read} per_page={:.0} ms_per_entry={:.3}",
            read as f64 / pages as f64,
            ms / entries.max(1) as f64,
        );
    }
    drop(service);
    owner.stop().unwrap();
    if let Ok(save) = std::env::var("LAYERFS_364_RELINK_SAVE") {
        let save = std::path::PathBuf::from(save);
        std::fs::copy(&path, &save).unwrap();
        if wal(&path).exists() {
            std::fs::copy(wal(&path), wal(&save)).unwrap();
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
