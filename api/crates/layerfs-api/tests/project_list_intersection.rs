//! project.list and project.get intersect the home's project entries with
//! the Store's filtered listing (spec §4.2): a home entry whose Store
//! project is retired (hidden) or absent is not listed, and project.get
//! answers it `NoSuchProject`; a live project still lists and gets.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::{hex, unhex};
use layerfs_history::LayerStackId;
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::StoreRecord;
use layerfs_store::service::with_service;
use layerfs_store_service::history::deletion::delete_stack;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-project-intersection-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
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

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// Marks Store project `name` hidden, as the project.delete write path will
/// (T4.5b); the home entry stays, exactly the stale case §4.2 hides.
fn retire(home: &Path, store: &str, name: &str, stack: &str) {
    let record = StoreRecord::read(home, store).unwrap().unwrap();
    let open = OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(store.to_owned()),
        project: Some(name.to_owned()),
        store_path: Some(paths::store_file(home, store).display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    let stack = LayerStackId::from_slice(&unhex(stack).unwrap()).unwrap();
    with_service(&open, |service| {
        delete_stack(&service.store, stack, 1234).unwrap();
        Ok(SdkValue::Null)
    })
    .unwrap();
}

#[test]
fn a_retired_or_absent_store_project_is_not_listed_or_got() {
    let home = tempdir("home");
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let source = tempdir("folder");
    std::fs::write(source.join("README.md"), "live\n").unwrap();
    let init = |name: &str| {
        ok(
            &home,
            "project.init",
            &[
                ("project", t(&format!("shop/{name}"))),
                ("folder", t(&source.display().to_string())),
            ],
        )
    };
    init("live");
    init("retired");
    let retired = ProjectRecord::read(&home, "shop", "retired")
        .unwrap()
        .expect("the retired entry");
    let retired_stack = hex(&retired.stack().expect("the entry's manifest stack"));
    retire(&home, "shop", "retired", &retired_stack);

    // A home entry whose Store project is absent: a second home's stale
    // record, or a failed home-entry removal. Its stack is not in the Store.
    let absent = LayerStackId::from_authority([9; 16]).to_bytes();
    write_record(
        &paths::project_record(&home, "shop", "absent"),
        &SdkValue::map([
            ("name", t("absent")),
            ("stack", t(&hex(&absent))),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();

    // Only the live project lists; the retired and absent entries do not.
    let list = ok(&home, "project.list", &[]);
    let SdkValue::List(items) = list.get("items").unwrap() else {
        panic!("{list:?}")
    };
    let names: Vec<&str> = items.iter().filter_map(|i| i.get_text("project")).collect();
    assert_eq!(names, vec!["shop/live"]);
    // the named listing intersects the same way
    let one = ok(&home, "project.list", &[("store", t("shop"))]);
    let SdkValue::List(items) = one.get("items").unwrap() else {
        panic!("{one:?}")
    };
    let names: Vec<&str> = items.iter().filter_map(|i| i.get_text("project")).collect();
    assert_eq!(names, vec!["shop/live"]);
    // both stale home entries are still there; the Store, not the home, hides them
    for name in ["retired", "absent"] {
        assert!(
            paths::project_record(&home, "shop", name).exists(),
            "{name}"
        );
    }

    // project.get refuses a hidden project even with its home entry present,
    // and an entry the Store does not hold.
    for name in ["retired", "absent"] {
        let (code, message) = call(
            &home,
            "project.get",
            &[("project", t(&format!("shop/{name}")))],
        )
        .unwrap_err();
        assert_eq!(code, "NotFound", "{name}: {message}");
    }
    let live = ok(&home, "project.get", &[("project", t("shop/live"))]);
    assert_eq!(live.get_text("project"), Some("shop/live"));

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(source);
}
