//! A file write names what is in its way and what is wrong with its input as
//! the read calls do: a directory is `IsDirectory`, any other non-file
//! `NotFile`, and a name the name grammar refuses is `InvalidInput`, not an
//! engine failure. Nothing is committed.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::Fixture;

fn refused(fixture: &Fixture, method: &str, path: &str) -> SdkRefusal {
    let mut fields = vec![
        ("branch", SdkValue::text("feat")),
        ("path", SdkValue::text(path)),
    ];
    if method == "fs.write" {
        fields.push(("content", SdkValue::text("x")));
    }
    fixture
        .try_call(method, fields)
        .expect_err("the call is refused")
}

#[test]
fn a_write_refusal_names_its_cause() {
    let fixture = Fixture::new("write-refusals", |source| {
        fs::create_dir(source.join("dir")).unwrap();
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        std::os::unix::fs::symlink("a.txt", source.join("link")).unwrap();
        3
    });
    fixture.fork("feat");

    // The read calls already tell the two apart.
    assert_eq!(refused(&fixture, "fs.read", "/link").code, "NotFile");

    assert_eq!(refused(&fixture, "fs.write", "/dir").code, "IsDirectory");
    let link = refused(&fixture, "fs.write", "/link");
    assert_eq!(
        (link.code.as_str(), link.message.as_str()),
        ("NotFile", "expected a regular file")
    );

    // A `\` or a name over 255 bytes passes the path check and is refused by
    // the name grammar, on the way to the entry and at the entry itself.
    let long = format!("/{}", "n".repeat(256));
    for (method, path) in [
        ("fs.write", "/a\\b"),
        ("fs.write", "/dir/a\\b"),
        ("fs.write", long.as_str()),
        ("fs.mkdir", "/a\\b"),
        ("fs.rm", "/a\\b"),
    ] {
        let r = refused(&fixture, method, path);
        assert_eq!(
            (r.code.as_str(), r.message.as_str()),
            ("InvalidInput", format!("`{path}` is not valid").as_str()),
            "{method} {path}"
        );
    }
    // The read route answers the same input the same way.
    assert_eq!(refused(&fixture, "fs.stat", "/a\\b").code, "InvalidInput");

    let commits = fixture.call(
        "branch.list_commits",
        vec![("branch", SdkValue::text("feat"))],
    );
    assert!(support::items(&commits).is_empty());
    fixture.cleanup();
}

/// A name below a file or a symlink (which is not followed) is refused
/// `NotDirectory` at that entry by every write call, as `fs.mkdir` answers.
/// Such a call keeps no Store call Workspace: more of them than the Store
/// has call Workspaces leave a valid write working. `fs.mkdir` never took
/// one, so only the other calls count toward that number.
#[test]
fn a_write_through_a_non_directory_is_not_directory() {
    let fixture = Fixture::new("write-through-non-directory", |source| {
        fs::create_dir(source.join("dir")).unwrap();
        fs::write(source.join("dir/inner.txt"), b"inner\n").unwrap();
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        std::os::unix::fs::symlink("dir", source.join("dlink")).unwrap();
        4
    });
    fixture.fork("feat");
    let t = SdkValue::text;
    let cases = [
        (
            "fs.write",
            vec![("path", t("/a.txt/x")), ("content", t("x"))],
            "/a.txt",
        ),
        (
            "fs.write",
            vec![("path", t("/dlink/x")), ("content", t("x"))],
            "/dlink",
        ),
        ("fs.rm", vec![("path", t("/a.txt/x"))], "/a.txt"),
        ("fs.mkdir", vec![("path", t("/a.txt/x"))], "/a.txt"),
        (
            "fs.edit",
            vec![
                ("path", t("/dlink/inner.txt")),
                ("oldText", t("inner")),
                ("newText", t("x")),
            ],
            "/dlink",
        ),
        (
            "fs.cp",
            vec![("from", t("/a.txt")), ("to", t("/dlink/c.txt"))],
            "/dlink",
        ),
        (
            "fs.mv",
            vec![("from", t("/dir/inner.txt")), ("to", t("/a.txt/y"))],
            "/a.txt",
        ),
    ];
    let calls = layerfs_store_service::OwnerConfig::default().namespaces + 1;
    let held_a_workspace = cases.iter().filter(|(method, ..)| *method != "fs.mkdir");
    for (method, fields, at) in cases.iter().chain(held_a_workspace.cycle().take(calls)) {
        let mut fields = fields.clone();
        fields.push(("branch", t("feat")));
        let r = fixture
            .try_call(method, fields)
            .expect_err("the call is refused");
        assert_eq!(
            (r.code.as_str(), r.message.as_str()),
            ("NotDirectory", *at),
            "{method}"
        );
    }
    fixture.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/after.txt")),
            ("content", t("after")),
        ],
    );
    fixture.cleanup();
}

