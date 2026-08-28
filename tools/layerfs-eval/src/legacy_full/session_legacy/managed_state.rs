use super::super::OperationCounters;
use layerfs_core::CanonicalPath;
use layerfs_storage::refs::RefState;
use layerfs_storage::Engine;

use super::layer_fs::topology_edge_key;
use super::{ManagedState, ManagedWorkspace, VfsError, VfsResult};
pub(super) fn refresh_error_state(possibly_visible: bool) -> ManagedState {
    if possibly_visible {
        ManagedState::IncompleteDerived
    } else {
        ManagedState::Live
    }
}

pub(super) fn managed_rename_edges(
    engine: &Engine,
    expected: &RefState,
    edits: &[super::super::managed_edit_legacy::ManagedEdit],
    from: &CanonicalPath,
    to: &CanonicalPath,
    counters: &mut OperationCounters,
) -> VfsResult<(Vec<u8>, Vec<u8>)> {
    let namespace = super::super::resolver_legacy::namespace(engine, expected.root)?;
    let original = translate_prior_renames(from, edits)?;
    let source_parent_path = translate_prior_renames(&parent_path(from)?, edits)?;
    let target_parent_path = translate_prior_renames(&parent_path(to)?, edits)?;
    let (child, _) =
        super::super::resolver_legacy::resolve(engine, namespace, &original, counters)?;
    let (source_parent, _) =
        super::super::resolver_legacy::resolve(engine, namespace, &source_parent_path, counters)?;
    let (target_parent, _) =
        super::super::resolver_legacy::resolve(engine, namespace, &target_parent_path, counters)?;
    Ok((
        topology_edge_key(child, source_parent, basename(from)?),
        topology_edge_key(child, target_parent, basename(to)?),
    ))
}

fn translate_prior_renames(
    path: &CanonicalPath,
    edits: &[super::super::managed_edit_legacy::ManagedEdit],
) -> VfsResult<CanonicalPath> {
    let mut bytes = path.as_bytes().to_vec();
    for edit in edits.iter().rev() {
        let super::super::managed_edit_legacy::ManagedEdit::Rename { from, to, .. } = edit else {
            continue;
        };
        let target = to.as_bytes();
        if bytes == target
            || bytes
                .strip_prefix(target)
                .is_some_and(|suffix| suffix.first() == Some(&b'/'))
        {
            let suffix = &bytes[target.len()..];
            let mut translated = Vec::with_capacity(from.as_bytes().len() + suffix.len());
            translated.extend_from_slice(from.as_bytes());
            translated.extend_from_slice(suffix);
            bytes = translated;
        }
    }
    Ok(CanonicalPath::from_bytes(&bytes)?)
}

fn parent_path(path: &CanonicalPath) -> VfsResult<CanonicalPath> {
    let bytes = path.as_bytes();
    Ok(CanonicalPath::from_bytes(
        bytes
            .iter()
            .rposition(|byte| *byte == b'/')
            .map_or(&[][..], |separator| &bytes[..separator]),
    )?)
}

fn basename(path: &CanonicalPath) -> VfsResult<&[u8]> {
    let bytes = path.as_bytes();
    let name = bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(bytes, |separator| &bytes[separator + 1..]);
    if name.is_empty() {
        return Err(VfsError::InvalidState);
    }
    Ok(name)
}

impl Drop for ManagedWorkspace {
    fn drop(&mut self) {
        if let Some(external) = self.external.as_mut() {
            let _ = external.discard();
        }
        let _ = self.remove_spool();
    }
}
