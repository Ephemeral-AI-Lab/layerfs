//! One post-Site-retirement effective graph proof before any root construction.
use super::facts::ValidationState;
use super::{
    charge_site, check_binding_input, check_binding_semantics, check_root_checks, claim_shape,
    site_aliases, BindingSites, CheckedTopologyInput, FilesystemTopology, ValidationWork,
};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{GraphConstructionScopes, GraphConstructionState, GraphMode};
use crate::object::AuthenticatedObjects;
use std::collections::BTreeMap;
/// Actual construction/solver work; records and attempts are separate from physical memory.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValidationGraphWork {
    /// Raw selected directory-child seed occurrences.
    pub seed_occurrences: u64,
    /// Submitted bounded seed batches, including failed attempts.
    pub seed_batches: u64,
    /// Distinct directories whose adjacency EOF was acknowledged.
    pub node_expansions: u64,
    /// Effective entries emitted once during graph construction.
    pub source_entries: u64,
    /// Emitted directory-arc occurrences, including parallel endpoints.
    pub raw_directory_edges: u64,
    /// Submitted bounded adjacency batches.
    pub append_batches: u64,
    /// Final acknowledged distinct vertices.
    pub nodes: u64,
    /// Final acknowledged distinct arcs.
    pub edges: u64,
    /// Final acknowledged source arc multiplicity.
    pub multiplicity: u64,
    /// Acknowledged vertex solver entries.
    pub solver_entered: u64,
    /// Distinct selected outgoing arcs examined during solving.
    pub edge_steps: u64,
    /// Submitted closed solver mutation batches.
    pub cas_batches: u64,
    /// Independently checked completed SCC members.
    pub popped_members: u64,
    /// Submitted bounded member windows.
    pub pop_batches: u64,
    /// Largest observed current page/member record count.
    pub max_page_records: u64,
    /// Largest actual raw child append window.
    pub max_children: u64,
    /// Largest acknowledged combined target count.
    pub max_targets: u64,
    /// Independently checked terminal proof pages.
    pub proof_pages: u64,
}
/// Preselection alone: foreign/unavailable source/context cannot consume another owner.
pub(crate) fn select_graph_input<S: GraphConstructionState + ?Sized>(
    input: &dyn PreparedBindingRows,
    state: &S,
    scopes: &GraphConstructionScopes,
) -> ContentResult<()> {
    state.graph_select(scopes.graph())?;
    let subject = scopes.graph().subject();
    if input.binding_source_id()? != subject.source_id()
        || input.scope() != subject.namespace()
        || input.base() != subject.base()
        || input.root_serial() != subject.root_serial()
    {
        return Err(ContentError::InvalidOrderingRecord("graph input subject"));
    }
    Ok(())
}
/// Checks current binding/alias/root semantics and one admitted effective graph.
pub fn check_with_graph<'a, S: GraphConstructionState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut S,
    scopes: &GraphConstructionScopes,
) -> ContentResult<CheckedTopologyInput<'a>> {
    select_graph_input(input, state, scopes)?;
    check_graph_selected(reader, input, unreachable, work, state, scopes)
}
pub(crate) fn check_graph_selected<'a, S: GraphConstructionState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &'a dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut S,
    scopes: &GraphConstructionScopes,
) -> ContentResult<CheckedTopologyInput<'a>> {
    let outcome = (|| {
        let (topology, mut facts) = site_facts(reader, input, unreachable, work, state, scopes)?;
        check_root_checks(input)?;
        effective(
            super::graph_build::GraphInput {
                reader,
                input,
                topology,
                unreachable,
            },
            work,
            &mut facts,
            state,
            scopes,
        )?;
        Ok(CheckedTopologyInput { input, topology })
    })();
    if outcome.is_err() {
        let _ = state.site_abandon(scopes.sites());
        let _ = state.graph_abandon(scopes.graph());
    }
    outcome
}
fn site_facts<S: GraphConstructionState + ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &dyn PreparedBindingRows,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut S,
    scopes: &GraphConstructionScopes,
) -> ContentResult<(FilesystemTopology, ValidationState)> {
    check_binding_input(input)?;
    let declared = claim_shape(input)?;
    let mut sites = BindingSites::new(state, scopes.sites().clone(), declared)?;
    let (topology, mut facts) = check_binding_semantics(
        reader,
        input,
        work,
        |child, header, ordinal, has_base, _name| {
            sites.claim(
                child,
                header,
                ordinal,
                has_base,
                !unreachable.contains_key(&header.parent()),
            )
        },
    )?;
    let mut sites = sites.close_membership()?;
    let members = sites.members().clone();
    let before = work.inode_pages_read;
    let outcome = site_aliases::check(
        site_aliases::SiteAliasInput {
            reader,
            input,
            topology: &topology,
            members: &members,
            active_stored: sites.active_stored(),
            unreachable,
        },
        work,
        &mut facts,
        sites.state(),
    );
    charge_site(work, before, |sites| &mut sites.aliases);
    outcome?;
    sites.finish(input, unreachable, &members)?;
    Ok((topology, facts))
}
fn effective<S: GraphConstructionState + ?Sized>(
    context: super::graph_build::GraphInput<'_>,
    work: &mut ValidationWork,
    facts: &mut ValidationState,
    state: &mut S,
    scopes: &GraphConstructionScopes,
) -> ContentResult<()> {
    let before = work.inode_pages_read;
    let outcome = (|| {
        let input = context.input;
        let unreachable = context.unreachable;
        let seal = super::graph_build::build(context, work, facts, state, scopes.graph())?;
        super::graph_verify::adjacency(state, &seal, work)?;
        if scopes.graph().subject().mode() == GraphMode::Fresh {
            super::graph_verify::fresh(input, unreachable, state, &seal)?;
        } else {
            super::graph_solve::solve_selected(state, &seal, &mut work.graph)?;
        }
        let proof = super::graph_verify::proof(state, &seal, work)?;
        state.graph_retire(&proof)
    })();
    if scopes.graph().subject().mode() == GraphMode::Fresh {
        charge_site(work, before, |sites| &mut sites.reachability);
    } else {
        charge_site(work, before, |sites| &mut sites.cycles);
    }
    outcome
}
