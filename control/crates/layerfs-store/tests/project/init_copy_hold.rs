//! A worker's folder copy for `project.init` is held by the Init that
//! reads it (spec v20): a `project.stage` `discard` (which `project delete`
//! sends to settle an Init whose owner is gone) leaves a held copy to that
//! Init, which removes it when it ends, and removes a copy nothing holds.
use layerfs_store::data::project::frame::{StageFrame, StageStep};
use layerfs_store::data::project::init::staged;
use std::path::PathBuf;

fn root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-init-copy-hold-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_discard_leaves_a_held_copy_and_removes_a_free_one() {
    let root = root();
    let copy = "00112233aabbccdd";
    let folder = staged::folder(&root, copy);
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::write(folder.join("src/main.rs"), b"fn main() {}\n").unwrap();
    let discard = StageFrame {
        store: "shop".to_owned(),
        name: "web".to_owned(),
        id: copy.to_owned(),
        step: StageStep::Discard,
    };

    // an Init reads it
    let held = staged::hold(&folder)
        .unwrap()
        .expect("nothing holds it yet");
    assert!(held.is_some());
    assert!(staged::hold(&folder).unwrap().is_none(), "held twice");
    let answer = staged::run(&root, &discard).unwrap();
    assert!(
        !answer.get_bool("discarded") && answer.get_bool("held"),
        "{answer:?}"
    );
    assert!(folder.join("src/main.rs").is_file());

    // its Init ended without removing it: the next discard does
    drop(held);
    let answer = staged::run(&root, &discard).unwrap();
    assert!(answer.get_bool("discarded"), "{answer:?}");
    assert!(!folder.exists());
    // gone already: nothing to discard, nothing held
    let answer = staged::run(&root, &discard).unwrap();
    assert!(!answer.get_bool("discarded") && !answer.get_bool("held"));
    let _ = std::fs::remove_dir_all(root);
}
