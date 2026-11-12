//! Bounded canonical storage reports over explicit saved snapshots, through
//! the call table on a native SQLite Store.
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("storage-report-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn args(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn call(home: &Path, method: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    layerfs_api::call(Some(home), method, args(fields), &mut |_| {})
        .unwrap_or_else(|r| panic!("{method}: {r:?}"))
}
fn refused(home: &Path, method: &str, fields: &[(&str, SdkValue)]) -> String {
    layerfs_api::call(Some(home), method, args(fields), &mut |_| {})
        .map(|value| panic!("{method} answered {value:?}"))
        .unwrap_err()
        .code
}
fn head(home: &Path, project: &str, branch: &str) -> String {
    call(
        home,
        "branch.commits",
        &[("project", t(project)), ("branch", t(branch))],
    )
    .get("at_commit")
    .unwrap()
    .get_text("id")
    .unwrap()
    .to_owned()
}
fn pin(project: Option<&str>, branch: &str, commit: &str) -> SdkValue {
    let mut fields = Vec::new();
    if let Some(project) = project {
        fields.push(("project", t(project)));
    }
    fields.extend([("branch", t(branch)), ("commit", t(commit))]);
    args(&fields)
}

/// A Store `stats` whose project `stats/p` has `main` and a published `dev`.
fn published(tag: &str) -> (PathBuf, PathBuf) {
    let home = folder(&format!("{tag}-home"));
    let source = folder(&format!("{tag}-input"));
    std::fs::write(source.join("file"), "hello").unwrap();
    call(&home, "store.create", &[("name", t("stats"))]);
    call(
        &home,
        "project.init",
        &[
            ("project", t("stats/p")),
            ("folder", t(source.to_str().unwrap())),
        ],
    );
    call(
        &home,
        "branch.fork",
        &[
            ("project", t("stats/p")),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    );
    call(
        &home,
        "file.write",
        &[
            ("project", t("stats/p")),
            ("branch", t("dev")),
            ("path", t("/second")),
            ("content", t("second file\n")),
        ],
    );
    call(
        &home,
        "branch.publish",
        &[("project", t("stats/p")), ("branch", t("dev"))],
    );
    (home, source)
}

#[test]
fn store_and_project_reports_agree_and_encoding_is_opt_in() {
    let (home, source) = published("agree");
    let (main, dev) = (
        head(&home, "stats/p", "main"),
        head(&home, "stats/p", "dev"),
    );
    let snapshots = SdkValue::List(vec![
        pin(Some("stats/p"), "main", &main),
        pin(Some("stats/p"), "dev", &dev),
    ]);
    let project_pins = SdkValue::List(vec![pin(None, "main", &main), pin(None, "dev", &dev)]);
    let store = |extra: &[(&str, SdkValue)]| {
        let mut fields = vec![("name", t("stats")), ("snapshots", snapshots.clone())];
        fields.extend_from_slice(extra);
        call(&home, "store.storage_report", &fields)
    };
    let project = |extra: &[(&str, SdkValue)]| {
        let mut fields = vec![
            ("project", t("stats/p")),
            ("snapshots", project_pins.clone()),
        ];
        fields.extend_from_slice(extra);
        call(&home, "project.storage_report", &fields)
    };

    let report = store(&[]);
    assert!(report.get_bool("complete"), "{report:?}");
    assert_eq!(report.get_int("definition_version"), Some(1));
    assert_eq!(
        report.get_text("population"),
        Some("explicit_saved_snapshots")
    );
    assert_eq!(report.get_text("population_id").unwrap().len(), 64);
    // the publication fast-forwards main to dev's tree: every object twice
    assert_eq!(
        report.get_int("referenced_canonical_bytes"),
        report.get_int("unique_canonical_bytes").map(|n| 2 * n)
    );
    assert_eq!(
        report.get_int("canonical_bytes_read"),
        report.get_int("referenced_canonical_bytes")
    );
    assert_eq!(report.get("physical_bytes"), Some(&SdkValue::Null));
    assert_eq!(report.get("reclaimable_bytes"), Some(&SdkValue::Null));
    assert_eq!(report.get_text("scope"), Some("store"));
    assert_eq!(report.get_text("name"), Some("stats"));
    let Some(SdkValue::List(listed)) = report.get("snapshots") else {
        panic!("{report:?}")
    };
    assert_eq!(listed[0].get_text("project"), Some("stats/p"));
    assert_eq!(
        listed[1].get("at").unwrap().get_text("id"),
        Some(dev.as_str())
    );

    let plain = project(&[]);
    assert!(
        plain.get("payload_encoding").is_none(),
        "encoding work is explicitly opt-in"
    );
    assert_eq!(plain.get_text("scope"), Some("project"));
    assert_eq!(plain.get_text("project"), Some("stats/p"));
    assert!(plain.get("name").is_none());
    for field in [
        "population_id",
        "snapshots",
        "referenced_canonical_bytes",
        "unique_canonical_bytes",
        "projects",
        "physical_bytes",
        "reclaimable_bytes",
    ] {
        assert_eq!(plain.get(field), report.get(field), "{field}");
    }

    let encoded = project(&[("include_encoding", SdkValue::Bool(true))]);
    assert_eq!(
        store(&[("include_encoding", SdkValue::Bool(true))]).get("payload_encoding"),
        encoded.get("payload_encoding")
    );
    assert_eq!(
        encoded.get("unique_canonical_bytes"),
        plain.get("unique_canonical_bytes")
    );
    let encoding = encoded.get("payload_encoding").unwrap();
    assert!(encoding.get_bool("available"));
    assert_eq!(
        encoding.get_text("population"),
        Some("selected_payload_object_records")
    );
    assert_eq!(
        encoding.get_int("referenced_canonical_bytes"),
        encoding.get_int("unique_canonical_bytes").map(|n| 2 * n)
    );
    assert!(encoding.get_int("encoded_record_bytes").unwrap() > 0);
    assert_eq!(encoding.get_int("external_direct_base_count"), Some(0));
    assert!(!encoding.get_bool("dependency_closure_included"));
    assert!(!encoding.get_bool("shared_framing_included"));

    // a missed budget leaves every total null, encoding included
    let limited = project(&[
        ("include_encoding", SdkValue::Bool(true)),
        ("max_objects", SdkValue::Int(1)),
    ]);
    assert!(!limited.get_bool("complete"));
    assert_eq!(limited.get("unique_canonical_bytes"), Some(&SdkValue::Null));
    let unavailable = limited.get("payload_encoding").unwrap();
    assert!(!unavailable.get_bool("available"));
    for key in [
        "referenced_canonical_bytes",
        "unique_canonical_bytes",
        "encoded_record_bytes",
        "external_direct_base_count",
    ] {
        assert_eq!(unavailable.get(key), Some(&SdkValue::Null), "{key}");
    }
    let incomplete = store(&[("max_objects", SdkValue::Int(1))]);
    assert!(!incomplete.get_bool("complete"));
    assert_eq!(incomplete.get_int("examined_objects"), Some(1));
    assert_eq!(
        incomplete.get("referenced_canonical_bytes"),
        Some(&SdkValue::Null)
    );
    assert_eq!(
        incomplete.get_text("reason"),
        Some("object frontier exceeds remaining visit budget")
    );
    let starved = store(&[("max_canonical_bytes", SdkValue::Int(10))]);
    assert_eq!(starved.get_int("canonical_bytes_read"), Some(0));
    assert_eq!(
        starved.get_text("reason"),
        Some("canonical byte read budget exhausted")
    );

    // a project snapshot names no project; the call supplies it
    assert_eq!(
        refused(
            &home,
            "project.storage_report",
            &[("project", t("stats/p")), ("snapshots", snapshots.clone())]
        ),
        "InvalidInput"
    );
    for (name, value) in [
        ("max_objects", SdkValue::Int(0)),
        ("max_objects", SdkValue::Int(10_001)),
        ("max_canonical_bytes", SdkValue::Int(64 * 1024 * 1024 + 1)),
    ] {
        assert_eq!(
            refused(
                &home,
                "store.storage_report",
                &[
                    ("name", t("stats")),
                    ("snapshots", snapshots.clone()),
                    (name, value)
                ]
            ),
            "InvalidInput"
        );
    }
    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}

#[test]
fn canonical_report_shares_payloads_across_explicit_project_pins_and_validates_population() {
    let home = folder("share-home");
    let source = folder("share-input");
    std::fs::write(source.join("file"), "shared file payload\n").unwrap();
    call(&home, "store.create", &[("name", t("stats"))]);
    let mut snapshots = Vec::new();
    for project in ["stats/left", "stats/right"] {
        call(
            &home,
            "project.init",
            &[
                ("project", t(project)),
                ("folder", t(source.to_str().unwrap())),
            ],
        );
        snapshots.push(pin(Some(project), "main", &head(&home, project, "main")));
    }
    let report = |pins: Vec<SdkValue>| {
        call(
            &home,
            "store.storage_report",
            &[("name", t("stats")), ("snapshots", SdkValue::List(pins))],
        )
    };
    let first = report(snapshots.clone());
    assert!(first.get_bool("complete"));
    assert!(
        first.get_int("unique_canonical_bytes").unwrap()
            < first.get_int("referenced_canonical_bytes").unwrap()
    );
    let Some(SdkValue::List(projects)) = first.get("projects") else {
        panic!("{first:?}")
    };
    let names: Vec<_> = projects.iter().map(|p| p.get_text("project")).collect();
    assert_eq!(names, [Some("stats/left"), Some("stats/right")]);
    for project in projects {
        let bytes = |field| project.get_int(field).unwrap();
        assert!(bytes("shared_within_population_canonical_bytes") > 0);
        // one snapshot per project: its objects are exactly its union
        assert_eq!(
            bytes("exclusive_within_population_canonical_bytes")
                + bytes("shared_within_population_canonical_bytes"),
            bytes("referenced_canonical_bytes")
        );
    }
    snapshots.reverse();
    assert_eq!(
        first.get("population_id"),
        report(snapshots.clone()).get("population_id"),
        "the population identity ignores order"
    );
    let refuse = |pins: Vec<SdkValue>| {
        refused(
            &home,
            "store.storage_report",
            &[("name", t("stats")), ("snapshots", SdkValue::List(pins))],
        )
    };
    assert_eq!(refuse(vec![]), "InvalidInput");
    assert_eq!(refuse(vec![snapshots[0].clone(); 65]), "InvalidInput");
    assert_eq!(refuse(vec![snapshots[0].clone(); 2]), "InvalidInput");
    let commit = snapshots[0].get_text("commit").unwrap().to_owned();
    assert_eq!(
        refuse(vec![pin(Some("other/project"), "main", &commit)]),
        "InvalidInput"
    );
    // `snapshots[0]` is now right's Layer; left's history does not hold it
    assert_eq!(
        refuse(vec![pin(Some("stats/left"), "main", &commit)]),
        "InvalidInput"
    );
    assert_eq!(
        refuse(vec![pin(Some("stats/left"), "main", "1234")]),
        "InvalidInput"
    );
    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}
