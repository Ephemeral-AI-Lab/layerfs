use crate::dump::render_to_string;

#[test]
fn every_page_renders_at_all_frozen_sizes() {
    for page in [
        "projects",
        "topology",
        "branch",
        "workspaces",
        "activity",
        "diff",
    ] {
        for (width, height) in [(80, 24), (120, 40), (200, 60)] {
            let screen = render_to_string(page, width, height, false).unwrap();
            assert!(screen.contains("LayerFS"), "{page} {width}x{height}");
            assert_eq!(screen.lines().count(), height as usize);
        }
    }
}

#[test]
fn monochrome_keeps_textual_state_signals() {
    let screen = render_to_string("topology", 200, 60, true).unwrap();
    for signal in ["AUTH", "WORK", "REP", "PULL", "[B]", "[C]"] {
        assert!(screen.contains(signal), "missing {signal}");
    }
}

#[test]
fn resize_gate_is_explicit() {
    let screen = render_to_string("projects", 79, 23, true).unwrap();
    assert!(screen.contains("at least 80x24"));
}

#[test]
fn compact_secondary_panes_and_overlays_are_provable() {
    for (page, expected) in [
        ("topology:graph", "AUTHORITY GRAPH"),
        ("topology:inspector", "INSPECTOR"),
        ("branch:inspector", "Workspace unavailable: Fork first"),
        ("workspaces:detail", "Anchor"),
        ("activity:receipt", "RECEIPT"),
        ("storage:dedup", "DEDUP"),
        ("diff:content", "BEFORE"),
        ("diff:details", "Final-state diff"),
        ("projects:help", "Signals"),
        ("projects:command", "COMPLETIONS"),
        ("projects:plan", "COMMAND PLAN"),
        ("projects:operation", "roots 17/21"),
        ("projects:error", "mock integrity failure"),
    ] {
        let screen = render_to_string(page, 80, 24, true).unwrap();
        assert!(screen.contains(expected), "{page}: missing {expected}");
    }
}

#[test]
fn branch_selection_is_scrolled_into_compact_view() {
    let screen = render_to_string("branch", 80, 24, true).unwrap();
    assert!(screen.contains(">│ ├─ [C] C35"), "{screen}");
    assert!(screen.contains("–"));
}

#[test]
fn empty_context_has_an_actionable_landing_page() {
    let screen = render_to_string("empty", 80, 24, true).unwrap();
    assert!(screen.contains("No LayerStacks exist"));
    assert!(screen.contains("layerstack init --name <name> --empty"));
}

#[test]
fn compact_reconciliation_shows_every_unresolved_conflict() {
    let screen = render_to_string("reconciliation:detail", 80, 24, true).unwrap();
    assert!(screen.contains("Commit C-B-local-sync-04 vs current Layer L-A-19"));
    assert!(screen.contains("conflict-1  src/model.rs · Content"));
    assert!(screen.contains("conflict-2  src/schema.rs · Directory"));
}
