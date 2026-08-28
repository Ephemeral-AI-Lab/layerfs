use super::*;

pub(super) fn encode_link_state(remaining: u64, path: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(8 + path.len());
    value.extend_from_slice(&remaining.to_be_bytes());
    value.extend_from_slice(path);
    value
}

pub(super) fn decode_link_state(value: &[u8]) -> VfsResult<(u64, &[u8])> {
    let remaining = u64::from_be_bytes(
        value
            .get(..8)
            .ok_or(VfsError::InvalidState)?
            .try_into()
            .unwrap(),
    );
    let path = value.get(8..).ok_or(VfsError::InvalidState)?;
    if remaining == 0 || path.is_empty() {
        return Err(VfsError::InvalidState);
    }
    Ok((remaining, path))
}

pub(super) fn child_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = parent.to_vec();
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

pub(super) fn create_hard_link_from_path(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    path: &[u8],
    target_parent: &dyn DirectoryHandle,
    target: &[u8],
) -> VfsResult<()> {
    let mut components = path.split(|byte| *byte == b'/');
    let first = components.next().ok_or(VfsError::InvalidState)?;
    create_hard_link_from_components(
        workspace,
        current,
        first,
        components.collect::<Vec<_>>().as_slice(),
        target_parent,
        target,
    )
}

pub(super) fn create_hard_link_from_components(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    component: &[u8],
    remaining: &[&[u8]],
    target_parent: &dyn DirectoryHandle,
    target: &[u8],
) -> VfsResult<()> {
    if remaining.is_empty() {
        let expected = workspace.identity_at(current, component)?;
        return Ok(workspace.create_hard_link_at(
            current,
            component,
            &expected,
            target_parent,
            target,
        )?);
    }
    let expected = workspace.token_at(current, component)?;
    let child = workspace.open_directory_at(current, component, Some(&expected))?;
    create_hard_link_from_components(
        workspace,
        child.as_ref(),
        remaining[0],
        &remaining[1..],
        target_parent,
        target,
    )
}

pub(super) fn finish_hard_link_from_path(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    path: &[u8],
    metadata: &NativeMetadata,
) -> VfsResult<()> {
    let mut components = path.split(|byte| *byte == b'/');
    let first = components.next().ok_or(VfsError::InvalidState)?;
    finish_hard_link_from_components(
        workspace,
        current,
        first,
        components.collect::<Vec<_>>().as_slice(),
        metadata,
    )
}

pub(super) fn finish_hard_link_from_components(
    workspace: &dyn ProjectionWorkspace,
    current: &dyn DirectoryHandle,
    component: &[u8],
    remaining: &[&[u8]],
    metadata: &NativeMetadata,
) -> VfsResult<()> {
    if remaining.is_empty() {
        let expected = workspace.identity_at(current, component)?;
        return Ok(workspace.finish_hard_link_at(current, component, &expected, metadata)?);
    }
    let expected = workspace.token_at(current, component)?;
    let child = workspace.open_directory_at(current, component, Some(&expected))?;
    finish_hard_link_from_components(
        workspace,
        child.as_ref(),
        remaining[0],
        &remaining[1..],
        metadata,
    )
}
