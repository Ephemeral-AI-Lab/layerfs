//! New full source/native binding and funded phase owners on the same working controller.
use super::{
    graph_state::{Graph, GraphOwner},
    header::Header,
    plan::Plan,
    session::Resource,
    sites::Sites,
};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{GraphSubject, StateSelection};
pub(crate) struct Context {
    pub(crate) selection: StateSelection,
    pub(crate) plan: Plan,
    pub(crate) subject: Option<GraphSubject>,
    pub(crate) header: Header,
    pub(crate) sites: Option<Sites>,
    pub(crate) graph: Option<GraphOwner>,
    pub(crate) aliases: Option<super::alias_state::AliasesOwner>,
    pub(crate) facts: Option<super::fact_state::FactsOwner>,
    pub(crate) draft: Option<Box<super::draft_state::Drafts>>,
}
impl Context {
    pub(crate) fn prepare(
        r: &Resource,
        mut selection: StateSelection,
        plan: Plan,
        subject: Option<GraphSubject>,
    ) -> StorageResult<Self> {
        r.native.verify()?;
        let (binding, header) = if plan.version() == 4 {
            let graph = subject
                .as_ref()
                .ok_or(StorageError::Integrity("scratch rebind graph subject"))?;
            let binding = r
                .native
                .graph_binding(selection.selector(), selection.token(), graph)?;
            let header = Header::Graph(r.native.graph_header(
                selection.selector(),
                selection.token(),
                &binding,
                graph,
            )?);
            (binding, header)
        } else {
            let suffix = plan.profile_subject(subject.as_ref())?;
            if suffix.capacity() > 194 {
                return Err(StorageError::Integrity(
                    "scratch rebind subject actual capacity",
                ));
            }
            let binding = r.native.profile_binding(
                selection.selector(),
                selection.token(),
                plan.version(),
                &suffix,
            )?;
            let header = match plan.version() {
                5 => Header::AliasGraph(r.native.profile_header(
                    selection.selector(),
                    selection.token(),
                    &binding,
                    5,
                    &suffix,
                )?),
                6 => Header::Draft(r.native.profile_header(
                    selection.selector(),
                    selection.token(),
                    &binding,
                    6,
                    &suffix,
                )?),
                7 => Header::Namespace(r.native.profile_header(
                    selection.selector(),
                    selection.token(),
                    &binding,
                    7,
                    &suffix,
                )?),
                8 => Header::Canonical(r.native.profile_header(
                    selection.selector(),
                    selection.token(),
                    &binding,
                    8,
                    &suffix,
                )?),
                _ => return Err(StorageError::Integrity("scratch rebind profile")),
            };
            (binding, header)
        };
        selection.bind_owner(binding)?;
        let mut context = Self {
            selection,
            plan,
            subject,
            header,
            sites: None,
            graph: None,
            aliases: None,
            facts: None,
            draft: None,
        };
        if plan.version() == 6 {
            let Plan::Drafts { capacity } = plan else {
                return Err(StorageError::Integrity("scratch rebind draft plan"));
            };
            context.draft = Some(super::draft_state::Drafts::new(
                layerfs_content::file::edit::DraftScope::new(context.selection.clone(), capacity)?,
            )?);
        } else {
            let subject = context
                .subject
                .as_ref()
                .ok_or(StorageError::Integrity("scratch rebind source subject"))?;
            context.sites = Some(Sites::with_roots_phase(
                &context.selection,
                plan.root_limit(),
                plan.binding_limit(),
                subject.source_id(),
                3,
            )?);
            let memory = r
                .graph_memory
                .as_ref()
                .ok_or(StorageError::Integrity("scratch rebind working controller"))?;
            let graph_lease = memory.reserve(std::mem::size_of::<Graph>())?;
            context.graph = Some(Graph::new(
                &context.selection,
                subject.clone(),
                plan.binding_limit(),
                memory.clone(),
                graph_lease,
            )?);
            if let Some(capacity) = plan.aliases() {
                context.aliases = Some(super::alias_state::Aliases::new(
                    context.sites.as_ref().unwrap().scope.clone(),
                    capacity,
                    memory.clone(),
                )?);
            }
            if let Some(capacity) = plan.facts() {
                context.facts = Some(super::fact_state::Facts::new(
                    subject.clone(),
                    capacity,
                    memory.clone(),
                )?);
            }
        }
        Ok(context)
    }
    pub(crate) fn apply(self, r: &mut Resource) {
        // Only after complete new-row and native acknowledgement. Old funded
        // payloads are destroyed before their own credits return.
        r.selection = self.selection;
        r.plan = self.plan;
        r.graph_subject = self.subject;
        r.header = Some(self.header);
        r.ledger = None;
        r.seal = None;
        r.logical_released = false;
        r.phased = None;
        r.sites = self.sites;
        r.graph = self.graph;
        r.graph_memory_lease = None;
        r.aliases = self.aliases;
        r.facts = self.facts;
        r.draft = self.draft;
        r.counts = None;
        r.releasing = None;
        r.canonical_capacity = self.plan.canonical_capacity();
        r.root_retirement = None;
        r.known_clean = false;
    }
}
