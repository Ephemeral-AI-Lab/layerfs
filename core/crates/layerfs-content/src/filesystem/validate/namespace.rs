//! Supplied selected base facts and parent eligibility in the actual graph lane.
use super::{
    binding::{Bindings, Headers},
    binding_semantics, charge_site, check_binding_input, check_root_checks, claim_shape,
    fact_access::FactAccess,
    fact_verify, graph, graph_build, site_aliases, CheckedTopologyInput, ValidationWork,
};
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{
    BindingSites, EligibilityView, FactScope, FactSubject, GraphConstructionScopes,
    NamespaceConstructionState, ParentCalls, ParentSeal,
};
use crate::object::AuthenticatedObjects;
use crate::{ContentError, ContentResult};

pub(crate) fn check_namespace_selected<'a, S: NamespaceConstructionState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    work: &mut ValidationWork,
    state: &mut S,
    scopes: &GraphConstructionScopes,
) -> ContentResult<(CheckedTopologyInput<'a>, ParentSeal)> {
    check_selected_by(reader, input, work, state, scopes, true)
}
/// Profile8 retains the same immutable base authority through canonical consumers.
pub(crate) fn check_canonical_selected<'a, S: NamespaceConstructionState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    work: &mut ValidationWork,
    state: &mut S,
    scopes: &GraphConstructionScopes,
) -> ContentResult<(CheckedTopologyInput<'a>, ParentSeal)> {
    check_selected_by(reader, input, work, state, scopes, false)
}
#[allow(clippy::too_many_arguments)]
fn check_selected_by<'a, S: NamespaceConstructionState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    work: &mut ValidationWork,
    state: &mut S,
    scopes: &GraphConstructionScopes,
    retire_base: bool,
) -> ContentResult<(CheckedTopologyInput<'a>, ParentSeal)> {
    let mut bound = None;
    let outcome = (|| {
        check_binding_input(input)?;
        let topology = binding_semantics::initial(reader, input, work)?;
        let subject = FactSubject::new(scopes.graph().subject().clone(), topology.table)?;
        let selection = scopes.roots().selection().clone();
        let base = FactScope::new(selection.clone(), subject.clone())?;
        let parent = FactScope::parents(selection, subject)?;
        state.fact_bind(&base)?;
        bound = Some((base.clone(), parent.clone()));
        state.parent_bind(&parent)?;
        let parents = prepare_parents(input, state, &parent, &base)?;
        let mut facts = FactAccess::supplied().initialize(state, &base)?;
        let mut sites = BindingSites::new(state, scopes.sites().clone(), claim_shape(input)?)?;
        let (_, updated) = binding_semantics::checked(
            reader,
            input,
            topology,
            work,
            facts,
            &mut sites,
            FactAccess::supplied_sites(),
            |sites, child, header, ordinal, has_base, _| {
                let active = !EligibilityView::Supplied(&parents).contains(
                    sites.state(),
                    header.parent(),
                    ParentCalls::supplied(),
                )?;
                sites.claim(child, header, ordinal, has_base, active)
            },
        )?;
        facts = updated;
        let mut sites = sites.close_membership()?;
        let members = sites.members().clone();
        let before = work.inode_pages_read;
        let result = site_aliases::check_namespace(
            site_aliases::SiteAliasInput {
                reader,
                input,
                topology: &topology,
                members: &members,
                active_stored: sites.active_stored(),
                unreachable: EligibilityView::Supplied(&parents),
            },
            work,
            &mut facts,
            sites.state(),
        );
        charge_site(work, before, |sites| &mut sites.aliases);
        result?;
        sites.finish_with(
            input,
            EligibilityView::Supplied(&parents),
            &members,
            ParentCalls::supplied(),
        )?;
        check_root_checks(input)?;
        graph::effective_with(
            graph_build::GraphInput {
                reader,
                input,
                topology,
                unreachable: EligibilityView::Supplied(&parents),
            },
            work,
            &mut facts,
            state,
            scopes,
            FactAccess::supplied(),
            ParentCalls::supplied(),
        )?;
        drop(facts);
        if retire_base {
            retire_canonical_facts(state, &base)?;
        }
        Ok((CheckedTopologyInput { input, topology }, parents))
    })();
    if outcome.is_err() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
        if let Some((base, parent)) = bound {
            let _ = state.fact_abandon(&base);
            let _ = state.parent_abandon(&parent);
        }
    }
    outcome
}
fn prepare_parents<S: NamespaceConstructionState + ?Sized>(
    input: &dyn PreparedBindingRows,
    state: &mut S,
    scope: &FactScope,
    base: &FactScope,
) -> ContentResult<ParentSeal> {
    // A fixed scalar window is admitted before initialization and retained only
    // through the two advancing complete source passes.
    let _lease = state
        .fact_memory(base)?
        .reserve(super::facts::validation_parent_working_bytes())?;
    let mut pending = [0; 128];
    let mut used = 0;
    let mut headers = Headers::new(input)?;
    while let Some(header) = headers.next()? {
        if header.parent() != input.root_serial() && input.is_new(header.parent())? {
            pending[used] = header.parent();
            used += 1;
            if used == 128 {
                state.parent_insert(scope, &pending)?;
                used = 0;
            }
        }
    }
    state.parent_insert(scope, &pending[..used])?;
    state.parent_close_declarations(scope)?;
    used = 0;
    let mut headers = Headers::new(input)?;
    while let Some(header) = headers.next()? {
        let mut bindings = Bindings::new(input, header)?;
        while let Some((_, child)) = bindings.next()? {
            if let Some(child) = child {
                pending[used] = child;
                used += 1;
                if used == 128 {
                    state.parent_mark_bound(scope, &pending)?;
                    used = 0;
                }
            }
        }
    }
    state.parent_mark_bound(scope, &pending[..used])?;
    let seal = state.parent_seal(scope)?;
    if seal.facts.scope != *scope {
        return Err(ContentError::InvalidOrderingRecord("parent selected seal"));
    }
    fact_verify::parents(state, &seal)?;
    Ok(seal)
}

/// Exact BaseFacts seal, independent transcript/EOF and known retirement before FSroot.
pub(crate) fn retire_canonical_facts<S: crate::filesystem::state::FactState + ?Sized>(
    state: &mut S,
    scope: &FactScope,
) -> ContentResult<()> {
    let _seal = state
        .fact_memory(scope)?
        .reserve(std::mem::size_of::<crate::filesystem::state::FactSeal>())?;
    let seal = state.fact_seal(scope)?;
    fact_verify::base(state, &seal)?;
    state.fact_retire(&seal)
}