/// Changes the stored file contents of the closed Store at `path` out of
/// band: every pack that holds only whole-file or chunk objects has one byte
/// flipped. Metadata packs, which a Workspace bind reads, stay intact. The
/// immutability triggers are dropped for this and created again from their
/// recorded SQL.
fn damage_file_contents(path: &std::path::Path) {
    let db = rusqlite::Connection::open(path).unwrap();
    let triggers: Vec<(String, String)> = db
        .prepare(
            "SELECT name, sql FROM sqlite_master \
             WHERE type = 'trigger' AND tbl_name IN ('pack', 'pack_unit')",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!triggers.is_empty(), "the pack tables are immutable");
    for (name, _) in &triggers {
        db.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
    }
    // Object roles 1 and 2 are whole files and chunks.
    let content = "SELECT pack_id FROM object_location GROUP BY pack_id \
                   HAVING min(role IN (1, 2)) = 1";
    let units: bool = db
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE name = 'pack_unit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let (table, key) = if units {
        ("pack_unit", "unit_id")
    } else {
        ("pack", "pack_id")
    };
    let rows: Vec<(i64, Vec<u8>)> = db
        .prepare(&format!(
            "SELECT {key}, body FROM {table} WHERE pack_id IN ({content})"
        ))
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!rows.is_empty(), "a pack holds the file contents");
    for (id, mut body) in rows {
        let middle = body.len() / 2;
        body[middle] ^= 0xff;
        db.execute(
            &format!("UPDATE {table} SET body = ?1 WHERE {key} = ?2"),
            rusqlite::params![body, id],
        )
        .unwrap();
    }
    for (_, sql) in &triggers {
        db.execute_batch(sql).unwrap();
    }
}

/// A write call that fails reading the Store, before it changes anything,
/// is a known `Failed` and closes its Store call Workspace, not the
/// uncertain `Unknown` that keeps one: more such calls than the Store has
/// call Workspaces are each refused for the read, never for capacity.
#[test]
fn a_failed_read_keeps_no_store_call_workspace() {
    let fixture = Fixture::new("write-failed-read", |source| {
        fs::create_dir(source.join("dir")).unwrap();
        fs::write(source.join("dir/inner.txt"), b"inner\n").unwrap();
        2
    });
    fixture.fork("feat");
    let host = fixture.close();
    damage_file_contents(&host.config.path);
    // The owner leaves its overlay template behind; the reopened owner makes its own.
    fs::remove_file(host.directory.join("overlay.sqlite")).unwrap();
    let fixture = Fixture::open(host);

    // `fs.edit` resolves the file, then reads its damaged contents.
    let edit = || {
        fixture
            .try_call(
                "fs.edit",
                vec![
                    ("branch", SdkValue::text("feat")),
                    ("path", SdkValue::text("/dir/inner.txt")),
                    ("oldText", SdkValue::text("inner")),
                    ("newText", SdkValue::text("x")),
                ],
            )
            .expect_err("the file contents cannot be read")
    };
    let first = edit();
    assert_eq!(first.code, "Failed", "{}", first.message);
    let calls = layerfs_store_service::OwnerConfig::default().namespaces + 1;
    for call in 1..calls {
        let r = edit();
        assert_eq!(
            (r.code.as_str(), r.message.as_str()),
            (first.code.as_str(), first.message.as_str()),
            "call {call}"
        );
    }
    fixture.cleanup();
}
