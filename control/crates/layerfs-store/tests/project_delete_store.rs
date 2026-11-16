//! `project.delete`'s home path against a real local Store (T4.6): the
//! control layer writes its sidecar naming the admission, then admits the
//! project scope through `gc_register`/`gc_release`, so a live mount refuses
//! with `MountsActive` (its holders) and a live operation with `Busy`; a
//! refusal leaves a pending sidecar, no marker and no leaked admission
//! intent.
use layerfs_bridge::control::SdkValue;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryName, LayerStackId, NamedSnapshot};
use layerfs_home::events::Events;
use layerfs_home::home::{layout, paths};
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::add::{add_project, ProjectAdd};
use layerfs_store::project::delete::{delete, Intent};
use layerfs_store::record::StoreRecord;
use layerfs_store::service::with_service;
use layerfs_store::sqlite::create::create;
use layerfs_store_service::history::lookup;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-delete-store-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    layout::create(&directory).unwrap();
    directory
}

/// A folder to initialize a project from: no home layout inside it.
fn plaindir(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-delete-store-folder-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// True when the sandbox filesystem cannot hold a directly opened Store, so
/// these real-Store cases cannot run here. The fake-home cases in
/// `project_delete` cover the same home path without a Store.
fn no_direct_store(refusal: &layerfs_bridge::control::SdkRefusal) -> bool {
    refusal.code == "Unsupported" && refusal.message.contains("does not carry SQLite locks")
}

/// A home whose Store `shop` holds the live project `app`, and the
/// `OpenArgs` (with the Store record) that reach it. `None` when this
/// filesystem cannot hold a directly opened Store.
fn shop(tag: &str) -> Option<(PathBuf, OpenArgs, LayerStackId)> {
    let home = tempdir(tag);
    match create(&home, &StoreName::parse("shop").unwrap()) {
        Ok(_) => {}
        Err(refusal) if no_direct_store(&refusal) => {
            eprintln!("NOT RUN: this filesystem cannot hold a directly opened Store: {refusal:?}");
            let _ = std::fs::remove_dir_all(&home);
            return None;
        }
        Err(refusal) => panic!("store.create: {refusal:?}"),
    }
    let source = plaindir(tag);
    std::fs::write(source.join("a.txt"), b"a\n").unwrap();
    add_project(&ProjectAdd {
        home: home.clone(),
        store: StoreName::parse("shop").unwrap(),
        name: "app".to_owned(),
        source,
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    let record = StoreRecord::read(&home, "shop").unwrap().unwrap();
    let open = OpenArgs {
        home: Some(home.clone()),
        store: Some("shop".to_owned()),
        project: Some("app".to_owned()),
        store_path: Some(paths::store_file(&home, "shop").display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    let stack = with_service(&open, |service| {
        Ok(lookup::project(&service.store, "app").unwrap().id)
    })
    .unwrap();
    Some((home, open, stack))
}

/// `project.delete shop/app` through the control-layer home path.
fn remove(home: &std::path::Path) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    let mut sink = |_| {};
    delete(
        home,
        &mut Events::new(&mut sink),
        &ProjectRef::parse("shop/app").unwrap(),
    )
}

/// The live intents at the project scope.
fn intents(open: &OpenArgs, stack: LayerStackId) -> Vec<GcIntent> {
    with_service(open, |service| {
        Ok(service
            .store
            .history()
            .gc_intents(GcScopeKind::Project, stack.as_slice())
            .unwrap())
    })
    .unwrap()
}

/// Registers a live mount of the project's `main` Branch, as a Workspace
/// bind does before it demands the root.
fn mount_main(open: &OpenArgs, stack: LayerStackId, op: [u8; 16]) {
    // `lookup::branch` refuses `main` by design; the snapshot lookup names it
    let branch = with_service(open, |service| {
        let project = HistoryName::new("app").unwrap();
        let main = HistoryName::new("main").unwrap();
        match service
            .store
            .published(|history| history.branch_snapshot_named(&project, &main))
            .unwrap()
        {
            NamedSnapshot::Found(snapshot) => Ok(snapshot.branch.id),
            other => panic!("no main Branch: {other:?}"),
        }
    })
    .unwrap();
    let intent = GcIntent::shared(
        op,
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("app"), Some("main"))
    .placed(Some([9; 32]), Some("sandbox-9"), Some("machine-9"));
    with_service(open, |service| {
        service.store.history().gc_register(&intent).unwrap();
        Ok(())
    })
    .unwrap();
}

fn release(open: &OpenArgs, op: [u8; 16]) {
    with_service(open, |service| {
        service.store.history().gc_release(&op).unwrap();
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_live_mount_refuses_the_delete_after_the_sidecar_and_leaves_no_intent() {
    let Some((home, open, stack)) = shop("mount") else {
        return;
    };
    mount_main(&open, stack, [9; 16]);
    assert!(intents(&open, stack).is_empty(), "no project-scope row yet");

    let refusal = remove(&home).expect_err("a live mount holds the project");
    assert_eq!(refusal.code, "MountsActive", "{refusal:?}");
    let SdkValue::List(holders) = refusal.detail.get("holders").expect("holders") else {
        panic!("{refusal:?}");
    };
    assert_eq!(holders[0].get_text("kind"), Some("mount"));
    assert_eq!(holders[0].get_text("sandbox"), Some("sandbox-9"));

    // A pending sidecar names the identity, but no marker and no leaked
    // admission intent.
    let pending = Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the pending sidecar precedes the admission");
    assert_eq!(pending.state, layerfs_store::project::delete::IntentState::Pending);
    assert!(pending.op.is_some());
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    assert!(
        intents(&open, stack).is_empty(),
        "the refused admission left no project-scope row"
    );
    with_service(&open, |service| {
        assert_eq!(
            lookup::project(&service.store, "app")
                .expect("still live")
                .id,
            stack
        );
        Ok(())
    })
    .unwrap();

    // The mount goes and the same home path retries the pending sidecar: it
    // writes the marker and settles, keeping the entry for the next call.
    release(&open, [9; 16]);
    let outcome = remove(&home).unwrap();
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert_eq!(outcome.get_text("project"), Some("shop/app"));
    assert!(
        !outcome.get_bool("home_entry_removed"),
        "a pending retry settles without finishing the entry"
    );
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    let settled = Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the sidecar is marked");
    assert_eq!(
        settled.state,
        layerfs_store::project::delete::IntentState::Marked
    );
    assert!(
        intents(&open, stack).is_empty(),
        "the admission was released"
    );

    // The marker is confirmed: the next call removes the entry and unlinks
    // the sidecar.
    let outcome = remove(&home).unwrap();
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, "shop", "app").exists());
    assert_eq!(Intent::read(&home, "shop", "app").unwrap(), None);
    assert!(
        intents(&open, stack).is_empty(),
        "the admission was released"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_live_operation_at_the_project_scope_is_busy_and_keeps_the_pending_sidecar() {
    let Some((home, open, stack)) = shop("busy") else {
        return;
    };
    assert!(intents(&open, stack).is_empty());
    let holder = GcIntent::operation(
        [7; 16],
        GcIntentKind::Removal,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("app"), None);
    with_service(&open, |service| {
        service.store.history().gc_register(&holder).unwrap();
        Ok(())
    })
    .unwrap();

    let refusal = remove(&home).expect_err("another removal holds the project");
    assert_eq!(refusal.code, "Busy", "{refusal:?}");
    let pending = Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the pending sidecar stays");
    assert_eq!(pending.state, layerfs_store::project::delete::IntentState::Pending);
    assert!(pending.op.is_some());
    assert_eq!(intents(&open, stack).len(), 1, "only the test's own holder");
    assert_eq!(intents(&open, stack)[0].op_id, [7; 16]);

    // The operation ends; the pending retry redos the marker, keeps the
    // entry and releases its own admission.
    release(&open, [7; 16]);
    let outcome = remove(&home).unwrap();
    assert!(!outcome.get_bool("home_entry_removed"));
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    assert_eq!(
        Intent::read(&home, "shop", "app").unwrap().unwrap().state,
        layerfs_store::project::delete::IntentState::Marked
    );
    assert!(intents(&open, stack).is_empty());
    // The marker is confirmed: the next call finishes the entry.
    assert!(remove(&home).unwrap().get_bool("home_entry_removed"));
    assert_eq!(Intent::read(&home, "shop", "app").unwrap(), None);
    assert!(intents(&open, stack).is_empty());
    let _ = std::fs::remove_dir_all(&home);
}
