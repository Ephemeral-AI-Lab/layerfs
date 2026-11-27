//! #G4: a daemon mount registers the Sandbox and machine beside its
//! Workspace, and a mount that knows neither still registers the Workspace
//! alone. `MountsActive` reports exactly what the intent placed.
#[allow(dead_code)]
#[path = "../support/install/control_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../support/install/host.rs"]
mod support;

use layerfs_history::records::GcScopeKind;
use layerfs_history::{BranchId, ForkRequest, ForkSource, HistoryName, LayerStackId, WorkspaceId};
use layerfs_protocol::daemon::{MountPlace, Reply, Request, WorkspaceToken};
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::history::{lookup, removal};

/// The fixture's Store with a `work` Branch forked from the project's head.
fn work(f: &fixture::Fixture) -> (LayerStackId, BranchId) {
    let store = &f.installed.opened.store;
    let stack = LayerStackId::from_authority([54; 16]);
    let base = store
        .history()
        .layer_stack(stack)
        .unwrap()
        .unwrap()
        .head_layer;
    let branch = BranchId::from_authority([64; 16]);
    store
        .history()
        .fork(&ForkRequest {
            stack,
            branch,
            name: HistoryName::new("work").unwrap(),
            source: ForkSource::Layer(base),
        })
        .unwrap();
    (stack, branch)
}

/// Executes one mount request and returns the bound token.
fn bound(f: &fixture::Fixture, request: Request) -> WorkspaceToken {
    match f.service.execute_control(&request).unwrap().reply {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    }
}

/// A `workspace.mount`-shaped named bind that knows its Sandbox and machine.
fn bind_named_placed(f: &fixture::Fixture, tag: u8, place: MountPlace) -> WorkspaceToken {
    bound(
        f,
        Request::MountNamedPlaced {
            workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
            project: "project".to_owned(),
            branch: "work".to_owned(),
            place,
        },
    )
}

/// A named bind whose caller knows neither the Sandbox nor the machine.
fn bind_named(f: &fixture::Fixture, tag: u8) -> WorkspaceToken {
    bound(
        f,
        Request::MountNamed {
            workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
            project: "project".to_owned(),
            branch: "work".to_owned(),
        },
    )
}

fn unmount(f: &fixture::Fixture, token: WorkspaceToken) {
    let reply = f
        .service
        .execute_control(&Request::Unmount(
            layerfs_protocol::daemon::UnmountControl {
                token,
                discard: true,
            },
        ))
        .unwrap()
        .reply;
    assert_eq!(reply, Reply::Unmounted(token));
}

/// The one holder a live mount leaves in `MountsActive`.
fn holder(f: &fixture::Fixture, stack: &layerfs_history::LayerStackRecord) -> SdkValue {
    let refused = removal::remove(&f.installed.opened.store, stack, "work")
        .expect_err("the live mount refuses the removal");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");
    let SdkValue::List(holders) = refused.detail.get("holders").expect("holders") else {
        panic!("{refused:?}");
    };
    assert_eq!(holders.len(), 1, "{holders:?}");
    holders[0].clone()
}

/// The fixture project's live stack record, as the removal names it.
fn stack(f: &fixture::Fixture) -> layerfs_history::LayerStackRecord {
    lookup::project(&f.installed.opened.store, "project").expect("project")
}

#[test]
fn a_placed_daemon_mount_registers_sandbox_machine_and_workspace() {
    let f = fixture::Fixture::new("g4-mount-place");
    let (_, branch) = work(&f);
    let incarnation = "p1:worker:10:aa";
    let generation = "p1:sandbox:10:aa";
    let token = bind_named_placed(
        &f,
        7,
        MountPlace::new(Some("g4sbx"), Some("docker"))
            .with_generation(Some(generation))
            .with_machine_incarnation(Some(incarnation)),
    );
    let holder = holder(&f, &stack(&f));
    assert_eq!(holder.get_text("kind"), Some("mount"));
    assert_eq!(holder.get_text("sandbox"), Some("g4sbx"));
    assert_eq!(holder.get_text("machine"), Some("docker"));
    let workspace = layerfs_protocol::sdk::hex(&[7; 32]);
    assert_eq!(holder.get_text("workspace"), Some(workspace.as_str()));
    // the intent records the generation the request place carried
    let intents = f
        .installed
        .opened
        .store
        .history()
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .expect("intents");
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].machine_incarnation.as_deref(), Some(incarnation));
    assert_eq!(intents[0].sandbox_generation.as_deref(), Some(generation));
    unmount(&f, token);
    f.cleanup();
}

#[test]
fn a_mount_with_no_known_context_registers_the_workspace_alone() {
    let f = fixture::Fixture::new("g4-mount-unknown");
    work(&f);
    let token = bind_named(&f, 8);
    let holder = holder(&f, &stack(&f));
    assert_eq!(holder.get_text("kind"), Some("mount"));
    assert_eq!(holder.get_text("sandbox"), None);
    assert_eq!(holder.get_text("machine"), None);
    assert!(holder.get_text("workspace").is_some());
    unmount(&f, token);
    f.cleanup();
}
