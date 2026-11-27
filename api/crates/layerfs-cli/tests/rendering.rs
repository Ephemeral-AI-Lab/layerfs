//! Tests for terminal front-end rendering and suggestions (A1-A4).
use layerfs_protocol::provision::install::SdkValue;

#[test]
fn format_size_human_units() {
    assert_eq!(layerfs_cli::output::format_size(0), "0 B");
    assert_eq!(layerfs_cli::output::format_size(500), "500 B");
    assert_eq!(layerfs_cli::output::format_size(1023), "1023 B");
    assert_eq!(layerfs_cli::output::format_size(1024), "1 KiB");
    assert_eq!(layerfs_cli::output::format_size(1536), "1.5 KiB");
    assert_eq!(layerfs_cli::output::format_size(1048576), "1 MiB");
    assert_eq!(layerfs_cli::output::format_size(5242880), "5 MiB");
    assert_eq!(layerfs_cli::output::format_size(1073741824), "1 GiB");
    assert_eq!(layerfs_cli::output::format_size(2147483648), "2 GiB");
}

#[test]
fn format_field_renders_matching_keys() {
    let s = SdkValue::Int(1048576);
    assert_eq!(layerfs_cli::output::format_field("size", &s), "1 MiB");
    assert_eq!(layerfs_cli::output::format_field("bytes", &s), "1 MiB");
    assert_eq!(
        layerfs_cli::output::format_field("stdout_bytes", &s),
        "1 MiB"
    );

    // Other integer fields remain raw
    let other = SdkValue::Int(12345);
    assert_eq!(layerfs_cli::output::format_field("inode", &other), "12345");
    assert_eq!(layerfs_cli::output::format_field("step", &other), "12345");
}

/// The terminal progress line counts steps as plain numbers and only a
/// phase that moves bytes in byte units (conformance R4 F8).
#[test]
fn progress_counts_steps_and_bytes_apart() {
    use layerfs_cli::output::progress_line;
    assert_eq!(
        progress_line("stores", Some(2), Some(4), 1.0),
        "phase: stores  2/4"
    );
    assert_eq!(
        progress_line("start", Some(0), Some(1), 0.0),
        "phase: start  0/1"
    );
    assert_eq!(progress_line("lock", None, None, 0.0), "phase: lock");
    assert_eq!(
        progress_line("upload", Some(2048), Some(4096), 0.1),
        "phase: upload  2 KiB/4 KiB"
    );
    assert!(progress_line("send", Some(2048), None, 2.0).ends_with("/s)"));
    // project.export's bytes written, and a Store move's hashing verify
    // (RF re-check N-C)
    assert_eq!(
        progress_line("export", Some(119032), None, 0.1),
        "phase: export  116.2 KiB"
    );
    assert_eq!(
        progress_line("finalize", Some(8), None, 0.1),
        "phase: finalize  8 B"
    );
    assert_eq!(
        progress_line("verify", Some(4096), Some(4096), 0.1),
        "phase: verify  4 KiB/4 KiB"
    );
    // a doctor's Full-check data phase counts objects, not bytes
    assert_eq!(
        progress_line("data", Some(3), Some(3), 0.1),
        "phase: data  3/3"
    );
}

