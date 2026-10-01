//! Actual supplied counts and paged descendant release after namespace validation.
use super::reduction::SuppliedReduction;
use super::{FilesystemResult, ValidatedInput};
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{
    CanonicalConstructionState, CanonicalScope, DirectoryRoots, EligibilityAuthority,
    GraphConstructionScopes, ParentCalls,
};
use crate::filesystem::validate::{check_canonical_selected, select_graph_input, ValidationWork};
use crate::{ContentError, ContentResult};
/// Build through actual supplied count/final-row and descendant-frontier authorities.
pub fn build_filesystem_binding_rows_with_canonical_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn CanonicalConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    select_graph_input(input, state, scopes)?;
    if input.base().is_some() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, input, backing, state, scopes, phases)
}
/// Update through the same selected immutable base and exact native count/release epochs.
pub fn update_filesystem_binding_rows_with_canonical_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn CanonicalConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    select_graph_input(input, state, scopes)?;
    if input.base().is_none() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, input, backing, state, scopes, phases)
}
fn run(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    mut backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn CanonicalConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    let mut cleanup_attempted = false;
    let mut selected = None;
    let mut parents_known = None;
    let outcome = (|| {
        input.resources().check()?;
        let mut validation = ValidationWork::default();
        let (checked, parents) = phases.phase("validate", || {
            check_canonical_selected(objects.reader(), input, &mut validation, state, scopes)
        })?;
        let scope = CanonicalScope::counts(
            scopes.roots().selection().clone(),
            parents.facts.scope.subject().clone(),
        )?;
        selected = Some(scope.clone());
        parents_known = Some(parents.clone());
        let validated = ValidatedInput {
            topology: checked.topology,
            validation,
            unreachable: EligibilityAuthority::Supplied(parents),
        };
        let mut reducer = SuppliedReduction::new(state, scope, backing.as_deref_mut())?;
        let memory = state.count_memory(selected.as_ref().unwrap())?;
        let mut roots = DirectoryRoots::new_with_memory(
            state,
            scopes.roots().clone(),
            input.directory_rows(),
            memory,
        )?;
        let result = super::canonical_body::run_selected_body(
            objects,
            input,
            phases,
            &mut cleanup_attempted,
            &mut roots,
            validated,
            state,
            ParentCalls::supplied(),
            &mut reducer,
        );
        drop(reducer);
        if result.is_err() {
            let _ = roots.release(state);
        }
        result
    })();
    if outcome.is_err() {
        if let Some(scope) = selected {
            let _ = state.count_abandon(&scope);
            if let Ok(jobs) = scope.jobs() {
                let _ = state.release_abandon(&jobs);
            }
        }
        if let Some(parents) = parents_known {
            let _ = state.parent_abandon(&parents.facts.scope);
        }
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        if !cleanup_attempted {
            if let Some(backing) = backing {
                let _ = backing.release();
            }
        }
    }
    outcome
}
