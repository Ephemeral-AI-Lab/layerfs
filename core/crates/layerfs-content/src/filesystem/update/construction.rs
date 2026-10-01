//! Complete claim validation/retirement before any DirectoryRoots construction.
use super::{run_canonical_body, unreachable_parents, FilesystemResult, ValidatedInput};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::objects::{FilesystemObjects, FilesystemPhases};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{ConstructionScopes, ConstructionState, DirectoryRoots};
use crate::filesystem::validate::{check_with_claims, ValidationWork};

/// Builds scalar input through one selected claim/root construction owner.
pub fn build_filesystem_binding_rows_with_construction_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn ConstructionState,
    scopes: &ConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_some() {
        let _ = state.claim_abandon(scopes.claims());
        return Err(ContentError::InvalidRecord("initial build base"));
    }
    run(objects, input, backing, state, scopes, phases)
}

/// Updates scalar input after full checked claim seal and known retirement.
pub fn update_filesystem_binding_rows_with_construction_state(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn ConstructionState,
    scopes: &ConstructionScopes,
    phases: &FilesystemPhases<'_>,
) -> ContentResult<FilesystemResult> {
    if input.base().is_none() {
        let _ = state.claim_abandon(scopes.claims());
        return Err(ContentError::InvalidRecord("update base root"));
    }
    run(objects, input, backing, state, scopes, phases)
}

fn run(
    objects: &mut FilesystemObjects<'_>,
    input: &dyn PreparedBindingRows,
    backing: Option<&mut dyn OrderingBacking>,
    state: &mut dyn ConstructionState,
    scopes: &ConstructionScopes,
    phases: &FilesystemPhases<'_>,
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
            check_with_claims(
                objects.reader(),
                input,
                &unreachable,
                &mut validation,
                state,
                scopes.claims(),
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
        let _ = state.claim_abandon(scopes.claims());
    }
    if outcome.is_err() && !cleanup_attempted {
        if let Some(backing) = backing {
            let _ = backing.release();
        }
    }
    outcome
}
