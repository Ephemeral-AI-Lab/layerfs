//! First-use writer ownership for a directly opened SQLite Store.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use layerfs_protocol::provision::install::{SdkCall, SdkRefusal, SdkValue};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryError, HistoryName,
};
use layerfs_persistence::SqliteHandles;
use layerfs_store_service::StoreService;
use std::{sync::mpsc, time::Duration};

#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;

fn open(fixture: &installed::Fixture) -> StoreService {
    StoreService::open_direct(
        &fixture.config.path,
        installed::BINDING,
        installed::CURSOR,
        &fixture.directory.join("overlay.sqlite"),
    )
    .unwrap()
}

fn call(
    service: &StoreService,
    method: &str,
    fields: impl IntoIterator<Item = (&'static str, SdkValue)>,
) -> Result<SdkValue, SdkRefusal> {
    let fields = fields
        .into_iter()
        .chain([("project", SdkValue::text("installed"))]);
    layerfs_store_service::methods::call(
        service,
        &SdkCall {
            method: method.to_owned(),
            args: SdkValue::Map(
                fields
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ),
        },
    )
}

fn fork(service: &StoreService, branch: &str) -> Result<SdkValue, SdkRefusal> {
    call(service, "branch.fork", [("name", SdkValue::text(branch))])
}

fn stat(service: &StoreService, branch: &str, path: &str) -> SdkValue {
    call(
        service,
        "fs.stat",
        [
            ("branch", SdkValue::text(branch)),
            ("path", SdkValue::text(path)),
        ],
    )
    .unwrap()
}

fn seed_work(
    fixture: &installed::Fixture,
) -> (layerfs_history::LayerStackId, layerfs_history::LayerId) {
    let handles = SqliteHandles::open_writable(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
    )
    .unwrap();
    let main = handles.history.branch(fixture.branch).unwrap().unwrap();
    let name = HistoryName::new("work").unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack: main.stack,
            branch: BranchId::named(main.stack, &name),
            name,
            source: ForkSource::Layer(main.base_layer),
        })
        .unwrap();
    (main.stack, main.base_layer)
}

fn write(service: &StoreService) -> Result<SdkValue, SdkRefusal> {
    call(
        service,
        "fs.write",
        [
            ("branch", SdkValue::text("work")),
            ("path", SdkValue::text("/written")),
            ("bytes", SdkValue::Bytes(b"hello\n".to_vec())),
        ],
    )
}

#[test]
fn read_opens_no_writer_and_close_leaves_none_to_checkpoint() {
    let fixture = installed::Fixture::new(1, "lazy-read-close");
    let service = open(&fixture);
    assert!(service.opened.session_diagnostics(None).unwrap().is_none());
    assert_eq!(
        stat(&service, "main", "/file-000000").get_int("size"),
        Some(installed::bytes(0).len() as i64)
    );
    assert!(service.opened.session_diagnostics(None).unwrap().is_none());
    assert_eq!(service.opened.checkpoint(false).unwrap(), None);
    service.close().unwrap();
    assert!(!fixture.directory.join("overlay.sqlite").exists());
    fixture.cleanup();
}

#[test]
fn first_file_write_uses_the_writer_and_close_preserves_publication() {
    let fixture = installed::Fixture::new(1, "lazy-first-write");
    let (stack, base) = seed_work(&fixture);
    let service = open(&fixture);
    assert!(service.opened.session_diagnostics(None).unwrap().is_none());
    let changed = write(&service).unwrap();
    assert_eq!(changed.get_text("kind"), Some("Committed"));
    assert!(
        service
            .opened
            .session_diagnostics(None)
            .unwrap()
            .unwrap()
            .write_commits
            > 0
    );
    assert_eq!(service.opened.checkpoint(false).unwrap(), Some(true));
    service.close().unwrap();

    let reopened = open(&fixture);
    let id = BranchId::named(stack, &HistoryName::new("work").unwrap());
    let branch = reopened.store.history().branch(id).unwrap().unwrap();
    assert_eq!(branch.base_layer, base);
    assert!(branch.head_commit.is_some());
    assert_eq!(stat(&reopened, "work", "/written").get_int("size"), Some(6));
    assert!(reopened.opened.session_diagnostics(None).unwrap().is_none());
    reopened.close().unwrap();
    fixture.cleanup();
}

