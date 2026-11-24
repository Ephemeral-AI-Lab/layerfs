//! End-to-end place authority over the daemon's own control channel: a real
//! daemon mount reads the Sandbox and machine records under the Store's home,
//! records their generation and incarnation on its mount intent, and after
//! the Sandbox's retirement tombstone admission settles it so the Branch can
//! be removed without an operator.
#[allow(dead_code)]
#[path = "../../../../engine/crates/layerfs-daemon/tests/support/control_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../../../../engine/crates/layerfs-daemon/tests/support/host_install.rs"]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_history::records::GcScopeKind;
use layerfs_history::{BranchId, ForkRequest, ForkSource, HistoryName, LayerStackId, WorkspaceId};
use layerfs_machine::record::MachineRecord;
use layerfs_sandbox::internal::Control;
use layerfs_sandbox::record::{mount_place, retire_in, SandboxRecord};
use layerfs_store::place_view;
use layerfs_store_service::history::{admission, removal};

#[test]
fn a_daemon_mount_records_the_home_place_and_its_tombstone_settles_it() {
    let f = fixture::Fixture::new("mount-place-authority");
    // The home the Store serves from: its machine record names the current
    // ordered incarnation, its Sandbox record the generation its create
    // minted.
    let home = f.host.directory.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let machine = "docker";
    let incarnation = "p1:aa11.worker:10:aa";
    MachineRecord::built_in(machine, "home-id")
        .with_incarnation(incarnation)
        .write(&home)
        .unwrap();
    let sandbox = "place-sbx";
    let generation = "p1:aa11.sandbox:10:bb";
    let directory = layerfs_home::home::paths::sandbox_dir(&home, sandbox);
    std::fs::create_dir_all(&directory).unwrap();
    SandboxRecord::from_record(
        sandbox,
        SdkValue::map([
            ("name", SdkValue::text(sandbox)),
            ("machine", SdkValue::text(machine)),
            ("generation", SdkValue::text(generation)),
        ]),
    )
    .write_in(&directory)
    .unwrap();

    // `main` is not removable: mount and later remove a `work` Branch.
    let history = f.installed.opened.store.history();
    let stack = history
        .layer_stack(LayerStackId::from_authority([54; 16]))
        .expect("the project's stack")
        .expect("the project");
    let work = BranchId::from_authority([64; 16]);
    history
        .fork(&ForkRequest {
            stack: stack.id,
            branch: work,
            name: HistoryName::new("work").unwrap(),
            source: ForkSource::Layer(stack.head_layer),
        })
        .expect("a work Branch");

    // The place the control side sends, from the records it read.
    let place = mount_place(&home, &directory, sandbox).expect("the home's place");
    assert_eq!(place.sandbox.as_deref(), Some(sandbox));
    assert_eq!(place.machine.as_deref(), Some(machine));
    assert_eq!(place.generation.as_deref(), Some(generation));
    assert_eq!(place.machine_incarnation.as_deref(), Some(incarnation));

    // One real daemon mount over its authenticated control channel.
    let service = f.service.clone();
    let (channel, worker) = support::pair(move |connection| {
        for _ in 0..2 {
            drop(
                service
                    .serve_one(connection, |_, _, _| unreachable!())
                    .unwrap(),
            );
        }
    });
    let mut client = Control::new(channel);
    let workspace = WorkspaceId::from_authority([91; 32]).unwrap();
    let bound = layerfs_sandbox::internal::workspace(&mut client)
        .placed(place)
        .bind(workspace, work)
        .expect("the placed daemon mount");
    assert_eq!(bound.token.workspace, workspace);

    // The mount intent carries both place values; the daemon invents no lease
    // or fence it does not hold.
    let intents = history
        .gc_intents(GcScopeKind::Branch, work.as_slice())
        .expect("intents");
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].machine.as_deref(), Some(machine));
    assert_eq!(intents[0].sandbox.as_deref(), Some(sandbox));
    assert_eq!(intents[0].machine_incarnation.as_deref(), Some(incarnation));
    assert_eq!(intents[0].sandbox_generation.as_deref(), Some(generation));

    // Retire the Sandbox: the tombstone names the generation the mount
    // recorded, written before the record is deleted. Admission's bounded
    // view from the same home then proves the mount dead, so the Branch
    // removal settles it and proceeds without an operator.
    retire_in(&home, &directory, sandbox).expect("the tombstone");
    std::fs::remove_dir_all(&directory).unwrap();
    let view = place_view::from_home(&home).expect("the home's places");
    let removed = admission::with_place_view(Some(view), || {
        removal::remove(&f.installed.opened.store, &stack, "work")
    })
    .expect("the retired Sandbox settles its mount");
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    assert!(history
        .gc_intents(GcScopeKind::Branch, work.as_slice())
        .expect("intents")
        .is_empty());

    // The owner's own unmount after the settlement finds no row to release.
    layerfs_sandbox::internal::workspace(&mut client)
        .unmount(bound.token, false)
        .expect("the settled mount closes");
    worker.join();
    f.cleanup();
}
