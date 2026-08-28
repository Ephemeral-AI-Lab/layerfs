fn split_optional<S: ObjectStore>(
    store: &mut S,
    root: Option<Summary>,
    offset: u64,
    counters: &mut RopeCounters,
) -> CoreResult<(Option<Summary>, Option<Summary>)> {
    match root {
        Some(root) => split(store, root, offset, true, counters),
        None if offset == 0 => Ok((None, None)),
        None => Err(CoreError::InvalidRange {
            start: offset,
            end: offset,
            length: 0,
        }),
    }
}

fn split<S: ObjectStore>(
    store: &mut S,
    root: Summary,
    offset: u64,
    root_context: bool,
    counters: &mut RopeCounters,
) -> CoreResult<(Option<Summary>, Option<Summary>)> {
    if offset > root.bytes {
        return Err(CoreError::InvalidRange {
            start: offset,
            end: offset,
            length: root.bytes,
        });
    }
    if offset == 0 {
        return Ok((None, Some(root)));
    }
    if offset == root.bytes {
        return Ok((Some(root), None));
    }
    let node = load_node(store, root, root_context, counters)?;
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            let mut left = Vec::new();
            let mut right = Vec::new();
            let mut logical = 0_u64;
            for extent in extents {
                let end = add(logical, u64::from(extent.logical_length))?;
                if end <= offset {
                    left.push(extent);
                } else if logical >= offset {
                    right.push(extent);
                } else {
                    let left_len =
                        u32::try_from(offset - logical).map_err(|_| CoreError::LengthOverflow)?;
                    left.push(ExtentSliceV3::new(
                        extent.payload_object_id,
                        extent.source_offset,
                        left_len,
                    )?);
                    right.push(ExtentSliceV3::new(
                        extent.payload_object_id,
                        extent
                            .source_offset
                            .checked_add(left_len)
                            .ok_or(CoreError::LengthOverflow)?,
                        extent.logical_length - left_len,
                    )?);
                }
                logical = end;
            }
            Ok((
                Some(emit_leaf(store, left, counters)?),
                Some(emit_leaf(store, right, counters)?),
            ))
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            let index = children.partition_point(|child| child.cumulative_logical_end < offset);
            let before_bytes = if index == 0 {
                0
            } else {
                children[index - 1].cumulative_logical_end
            };
            let summaries = child_summaries(&children, level - 1);
            let child_summary = summaries[index];
            let (child_left, child_right) =
                split(store, child_summary, offset - before_bytes, false, counters)?;
            let prefix = root_from_children(store, summaries[..index].to_vec(), counters)?;
            let suffix = root_from_children(store, summaries[index + 1..].to_vec(), counters)?;
            Ok((
                concat_optional(store, prefix, child_left, counters)?,
                concat_optional(store, child_right, suffix, counters)?,
            ))
        }
    }
}

fn concat_optional<S: ObjectStore>(
    store: &mut S,
    left: Option<Summary>,
    right: Option<Summary>,
    counters: &mut RopeCounters,
) -> CoreResult<Option<Summary>> {
    match (left, right) {
        (None, value) | (value, None) => Ok(value),
        (Some(left), Some(right)) => concat(store, left, right, counters).map(Some),
    }
}

fn concat<S: ObjectStore>(
    store: &mut S,
    left: Summary,
    right: Summary,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    concat_inner(store, left, right, counters, 0)
}

