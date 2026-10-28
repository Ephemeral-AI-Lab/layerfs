//! Every page of one Commit difference on a replay Store (#364 item 4).
//!
//! Set `LAYERFS_364_DIFF_STORE` to a closed Store file,
//! `LAYERFS_364_DIFF_BINDING` to its history binding key (hex) and
//! `LAYERFS_364_DIFF_FROM` / `_TO` to the two Commits (hex), then run with
//! `--ignored`. The Store is copied first. The pages are asked as the
//! per-range verify asks them (no hunks, page size 1000, `LAYERFS_364_DIFF_PAGE`
//! overrides it), each resuming from the previous page's cursor. It prints one
//! `DIFF_PAGE` line per page and one `DIFF_REPLAY` line for the whole
//! difference, its time the sum of the calls'; `LAYERFS_364_DIFF_OUT=<path>` writes every page's reply there,
//! to compare two builds' answers byte for byte. `LAYERFS_364_DIFF_PAGES`
//! stops after that many pages; `LAYERFS_364_DIFF_CURSOR` and `_POS` start
//! from an earlier run's next page, to split a difference too long for one
//! process.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;

use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_store_service::{bootstrap::open_store_observed, Owner, OwnerConfig, StoreService};
use std::io::Write;

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// Statements, VM steps and blob bytes read of the writer and every reader.
fn work(service: &StoreService) -> [u64; 3] {
    let all = service.opened.diagnostics().unwrap();
    let mut out = [0; 3];
    for w in std::iter::once(&all.writer).chain(all.readers.iter()) {
        out[0] += w.statements;
        out[1] += w.vm_steps;
        out[2] += w.blob_read_bytes;
    }
    out
}

#[test]
#[ignore = "needs a replay Store: LAYERFS_364_DIFF_STORE, _BINDING, _FROM and _TO"]
fn diff_replay() {
    let var = |name: &str| std::env::var(format!("LAYERFS_364_DIFF_{name}")).ok();
    let source = var("STORE").expect("replay Store");
    let binding = unhex(&var("BINDING").expect("binding key"));
    let (from, to) = (var("FROM").expect("from"), var("TO").expect("to"));
    let size: i64 = var("PAGE").map_or(1000, |page| page.parse().unwrap());
    let most: u64 = var("PAGES").map_or(u64::MAX, |pages| pages.parse().unwrap());
    let directory = std::env::temp_dir().join(format!("layerfs-364-diff-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("store.sqlite");
    std::fs::copy(&source, &path).unwrap();
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
    let mut out = var("OUT").map(|out| std::fs::File::create(out).unwrap());
    let mut cursor = var("CURSOR").map_or(SdkValue::Null, SdkValue::Text);
    let mut pos: i64 = var("POS").map_or(0, |pos| pos.parse().unwrap());
    let (mut pages, mut entries) = (0u64, 0usize);
    let first = work(&service);
    let mut ms = 0.0;
    loop {
        let before = work(&service);
        let at = std::time::Instant::now();
        let call = SdkCall {
            method: "branch.diff_commits".into(),
            args: SdkValue::Map(vec![
                ("project".into(), SdkValue::text("dsh")),
                ("from".into(), SdkValue::text(&from)),
                ("to".into(), SdkValue::text(&to)),
                ("pageSize".into(), SdkValue::Int(size)),
                ("pagePos".into(), SdkValue::Int(pos)),
                ("cursor".into(), cursor.clone()),
                ("hunks".into(), SdkValue::Bool(false)),
            ]),
        };
        let page = layerfs_api::call(&service, &call).unwrap();
        let call_ms = at.elapsed().as_secs_f64() * 1e3;
        ms += call_ms;
        let after = work(&service);
        let listed = match page.get("items") {
            Some(SdkValue::List(items)) => items.len(),
            _ => 0,
        };
        println!(
            "DIFF_PAGE page={pages} entries={listed} ms={call_ms:.2} statements={} vm_steps={} blob_bytes={}",
            after[0] - before[0],
            after[1] - before[1],
            after[2] - before[2],
        );
        if let Some(out) = out.as_mut() {
            writeln!(out, "{:?}", page.get("items")).unwrap();
        }
        pages += 1;
        entries += listed;
        cursor = page.get("nextCursor").cloned().unwrap_or(SdkValue::Null);
        pos = match page.get("nextPagePos") {
            Some(SdkValue::Int(next)) => *next,
            _ => 0,
        };
        if matches!(cursor, SdkValue::Null) || pages == most {
            println!("DIFF_NEXT pos={pos} cursor={cursor:?}");
            break;
        }
    }
    let last = work(&service);
    let per = |total: u64| total as f64 / entries.max(1) as f64;
    println!(
        "DIFF_REPLAY pages={pages} entries={entries} ms={ms:.1} statements={} vm_steps={} blob_bytes={} ms_per_entry={:.4} statements_per_entry={:.2} vm_steps_per_entry={:.0}",
        last[0] - first[0],
        last[1] - first[1],
        last[2] - first[2],
        ms / entries.max(1) as f64,
        per(last[0] - first[0]),
        per(last[1] - first[1]),
    );
    drop(service);
    owner.stop().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
