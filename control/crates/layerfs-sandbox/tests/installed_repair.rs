//! `sandbox.create`'s `installed` step repairs an incomplete SQLite project
//! record (missing `manifest`/`installed`, no `op`) from the local Store file
//! before launching the daemon (#749).
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{paths, private_dir};
use layerfs_home::names::StoreName;
use layerfs_sandbox::host::{Deploy, Host, Message};
use layerfs_sandbox::lifecycle;
use layerfs_store::backup::restored;
use layerfs_store::project::add::{add_project, ProjectAdd};
use layerfs_store::project::discovery;
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::StoreRecord;
use layerfs_store::sqlite::create::create;

#[test]
fn sandbox_create_repairs_an_incomplete_sqlite_project_record() {
    let home = std::env::temp_dir().join(format!("lfs-installed-repair-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    private_dir(&home).unwrap();
    let store_record = match create(&home, "shop", None) {
        Ok(_) => StoreRecord::read(&home, "shop").unwrap().unwrap(),
        Err(refusal) if refusal.code == "Unsupported" => {
            eprintln!("NOT_RUN: unsupported Store filesystem: {refusal:?}");
            let _ = std::fs::remove_dir_all(&home);
            return;
        }
        Err(refusal) => panic!("create: {refusal:?}"),
    };
    let source = home.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), b"hello\n").unwrap();
    let added = add_project(&ProjectAdd {
        home: home.clone(),
        store: StoreName::parse("shop").unwrap(),
        name: "api".to_owned(),
        source,
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    layerfs_store::open::close_idle(std::time::Duration::ZERO);

    let stack = added.get_text("stack").unwrap().to_owned();
    ProjectRecord::from_record(
        "shop",
        "api",
        SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", SdkValue::text("api")),
            ("stack", SdkValue::text(&stack)),
            ("created", SdkValue::Int(55)),
        ]),
    )
    .write(&home)
    .unwrap();
    assert_eq!(
        discovery::project(Some(&home), Some("shop"), Some("api"))
            .err()
            .map(|r| r.code),
        Some("NoSuchProject".to_owned())
    );

    let sandbox = if cfg!(target_os = "linux") {
        "namespaces"
    } else {
        "native"
    };
    let host = Host {
        home: home.clone(),
        store: "shop".to_owned(),
        project: "api".to_owned(),
        sandbox: Some("s1".to_owned()),
        store_record: Some(store_record),
        store_path: Some(paths::store_file(&home, "shop").display().to_string()),
        deploy: Some(Deploy {
            sandbox: Some(sandbox.to_owned()),
            ..Deploy::default()
        }),
        postgresql: None,
        message: Message::Absent,
        started: None,
        reach_hint: None,
    };
    // the repair happens before launch; a host that can launch a daemon
    // starts one, which is removed again
    match lifecycle::create(&host, None, None) {
        Ok(_) => {
            lifecycle::close(&host, true, &[]).expect("the started Sandbox is removed");
        }
        Err(err) => assert_ne!(
            err.code, "NoSuchProject",
            "must heal manifest instead of failing NoSuchProject: {err:?}"
        ),
    }
    let repaired = ProjectRecord::read(&home, "shop", "api").unwrap().unwrap();
    assert_eq!(repaired.created(), Some(&SdkValue::Int(55)));
    assert!(restored::complete(&repaired));
    discovery::project(Some(&home), Some("shop"), Some("api"))
        .expect("repaired project opens cleanly");
    let _ = std::fs::remove_dir_all(&home);
}