fn concat_inner<S: ObjectStore>(
    store: &mut S,
    left: Summary,
    right: Summary,
    counters: &mut RopeCounters,
    depth: u8,
) -> CoreResult<Summary> {
    if depth > MAX_LEVEL {
        return Err(CoreError::MappingDepthExceeded);
    }
    if left.level == right.level {
        let left_node = load_node(store, left, true, counters)?;
        let right_node = load_node(store, right, true, counters)?;
        return match (left_node, right_node) {
            (ExtentNodeV3::Leaf { mut extents, .. }, ExtentNodeV3::Leaf { extents: right, .. }) => {
                extents.extend(right);
                coalesce(&mut extents)?;
                root_from_extents(store, extents, counters)
            }
            (
                ExtentNodeV3::Branch {
                    level, children, ..
                },
                ExtentNodeV3::Branch {
                    children: right, ..
                },
            ) => {
                let mut summaries = child_summaries(&children, level - 1);
                summaries.extend(child_summaries(&right, level - 1));
                root_from_children(store, summaries, counters)?
                    .ok_or(CoreError::InvalidRecord("empty branch concat"))
            }
            _ => Err(CoreError::WrongLogicalRole),
        };
    }
    if left.level > right.level {
        let ExtentNodeV3::Branch {
            level, children, ..
        } = load_node(store, left, true, counters)?
        else {
            return Err(CoreError::WrongLogicalRole);
        };
        let summaries = child_summaries(&children, level - 1);
        let (last, prefix) = summaries
            .split_last()
            .ok_or(CoreError::InvalidRecord("empty branch"))?;
        load_node(store, *last, false, counters)?;
        let prefix = root_from_children(store, prefix.to_vec(), counters)?;
        let boundary = concat_inner(store, *last, right, counters, depth + 1)?;
        return match prefix {
            None => Ok(boundary),
            Some(prefix) if prefix.level == boundary.level => {
                concat_inner(store, prefix, boundary, counters, depth + 1)
            }
            Some(prefix) if prefix.level.checked_add(1) == Some(boundary.level) => {
                let ExtentNodeV3::Branch {
                    level, children, ..
                } = load_node(store, boundary, true, counters)?
                else {
                    return Err(CoreError::WrongLogicalRole);
                };
                let mut children = child_summaries(&children, level - 1);
                children.insert(0, prefix);
                root_from_children(store, children, counters)?
                    .ok_or(CoreError::InvalidRecord("empty concat"))
            }
            Some(prefix) if boundary.level.checked_add(1) == Some(prefix.level) => {
                let ExtentNodeV3::Branch {
                    level, children, ..
                } = load_node(store, prefix, true, counters)?
                else {
                    return Err(CoreError::WrongLogicalRole);
                };
                let mut children = child_summaries(&children, level - 1);
                children.push(boundary);
                root_from_children(store, children, counters)?
                    .ok_or(CoreError::InvalidRecord("empty concat"))
            }
            Some(_) => Err(CoreError::InvalidRecord("left concat levels")),
        };
    }
    let ExtentNodeV3::Branch {
        level, children, ..
    } = load_node(store, right, true, counters)?
    else {
        return Err(CoreError::WrongLogicalRole);
    };
    let summaries = child_summaries(&children, level - 1);
    let (first, suffix) = summaries
        .split_first()
        .ok_or(CoreError::InvalidRecord("empty branch"))?;
    load_node(store, *first, false, counters)?;
    let boundary = concat_inner(store, left, *first, counters, depth + 1)?;
    let suffix = root_from_children(store, suffix.to_vec(), counters)?;
    match suffix {
        None => Ok(boundary),
        Some(suffix) if suffix.level == boundary.level => {
            concat_inner(store, boundary, suffix, counters, depth + 1)
        }
        Some(suffix) if suffix.level.checked_add(1) == Some(boundary.level) => {
            let ExtentNodeV3::Branch {
                level, children, ..
            } = load_node(store, boundary, true, counters)?
            else {
                return Err(CoreError::WrongLogicalRole);
            };
            let mut children = child_summaries(&children, level - 1);
            children.push(suffix);
            root_from_children(store, children, counters)?
                .ok_or(CoreError::InvalidRecord("empty concat"))
        }
        Some(suffix) if boundary.level.checked_add(1) == Some(suffix.level) => {
            let ExtentNodeV3::Branch {
                level, children, ..
            } = load_node(store, suffix, true, counters)?
            else {
                return Err(CoreError::WrongLogicalRole);
            };
            let mut children = child_summaries(&children, level - 1);
            children.insert(0, boundary);
            root_from_children(store, children, counters)?
                .ok_or(CoreError::InvalidRecord("empty concat"))
        }
        Some(_) => Err(CoreError::InvalidRecord("right concat levels")),
    }
}
