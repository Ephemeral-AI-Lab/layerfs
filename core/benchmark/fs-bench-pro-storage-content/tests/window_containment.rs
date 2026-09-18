//! The envelope checks: containment, sibling disjointness and root coverage.
//!
//! **What is deliberately not tested as a detector.** `window.rs` states that
//! `Sigma self_ns == root.elapsed_ns` is a tautology of the product's own tree
//! arithmetic and not an `attach` detector. The test that matters is therefore the
//! negative one: two trees with **identical** root elapsed values and **identical**
//! children's elapsed sums, one a valid envelope and one carrying an overlapping
//! sibling pair, must receive different verdicts. If the elapsed sum were being
//! used to decide, they could not.
//!
//! Every bracket here is built by hand with an explicit parent, so a defect is
//! injected deliberately rather than waited for.

use fs_bench_storage_content::support::window::{mono_raw_ns, Bracket, Defect, WindowTree};

/// One hand-built bracket with explicit stamps.
fn bracket(id: u32, parent: Option<u32>, open_ns: u64, close_ns: u64) -> Bracket {
    Bracket {
        id,
        parent,
        label: format!("bracket-{id}"),
        open_ns,
        close_ns,
    }
}

/// A root `0..1000` plus whatever children are supplied.
fn tree_with(children: Vec<Bracket>) -> WindowTree {
    let mut brackets = vec![bracket(0, None, 0, 1000)];
    brackets.extend(children);
    WindowTree { brackets }
}

#[test]
fn a_balanced_tree_is_a_valid_envelope() {
    // Two disjoint children that together tile the root exactly.
    let tree = tree_with(vec![
        bracket(1, Some(0), 0, 400),
        bracket(2, Some(0), 400, 1000),
    ]);
    assert_eq!(
        tree.check(),
        Vec::new(),
        "a balanced tree must report no defects"
    );
    assert_eq!(tree.root_elapsed_ns(), 1000);
}

#[test]
fn a_child_that_extends_past_its_parent_is_not_contained() {
    let tree = tree_with(vec![bracket(1, Some(0), 200, 1200)]);
    assert!(
        tree.check()
            .contains(&Defect::NotContained { id: 1, parent: 0 }),
        "a child closing after its parent must be reported"
    );

    let early = tree_with(vec![bracket(1, Some(0), 0, 1001)]);
    assert!(early
        .check()
        .contains(&Defect::NotContained { id: 1, parent: 0 }));

    // Opening before the parent is the same defect on the other edge.
    let mut root_shifted = tree_with(vec![bracket(1, Some(0), 10, 900)]);
    root_shifted.brackets[0].open_ns = 20;
    assert!(root_shifted
        .check()
        .contains(&Defect::NotContained { id: 1, parent: 0 }));
}

#[test]
fn a_child_may_touch_its_parent_bounds_exactly() {
    // Containment is inclusive: a child that opens with its parent and closes with
    // it is inside it, so a boundary comparison must not be strict.
    let tree = tree_with(vec![bracket(1, Some(0), 0, 1000)]);
    assert_eq!(tree.check(), Vec::new());
}

#[test]
fn siblings_that_overlap_are_reported() {
    let tree = tree_with(vec![
        bracket(1, Some(0), 0, 600),
        bracket(2, Some(0), 400, 1000),
    ]);
    assert!(
        tree.check()
            .contains(&Defect::SiblingOverlap { left: 1, right: 2 }),
        "two overlapping children of the same parent must be reported"
    );
}

#[test]
fn siblings_that_merely_touch_do_not_overlap() {
    // `[0, 400]` and `[400, 1000]` share an instant but no duration, so a half-open
    // reading would call this clean and a closed reading would not. The frozen
    // reading is the half-open one: touching is not overlapping.
    let tree = tree_with(vec![
        bracket(1, Some(0), 0, 400),
        bracket(2, Some(0), 400, 1000),
    ]);
    assert_eq!(tree.check(), Vec::new());
}

#[test]
fn a_bracket_outside_the_root_is_reported() {
    // The root does not enclose a child that closes past it even when that child's
    // own parent is generous.
    let tree = tree_with(vec![
        bracket(1, Some(0), 0, 1000),
        bracket(2, Some(1), 0, 5000),
    ]);
    let defects = tree.check();
    assert!(defects.contains(&Defect::RootDoesNotEnclose { id: 2 }));
    assert!(defects.contains(&Defect::NotContained { id: 2, parent: 1 }));
}

