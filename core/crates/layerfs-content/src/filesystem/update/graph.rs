//! Complete site/graph validation/retirement before any DirectoryRoots construction.
use super::{run_canonical_body, unreachable_parents, FilesystemResult, ValidatedInput};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{
    AliasGraphConstructionState, DirectoryRoots, GraphConstructionScopes, GraphConstructionState,
};
use crate::filesystem::validate::{check_graph_selected, select_graph_input, ValidationWork};

/// Builds scalar input through one selected site/graph/root construction owner.
pub fn build_filesystem_binding_rows_with_graph_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn GraphConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    select_graph_input(input, state, scopes)?;
    if input.base().is_some() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(
        objects,
        input,
        backing,
        state,
        scopes,
        phases,
        check_graph_selected::<dyn GraphConstructionState>,
    )
}

/// Updates scalar input after full checked site and graph proofs and known retirement.
pub fn update_filesystem_binding_rows_with_graph_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn GraphConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    select_graph_input(input, state, scopes)?;
    if input.base().is_none() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(
        objects,
        input,
        backing,
        state,
        scopes,
        phases,
        check_graph_selected::<dyn GraphConstructionState>,
    )
}

/// Builds through explicitly supplied profile5 alias discovery and graph state.
pub fn build_filesystem_binding_rows_with_alias_graph_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn AliasGraphConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    select_graph_input(input, state, scopes)?;
    if input.base().is_some() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(
        objects,
        input,
        backing,
        state,
        scopes,
        phases,
        crate::filesystem::validate::check_alias_graph_selected::<dyn AliasGraphConstructionState>,
    )
}
/// Updates through explicitly supplied profile5 alias discovery and graph state.
pub fn update_filesystem_binding_rows_with_alias_graph_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn AliasGraphConstructionState,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    select_graph_input(input, state, scopes)?;
    if input.base().is_none() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(
        objects,
        input,
        backing,
        state,
        scopes,
        phases,
        crate::filesystem::validate::check_alias_graph_selected::<dyn AliasGraphConstructionState>,
    )
}

type GraphCheck<S> =
    for<'a> fn(
        &dyn crate::object::AuthenticatedObjects,
        &'a dyn PreparedBindingRows,
        &std::collections::BTreeMap<u64, ()>,
        &mut ValidationWork,
        &mut S,
        &GraphConstructionScopes,
    ) -> ContentResult<crate::filesystem::validate::CheckedTopologyInput<'a>>;

fn run<S: GraphConstructionState + ?Sized>(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut S,
    scopes: &GraphConstructionScopes,
    phases: &FilesystemPhases<'_>,
    check: GraphCheck<S>,
) -> ContentResult<FilesystemResult> {
    let mut backing = backing;
    let mut cleanup_attempted = false;
    let mut checker_entered = false;
    let mut checker_succeeded = false;
    let outcome = (|| {
        input.resources().check()?;
        let unreachable = unreachable_parents(input)?;
        let mut validation = ValidationWork::default();
        let checked = phases.phase("validate", || {
            checker_entered = true;
            check(
                objects.reader(),
                input,
                &unreachable,
                &mut validation,
                state,
                scopes,
            )
        })?;
        checker_succeeded = true;
        let validated = ValidatedInput {
            topology: checked.topology,
            validation,
            unreachable: super::EligibilityAuthority::Legacy(unreachable),
        };
        // The provider acknowledges complete retirement before this constructor
        // acquires its first root window or checks root-phase capacity.
        let mut contents = DirectoryRoots::new(
            state.indexed(),
            scopes.roots().clone(),
            input.directory_rows(),
        )?;
        let borrowed: Option<&mut dyn OrderingBacking> = backing.as_deref_mut();
        let result = run_canonical_body(
            objects,
            input,
            borrowed,
            phases,
            &mut cleanup_attempted,
            &mut contents,
            validated,
            state.indexed(),
            super::ParentCalls::compatibility(),
        );
        if result.is_err() {
            let _ = contents.release(state.indexed());
        }
        result
    })();
    // The checker terminalizes its own errors. Both earlier refusals and later
    // failures after retirement close this exact attempt, without native I/O.
    if outcome.is_err() && (!checker_entered || checker_succeeded) {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
    }
    if outcome.is_err() && !cleanup_attempted {
        if let Some(backing) = backing {
            let _ = backing.release();
        }
    }
    outcome
}
