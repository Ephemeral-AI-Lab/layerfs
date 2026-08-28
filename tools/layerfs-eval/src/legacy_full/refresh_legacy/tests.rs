use super::primitives::{mark_ambiguous, ordered_path};
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::DriverError;

#[test]
fn ordered_paths_support_the_valid_256_component_depth() {
    let parent = std::iter::repeat_n("x", 255).collect::<Vec<_>>().join("/");
    let child = format!("{parent}/x");
    let parent = CanonicalPath::new(&parent).unwrap();
    let child = CanonicalPath::new(&child).unwrap();

    assert!(
        ordered_path(parent.as_bytes(), false).unwrap()
            < ordered_path(child.as_bytes(), false).unwrap()
    );
    assert!(
        ordered_path(child.as_bytes(), true).unwrap()
            < ordered_path(parent.as_bytes(), true).unwrap()
    );
    assert!(ordered_path(b"", false).unwrap() < ordered_path(b"x", false).unwrap());
}

#[test]
fn ambiguous_driver_failures_mark_possible_visibility() {
    for error in [
        DriverError::VisibilityAmbiguous,
        DriverError::DurabilityAmbiguous,
    ] {
        let mut visible = false;
        mark_ambiguous(&mut visible, &error);
        assert!(visible);
    }
    let mut visible = false;
    mark_ambiguous(&mut visible, &DriverError::Conflict);
    assert!(!visible);
}