#[test]
fn an_unknown_parent_is_reported_and_not_contained() {
    let tree = tree_with(vec![bracket(1, Some(9), 0, 500)]);
    let defects = tree.check();
    assert!(defects.contains(&Defect::UnknownParent { id: 1, parent: 9 }));
    assert!(
        !defects.contains(&Defect::NotContained { id: 1, parent: 9 }),
        "a bracket with no enclosing bracket cannot be tested for containment"
    );
}

#[test]
fn a_bracket_that_closes_before_it_opens_is_unbalanced() {
    let tree = tree_with(vec![bracket(1, Some(0), 800, 200)]);
    let defects = tree.check();
    assert!(defects.contains(&Defect::Unbalanced { id: 1 }));
}

#[test]
fn an_empty_tree_has_no_defects_and_no_elapsed() {
    let empty = WindowTree::default();
    assert_eq!(empty.check(), Vec::new());
    assert_eq!(empty.root_elapsed_ns(), 0);
}

#[test]
fn a_matching_elapsed_sum_is_not_used_as_an_attach_detector() {
    // The valid envelope and the defective one are built to have the *same* root
    // elapsed (1000 ns) and the *same* sum of child elapsed (1000 ns). Only the
    // second has overlapping siblings. If the elapsed arithmetic were the
    // detector, these two could not disagree.
    let valid = tree_with(vec![
        bracket(1, Some(0), 0, 400),
        bracket(2, Some(0), 400, 1000),
    ]);
    let defective = tree_with(vec![
        bracket(1, Some(0), 0, 400),
        bracket(2, Some(0), 0, 600),
    ]);

    let child_sum = |tree: &WindowTree| -> u64 {
        tree.brackets
            .iter()
            .filter(|bracket| bracket.parent.is_some())
            .map(|bracket| bracket.close_ns - bracket.open_ns)
            .sum()
    };

    assert_eq!(valid.root_elapsed_ns(), defective.root_elapsed_ns());
    assert_eq!(child_sum(&valid), child_sum(&defective));
    assert_eq!(child_sum(&valid), valid.root_elapsed_ns());

    assert_eq!(valid.check(), Vec::new(), "the balanced tree must pass");
    assert!(
        !defective.check().is_empty(),
        "the overlapping tree must fail despite the identical elapsed arithmetic"
    );
}

#[test]
fn the_recorded_api_links_children_and_closes_a_real_envelope() {
    // Exercises the constructor path rather than hand-built brackets: the stamps
    // come from `CLOCK_MONOTONIC_RAW`, so this also proves the clock is readable on
    // this host and that nesting produced by the real API is contained.
    let (mut tree, root_open) = WindowTree::open("root");
    let (child, child_open) = tree.open_child(0, "timed-phase");
    let (inner, _) = tree.open_child(child, "counter-read");
    tree.close(inner);
    tree.close(child);
    tree.close(0);

    assert_eq!(tree.brackets.len(), 3);
    assert_eq!(tree.brackets[1].parent, Some(0));
    assert_eq!(tree.brackets[2].parent, Some(child));
    assert_eq!(tree.brackets[1].label, "timed-phase");
    assert_eq!(
        tree.check(),
        Vec::new(),
        "a real nested envelope must be valid"
    );
    assert!(tree.brackets[0].close_ns >= root_open);
    assert!(tree.brackets[1].open_ns >= child_open);
    // Containment has a consequence the envelope must satisfy: a child cannot
    // outlast the root that encloses it.
    let root_elapsed = tree.root_elapsed_ns();
    for bracket in tree.brackets.iter().skip(1) {
        assert!(
            bracket.close_ns - bracket.open_ns <= root_elapsed,
            "a contained bracket outlasted its root"
        );
    }
}

#[test]
fn the_one_clock_domain_is_readable_and_non_decreasing() {
    let first = mono_raw_ns().expect("CLOCK_MONOTONIC_RAW must be readable on a unix host");
    let second = mono_raw_ns().expect("CLOCK_MONOTONIC_RAW must be readable on a unix host");
    assert!(
        second >= first,
        "a monotonic clock went backwards: {first} then {second}"
    );
}
