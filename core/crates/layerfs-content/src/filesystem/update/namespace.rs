//! Profile7 namespace authorities through validation and final canonical consumers.
use super::{run_canonical_body, FilesystemResult, ValidatedInput};
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{
    DirectoryRoots, EligibilityAuthority, GraphConstructionScopes, NamespaceConstructionState,
    ParentCalls,
};
use crate::filesystem::validate::{check_namespace_selected, select_graph_input, ValidationWork};
use crate::{ContentError, ContentResult};
/// Builds scalar rows with supplied base facts, parent eligibility, aliases and graph.
pub fn build_filesystem_binding_rows_with_namespace_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn NamespaceConstructionState,
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
/// Updates scalar rows with the same selected immutable table authority throughout.
pub fn update_filesystem_binding_rows_with_namespace_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn NamespaceConstructionState,
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
    state: &mut dyn NamespaceConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    let mut cleanup_attempted = false;
    let mut checked_parent = None;
    let outcome = (|| {
        input.resources().check()?;
        let mut validation = ValidationWork::default();
        let (checked, parents) = phases.phase("validate", || {
            check_namespace_selected(objects.reader(), input, &mut validation, state, scopes)
        })?;
        checked_parent = Some(parents.clone());
        let validated = ValidatedInput {
            topology: checked.topology,
            validation,
            unreachable: EligibilityAuthority::Supplied(parents),
        };
        let mut roots = DirectoryRoots::new(state, scopes.roots().clone(), input.directory_rows())?;
        let result = run_canonical_body(
            objects,
            input,
            backing.as_deref_mut(),
            phases,
            &mut cleanup_attempted,
            &mut roots,
            validated,
            state,
            ParentCalls::supplied(),
        );
        if result.is_err() {
            let _ = roots.release(state);
        }
        result
    })();
    if outcome.is_err() {
        if let Some(parents) = checked_parent {
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
