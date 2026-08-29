use layerfs_sdk::{
    BranchParent, BranchSource, Client, LayerInitialization, Query, QueryResult, SdkError,
    StoreLocation,
};

#[test]
fn one_layer_owns_many_fixed_parent_routes_without_reparenting() {
    let root = run_dir("many-routes");
    let mut client = Client::new();
    let layer = client
        .create_layer(StoreLocation::local(root.join("layer.sqlite")))
        .unwrap();
    let (_, genesis) = layer.store.initialize(LayerInitialization::Empty).unwrap();

    let direct_a = client
        .create_branch(StoreLocation::local(root.join("direct-a.sqlite")))
        .unwrap();
    let direct_b = client
        .create_branch(StoreLocation::local(root.join("direct-b.sqlite")))
        .unwrap();
    let stack_a = client
        .create_stack(StoreLocation::local(root.join("stack-a.sqlite")))
        .unwrap();
    let stack_store = client
        .context()
        .unwrap()
        .stacks
        .iter()
        .find(|stack| stack.id == stack_a)
        .unwrap()
        .store
        .clone();
    stack_store.pull_layer(genesis.id).unwrap();
    let (_, seed) = stack_store.create_stack(genesis.id).unwrap();
    let stacked_a = client
        .create_branch(StoreLocation::local(root.join("stacked-a.sqlite")))
        .unwrap();
    let stacked_b = client
        .create_branch(StoreLocation::local(root.join("stacked-b.sqlite")))
        .unwrap();
    let stack_b = client
        .create_stack(StoreLocation::local(root.join("stack-b.sqlite")))
        .unwrap();
    let stacked_c = client
        .create_branch(StoreLocation::local(root.join("stacked-c.sqlite")))
        .unwrap();

    let QueryResult::Topology(topology) = client.query(Query::Topology).unwrap() else {
        panic!("topology result")
    };
    assert_eq!(topology.stacks.len(), 2);
    assert_eq!(topology.branches.len(), 5);
    assert_eq!(parent(&topology, direct_a), BranchParent::Layer(layer.id));
    assert_eq!(parent(&topology, direct_b), BranchParent::Layer(layer.id));
    assert_eq!(parent(&topology, stacked_a), BranchParent::Stack(stack_a));
    assert_eq!(parent(&topology, stacked_b), BranchParent::Stack(stack_a));
    assert_eq!(parent(&topology, stacked_c), BranchParent::Stack(stack_b));

    let direct = topology
        .branches
        .iter()
        .find(|branch| branch.id == direct_a)
        .unwrap();
    direct
        .store
        .create_branch(BranchSource::Layer(genesis.id))
        .unwrap();
    let stacked = topology
        .branches
        .iter()
        .find(|branch| branch.id == stacked_a)
        .unwrap();
    stacked
        .store
        .create_branch(BranchSource::Stack(seed.id))
        .unwrap();

    assert!(matches!(
        client.disconnect_stack(stack_a),
        Err(SdkError::ActiveDependents)
    ));
    client.disconnect_branch(stacked_a).unwrap();
    client.disconnect_branch(stacked_b).unwrap();
    client.disconnect_stack(stack_a).unwrap();
    assert_eq!(
        parent(client.context().unwrap(), direct_a),
        BranchParent::Layer(layer.id)
    );

    drop(client);
    std::fs::remove_dir_all(root).unwrap();
}

fn parent(
    context: &layerfs_sdk::ConnectionContext,
    id: layerfs_sdk::BranchConnectionId,
) -> BranchParent {
    context
        .branches
        .iter()
        .find(|branch| branch.id == id)
        .unwrap()
        .parent
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-sdk-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
