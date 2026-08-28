use super::super::artifact::unix_ns;
use super::super::engine_counters::EngineDelta;
use super::super::locality::verify_locality;
use super::super::resources::open_store_connection_count;
use crate::legacy_full::{IntegrityMode, LayerFs};
use std::fs;
use std::io::Cursor;

#[cfg(target_os = "macos")]
#[test]
fn small_real_apple_routes_cover_both_directions_burst_history_and_metadata() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-stage1.1-small-routes-{}-{}",
            std::process::id(),
            unix_ns().unwrap()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open_with_integrity(&store, IntegrityMode::TrustedLocalDev).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    let mut expected = (0..64 * 1024)
        .map(|index| (index as u8).wrapping_mul(13))
        .collect::<Vec<_>>();
    fs::write(source.path().join("file"), &expected).unwrap();
    let root0 = source.capture_quiescent().unwrap();
    drop(source);
    fs::remove_dir_all(base.join("source")).unwrap();
    let state0 = opened.fs.current_head("main").unwrap();
    assert_eq!(state0.root, root0);
    let mut managed = opened.fs.materialize_managed(root0).unwrap();
    let native = managed
        .replace_observed("file", 1_003, 0, b"physical-insert")
        .unwrap();
    assert_eq!(
        native.native.route,
        Some(crate::legacy_full::NativeRoute::InPlaceShift)
    );
    expected.splice(1_003..1_003, *b"physical-insert");
    let mut live = Vec::new();
    managed.read_to("file", &mut live).unwrap();
    assert_eq!(live, expected);
    let before = opened.fs.diagnostics().unwrap();
    let (state1, checkpoint) = managed.checkpoint_observed().unwrap();
    let after = opened.fs.diagnostics().unwrap();
    let checkpoint_delta = EngineDelta::between(&before, &after).unwrap();
    checkpoint_delta.verify_trusted_transition().unwrap();
    assert_eq!(checkpoint.descriptor_resets, 1);
    let mut canonical = Vec::new();
    opened
        .fs
        .read_to(state1.root, "file", &mut canonical)
        .unwrap();
    assert_eq!(canonical, expected);
    let (state2, logical) = opened
        .fs
        .replace_range_observed(&state1, "file", 5_007, 4, Cursor::new(*b"LOGI"))
        .unwrap();
    assert_eq!(logical.rope.cdc_bytes_scanned, 4);
    expected.splice(5_007..5_011, *b"LOGI");
    let refresh = managed.refresh(&state2).unwrap();
    assert!(matches!(
        refresh.native.route,
        Some(
            crate::legacy_full::NativeRoute::ClonePatch
                | crate::legacy_full::NativeRoute::InPlacePatch
        )
    ));
    let (accepted, logical) = opened
        .fs
        .replace_range_for_refresh_observed(
            &state2,
            "file",
            7_777,
            3,
            Cursor::new(*b"random-size-change"),
        )
        .unwrap();
    assert_eq!(logical.rope.cdc_bytes_scanned, 18);
    let suffix = expected.len() as u64 - 7_777 - 3;
    expected.splice(7_777..7_780, *b"random-size-change");
    let refresh = managed.refresh_splice(&accepted).unwrap();
    assert!(matches!(
        refresh.native.route,
        Some(
            crate::legacy_full::NativeRoute::CloneShift
                | crate::legacy_full::NativeRoute::InPlaceShift
        )
    ));
    assert_eq!(refresh.full_fallback_files, 0);
    assert_eq!(refresh.native.suffix_bytes_shifted, suffix);
    assert_eq!(refresh.native.bytes_read, suffix);
    assert_eq!(refresh.native.bytes_written, suffix + 18);
    managed
        .replace_observed("file", 10_001, 0, b"burst")
        .unwrap();
    expected.splice(10_001..10_001, *b"burst");
    managed.replace_observed("file", 20_003, 3, b"B").unwrap();
    expected.splice(20_003..20_006, *b"B");
    let before_burst = opened.fs.diagnostics().unwrap();
    let (state3, burst, steps) = managed.checkpoint_observed_detailed().unwrap();
    let after_burst = opened.fs.diagnostics().unwrap();
    let burst_delta = EngineDelta::between(&before_burst, &after_burst).unwrap();
    burst_delta.verify_trusted_transition().unwrap();
    assert_eq!(burst.descriptor_resets, 1);
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].tree_level_before, Some(0));
    verify_locality(&steps[0].counters, 5, 0).unwrap();
    verify_locality(&steps[1].counters, 1, 0).unwrap();
    let retained_metadata = managed.read_metadata("file").unwrap();
    let verified = LayerFs::open(&store).unwrap();
    assert_eq!(opened.fs.counter_snapshot().unwrap().active_connections, 1);
    assert_eq!(
        verified.fs.counter_snapshot().unwrap().active_connections,
        1
    );
    assert_eq!(open_store_connection_count(Some(&store)).unwrap(), 2);
    let mut old = Vec::new();
    verified.fs.read_to(root0, "file", &mut old).unwrap();
    assert_eq!(old.len(), 64 * 1024);
    let mut terminal = Vec::new();
    verified
        .fs
        .read_to(state3.root, "file", &mut terminal)
        .unwrap();
    assert_eq!(terminal, expected);
    let mut witness = verified
        .fs
        .materialize_external(state3.root, &base.join("witness"))
        .unwrap();
    assert_eq!(fs::read(witness.path().join("file")).unwrap(), expected);
    assert_eq!(witness.read_metadata("file").unwrap(), retained_metadata);
    witness.discard().unwrap();
    drop(witness);
    fs::remove_dir_all(base.join("witness")).unwrap();
    drop(verified);
    managed.discard().unwrap();
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}
