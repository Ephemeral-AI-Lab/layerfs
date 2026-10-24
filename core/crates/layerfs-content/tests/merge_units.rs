//! Merge units (#352 F-057): admission, which unit covers a path, and what
//! counts as a change in one.
mod support;

use layerfs_content::merge::{decide_units, unit_changes, MergeUnit, Side, UnitChange, Units};
use support::filesystem::TreeStore;
use support::topology::{Shape, ROOT};

fn unit(name: &str, patterns: &[&str]) -> MergeUnit {
    MergeUnit {
        name: name.to_owned(),
        patterns: patterns.iter().map(|p| (*p).to_owned()).collect(),
    }
}

#[test]
fn malformed_patterns_and_bad_names_are_refused() {
    for units in [
        vec![unit("", &["a"])],
        vec![unit("a", &["a"]), unit("a", &["b"])],
        vec![unit("a", &[""])],
        vec![unit("a", &["/"])],
        vec![unit("a", &["#a"])],
        vec![unit("a", &["a//b"])],
        vec![unit("a", &["a\nb"])],
        vec![unit("a", &[])],
        vec![unit("a", &["!a"])],
    ] {
        assert!(Units::new(&units).is_err(), "{units:?} admitted");
    }
    assert!(Units::new(&[unit("a", &["a", "!a/keep"])]).is_ok());
    assert!(Units::new(&[]).unwrap().is_empty());
}

#[test]
fn a_unit_covers_its_matches_and_below_and_the_first_listed_wins() {
    let units = Units::new(&[
        unit("node_modules", &["node_modules"]),
        unit("build", &["packages/*/lib", "**/tsconfig.tsbuildinfo"]),
    ])
    .unwrap();
    assert_eq!(units.unit_of("/"), None);
    assert_eq!(units.unit_of("/node_modules"), Some(0));
    assert_eq!(units.unit_of("/a/node_modules/x/y"), Some(0));
    assert_eq!(units.unit_of("/packages/p/lib/i.js"), Some(1));
    assert_eq!(units.unit_of("/x/tsconfig.tsbuildinfo"), Some(1));
    assert_eq!(units.unit_of("/packages/p/src/i.js"), None);
    // Matched by both: the first listed.
    assert_eq!(units.unit_of("/node_modules/packages/p/lib"), Some(0));
    let reversed = Units::new(&[
        unit("lib", &["**/lib"]),
        unit("node_modules", &["node_modules"]),
    ])
    .unwrap();
    assert_eq!(reversed.unit_of("/node_modules/x/lib/a"), Some(0));
    assert_eq!(reversed.unit_of("/node_modules/x/a"), Some(1));
    // A negated pattern clears the match below it.
    let negated = Units::new(&[unit("nm", &["nm", "!nm/keep"])]).unwrap();
    assert_eq!(negated.unit_of("/nm/a"), Some(0));
    assert_eq!(negated.unit_of("/nm/keep/a"), None);
}

#[test]
fn a_relink_that_rewrote_nothing_is_not_a_change() {
    // `d` holds the same files on both sides; the right side allocates one
    // directory first, so `d` and every file under it has another serial.
    let build = |shift: bool| {
        let mut shape = Shape::new();
        if shift {
            shape.dir(ROOT, "a");
        }
        let d = shape.dir(ROOT, "d");
        for file in 0..20 {
            shape.file(d, &format!("f{file:02}"));
        }
        shape.build()
    };
    let (base, side) = (build(false), build(true));
    let mut store = TreeStore::new();
    store.absorb(&base.store);
    store.absorb(&side.store);
    let units = Units::new(&[unit("d", &["d"])]).unwrap();
    let changes = unit_changes(&store, None, base.root, side.root, &units).unwrap();
    assert_eq!(changes, vec![UnitChange::default()]);
}

#[test]
fn only_units_both_sides_changed_are_decided_and_ours_wins_a_tie() {
    let units = Units::new(&[unit("a", &["a"]), unit("b", &["b"]), unit("c", &["c"])]).unwrap();
    let change = |changed, seconds| UnitChange {
        changed,
        latest: Some((seconds, 0)),
    };
    let ours = [change(2, 10), change(1, 5), change(3, 7)];
    let theirs = [UnitChange::default(), change(4, 6), change(1, 7)];
    let reports = decide_units(&units, &ours, &theirs);
    assert_eq!(reports.len(), 2);
    assert_eq!(
        (reports[0].name.as_str(), reports[0].kept),
        ("b", Side::Theirs)
    );
    assert_eq!((reports[0].kept_time, reports[0].dropped), ((6, 0), 1));
    assert_eq!(
        (reports[1].name.as_str(), reports[1].kept),
        ("c", Side::Ours)
    );
    assert_eq!((reports[1].changed_ours, reports[1].changed_theirs), (3, 1));
    assert_eq!(reports[1].dropped, 1);
}
