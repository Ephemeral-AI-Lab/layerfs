//! `project.remove_file` over the public Store method table: one marker hides
//! a read and a stat with a `Removed` detail, a listing and a glob omit the
//! entry and count it, the subtree matches only at `/`, and a post-frontier
//! re-creation is visible again. The path grammar is refused as `InvalidInput`.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{paths, write, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn remove(fixture: &Fixture, path: &str) -> SdkValue {
    fixture.call("project.remove_file", vec![("path", text(path))])
}

fn read(fixture: &Fixture, path: &str) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    fixture.try_call(
        "fs.read",
        vec![("branch", text("work")), ("path", text(path))],
    )
}

#[test]
fn a_removed_path_is_hidden_in_reads_stat_list_and_glob_until_recreated() {
    let fixture = Fixture::new("sdk-path-removal", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        fs::write(source.join("ab"), "ab\n").unwrap();
        2
    });
    fixture.fork("work");
    fixture.apply("work", vec![write("/x", "x\n")]);

    let removed = remove(&fixture, "/a");
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    assert_eq!(removed.get_text("project"), Some("installed"));
    assert_eq!(
        removed.get_text("path"),
        Some("a"),
        "the stored normalized path"
    );
    let frontier = removed.get_int("frontier_serial").unwrap();
    assert!(frontier >= 2, "the Store position covers the commits");
    let removed_ms = removed.get_int("removed_ms").unwrap();

    // Reads and stats refuse with the one detail shape.
    let refusal = read(&fixture, "/a").expect_err("hidden read");
    assert_eq!(refusal.code, "Removed");
    let Some(SdkValue::List(hidden)) = refusal.detail.get("paths") else {
        panic!("no paths detail: {:?}", refusal.detail);
    };
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].get_text("path"), Some("/a"));
    assert_eq!(hidden[0].get_int("frontier_serial"), Some(frontier));
    let stat = fixture
        .try_call(
            "fs.stat",
            vec![("branch", text("work")), ("path", text("/a"))],
        )
        .expect_err("hidden stat");
    assert_eq!(stat.code, "Removed");

    // The subtree matches only at a `/` boundary.
    let sibling = read(&fixture, "/ab").expect("ab stays visible");
    assert_eq!(sibling.get_text("text"), Some("ab\n"));

    // A listing omits the entry and counts it; a glob does the same.
    let listed = fixture.call(
        "fs.list",
        vec![("branch", text("work")), ("path", text("/"))],
    );
    let names = paths(&listed);
    assert!(names.contains(&"/ab"), "{names:?}");
    assert!(names.contains(&"/x"), "{names:?}");
    assert!(!names.contains(&"/a"), "{names:?}");
    assert_eq!(listed.get_int("removed_count"), Some(1));
    let globbed = fixture.call(
        "fs.glob",
        vec![
            ("branch", text("work")),
            ("pattern", text("/*")),
            ("path", text("/")),
        ],
    );
    let matched: Vec<&str> = match globbed.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                SdkValue::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect(),
        other => panic!("no glob items: {other:?}"),
    };
    assert!(matched.contains(&"/ab"), "{matched:?}");
    assert!(!matched.contains(&"/a"), "{matched:?}");
    assert_eq!(globbed.get_int("removed_count"), Some(1));

    // No newer project node: the repeat is UpToDate and keeps the stored values.
    let repeat = remove(&fixture, "/a");
    assert_eq!(repeat.get_text("kind"), Some("UpToDate"));
    assert_eq!(repeat.get_int("frontier_serial"), Some(frontier));
    assert_eq!(repeat.get_int("removed_ms"), Some(removed_ms));

    // A post-frontier re-creation is an ordinary visible write.
    fixture.apply("work", vec![write("/a", "again\n")]);
    let again = read(&fixture, "/a").expect("the re-creation is visible");
    assert_eq!(again.get_text("text"), Some("again\n"));
    fixture.cleanup();
}

#[test]
fn the_remove_file_path_grammar_is_refused() {
    let fixture = Fixture::new("sdk-path-grammar", |source| {
        fs::write(source.join("a"), "a\n").unwrap();
        1
    });
    fixture.fork("work");
    for bad in [
        "", "/", "//a", "a//b", "a/", "./a", "a/./b", ".", "..", "a/b/", "a/../b",
    ] {
        let refusal = fixture
            .try_call("project.remove_file", vec![("path", text(bad))])
            .unwrap_err();
        assert_eq!(refusal.code, "InvalidInput", "{bad:?} was accepted");
    }
    fixture.cleanup();
}

#[test]
fn a_non_directory_component_is_refused_while_a_leaf_symlink_is_hidden() {
    let fixture = Fixture::new("sdk-path-symlink", |source| {
        fs::create_dir(source.join("d")).unwrap();
        fs::write(source.join("d/f"), "f\n").unwrap();
        fs::write(source.join("a.txt"), "a\n").unwrap();
        std::os::unix::fs::symlink("a.txt", source.join("link")).unwrap();
        std::os::unix::fs::symlink("d", source.join("dlink")).unwrap();
        5
    });
    fixture.fork("work");

    // A component before the last that is not a directory is ill-typed, not a
    // path to hide: the marker is refused and nothing is written.
    for path in ["/link/f", "/dlink/f", "/a.txt/f"] {
        let refusal = fixture
            .try_call("project.remove_file", vec![("path", text(path))])
            .expect_err("a non-directory component");
        assert_eq!(refusal.code, "InvalidInput", "{path}: {refusal:?}");
    }
    // Reads and stats through that component answer the same way.
    for method in ["fs.read", "fs.stat"] {
        for path in ["/link/f", "/dlink/f", "/a.txt/f"] {
            let refusal = fixture
                .try_call(method, vec![("branch", text("work")), ("path", text(path))])
                .expect_err("a read below a non-directory");
            assert_eq!(refusal.code, "InvalidInput", "{method} {path}: {refusal:?}");
        }
    }

    // A symlink as the final component is an ordinary entry: it stays
    // visible until it is removed, then it is hidden like any other path.
    let present = fixture.call(
        "fs.stat",
        vec![("branch", text("work")), ("path", text("/link"))],
    );
    assert_eq!(present.get_text("type"), Some("symlink"), "{present:?}");
    let removed = remove(&fixture, "/link");
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    for method in ["fs.read", "fs.stat"] {
        let refusal = fixture
            .try_call(
                method,
                vec![("branch", text("work")), ("path", text("/link"))],
            )
            .expect_err("a hidden leaf symlink");
        assert_eq!(refusal.code, "Removed", "{method}: {refusal:?}");
    }
    let listed = fixture.call(
        "fs.list",
        vec![("branch", text("work")), ("path", text("/"))],
    );
    let names = paths(&listed);
    assert!(!names.contains(&"/link"), "{names:?}");
    assert!(names.contains(&"/dlink"), "{names:?}");
    assert_eq!(listed.get_int("removed_count"), Some(1), "{listed:?}");
    fixture.cleanup();
}
