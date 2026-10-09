//! Current native read decisions over a real canonical namespace and Overlay.
mod common;
mod harness;
use harness::{unlink, Bench, T2};
use layerfs_overlay::{NativeMount, NativeMountState};
use layerfs_workspace::{NativeReadDecision, NativeReadOperation, NativeReadStage};
use std::sync::Arc;

fn lookup(b: &Bench, mount: NativeMount, request: u64, name: &str) -> u64 {
    let source = b.overlay.acquire_native_source(mount, request, 1).unwrap();
    let view = b.workspace.view_for_source(source).unwrap();
    let mut plan = view
        .native_read_plan(
            mount,
            NativeReadOperation::Lookup {
                parent: 1,
                name: common::name(name),
            },
        )
        .unwrap();
    let mut final_value = None;
    for _ in 0..4 {
        let before = b.demand();
        let outcome = Arc::new(plan.job().unwrap().perform(&b.overlay));
        assert_eq!(b.demand(), before, "owner round performed provider work");
        if let Some(value) = plan.accept(outcome).unwrap() {
            final_value = Some(value);
            break;
        }
        assert_eq!(plan.stage(), NativeReadStage::Base);
        plan.supply(&view).unwrap();
    }
    let value = final_value.expect("bounded lookup rounds");
    assert_eq!(plan.stage(), NativeReadStage::Finished);
    let serial = value.stat.serial;
    // A lookup answers with attributes only and retains no read.
    assert_eq!(value.read, None);
    b.overlay.release_base_source(source).unwrap();
    serial
}

#[test]
fn positive_lookup_preserves_alias_identity_and_exact_kernel_counts() {
    let b = Bench::new("native-lookup-alias");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let file = lookup(&b, mount, 1, "file");
    assert_eq!(lookup(&b, mount, 2, "alias"), file);
    assert_eq!(b.overlay.native_lookup_count(mount, file).unwrap(), Some(2));
    b.overlay.forget_native(mount, file, 2).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn deciding_lookup_sees_mutation_after_missing_fact_round() {
    let b = Bench::new("native-lookup-interleave");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let source = b.overlay.acquire_native_source(mount, 10, 1).unwrap();
    let view = b.workspace.view_for_source(source).unwrap();
    let mut plan = view
        .native_read_plan(
            mount,
            NativeReadOperation::Lookup {
                parent: 1,
                name: common::name("file"),
            },
        )
        .unwrap();
    assert!(plan
        .accept(Arc::new(plan.job().unwrap().perform(&b.overlay)))
        .unwrap()
        .is_none());
    b.applied(unlink(1, "file"), T2);
    plan.supply(&view).unwrap();
    let failed = plan
        .accept(Arc::new(plan.job().unwrap().perform(&b.overlay)))
        .unwrap_err();
    assert!(matches!(
        failed.original.decision,
        Some(NativeReadDecision::Refused(
            layerfs_workspace::Refusal::Missing
        ))
    ));
    assert_eq!(plan.stage(), NativeReadStage::Finished);
    assert_eq!(b.overlay.native_lookup_count(mount, 2).unwrap(), None);
    assert!(matches!(failed.original.result, Ok(None)));
    b.overlay.release_base_source(source).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn processing_source_survives_forget_and_last_name_removal_before_getattr() {
    let b = Bench::new("native-removed-stat");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let serial = lookup(&b, mount, 1, "file");
    let source = b.overlay.acquire_native_source(mount, 2, serial).unwrap();
    let view = b.workspace.view_for_source(source).unwrap();
    b.overlay.forget_native(mount, serial, 1).unwrap();
    b.applied(unlink(1, "file"), T2);
    b.applied(unlink(1, "alias"), T2);
    let mut plan = view
        .native_read_plan(mount, NativeReadOperation::Data { serial })
        .unwrap();
    let value = plan
        .accept(Arc::new(plan.job().unwrap().perform(&b.overlay)))
        .unwrap()
        .unwrap();
    assert_eq!(value.stat.serial, serial);
    assert_eq!(value.stat.namespace_refs, 0);
    assert!(b.overlay.revoke_native_mount(mount).is_err());
    b.overlay.close(b.route()).unwrap();
    assert!(b
        .overlay
        .source_inode(value.read.unwrap().source(), serial)
        .unwrap()
        .is_some());
    b.overlay.release_file_read(value.read.unwrap()).unwrap();
    b.overlay.release_base_source(source).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
    assert_eq!(
        b.overlay.native_mount_state(mount).unwrap(),
        NativeMountState::Revoked
    );
}

#[test]
fn native_open_retains_removed_metadata_after_last_lookup_and_handle_release() {
    let b = Bench::new("native-open-unlinked");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let serial = lookup(&b, mount, 1, "file");
    let source = b.overlay.acquire_native_source(mount, 2, serial).unwrap();
    let view = b.workspace.view_for_source(source).unwrap();
    let mut plan = view
        .native_read_plan(
            mount,
            NativeReadOperation::Open {
                serial,
                writable: true,
            },
        )
        .unwrap();
    let mut opened = None;
    for _ in 0..4 {
        if let Some(value) = plan
            .accept(Arc::new(plan.job().unwrap().perform(&b.overlay)))
            .unwrap()
        {
            opened = Some(value);
            break;
        }
        plan.supply(&view).unwrap();
    }
    let opened = opened.expect("bounded open fact rounds");
    let file = opened.file.unwrap();
    b.overlay.release_file_read(opened.read.unwrap()).unwrap();
    b.overlay.release_base_source(source).unwrap();
    b.overlay.forget_native(mount, serial, 1).unwrap();
    b.applied(unlink(1, "file"), T2);
    b.applied(unlink(1, "alias"), T2);
    assert!(b.overlay.acquire_native_source(mount, 3, serial).is_err());
    let source = b
        .overlay
        .acquire_native_file_source(mount, 4, serial, file.owner_id())
        .unwrap();
    b.overlay
        .close_native_file(mount, serial, file.owner_id())
        .unwrap();
    let view = b.workspace.view_for_source(source).unwrap();
    let mut plan = view
        .native_read_plan(mount, NativeReadOperation::Data { serial })
        .unwrap();
    let value = plan
        .accept(Arc::new(plan.job().unwrap().perform(&b.overlay)))
        .unwrap()
        .unwrap();
    assert_eq!(value.stat.namespace_refs, 0);
    assert_eq!(value.stat.serial, serial);
    assert_eq!(value.file, None);
    b.overlay.release_file_read(value.read.unwrap()).unwrap();
    b.overlay.release_base_source(source).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}