/// `--text` of a list (UI pass 2, LF4): the short form leaves `state: ready`
/// out, so a row with no `state` prints `ready` in the STATE column, never
/// `-`; a list whose rows were all ready keeps that column; an absent
/// value of any other column is still `-`. Every list call whose rows
/// carry a state renders so.
#[test]
fn a_ready_row_prints_ready_in_a_list_s_text() {
    use layerfs_api::{Form, Part};
    use layerfs_cli::output::readable;
    let mut checked = Vec::new();
    for call in layerfs_api::calls() {
        let Some(form) = call.form else { continue };
        let Some(Part::Rows(rows)) = form.part("items") else {
            continue;
        };
        let rows: &Form = rows;
        if !rows.long.contains(&"state") {
            continue;
        }
        let name = call.name();
        // a row of its always keys, and one more that is stopped
        let row = |id: &str, stopped: bool| {
            let mut fields: Vec<(String, SdkValue)> = (rows.always.iter())
                .filter(|key| **key != "state")
                .map(|key| ((*key).to_owned(), SdkValue::text(format!("{id}-{key}"))))
                .collect();
            if stopped {
                fields.push(("state".to_owned(), SdkValue::text("stopped")));
            }
            SdkValue::Map(fields)
        };
        let lines = |value: &SdkValue| -> Vec<String> {
            readable(value, Some(form))
                .lines()
                .map(|line| line.to_owned())
                .collect()
        };
        let mixed = SdkValue::map([(
            "items",
            SdkValue::List(vec![row("a", false), row("b", true)]),
        )]);
        let shown = lines(&mixed);
        assert!(shown[0].contains("STATE"), "{name}: {shown:?}");
        let word = |line: &str, word: &str| line.split_whitespace().any(|w| w == word);
        assert!(word(&shown[1], "ready"), "{name}: {shown:?}");
        assert!(!word(&shown[1], "-"), "{name}: {shown:?}");
        assert!(word(&shown[2], "stopped"), "{name}: {shown:?}");
        let ready = SdkValue::map([(
            "items",
            SdkValue::List(vec![row("a", false), row("c", false)]),
        )]);
        let shown = lines(&ready);
        assert!(shown[0].contains("STATE"), "{name}: {shown:?}");
        assert!(
            shown[1..].iter().all(|l| word(l, "ready")),
            "{name}: {shown:?}"
        );
        checked.push(name);
    }
    for family in ["machine", "store", "sandbox"] {
        let list = format!("{family}.list");
        assert!(checked.contains(&list), "{list}: {checked:?}");
    }
    // another column a row leaves out is still `-`
    let value = SdkValue::map([(
        "items",
        SdkValue::List(vec![
            SdkValue::map([
                ("bucket", SdkValue::text("r2")),
                ("store", SdkValue::text("shop")),
            ]),
            SdkValue::map([("bucket", SdkValue::text("r2"))]),
        ]),
    )]);
    let shown = readable(&value, None);
    assert!(shown.lines().nth(2).unwrap().ends_with("  -"), "{shown}");
}

/// The readable form of a record: `done` for a bare success, aligned
/// labels, sizes for byte counts, Commit ids by a 14-character prefix,
/// nested maps indented under their label, a list of plain values one per
/// line, and a `*_not_listed` flag as a note under its table.
#[test]
fn records_read_as_aligned_labels() {
    use layerfs_cli::output::{readable, refusal_text};
    let t = |text: &str| SdkValue::text(text);
    assert_eq!(
        readable(&SdkValue::map([("ok", SdkValue::Bool(true))]), None),
        "done\n"
    );
    let id = format!("12{}", "ab".repeat(32));
    let record = SdkValue::map([
        ("commit", t(&id)),
        ("file_count", SdkValue::Int(4)),
        ("memory_used_bytes", SdkValue::Int(2048)),
        ("bundle", SdkValue::map([("source", t("runtime"))])),
    ]);
    assert_eq!(
        readable(&record, None),
        format!(
            "commit       {}\nfile count   4\nmemory used  2 KiB\nbundle\n  source  runtime\n",
            &id[..14]
        )
    );
    let paths = SdkValue::map([("items", SdkValue::List(vec![t("/a"), t("/b")]))]);
    assert_eq!(readable(&paths, None), "/a\n/b\n");
    let none = SdkValue::map([("items", SdkValue::List(Vec::new()))]);
    assert_eq!(readable(&none, None), "(none)\n");
    let flagged = SdkValue::map([(
        "items",
        SdkValue::List(vec![
            SdkValue::map([
                ("sandbox", t("sb1")),
                ("workspaces_not_listed", SdkValue::Bool(true)),
            ]),
            SdkValue::map([("sandbox", t("sb2"))]),
        ]),
    )]);
    let shown = readable(&flagged, None);
    assert_eq!(
        shown, "SANDBOX\nsb1\nsb2\nWorkspaces kept on the machine (`layerfs workspace list` reads them): sb1\n",
        "{shown}"
    );
    let refusal = SdkValue::map([
        ("error", t("InUse")),
        ("message", t("Store s1 has running Sandboxes: sb2")),
        ("fix", t("layerfs sandbox remove sb2")),
        ("sandboxes", SdkValue::List(vec![t("sb2")])),
    ]);
    assert_eq!(
        refusal_text(&refusal),
        "error: Store s1 has running Sandboxes: sb2  (InUse)\n  fix: layerfs sandbox remove sb2\n  sandboxes: sb2\n"
    );
}

