//! Every public Store method's answer and main refusals, through the method
//! table, against a recorded golden (#352 api shape recovery). Identities,
//! Commit times and the stored byte counts that encode them differ per run
//! and are masked; everything else is compared
//! exactly, refusal codes, messages and details included. Two calls with two
//! faults pin which refusal comes first.
//!
//! `LAYERFS_GOLDEN_PRINT=1` prints the transcript instead of comparing it.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{operation, write, Fixture};

const GOLDEN: &str = include_str!("golden/sdk_methods.txt");

/// A text with each run of at least 12 lowercase hex digits (an identity or
/// its short label) replaced by `<hex>`.
fn masked(text: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.len() >= 12 {
            out.push_str("<hex>");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() || ('a'..='f').contains(&c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// One value as stable text: identities masked, per-run counts as `<varies>`.
fn render(value: &SdkValue, out: &mut String) {
    match value {
        SdkValue::Null => out.push_str("null"),
        SdkValue::Bool(b) => out.push_str(&b.to_string()),
        SdkValue::Int(n) => out.push_str(&n.to_string()),
        SdkValue::Text(t) => out.push_str(&format!("{:?}", masked(t))),
        SdkValue::Bytes(b) => out.push_str(&format!("bytes{:?}", masked(&format!("{b:?}")))),
        SdkValue::List(items) => {
            out.push('[');
            for (n, item) in items.iter().enumerate() {
                if n > 0 {
                    out.push_str(", ");
                }
                render(item, out);
            }
            out.push(']');
        }
        SdkValue::Map(fields) => {
            out.push('{');
            for (n, (name, item)) in fields.iter().enumerate() {
                if n > 0 {
                    out.push_str(", ");
                }
                out.push_str(name);
                out.push_str(": ");
                if matches!(item, SdkValue::Int(_))
                    && matches!(name.as_str(), "time" | "newStored" | "newMetadata")
                {
                    out.push_str("<varies>");
                } else {
                    render(item, out);
                }
            }
            out.push('}');
        }
    }
}

struct Transcript<'a> {
    fixture: &'a Fixture,
    lines: Vec<String>,
}

impl Transcript<'_> {
    fn call(&mut self, method: &str, fields: Vec<(&str, SdkValue)>) {
        let mut line = format!("{method} -> ");
        match self.fixture.try_call(method, fields) {
            Ok(value) => {
                line.push_str("ok ");
                render(&value, &mut line);
            }
            Err(refusal) => {
                line.push_str(&format!(
                    "refused {} {:?} ",
                    refusal.code,
                    masked(&refusal.message)
                ));
                render(&refusal.detail, &mut line);
            }
        }
        self.lines.push(line);
    }
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

#[test]
fn public_methods_answer_as_recorded() {
    let fixture = Fixture::new("sdk-method-golden", |source| {
        fs::create_dir_all(source.join("dir/sub")).unwrap();
        fs::write(source.join("a.txt"), b"one\ntwo\nthree\n").unwrap();
        fs::write(source.join("dir/b.txt"), b"bee\n").unwrap();
        fs::write(source.join("dir/sub/c.rs"), b"fn main() {}\n").unwrap();
        fs::write(source.join("blob.bin"), [0u8, 159, 146, 150, 0, 1]).unwrap();
        6
    });
    let mut s = Transcript {
        fixture: &fixture,
        lines: Vec::new(),
    };

    // project
    s.call("project.list_branches", vec![]);
    s.call("project.list_branches", vec![("name", t("main"))]);
    s.call("project.list_branches", vec![("name", SdkValue::Int(3))]);
    s.call(
        "project.list_branches",
        vec![("pageSize", SdkValue::Int(0))],
    );

    // branch.fork
    s.call("branch.fork", vec![("name", t("feat"))]);
    s.call("branch.fork", vec![("name", t("other"))]);
    s.call("branch.fork", vec![("name", t("main"))]);
    s.call("branch.fork", vec![("name", t("feat"))]);
    s.call("branch.fork", vec![("name", t("x")), ("from", t("nope"))]);

    // writes
    s.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/new.txt")),
            ("content", t("hello\n")),
            ("message", t("first")),
        ],
    );
    s.call(
        "fs.write",
        vec![
            ("branch", t("main")),
            ("path", t("/x")),
            ("content", t("x")),
        ],
    );
    s.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/new.txt")),
            ("content", t("hello\n")),
        ],
    );
    s.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("bad//path")),
            ("content", t("x")),
        ],
    );
    s.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/y")),
            ("content", SdkValue::Int(1)),
        ],
    );
    s.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/y")),
            ("content", t("y")),
            ("message", SdkValue::Int(1)),
        ],
    );
    s.call(
        "fs.edit",
        vec![
            ("branch", t("feat")),
            ("path", t("/a.txt")),
            ("oldText", t("two")),
            ("newText", t("TWO")),
        ],
    );
    s.call(
        "fs.edit",
        vec![
            ("branch", t("feat")),
            ("path", t("/a.txt")),
            ("oldText", t("absent")),
            ("newText", t("x")),
        ],
    );
    s.call(
        "fs.mkdir",
        vec![("branch", t("feat")), ("path", t("/made/deep"))],
    );
    s.call(
        "fs.cp",
        vec![
            ("branch", t("feat")),
            ("from", t("/dir")),
            ("to", t("/dir2")),
        ],
    );
    s.call(
        "fs.mv",
        vec![
            ("branch", t("feat")),
            ("from", t("/dir2/b.txt")),
            ("to", t("/moved.txt")),
        ],
    );
    s.call("fs.rm", vec![("branch", t("feat")), ("path", t("/dir2"))]);
    s.call("fs.rm", vec![("branch", t("feat")), ("path", t("/gone"))]);
    s.call(
        "fs.apply",
        vec![
            ("branch", t("feat")),
            (
                "changes",
                SdkValue::List(vec![
                    write("/applied.txt", "applied\n"),
                    operation("mkdir", "/applied-dir"),
                ]),
            ),
        ],
    );
    // Two faults: the first change's own refusal comes before the second's
    // malformed op.
    s.call(
        "fs.apply",
        vec![
            ("branch", t("feat")),
            (
                "changes",
                SdkValue::List(vec![
                    operation("rm", "/gone"),
                    SdkValue::map([("op", t("bogus"))]),
                ]),
            ),
        ],
    );
    s.call(
        "fs.apply",
        vec![
            ("branch", t("feat")),
            (
                "changes",
                SdkValue::List(vec![
                    write("/ok.txt", "ok"),
                    SdkValue::map([("op", t("bogus"))]),
                ]),
            ),
        ],
    );
    s.call("fs.apply", vec![("branch", t("feat"))]);
    s.call(
        "fs.write",
        vec![
            ("branch", t("other")),
            ("path", t("/a.txt")),
            ("content", t("other side\n")),
        ],
    );

    // reads
    for branch in ["feat", "main"] {
        s.call(
            "fs.read",
            vec![("branch", t(branch)), ("path", t("/a.txt"))],
        );
    }
    s.call(
        "fs.read",
        vec![
            ("branch", t("feat")),
            ("path", t("/a.txt")),
            ("startLine", SdkValue::Int(2)),
            ("endLine", SdkValue::Int(2)),
        ],
    );
    s.call(
        "fs.read",
        vec![
            ("branch", t("feat")),
            ("path", t("/a.txt")),
            ("startLine", SdkValue::Int(3)),
            ("endLine", SdkValue::Int(2)),
        ],
    );
    s.call(
        "fs.read",
        vec![
            ("branch", t("feat")),
            ("path", t("/a.txt")),
            ("startLine", SdkValue::Int(0)),
        ],
    );
    s.call("fs.read", vec![("branch", t("feat")), ("path", t("/nope"))]);
    s.call(
        "fs.read",
        vec![("branch", t("ghost")), ("path", t("/a.txt"))],
    );
    s.call("fs.read", vec![("branch", t("feat"))]);
    s.call(
        "fs.read",
        vec![("branch", t("main")), ("path", t("/blob.bin"))],
    );
    s.call("fs.list", vec![("branch", t("feat"))]);
    s.call(
        "fs.list",
        vec![
            ("branch", t("feat")),
            ("path", t("/dir")),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    s.call(
        "fs.list",
        vec![("branch", t("feat")), ("path", t("/a.txt"))],
    );
    s.call(
        "fs.stat",
        vec![("branch", t("main")), ("path", t("/a.txt"))],
    );
    s.call("fs.stat", vec![("branch", t("feat")), ("path", t("/dir"))]);
    s.call(
        "fs.grep",
        vec![("branch", t("feat")), ("pattern", t("TWO|bee"))],
    );
    s.call(
        "fs.grep",
        vec![
            ("branch", t("feat")),
            ("pattern", t("e")),
            ("path", t("/dir")),
            ("pageSize", SdkValue::Int(1)),
        ],
    );
    s.call("fs.grep", vec![("branch", t("feat")), ("pattern", t("("))]);
    s.call(
        "fs.glob",
        vec![("branch", t("feat")), ("pattern", t("**/*.txt"))],
    );
    s.call(
        "fs.glob",
        vec![("branch", t("feat")), ("pattern", t("dir/**"))],
    );
    s.call(
        "fs.blame",
        vec![("branch", t("feat")), ("path", t("/a.txt"))],
    );
    s.call(
        "fs.blame",
        vec![
            ("branch", t("feat")),
            ("path", t("/a.txt")),
            ("fromLine", SdkValue::Int(2)),
        ],
    );
    s.call(
        "fs.diff_files",
        vec![
            ("branch", t("feat")),
            (
                "left",
                SdkValue::map([("path", t("/a.txt")), ("commit", t("main"))]),
            ),
            ("right", SdkValue::map([("path", t("/a.txt"))])),
        ],
    );
    s.call(
        "fs.diff_files",
        vec![
            ("left", SdkValue::map([("path", t("/dir"))])),
            ("right", SdkValue::map([("path", t("/a.txt"))])),
        ],
    );
    s.call(
        "fs.diff_files",
        vec![("left", SdkValue::map([("path", t("/a.txt"))]))],
    );

    // history
    s.call("branch.list_commits", vec![("branch", t("feat"))]);
    s.call(
        "branch.list_commits",
        vec![("branch", t("feat")), ("pageSize", SdkValue::Int(2))],
    );
    s.call("branch.list_commits", vec![("branch", t("main"))]);
    s.call(
        "branch.diff_branches",
        vec![("from", t("main")), ("to", t("feat"))],
    );
    s.call(
        "branch.diff_branches",
        vec![
            ("from", t("main")),
            ("to", t("feat")),
            ("pageSize", SdkValue::Int(2)),
            ("pagePos", SdkValue::Int(1)),
        ],
    );
    s.call(
        "branch.diff_commits",
        vec![("from", t("main")), ("to", t("other"))],
    );
    s.call(
        "branch.diff_commits",
        vec![("from", t("main")), ("to", t("ghost"))],
    );
    s.call(
        "branch.merge",
        vec![
            ("branch", t("feat")),
            ("with", t("other")),
            ("name", t("merged")),
        ],
    );
    let commits = fixture.call(
        "branch.list_commits",
        vec![("branch", t("feat")), ("pageSize", SdkValue::Int(2))],
    );
    let ids: Vec<String> = support::items(&commits)
        .iter()
        .map(|item| item.get_text("id").unwrap().to_owned())
        .collect();
    s.call(
        "branch.diff_commits",
        vec![("from", t(&ids[1])), ("to", t(&ids[0]))],
    );
    s.call(
        "fs.read",
        vec![("commit", t(&ids[1])), ("path", t("/new.txt"))],
    );
    s.call(
        "branch.fork",
        vec![("name", t("at-commit")), ("from", t(&ids[1]))],
    );
    for (name, path) in [("left", "/left.txt"), ("right", "/right.txt")] {
        s.call("branch.fork", vec![("name", t(name))]);
        s.call(
            "fs.write",
            vec![("branch", t(name)), ("path", t(path)), ("content", t(name))],
        );
    }
    s.call(
        "branch.merge",
        vec![
            ("branch", t("left")),
            ("with", t("right")),
            ("name", t("both")),
            ("message", t("merge both")),
        ],
    );
    s.call("fs.list", vec![("branch", t("both"))]);
    s.call(
        "branch.merge",
        vec![
            ("branch", t("left")),
            ("with", t("right")),
            ("name", t("both")),
        ],
    );
    s.call(
        "branch.merge",
        vec![
            ("branch", t("feat")),
            ("with", t("feat")),
            ("name", t("m2")),
        ],
    );
    s.call(
        "branch.merge",
        vec![
            ("branch", t("feat")),
            ("with", t("other")),
            ("name", t("main")),
        ],
    );
    s.call("branch.publish", vec![("branch", t("feat"))]);
    s.call("branch.publish", vec![("branch", t("feat"))]);
    s.call("branch.fork", vec![("name", t("empty"))]);
    s.call("branch.publish", vec![("branch", t("empty"))]);
    s.call("project.list_branches", vec![]);
    s.call(
        "fs.read",
        vec![("branch", t("main")), ("path", t("/a.txt"))],
    );

    // table
    s.call("fs.nope", vec![]);
    s.call("backend.store.project.get_storage", vec![]);
    s.call("project.list_overwritten", vec![]);

    let transcript = s.lines.join("\n") + "\n";
    // Two faults on one call: the unknown project is refused before the
    // missing path (the project is looked up first).
    let two_faults = {
        let call = layerfs_bridge::control::SdkCall {
            method: "fs.read".into(),
            args: SdkValue::map([("project", t("ghost")), ("branch", t("feat"))]),
        };
        layerfs_api::call(&fixture.service, &call).unwrap_err()
    };
    let transcript = transcript
        + &format!(
            "fs.read(project ghost, no path) -> refused {} {:?}\n",
            two_faults.code, two_faults.message
        );
    fixture.cleanup();
    if std::env::var_os("LAYERFS_GOLDEN_PRINT").is_some() {
        print!("{transcript}");
        return;
    }
    for (n, (got, want)) in transcript.lines().zip(GOLDEN.lines()).enumerate() {
        assert_eq!(got, want, "golden line {}", n + 1);
    }
    assert_eq!(transcript.lines().count(), GOLDEN.lines().count());
}
