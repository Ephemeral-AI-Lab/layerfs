//! Only the Store calls whose admission compares placed mount intents read
//! the caller's home place view (`place_view::admits`): the exclusive
//! removals. Reads, listings, file writes and merges never build or send it,
//! and the view a worker frame or a caller installed is kept, never replaced.
use layerfs_history::PlaceView;
use layerfs_persistence::{has_place_view, with_place_view};
use layerfs_store::admin::open::args::OpenArgs;
use layerfs_store::admin::route::place_view::{admits, scoped};

#[test]
fn only_the_exclusive_removals_read_the_place_view() {
    for method in ["branch.remove", "project.hide_path"] {
        assert!(admits(method), "{method}");
    }
    for method in [
        "store.stats",
        "project.stats",
        "branch.list",
        "branch.fork",
        "commit.list",
        "commit.get",
        "branch.merge",
        "branch.publish",
        "commit.diff",
        "file.read",
        "file.history",
        "file.list",
        "file.stat",
        "file.grep",
        "file.glob",
        "file.blame",
        "file.write",
        "file.edit",
        "file.mkdir",
        "file.rm",
        "file.cp",
        "file.mv",
        "file.apply",
    ] {
        assert!(
            !admits(method),
            "{method} must not read the home place view"
        );
    }
}

#[test]
fn an_admitting_call_reads_its_home_once_and_keeps_an_installed_view() {
    let home = std::env::temp_dir().join(format!("lfs-place-scope-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("machine")).unwrap();
    let open = OpenArgs {
        home: Some(home.clone()),
        ..OpenArgs::default()
    };
    // a home with no place evidence installs nothing
    assert!(!scoped(&open, has_place_view));
    // a worker frame's view stays installed: the call does not replace it
    let mut view = PlaceView::new();
    view.set_machine("box", "p1:worker:0000000000000001:00");
    assert!(with_place_view(Some(view), || scoped(
        &open,
        has_place_view
    )));
    assert!(!has_place_view());
    let _ = std::fs::remove_dir_all(&home);
}
