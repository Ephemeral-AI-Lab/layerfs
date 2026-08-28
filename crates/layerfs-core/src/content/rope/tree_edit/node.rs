fn root_from_extents<S: ObjectStore>(
    store: &mut S,
    extents: Vec<ExtentSliceV3>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    if extents.len() <= MAX_ENTRIES {
        return emit_leaf(store, extents, counters);
    }
    let split = extents.len() / 2;
    let left = emit_leaf(store, extents[..split].to_vec(), counters)?;
    let right = emit_leaf(store, extents[split..].to_vec(), counters)?;
    root_from_children(store, vec![left, right], counters)?
        .ok_or(CoreError::InvalidRecord("empty extent root"))
}

fn root_from_children<S: ObjectStore>(
    store: &mut S,
    children: Vec<Summary>,
    counters: &mut RopeCounters,
) -> CoreResult<Option<Summary>> {
    if children.is_empty() {
        return Ok(None);
    }
    if children.len() == 1 {
        return Ok(Some(children[0]));
    }
    if children.len() <= MAX_ENTRIES {
        return emit_branch(store, children, counters).map(Some);
    }
    let split = children.len() / 2;
    let left = emit_branch(store, children[..split].to_vec(), counters)?;
    let right = emit_branch(store, children[split..].to_vec(), counters)?;
    emit_branch(store, vec![left, right], counters).map(Some)
}

fn emit_leaf<S: ObjectStore>(
    store: &mut S,
    extents: Vec<ExtentSliceV3>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let bytes = extents.iter().try_fold(0_u64, |sum, extent| {
        add(sum, u64::from(extent.logical_length))
    })?;
    let node = ExtentNodeV3::Leaf {
        subtree_logical_bytes: bytes,
        extents,
    };
    emit_node(store, node, counters)
}

fn emit_branch<S: ObjectStore>(
    store: &mut S,
    children: Vec<Summary>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let level = children[0]
        .level
        .checked_add(1)
        .ok_or(CoreError::MappingDepthExceeded)?;
    if children.iter().any(|child| child.level + 1 != level) {
        return Err(CoreError::InvalidRecord("mixed branch levels"));
    }
    let mut bytes = 0;
    let mut extents = 0;
    let descriptors = children
        .into_iter()
        .map(|child| {
            bytes = add(bytes, child.bytes)?;
            extents = add(extents, child.extents)?;
            Ok(ChildDescriptorV3 {
                cumulative_logical_end: bytes,
                cumulative_extent_end: extents,
                child_object_id: child.id,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    emit_node(
        store,
        ExtentNodeV3::Branch {
            level,
            subtree_logical_bytes: bytes,
            subtree_extent_count: extents,
            children: descriptors,
        },
        counters,
    )
}

fn emit_node<S: ObjectStore>(
    store: &mut S,
    node: ExtentNodeV3,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let canonical = encode_node(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = add(counters.nodes_created, 1)?;
    Ok(Summary {
        id,
        bytes: node.logical_len(),
        extents: node.extent_count(),
        level: node.level(),
    })
}

fn load_node<S: ObjectRead>(
    store: &S,
    summary: Summary,
    root: bool,
    counters: &mut RopeCounters,
) -> CoreResult<ExtentNodeV3> {
    counters.nodes_read = add(counters.nodes_read, 1)?;
    let node = store.with_authenticated_canonical(summary.id, |canonical| {
        decode_node_with_context(canonical, root)
    })?;
    if node.level() != summary.level
        || node.logical_len() != summary.bytes
        || node.extent_count() != summary.extents
    {
        return Err(CoreError::InvalidRecord("extent summary"));
    }
    node.validate(root)?;
    Ok(node)
}

fn load_node_cached<S: ObjectRead>(
    store: &S,
    summary: Summary,
    root: bool,
    counters: &mut RopeCounters,
    cache: &mut Option<(ObjectId, ExtentNodeV3)>,
) -> CoreResult<ExtentNodeV3> {
    let node = match cache {
        Some((id, node)) if *id == summary.id => node.clone(),
        _ => {
            let node = load_node(store, summary, root, counters)?;
            *cache = Some((summary.id, node.clone()));
            node
        }
    };
    if node.level() != summary.level
        || node.logical_len() != summary.bytes
        || node.extent_count() != summary.extents
    {
        return Err(CoreError::InvalidRecord("extent summary"));
    }
    Ok(node)
}