#[test]
fn failed_first_write_leaves_the_slot_empty_for_a_later_call() {
    let fixture = installed::Fixture::new(1, "lazy-busy");
    let _ = seed_work(&fixture);
    let external_writer = rusqlite::Connection::open(&fixture.config.path).unwrap();
    external_writer.busy_timeout(Duration::ZERO).unwrap();
    external_writer.execute_batch("BEGIN IMMEDIATE").unwrap();

    let service = open(&fixture);
    assert!(stat(&service, "main", "/file-000000")
        .get_int("size")
        .is_some());
    let refusal = write(&service).unwrap_err();
    assert_eq!(refusal.code, "Busy", "{refusal:?}");
    assert!(service.opened.session_diagnostics(None).unwrap().is_none());
    assert!(!fixture.directory.join("overlay.sqlite").exists());
    external_writer.execute_batch("ROLLBACK").unwrap();
    drop(external_writer);

    let absent = call(
        &service,
        "fs.stat",
        [
            ("branch", SdkValue::text("work")),
            ("path", SdkValue::text("/written")),
        ],
    )
    .unwrap_err();
    assert_eq!(absent.code, "NoSuchPath");
    assert_eq!(write(&service).unwrap().get_text("kind"), Some("Committed"));
    assert!(service.opened.session_diagnostics(None).unwrap().is_some());
    service.close().unwrap();
    fixture.cleanup();
}

#[test]
fn concurrent_first_writes_share_one_writer() {
    let fixture = installed::Fixture::new(1, "lazy-concurrent");
    let service = open(&fixture);
    std::thread::scope(|scope| {
        let (readiness, ready) = mpsc::channel();
        let (result, results) = mpsc::channel();
        let mut starts = Vec::new();
        for branch in ["first", "second"] {
            let readiness = readiness.clone();
            let result = result.clone();
            let service = &service;
            let (start, starting) = mpsc::channel();
            starts.push(start);
            scope.spawn(move || {
                readiness.send(()).unwrap();
                starting.recv_timeout(Duration::from_secs(5)).unwrap();
                result.send(fork(service, branch)).unwrap();
            });
        }
        for _ in 0..2 {
            ready.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        for start in starts {
            start.send(()).unwrap();
        }
        for _ in 0..2 {
            let outcome = results
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(outcome.get_text("kind"), Some("Forked"));
        }
    });
    let writer = service.opened.session_diagnostics(None).unwrap().unwrap();
    // One initial merge-scratch sweep and two forks all ran on this session.
    assert_eq!(writer.write_commits, 3);
    service.close().unwrap();
    let reopened = open(&fixture);
    let main = reopened
        .store
        .history()
        .branch(fixture.branch)
        .unwrap()
        .unwrap();
    for branch in ["first", "second"] {
        let branch = HistoryName::new(branch).unwrap();
        assert!(reopened
            .store
            .history()
            .branch_named(main.stack, &branch)
            .unwrap()
            .is_some());
    }
    reopened.close().unwrap();
    fixture.cleanup();
}

#[test]
fn a_replaced_file_cannot_become_the_first_writer() {
    let fixture = installed::Fixture::new(1, "lazy-replaced");
    let service = open(&fixture);
    let main = service
        .store
        .history()
        .branch(fixture.branch)
        .unwrap()
        .unwrap();
    let replacement = fixture.store_directory().join("replacement.sqlite");
    std::fs::copy(&fixture.config.path, &replacement).unwrap();
    std::fs::rename(replacement, &fixture.config.path).unwrap();

    let name = HistoryName::new("on-replacement").unwrap();
    let branch = BranchId::named(main.stack, &name);
    let error = service
        .store
        .history()
        .fork(&ForkRequest {
            stack: main.stack,
            branch,
            name,
            source: ForkSource::Layer(main.base_layer),
        })
        .unwrap_err();
    assert!(matches!(error, HistoryError::StaleStore), "{error:?}");
    assert!(service.opened.session_diagnostics(None).unwrap().is_none());
    service.close().unwrap();

    let reopened = open(&fixture);
    assert!(reopened.store.history().branch(branch).unwrap().is_none());
    reopened.close().unwrap();
    fixture.cleanup();
}
