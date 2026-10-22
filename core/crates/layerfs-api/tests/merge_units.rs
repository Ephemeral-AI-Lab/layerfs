//! Merge units on `branch.merge` (#352 F-057). Each test forks two Branches
//! from one seeded tree, changes each through the SDK (the later call
//! writes the later time), merges them with and without `units` and checks
//! the merged trees' decoded views and the result's `units`.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
#[path = "support/view.rs"]
#[allow(dead_code)]
mod view;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::{operation, write, Change, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn mv(from: &str, to: &str) -> SdkValue {
    SdkValue::map([("op", text("mv")), ("from", text(from)), ("to", text(to))])
}

fn rm(path: &str) -> SdkValue {
    operation("rm", path)
}

fn bare(patterns: &[&str]) -> SdkValue {
    SdkValue::List(patterns.iter().map(|p| text(p)).collect())
}

fn group(name: &str, patterns: &[&str]) -> SdkValue {
    SdkValue::map([("name", text(name)), ("patterns", bare(patterns))])
}

/// `/nm/{a,b,c}`, `/nm/lib/x`, `/out/hl` hard-linked to `/nm/hl`,
/// `/src/{s,t}`, `/top.txt`, `/p.tsbuildinfo`, `/lib/i.js`.
fn seeded(label: &str) -> Fixture {
    Fixture::new(label, |source| {
        for dir in ["nm/lib", "out", "src", "lib"] {
            fs::create_dir_all(source.join(dir)).unwrap();
        }
        for (path, bytes) in [
            ("nm/a", "a\n"),
            ("nm/b", "b\n"),
            ("nm/c", "c\n"),
            ("nm/lib/x", "x\n"),
            ("out/hl", "linked\n"),
            ("src/s", "1\n2\n3\n"),
            ("src/t", "t\n"),
            ("top.txt", "top\n"),
            ("p.tsbuildinfo", "p\n"),
            ("lib/i.js", "i\n"),
        ] {
            fs::write(source.join(path), bytes).unwrap();
        }
        fs::hard_link(source.join("out/hl"), source.join("nm/hl")).unwrap();
        16
    })
}

/// Forks `l` and `r` and applies `first` to its Branch, then `second` to
/// the other, so the second Branch's writes are the later ones.
fn sides(fixture: &Fixture, first: (&str, Vec<SdkValue>), second: (&str, Vec<SdkValue>)) {
    fixture.fork("l");
    fixture.fork("r");
    for (branch, changes) in [first, second] {
        if !changes.is_empty() {
            fixture.apply(branch, changes);
        }
    }
}

fn merge(fixture: &Fixture, name: &str, units: Option<SdkValue>) -> Result<SdkValue, SdkRefusal> {
    let mut fields = vec![
        ("branch", text("l")),
        ("with", text("r")),
        ("name", text(name)),
    ];
    if let Some(units) = units {
        fields.push(("units", units));
    }
    fixture.try_call("branch.merge", fields)
}

fn view(fixture: &Fixture, root: &str) -> Vec<String> {
    view::Viewer::open(
        &fixture.host.config.path,
        installed::BINDING,
        installed::CURSOR,
    )
    .view(root, false)
}

fn root_of(fixture: &Fixture, branch: &str) -> String {
    let commits = fixture.call("branch.list_commits", vec![("branch", text(branch))]);
    support::items(&commits)[0]
        .get_text("root")
        .unwrap()
        .to_owned()
}

/// The view's lines under `prefix`, the prefix itself included.
fn under(view: &[String], prefix: &str) -> Vec<String> {
    view.iter()
        .filter(|line| {
            let path = line.split(' ').next().unwrap();
            path == prefix || path.starts_with(&format!("{prefix}/"))
        })
        .cloned()
        .collect()
}

fn outside(view: &[String], prefix: &str) -> Vec<String> {
    let inside = under(view, prefix);
    view.iter()
        .filter(|line| !inside.contains(line))
        .cloned()
        .collect()
}

fn units_of(reply: &SdkValue) -> &[SdkValue] {
    match reply.get("units") {
        Some(SdkValue::List(units)) => units,
        other => panic!("units: {other:?}"),
    }
}

fn int(value: &SdkValue, name: &str) -> i64 {
    match value.get(name) {
        Some(SdkValue::Int(n)) => *n,
        other => panic!("{name}: {other:?}"),
    }
}

fn read(fixture: &Fixture, branch: &str, path: &str) -> Option<String> {
    fixture
        .try_call(
            "fs.read",
            vec![("branch", text(branch)), ("path", text(path))],
        )
        .ok()
        .map(|reply| reply.get_text("text").unwrap().to_owned())
}

#[test]
fn a_unit_one_side_changed_merges_as_today() {
    let fixture = seeded("f057-one-side");
    // Ours changes the unit, theirs only outside it: no unit is decided.
    sides(
        &fixture,
        ("l", vec![write("/nm/a", "a, ours\n"), rm("/nm/b")]),
        ("r", vec![write("/src/t", "t, theirs\n")]),
    );
    let plain = merge(&fixture, "plain", None).unwrap();
    let united = merge(&fixture, "united", Some(bare(&["nm"]))).unwrap();
    assert_eq!(plain.get_text("root"), united.get_text("root"));
    assert!(units_of(&united).is_empty());
    assert_eq!(
        read(&fixture, "united", "/nm/a").as_deref(),
        Some("a, ours\n")
    );
    assert_eq!(
        read(&fixture, "united", "/src/t").as_deref(),
        Some("t, theirs\n")
    );
    // Neither side changed `src`; theirs alone changed `nm/lib`'s unit.
    let other = merge(&fixture, "other", Some(bare(&["src/s", "lib"]))).unwrap();
    assert_eq!(plain.get_text("root"), other.get_text("root"));
    fixture.cleanup();
}

#[test]
fn a_unit_both_sides_changed_comes_whole_from_the_later_side() {
    let fixture = seeded("f057-whole");
    sides(
        &fixture,
        (
            "l",
            vec![
                write("/nm/a", "a, ours\n"),
                write("/nm/new", "ours only\n"),
                write("/src/s", "1, ours\n2\n3\n"),
            ],
        ),
        (
            "r",
            vec![
                write("/nm/a", "a, theirs\n"),
                rm("/nm/c"),
                write("/src/s", "1\n2\n3, theirs\n"),
            ],
        ),
    );
    // Without units `/nm/a` conflicts.
    let refused = merge(&fixture, "plain", None).expect_err("a text conflict");
    assert!(format!("{refused:?}").contains("Conflict"), "{refused:?}");
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    let units = units_of(&reply);
    assert_eq!(units.len(), 1, "{reply:?}");
    let decided = &units[0];
    assert_eq!(decided.get_text("unit"), Some("nm"));
    assert_eq!(decided.get_text("kept"), Some("theirs"));
    // Each side's two paths and `/nm` itself, whose time moved.
    assert_eq!(int(decided, "changedOurs"), 3);
    assert_eq!(int(decided, "changedTheirs"), 3);
    assert_eq!(int(decided, "dropped"), 3);
    assert!(decided.get_text("keptTime").unwrap().ends_with('Z'));
    assert_eq!(reply.get("unitsTruncated"), Some(&SdkValue::Bool(false)));
    // The unit is theirs exactly; outside it both sides' edits merged.
    let merged = view(&fixture, reply.get_text("root").unwrap());
    let theirs = view(&fixture, &root_of(&fixture, "r"));
    assert_eq!(under(&merged, "/nm"), under(&theirs, "/nm"));
    assert_eq!(
        read(&fixture, "m", "/src/s").as_deref(),
        Some("1, ours\n2\n3, theirs\n")
    );
    // The record keeps the units the merge ran with.
    let commits = fixture.call("branch.list_commits", vec![("branch", text("m"))]);
    let recorded = support::items(&commits)[0].get("units").expect("units");
    assert_eq!(
        recorded,
        &SdkValue::List(vec![SdkValue::map([
            ("name", text("nm")),
            ("patterns", bare(&["nm"])),
        ])])
    );
    fixture.cleanup();
}

#[test]
fn ours_is_kept_on_a_tie() {
    let fixture = seeded("f057-tie");
    fixture.fork("l");
    fixture.fork("r");
    // Workspace writes carry the same time, 123 s, on both sides.
    fixture.workspace_commit("l", &[Change::Write("/nm/a", b"L\n")], 1, || {});
    fixture.workspace_commit("r", &[Change::Write("/nm/b", b"R\n")], 2, || {});
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    let units = units_of(&reply);
    assert_eq!(units[0].get_text("kept"), Some("ours"), "{reply:?}");
    assert!(units[0]
        .get_text("keptTime")
        .unwrap()
        .starts_with("1970-01-01T00:02:03"));
    assert_eq!(read(&fixture, "m", "/nm/a").as_deref(), Some("L\n"));
    assert_eq!(read(&fixture, "m", "/nm/b").as_deref(), Some("b\n"));
    fixture.cleanup();
}

#[test]
fn deleting_or_retyping_the_unit_root_is_a_change() {
    // Ours deletes the unit root last: it is absent in the result.
    let fixture = seeded("f057-delete");
    sides(
        &fixture,
        ("r", vec![write("/nm/a", "a, theirs\n")]),
        ("l", vec![rm("/nm")]),
    );
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    assert_eq!(
        units_of(&reply)[0].get_text("kept"),
        Some("ours"),
        "{reply:?}"
    );
    let merged = view(&fixture, reply.get_text("root").unwrap());
    assert!(under(&merged, "/nm").is_empty(), "{merged:?}");
    fixture.cleanup();
    // Theirs deletes it first and ours retypes it later: ours' file stands.
    let fixture = seeded("f057-retype");
    sides(
        &fixture,
        ("r", vec![rm("/nm")]),
        ("l", vec![rm("/nm"), write("/nm", "a file now\n")]),
    );
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    assert_eq!(
        units_of(&reply)[0].get_text("kept"),
        Some("ours"),
        "{reply:?}"
    );
    assert_eq!(read(&fixture, "m", "/nm").as_deref(), Some("a file now\n"));
    fixture.cleanup();
    // Theirs, later, keeps the directory it changed: ours' file is dropped.
    let fixture = seeded("f057-retype-lost");
    sides(
        &fixture,
        ("l", vec![rm("/nm"), write("/nm", "a file now\n")]),
        ("r", vec![write("/nm/a", "a, theirs\n")]),
    );
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    assert_eq!(units_of(&reply)[0].get_text("kept"), Some("theirs"));
    let merged = view(&fixture, reply.get_text("root").unwrap());
    let theirs = view(&fixture, &root_of(&fixture, "r"));
    assert_eq!(under(&merged, "/nm"), under(&theirs, "/nm"));
    fixture.cleanup();
}

#[test]
fn renames_across_and_within_a_unit() {
    let fixture = seeded("f057-renames");
    // Ours moves a file out of the unit, one into it and one within it;
    // theirs changes the unit later and wins it.
    sides(
        &fixture,
        (
            "l",
            vec![
                mv("/nm/a", "/moved_a"),
                mv("/top.txt", "/nm/top.txt"),
                mv("/nm/b", "/nm/b2"),
            ],
        ),
        ("r", vec![write("/nm/c", "c, theirs\n")]),
    );
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    let decided = &units_of(&reply)[0];
    assert_eq!(decided.get_text("kept"), Some("theirs"), "{reply:?}");
    // `a` and `b` gone, `top.txt` and `b2` added, and `/nm`'s time.
    assert_eq!(int(decided, "changedOurs"), 5);
    // `/nm/a` is back; the moved-out copy keeps its serial, a cross link.
    assert_eq!(int(decided, "crossLinks"), 1);
    let merged = view(&fixture, reply.get_text("root").unwrap());
    let theirs = view(&fixture, &root_of(&fixture, "r"));
    let mut expected = under(&theirs, "/nm");
    for line in &mut expected {
        if line.starts_with("/nm/a ") {
            line.push_str(" link=/moved_a");
        }
    }
    assert_eq!(under(&merged, "/nm"), expected);
    // Outside the unit: the move out stands, the move in is a deletion.
    assert_eq!(read(&fixture, "m", "/moved_a").as_deref(), Some("a\n"));
    assert_eq!(read(&fixture, "m", "/top.txt"), None);
    fixture.cleanup();
}

#[test]
fn a_directory_moved_out_of_a_lost_unit_is_restored_beside_its_move() {
    let fixture = seeded("f057-dir-move");
    sides(
        &fixture,
        ("l", vec![mv("/nm/lib", "/lib2")]),
        ("r", vec![write("/nm/a", "a, theirs\n")]),
    );
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    assert_eq!(units_of(&reply)[0].get_text("kept"), Some("theirs"));
    assert_eq!(read(&fixture, "m", "/nm/lib/x").as_deref(), Some("x\n"));
    assert_eq!(read(&fixture, "m", "/lib2/x").as_deref(), Some("x\n"));
    let merged = view(&fixture, reply.get_text("root").unwrap());
    let theirs = view(&fixture, &root_of(&fixture, "r"));
    let strip = |lines: Vec<String>| -> Vec<String> {
        lines
            .iter()
            .map(|line| line.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .collect()
    };
    assert_eq!(strip(under(&merged, "/nm")), strip(under(&theirs, "/nm")));
    fixture.cleanup();
}

#[test]
fn two_patterns_make_one_unit() {
    let build = group("build", &["lib", "*.tsbuildinfo"]);
    let fixture = seeded("f057-group");
    sides(
        &fixture,
        ("r", vec![write("/p.tsbuildinfo", "p, theirs\n")]),
        ("l", vec![write("/lib/i.js", "i, ours\n")]),
    );
    let reply = merge(&fixture, "m", Some(SdkValue::List(vec![build]))).unwrap();
    let decided = &units_of(&reply)[0];
    assert_eq!(decided.get_text("unit"), Some("build"));
    assert_eq!(decided.get_text("kept"), Some("ours"));
    assert_eq!(
        read(&fixture, "m", "/lib/i.js").as_deref(),
        Some("i, ours\n")
    );
    assert_eq!(
        read(&fixture, "m", "/p.tsbuildinfo").as_deref(),
        Some("p\n")
    );
    // As two units, each side's change stands.
    let apart = merge(&fixture, "apart", Some(bare(&["lib", "*.tsbuildinfo"]))).unwrap();
    assert!(units_of(&apart).is_empty());
    assert_eq!(
        read(&fixture, "apart", "/p.tsbuildinfo").as_deref(),
        Some("p, theirs\n")
    );
    fixture.cleanup();
}

#[test]
fn overlapping_units_belong_to_the_first_listed() {
    let fixture = seeded("f057-overlap");
    sides(
        &fixture,
        ("l", vec![write("/nm/lib/x", "x, ours\n")]),
        ("r", vec![write("/nm/a", "a, theirs\n")]),
    );
    // `nm` first: one unit both changed, theirs later.
    let first = merge(&fixture, "first", Some(bare(&["nm", "**/lib"]))).unwrap();
    assert_eq!(units_of(&first).len(), 1);
    assert_eq!(read(&fixture, "first", "/nm/lib/x").as_deref(), Some("x\n"));
    // `**/lib` first: each unit one side changed.
    let second = merge(&fixture, "second", Some(bare(&["**/lib", "nm"]))).unwrap();
    assert!(units_of(&second).is_empty());
    assert_eq!(
        read(&fixture, "second", "/nm/lib/x").as_deref(),
        Some("x, ours\n")
    );
    assert_eq!(
        read(&fixture, "second", "/nm/a").as_deref(),
        Some("a, theirs\n")
    );
    fixture.cleanup();
}

#[test]
fn a_hard_link_across_the_boundary_takes_the_units_value() {
    let fixture = seeded("f057-link");
    // Ours edits the file through its outside name; theirs wins the unit.
    sides(
        &fixture,
        ("l", vec![write("/out/hl", "edited outside\n")]),
        ("r", vec![write("/nm/a", "a, theirs\n")]),
    );
    let left = read(&fixture, "l", "/nm/hl");
    println!("F057_LINK inside name on ours: {left:?}");
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    if left.as_deref() == Some("edited outside\n") {
        // The edit reached the inside name: the unit's decision sets the
        // serial's value everywhere, and reports the link.
        let decided = &units_of(&reply)[0];
        assert_eq!(decided.get_text("kept"), Some("theirs"));
        assert_eq!(int(decided, "crossLinks"), 1, "{reply:?}");
        assert_eq!(read(&fixture, "m", "/nm/hl").as_deref(), Some("linked\n"));
        assert_eq!(read(&fixture, "m", "/out/hl").as_deref(), Some("linked\n"));
    } else {
        // The write replaced the outside name: no shared serial is left.
        assert!(units_of(&reply).is_empty(), "{reply:?}");
        assert_eq!(
            read(&fixture, "m", "/out/hl").as_deref(),
            Some("edited outside\n")
        );
    }
    fixture.cleanup();
}

#[test]
fn units_come_before_lww_rules_which_still_apply_outside() {
    let fixture = Fixture::new("f057-lww", |source| {
        fs::create_dir(source.join("nm")).unwrap();
        fs::write(source.join(".lww"), "*.gen\n").unwrap();
        fs::write(source.join("nm/x.gen"), "x\n").unwrap();
        fs::write(source.join("nm/y"), "y\n").unwrap();
        fs::write(source.join("out.gen"), "o\n").unwrap();
        5
    });
    sides(
        &fixture,
        ("r", vec![write("/out.gen", "o, theirs\n")]),
        (
            "l",
            vec![
                write("/nm/x.gen", "x, ours\n"),
                write("/out.gen", "o, ours\n"),
            ],
        ),
    );
    // Theirs changes the unit last.
    fixture.apply("r", vec![write("/nm/y", "y, theirs\n")]);
    let reply = merge(&fixture, "m", Some(bare(&["nm"]))).unwrap();
    assert_eq!(units_of(&reply)[0].get_text("kept"), Some("theirs"));
    assert_eq!(read(&fixture, "m", "/nm/x.gen").as_deref(), Some("x\n"));
    // Outside the unit the rule decided, newer wins.
    assert_eq!(
        read(&fixture, "m", "/out.gen").as_deref(),
        Some("o, ours\n")
    );
    let lww = match reply.get("lww") {
        Some(SdkValue::List(lww)) => lww,
        other => panic!("lww: {other:?}"),
    };
    assert_eq!(lww.len(), 1, "{reply:?}");
    assert_eq!(lww[0].get_text("path"), Some("/out.gen"));
    fixture.cleanup();
}

#[test]
fn a_long_unit_list_is_truncated_by_its_byte_window() {
    const UNITS: usize = 60;
    let fixture = Fixture::new("f057-window", |source| {
        for unit in 0..UNITS {
            fs::write(source.join(format!("u{unit:02}")), "base\n").unwrap();
        }
        UNITS
    });
    let edits = |side: &str| -> Vec<SdkValue> {
        (0..UNITS)
            .map(|unit| write(&format!("/u{unit:02}"), format!("{side}\n")))
            .collect()
    };
    sides(&fixture, ("l", edits("ours")), ("r", edits("theirs")));
    let units = SdkValue::List(
        (0..UNITS)
            .map(|unit| {
                group(
                    &format!("{unit:02}{}", "n".repeat(300)),
                    &[&format!("/u{unit:02}")],
                )
            })
            .collect(),
    );
    let reply = merge(&fixture, "m", Some(units)).unwrap();
    assert_eq!(reply.get("unitsTruncated"), Some(&SdkValue::Bool(true)));
    let listed = units_of(&reply).len();
    assert!(listed > 0 && listed < UNITS, "{listed} listed");
    assert_eq!(read(&fixture, "m", "/u07").as_deref(), Some("theirs\n"));
    fixture.cleanup();
}

#[test]
fn malformed_units_are_refused_before_any_work() {
    let fixture = seeded("f057-admission");
    sides(
        &fixture,
        ("l", vec![write("/nm/a", "l\n")]),
        ("r", vec![write("/nm/b", "r\n")]),
    );
    for units in [
        bare(&[""]),
        bare(&["#nm"]),
        bare(&["nm", "nm"]),
        SdkValue::List(vec![group("", &["nm"])]),
        SdkValue::List(vec![group("g", &[])]),
        SdkValue::List(vec![SdkValue::Int(1)]),
        text("nm"),
    ] {
        let refused = merge(&fixture, "m", Some(units.clone())).expect_err("refused");
        assert!(
            format!("{refused:?}").contains("InvalidInput"),
            "{units:?}: {refused:?}"
        );
    }
    assert!(
        read(&fixture, "m", "/nm/a").is_none(),
        "nothing was created"
    );
    fixture.cleanup();
}

#[test]
fn units_that_decide_nothing_leave_the_root_byte_identical() {
    let fixture = seeded("f057-golden");
    sides(
        &fixture,
        (
            "l",
            vec![write("/src/s", "1, ours\n2\n3\n"), mv("/nm/a", "/nm/a2")],
        ),
        (
            "r",
            vec![write("/src/s", "1\n2\n3, theirs\n"), rm("/top.txt")],
        ),
    );
    let plain = merge(&fixture, "plain", None).unwrap();
    for (name, units) in [
        ("empty", SdkValue::List(Vec::new())),
        ("unmatched", bare(&["absent"])),
        ("one-sided", bare(&["nm"])),
    ] {
        let reply = merge(&fixture, name, Some(units)).unwrap();
        assert_eq!(reply.get_text("root"), plain.get_text("root"), "{name}");
    }
    let merged = view(&fixture, plain.get_text("root").unwrap());
    assert!(!outside(&merged, "/nm").is_empty());
    fixture.cleanup();
}
