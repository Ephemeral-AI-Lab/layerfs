//! Explicit legacy/single-phase entry points sharing the canonical body.
use super::{run_binding_state, FilesystemResult};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::rows::{CompatibilityBindingRows, PreparedBindingRows, PreparedRows};
use crate::filesystem::state::{
    IndexedState, ResidentState, StateScope, StateSelection, StateTable,
};

/// Builds a new filesystem from strictly sorted final bindings.
pub fn build_filesystem(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, input, backing, &FilesystemPhases::disabled())
}

/// Builds a new filesystem while recording the caller's coarse phase scopes.
pub fn build_filesystem_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, input, backing, phases)
}

/// Applies one complete update to a checked immutable base root.
pub fn update_filesystem(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, input, backing, &FilesystemPhases::disabled())
}

/// Applies one complete update while recording the caller's coarse phase scopes.
pub fn update_filesystem_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, input, backing, phases)
}

/// Builds with the caller's admitted exact DirectoryRoots authority.
pub fn build_filesystem_with_state(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
) -> ContentResult<FilesystemResult> {
    build_filesystem_with_state_timed(
        objects,
        input,
        backing,
        state,
        scope,
        &FilesystemPhases::disabled(),
    )
}

/// Builds with supplied exact state while recording the caller's coarse phases.
pub fn build_filesystem_with_state_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run_with_state(objects, input, backing, state, scope, phases)
}

/// Updates through the same canonical algorithm with supplied exact metadata state.
pub fn update_filesystem_with_state(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
) -> ContentResult<FilesystemResult> {
    update_filesystem_with_state_timed(
        objects,
        input,
        backing,
        state,
        scope,
        &FilesystemPhases::disabled(),
    )
}

/// Updates with supplied exact state while recording the caller's coarse phases.
pub fn update_filesystem_with_state_timed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run_with_state(objects, input, backing, state, scope, phases)
}

fn run(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    let selected = CompatibilityBindingRows::new(input)?;
    run_bindings(objects, &selected, backing, phases)
}

fn run_bindings(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    let resources = input.resources();
    resources.check()?;
    let declared = input
        .directory_rows()
        .max(input.inode_rows())
        .max(input.new_rows());
    let limit = resources.maximum_touched_serials();
    if declared > limit {
        return Err(ContentError::ObjectLimitExceeded {
            limit,
            actual: declared,
        });
    }
    // This independently admitted compatibility owner uses the caller's exact
    // known shape. It introduces no 64 KiB clamp or allocation-error spill path.
    let mut selector = blake3::Hasher::new();
    selector.update(b"layerfs/indexed-state/filesystem-operation/v1\0");
    selector.update(input.scope().object().as_bytes());
    match input.base() {
        Some(base) => {
            selector.update(&[1]);
            selector.update(base.0.as_bytes());
        }
        None => {
            selector.update(&[0]);
            selector.update(&[0; 32]);
        }
    }
    selector.update(&input.root_serial().to_be_bytes());
    selector.update(
        &u64::try_from(input.directory_rows())
            .map_err(|_| ContentError::LengthOverflow)?
            .to_be_bytes(),
    );
    let mut selection = StateSelection::issue(*selector.finalize().as_bytes())?;
    let mut binding = blake3::Hasher::new();
    binding.update(b"layerfs/indexed-state/resident/v1\0");
    binding.update(selection.selector());
    binding.update(&selection.token().to_be_bytes());
    selection.bind_owner(*binding.finalize().as_bytes())?;
    let scope = StateScope::new(selection, 1, StateTable::DirectoryRoots)?;
    let mut state = ResidentState::new(scope.clone(), input.directory_rows())?;
    run_binding_state(objects, input, backing, &mut state, &scope, phases)
}

fn run_with_state(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    let selected = CompatibilityBindingRows::new(input)?;
    run_binding_state(objects, &selected, backing, state, scope, phases)
}

/// Applies scalar binding rows through the same canonical algorithm and exact state.
/// The explicitly selected source never falls back to complete directory rows.
pub fn update_filesystem_binding_rows_with_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run_binding_state(objects, input, backing, state, scope, phases)
}

/// Builds scalar binding rows with supplied exact state and coarse phase scopes.
pub fn build_filesystem_binding_rows_with_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn IndexedState,
    scope: &StateScope,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run_binding_state(objects, input, backing, state, scope, phases)
}
