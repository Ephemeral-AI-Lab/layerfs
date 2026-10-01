//! Profile-specific initialization under the one admitted native owner.
use super::{
    graph_state::Graph, header::Header, phased::Phased, plan::Plan, profile, sites::Sites,
    ScratchSession,
};
use crate::error::{StorageError, StorageResult};
impl ScratchSession {
    pub(crate) fn initialize(&mut self) -> StorageResult<()> {
        let result = (|| {
            let resource = self.resource.as_mut().unwrap();
            resource.check_engine()?;
            if let Err(error) = resource.native.initialize() {
                if resource.native.quarantined {
                    resource.unknown.set(true);
                    return Err(StorageError::UnknownOutcome {
                        original: Box::new(error),
                    });
                }
                return Err(error);
            }
            let profile_subject = if matches!(
                resource.plan,
                Plan::AliasesSitesGraphThenRoots { .. }
                    | Plan::NamespaceSitesGraphThenRoots { .. }
                    | Plan::CanonicalSitesGraphThenRoots { .. }
                    | Plan::Drafts { .. }
            ) {
                Some(
                    resource
                        .plan
                        .profile_subject(resource.graph_subject.as_ref())?,
                )
            } else {
                None
            };
            let binding = match resource.plan {
                Plan::AliasesSitesGraphThenRoots { .. }
                | Plan::NamespaceSitesGraphThenRoots { .. }
                | Plan::CanonicalSitesGraphThenRoots { .. }
                | Plan::Drafts { .. } => resource.native.profile_binding(
                    resource.selection.selector(),
                    resource.selection.token(),
                    resource.plan.version(),
                    profile_subject.as_ref().unwrap(),
                )?,
                Plan::SitesGraphThenRoots { .. } => resource.native.graph_binding(
                    resource.selection.selector(),
                    resource.selection.token(),
                    resource
                        .graph_subject
                        .as_ref()
                        .ok_or(StorageError::Integrity(
                            "construction scratch graph subject",
                        ))?,
                )?,
                Plan::SitesThenRoots { source, .. } => resource.native.sites_binding(
                    resource.selection.selector(),
                    resource.selection.token(),
                    source,
                )?,
                _ => resource
                    .native
                    .binding(resource.selection.selector(), resource.selection.token())?,
            };
            resource.selection.bind_owner(binding)?;
            let header = match resource.plan {
                Plan::CanonicalSitesGraphThenRoots { .. } => {
                    Header::Canonical(resource.native.profile_header(
                        resource.selection.selector(),
                        resource.selection.token(),
                        &binding,
                        8,
                        profile_subject.as_ref().unwrap(),
                    )?)
                }
                Plan::NamespaceSitesGraphThenRoots { .. } => {
                    Header::Namespace(resource.native.profile_header(
                        resource.selection.selector(),
                        resource.selection.token(),
                        &binding,
                        7,
                        profile_subject.as_ref().unwrap(),
                    )?)
                }
                Plan::AliasesSitesGraphThenRoots { .. } => {
                    Header::AliasGraph(resource.native.profile_header(
                        resource.selection.selector(),
                        resource.selection.token(),
                        &binding,
                        5,
                        profile_subject.as_ref().unwrap(),
                    )?)
                }
                Plan::Drafts { .. } => Header::Draft(resource.native.profile_header(
                    resource.selection.selector(),
                    resource.selection.token(),
                    &binding,
                    6,
                    profile_subject.as_ref().unwrap(),
                )?),
                Plan::SitesGraphThenRoots { .. } => Header::Graph(
                    resource.native.graph_header(
                        resource.selection.selector(),
                        resource.selection.token(),
                        &binding,
                        resource
                            .graph_subject
                            .as_ref()
                            .ok_or(StorageError::Integrity(
                                "construction scratch graph subject",
                            ))?,
                    )?,
                ),
                Plan::SitesThenRoots { source, .. } => {
                    Header::Sites(resource.native.sites_header(
                        resource.selection.selector(),
                        resource.selection.token(),
                        &binding,
                        source,
                    )?)
                }
                _ => Header::Earlier(resource.native.header(
                    resource.selection.selector(),
                    resource.selection.token(),
                    &binding,
                    resource.plan.version(),
                )?),
            };
            resource.header = Some(header);
            if let Plan::ClaimsThenRoots {
                directories,
                bindings,
            } = resource.plan
            {
                resource.phased = Some(Phased::new(&resource.selection, directories, bindings)?);
            }
            if let Plan::SitesThenRoots {
                directories,
                bindings,
                source,
            } = resource.plan
            {
                resource.sites = Some(Sites::new(
                    &resource.selection,
                    directories,
                    bindings,
                    source,
                )?);
            }
            if let Plan::SitesGraphThenRoots {
                directories,
                bindings,
                source,
                ..
            }
            | Plan::AliasesSitesGraphThenRoots {
                directories,
                bindings,
                source,
                ..
            }
            | Plan::NamespaceSitesGraphThenRoots {
                directories,
                bindings,
                source,
                ..
            }
            | Plan::CanonicalSitesGraphThenRoots {
                directories,
                bindings,
                source,
                ..
            } = resource.plan
            {
                resource.sites = Some(Sites::with_roots_phase(
                    &resource.selection,
                    directories,
                    bindings,
                    source,
                    3,
                )?);
                resource.graph = Some(Graph::new(
                    &resource.selection,
                    resource
                        .graph_subject
                        .as_ref()
                        .ok_or(StorageError::Integrity(
                            "construction scratch graph subject",
                        ))?
                        .clone(),
                    bindings,
                    resource.graph_memory.as_ref().unwrap().clone(),
                    resource.graph_memory_lease.take().unwrap(),
                )?);
            }
            if let Plan::AliasesSitesGraphThenRoots { aliases, .. }
            | Plan::NamespaceSitesGraphThenRoots { aliases, .. }
            | Plan::CanonicalSitesGraphThenRoots { aliases, .. } = resource.plan
            {
                resource.aliases = Some(super::alias_state::Aliases::new(
                    resource.sites.as_ref().unwrap().scope.clone(),
                    aliases,
                    resource.graph_memory.as_ref().unwrap().clone(),
                )?);
            }
            if let Plan::NamespaceSitesGraphThenRoots { facts, .. }
            | Plan::CanonicalSitesGraphThenRoots { facts, .. } = resource.plan
            {
                resource.facts = Some(super::fact_state::Facts::new(
                    resource.graph_subject.as_ref().unwrap().clone(),
                    facts,
                    resource.graph_memory.as_ref().unwrap().clone(),
                )?);
            }
            if let Plan::Drafts { capacity } = resource.plan {
                resource.draft = Some(super::draft_state::Drafts::new(
                    layerfs_content::file::edit::DraftScope::new(
                        resource.selection.clone(),
                        capacity,
                    )?,
                )?);
            }
            resource.check_engine()?;
            let connection = profile::open(&resource.native.path)?;
            resource.connection = Some(connection);
            resource.native.verify()?;
            let claims = resource
                .phased
                .as_ref()
                .map(|state| state.claims.as_bytes());
            let sites = resource.sites.as_ref().map(|state| state.scope.as_bytes());
            let graph = resource.graph.as_ref().map(|state| state.scope.as_bytes());
            resource.check_engine()?;
            profile::initialize(
                resource.connection.as_ref().unwrap(),
                resource.header.as_ref().unwrap().as_bytes(),
                resource.plan,
                claims.as_ref(),
                sites.as_ref(),
                graph.as_ref(),
                resource.draft.as_ref().map(|state| &state.scope),
                resource.engine,
            )?;
            resource.native.verify()?;
            resource.verify()?;
            Ok(())
        })();
        self.finish(result)
    }
}