/// A row's reason and what blocks it read in its STATE cell; its fix is a
/// line under the table, named by the row, so the table stays narrow.
#[test]
fn a_row_s_reason_reads_in_state_and_its_fix_under_the_table() {
    use layerfs_cli::output::readable;
    let t = |text: &str| SdkValue::text(text);
    let rows = SdkValue::map([(
        "items",
        SdkValue::List(vec![
            SdkValue::map([("store", t("a"))]),
            SdkValue::map([
                ("store", t("pg")),
                ("state", t("broken")),
                ("reason", t("outdated")),
                ("fix", t("layerfs store connect pg <url> --password-stdin")),
            ]),
            SdkValue::map([
                ("store", t("s")),
                ("state", t("blocked")),
                (
                    "blocked_by",
                    SdkValue::map([("machine", t("docker")), ("reason", t("outdated"))]),
                ),
            ]),
        ]),
    )]);
    assert_eq!(
        readable(&rows, None),
        "STORE  STATE\n\
         a      ready\n\
         pg     broken: outdated\n\
         s      blocked by machine docker (outdated)\n\
         \n\
         fix pg: layerfs store connect pg <url> --password-stdin\n"
    );
}

#[test]
fn doctor_text_aligns_columns_and_renders_detail_tree_and_evidence() {
    use layerfs_cli::output::doctor_text;
    let t = |text: &str| SdkValue::text(text);
    let report = SdkValue::map([
        ("ok", SdkValue::Bool(false)),
        (
            "findings",
            SdkValue::List(vec![SdkValue::map([
                ("store", t("pg")),
                ("check", t("store.connection")),
                ("severity", t("error")),
                ("message", t("cannot reach server")),
                ("fix", t("layerfs store connect pg <url> --password-stdin")),
                ("evidence", SdkValue::map([("step", t("handshake"))])),
            ])]),
        ),
        (
            "not_checked",
            SdkValue::List(vec![SdkValue::map([
                ("project", t("pg/app")),
                ("check", t("project.listed")),
                (
                    "blocked_by",
                    SdkValue::map([("store", t("pg")), ("reason", t("failed"))]),
                ),
            ])]),
        ),
        ("observed_at", t("2026-11-20T05:00:00Z")),
        (
            "tree",
            SdkValue::map([
                ("store", t("pg")),
                ("status", t("error")),
                ("coverage", t("shallow")),
                (
                    "checks",
                    SdkValue::List(vec![SdkValue::map([
                        ("check", t("store.record")),
                        ("outcome", t("pass")),
                    ])]),
                ),
            ]),
        ),
    ]);
    let shown = doctor_text(&report);
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(
        lines[1].find("store pg"),
        lines[4].find("project pg/app"),
        "{shown}"
    );
    assert!(shown.contains("evidence: step=handshake"), "{shown}");
    assert!(
        shown.contains("observed at: 2026-11-20T05:00:00Z"),
        "{shown}"
    );
    assert!(shown.contains("store.record: pass"), "{shown}");
}
