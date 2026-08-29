use layerfs_cli::{Completion, Fact};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoryGroup {
    pub(crate) name: String,
    pub(crate) role: String,
    pub(crate) location: String,
    pub(crate) parent: Option<String>,
    pub(crate) facts: Vec<Fact>,
    pub(crate) has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TopologyEntry {
    pub(crate) role: String,
    pub(crate) name: String,
    pub(crate) location: String,
    pub(crate) parent: Option<String>,
    pub(crate) active: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Focus {
    #[default]
    Stores,
    Histories,
    Details,
    Lineage,
    Command,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StoreRow {
    pub(crate) topology_index: usize,
    pub(crate) depth: usize,
    pub(crate) last: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HistoryCategory {
    Layers,
    Stacks,
    Branches,
}

impl HistoryCategory {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Layers => "LAYER HISTORIES",
            Self::Stacks => "STACK HISTORIES",
            Self::Branches => "BRANCH HISTORIES",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HistoryRow {
    pub(crate) category: HistoryCategory,
    pub(crate) fact: Option<usize>,
    pub(crate) depth: usize,
    pub(crate) head: bool,
    pub(crate) tail: bool,
    pub(crate) number: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LineageRow {
    pub(crate) fact_index: usize,
    pub(crate) depth: usize,
    pub(crate) head: bool,
    pub(crate) tail: bool,
    pub(crate) number: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RelationKind {
    Base,
    CreatedBy,
    Produces,
    Instantiates,
}

impl RelationKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::CreatedBy => "created by",
            Self::Produces => "publishes",
            Self::Instantiates => "instantiates",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LineageRelation {
    pub(crate) source: usize,
    pub(crate) target: usize,
    pub(crate) kind: RelationKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LineageChild {
    pub(crate) source: usize,
    pub(crate) container: usize,
    pub(crate) category: HistoryCategory,
    pub(crate) selected: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LineageBase {
    pub(crate) fact_index: usize,
    pub(crate) number: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    PullLayer,
    PullStack,
    PushStack,
    PullBranch,
    PullCommits,
    PushBranch,
}

impl Action {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PullLayer => "Pull layer",
            Self::PullStack => "Pull stack",
            Self::PushStack => "Push stack",
            Self::PullBranch => "Pull branch",
            Self::PullCommits => "Pull commits",
            Self::PushBranch => "Push branch",
        }
    }

    pub(crate) fn key(self) -> char {
        match self {
            Self::PullLayer | Self::PullStack | Self::PullBranch | Self::PullCommits => 'P',
            Self::PushStack | Self::PushBranch => 'p',
        }
    }
}

pub(crate) enum Message {
    Info(String),
    Error(String),
}

impl Message {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Info(text) | Self::Error(text) => text,
        }
    }

    pub(crate) fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

#[derive(Default)]
pub(crate) struct App {
    quit: bool,
    topology: Vec<TopologyEntry>,
    selected_store: usize,
    expanded_stores: Vec<bool>,
    histories: Vec<HistoryGroup>,
    selected_history: usize,
    expanded_categories: Vec<HistoryCategory>,
    focus: Focus,
    selected_action: usize,
    store_scroll: u16,
    history_scroll: u16,
    lineage_scroll: u16,
    lineage_child: Option<LineageChild>,
    lineage_parent_child: Option<LineageChild>,
    lineage_relation_focus: bool,
    lineage_relation: usize,
    details_scroll: u16,
    command: String,
    command_cursor: usize,
    command_running: bool,
    focus_after_command: Option<Focus>,
    completions: Vec<Completion>,
    selected_completion: usize,
    message: Option<Message>,
    help: bool,
}

impl App {
    pub(crate) fn quit(&mut self) {
        self.quit = true;
    }

    pub(crate) fn is_running(&self) -> bool {
        !self.quit
    }

    pub(crate) fn topology(&self) -> &[TopologyEntry] {
        &self.topology
    }

    pub(crate) fn store_rows(&self) -> Vec<StoreRow> {
        store_rows(&self.topology, &self.expanded_stores)
    }

    pub(crate) fn selected_store(&self) -> Option<&TopologyEntry> {
        let rows = self.store_rows();
        let row = rows.get(self.selected_store)?;
        self.topology.get(row.topology_index)
    }

    pub(crate) fn selected_store_index(&self) -> Option<usize> {
        self.store_rows()
            .get(self.selected_store)
            .map(|row| row.topology_index)
    }

    pub(crate) fn selected_store_history(&self) -> Option<&HistoryGroup> {
        let store = self.selected_store()?;
        self.histories.iter().find(|group| {
            group.location == store.location
                || (group.role == store.role && group.name == store.name)
        })
    }

    #[allow(dead_code)]
    pub(crate) fn history_groups(&self) -> &[HistoryGroup] {
        &self.histories
    }

    pub(crate) fn history_categories(&self) -> Vec<HistoryCategory> {
        match self.selected_store().map(|store| store.role.as_str()) {
            Some("branchstore") => vec![HistoryCategory::Branches],
            Some("layerstore" | "stackstore") => vec![
                HistoryCategory::Layers,
                HistoryCategory::Stacks,
                HistoryCategory::Branches,
            ],
            _ => Vec::new(),
        }
    }

    pub(crate) fn history_rows(&self) -> Vec<HistoryRow> {
        let Some(group) = self.selected_store_history() else {
            return Vec::new();
        };
        self.history_categories()
            .into_iter()
            .flat_map(|category| {
                let header = std::iter::once(HistoryRow {
                    category,
                    fact: None,
                    depth: 0,
                    head: false,
                    tail: false,
                    number: 0,
                });
                let facts = self
                    .history_category_expanded(category)
                    .then(|| category_fact_rows(group, category))
                    .into_iter()
                    .flatten()
                    .map(move |(fact, depth, head, tail, number)| HistoryRow {
                        category,
                        fact: Some(fact),
                        depth,
                        head,
                        tail,
                        number,
                    });
                header.chain(facts)
            })
            .collect()
    }

    pub(crate) fn selected_history(&self) -> Option<HistoryRow> {
        self.history_rows().get(self.selected_history).copied()
    }

    #[allow(dead_code)]
    pub(crate) fn selected_history_fact(&self) -> Option<Fact> {
        let row = self.selected_history()?;
        let group = self.selected_store_history()?;
        row.fact.and_then(|index| group.facts.get(index).copied())
    }

    pub(crate) fn active_lineage_fact(&self) -> Option<Fact> {
        let group = self.selected_store_history()?;
        let index = self
            .lineage_child
            .map(|child| child.selected)
            .or_else(|| self.selected_history()?.fact)?;
        group.facts.get(index).copied()
    }

    pub(crate) fn active_lineage_fact_index(&self) -> Option<usize> {
        self.lineage_child
            .map(|child| child.selected)
            .or_else(|| self.selected_history()?.fact)
    }

    pub(crate) fn lineage_child_lanes(&self) -> Vec<LineageChild> {
        self.lineage_parent_child
            .into_iter()
            .chain(self.lineage_child)
            .collect()
    }

    pub(crate) fn lineage_base(&self) -> Option<LineageBase> {
        let group = self.selected_store_history()?;
        let container = self.lineage_rows().first()?.fact_index;
        lineage_base_for_container(group, container)
    }

    pub(crate) fn lineage_base_for_child(&self, child: LineageChild) -> Option<LineageBase> {
        let group = self.selected_store_history()?;
        lineage_base_for_container(group, child.container)
    }

    pub(crate) fn lineage_base_prefix_width(&self) -> usize {
        let Some(group) = self.selected_store_history() else {
            return 0;
        };
        self.lineage_base()
            .map(|base| lineage_base_prefix_width(group, base))
            .unwrap_or_default()
    }

    pub(crate) fn lineage_base_prefix_width_for_child(&self, child: LineageChild) -> usize {
        let Some(group) = self.selected_store_history() else {
            return 0;
        };
        self.lineage_base_for_child(child)
            .map(|base| lineage_base_prefix_width(group, base))
            .unwrap_or_default()
    }

    pub(crate) fn lineage_relation_focus(&self) -> bool {
        self.lineage_relation_focus
    }

    pub(crate) fn lineage_relations(&self) -> Vec<LineageRelation> {
        let Some(group) = self.selected_store_history() else {
            return Vec::new();
        };
        let Some(active) = self.active_lineage_fact_index() else {
            return Vec::new();
        };
        let mut relations = if let Some(child) = self.lineage_child {
            if child.category == HistoryCategory::Stacks {
                match group.facts.get(child.selected).copied() {
                    Some(Fact::Stack(stack)) => {
                        stack_relations(group, child.selected, stack.id.to_bytes().as_slice())
                    }
                    _ => stack_history_relations(group, child.container),
                }
            } else {
                match group.facts.get(child.container).copied() {
                    Some(Fact::Branch(branch)) => {
                        branch_relations(group, child.container, branch.base_id.as_slice())
                    }
                    _ => Vec::new(),
                }
            }
        } else {
            fact_relations(group, active)
        };
        relations.sort_by_key(|relation| (relation.kind as u8, relation.target));
        relations.dedup();
        relations
    }

    pub(crate) fn select_lineage_relation_next(&mut self) -> bool {
        let count = self.lineage_relations().len();
        if count == 0 {
            return false;
        }
        self.lineage_relation = (self.lineage_relation + 1).min(count - 1);
        true
    }

    pub(crate) fn select_lineage_relation_previous(&mut self) -> bool {
        let count = self.lineage_relations().len();
        if count == 0 {
            return false;
        }
        self.lineage_relation = self.lineage_relation.saturating_sub(1);
        true
    }

    pub(crate) fn select_lineage_relation_first(&mut self) {
        self.lineage_relation = 0;
    }

    #[allow(dead_code)]
    pub(crate) fn select_lineage_relation_last(&mut self) {
        self.lineage_relation = self.lineage_relations().len().saturating_sub(1);
    }

    pub(crate) fn selected_lineage_relation(&self) -> Option<LineageRelation> {
        self.lineage_relations().get(self.lineage_relation).copied()
    }

    pub(crate) fn lineage_relation_offset(&self) -> usize {
        self.lineage_relation
    }

    pub(crate) fn focus_lineage_relations(&mut self) -> bool {
        if self.lineage_relations().is_empty() {
            return false;
        }
        self.lineage_relation_focus = true;
        self.lineage_relation = self
            .lineage_relation
            .min(self.lineage_relations().len().saturating_sub(1));
        true
    }

    pub(crate) fn focus_lineage_nodes(&mut self) {
        self.lineage_relation_focus = false;
    }

    pub(crate) fn close_lineage_child(&mut self) -> bool {
        let Some(child) = self.lineage_child.take() else {
            return false;
        };
        if let Some(parent) = self.lineage_parent_child.take() {
            self.lineage_child = Some(parent);
        }
        self.lineage_relation_focus = true;
        let relations = self.lineage_relations();
        self.lineage_relation = relations
            .iter()
            .position(|relation| relation.target == child.container)
            .unwrap_or(0);
        self.lineage_scroll = 0;
        true
    }

    pub(crate) fn open_lineage_relation(&mut self) -> bool {
        let Some(relation) = self.selected_lineage_relation() else {
            return false;
        };
        let Some(group) = self.selected_store_history() else {
            return false;
        };
        let Some(target) = group.facts.get(relation.target).copied() else {
            return false;
        };
        match target {
            Fact::StackHistory(_) => {
                let selected = lineage_first_node(group, HistoryCategory::Stacks, relation.target)
                    .unwrap_or(relation.target);
                self.lineage_child = Some(LineageChild {
                    source: relation.source,
                    container: relation.target,
                    category: HistoryCategory::Stacks,
                    selected,
                });
                self.lineage_parent_child = None;
                self.lineage_relation_focus = false;
                self.lineage_relation = 0;
                self.lineage_scroll = 0;
                true
            }
            Fact::Branch(_) => {
                let selected =
                    lineage_first_node(group, HistoryCategory::Branches, relation.target)
                        .unwrap_or(relation.target);
                self.lineage_parent_child = self.lineage_child;
                self.lineage_child = Some(LineageChild {
                    source: relation.source,
                    container: relation.target,
                    category: HistoryCategory::Branches,
                    selected,
                });
                self.lineage_relation_focus = false;
                self.lineage_relation = 0;
                self.lineage_scroll = 0;
                true
            }
            Fact::Layer(_) | Fact::Stack(_) => {
                self.lineage_child = None;
                self.lineage_parent_child = None;
                self.lineage_relation_focus = false;
                self.select_history_fact(relation.target)
            }
            _ => false,
        }
    }

    pub(crate) fn select_lineage_fact(&mut self, fact_index: usize) -> bool {
        if self.lineage_child.is_some() {
            let rows = self.lineage_child_visual_rows();
            if rows.iter().any(|row| row.fact_index == fact_index) {
                let offset = self.lineage_offset_for(rows, fact_index);
                if let Some(child) = self.lineage_child.as_mut() {
                    child.selected = fact_index;
                }
                self.lineage_relation_focus = false;
                self.lineage_scroll = offset as u16;
                self.selected_action = 0;
                self.clamp_scrolls();
                return true;
            }
        }
        self.select_history_fact(fact_index)
    }

    pub(crate) fn select_history_fact(&mut self, fact_index: usize) -> bool {
        let Some(row) = self
            .history_rows()
            .iter()
            .position(|row| row.fact == Some(fact_index))
        else {
            return false;
        };
        self.select_history_row(row);
        true
    }

    pub(crate) fn selected_history_category(&self) -> Option<HistoryCategory> {
        self.selected_history().map(|row| row.category)
    }

    pub(crate) fn lineage_rows(&self) -> Vec<LineageRow> {
        let Some(group) = self.selected_store_history() else {
            return Vec::new();
        };
        let Some(selected) = self.selected_history() else {
            return Vec::new();
        };
        let Some(selected_index) = selected.fact else {
            return Vec::new();
        };
        let rows = category_fact_rows(group, selected.category);
        let Some(container_index) =
            lineage_container_index(group, selected.category, selected_index, &rows)
        else {
            return rows
                .into_iter()
                .filter(|(index, _, _, _, _)| *index == selected_index)
                .map(|(fact_index, depth, head, tail, number)| LineageRow {
                    fact_index,
                    depth,
                    head,
                    tail,
                    number,
                })
                .collect();
        };
        let mut lineage = Vec::new();
        let mut active = false;
        for (fact_index, depth, head, tail, number) in rows {
            if fact_index == container_index {
                active = true;
            } else if active
                && group
                    .facts
                    .get(fact_index)
                    .is_some_and(|fact| is_history_container(selected.category, *fact))
            {
                break;
            }
            if active {
                lineage.push(LineageRow {
                    fact_index,
                    depth,
                    head,
                    tail,
                    number,
                });
            }
        }
        lineage
    }

    pub(crate) fn lineage_visual_rows(&self) -> Vec<LineageRow> {
        let rows = self.lineage_rows();
        let Some((first, rest)) = rows.split_first() else {
            return rows;
        };
        if first.depth == 1 {
            let mut visual = Vec::with_capacity(rows.len());
            visual.push(*first);
            visual.extend(rest.iter().rev().copied());
            visual
        } else {
            rows.into_iter().rev().collect()
        }
    }

    pub(crate) fn lineage_child_rows(&self) -> Vec<LineageRow> {
        let Some(child) = self.lineage_child else {
            return Vec::new();
        };
        self.lineage_rows_for_child(child)
    }

    pub(crate) fn lineage_rows_for_child(&self, child: LineageChild) -> Vec<LineageRow> {
        let Some(group) = self.selected_store_history() else {
            return Vec::new();
        };
        lineage_rows_for_selection(group, child.category, child.container)
    }

    pub(crate) fn lineage_child_visual_rows(&self) -> Vec<LineageRow> {
        lineage_visual_rows_from_rows(self.lineage_child_rows())
    }

    pub(crate) fn lineage_visual_rows_for_child(&self, child: LineageChild) -> Vec<LineageRow> {
        lineage_visual_rows_from_rows(self.lineage_rows_for_child(child))
    }

    #[allow(dead_code)]
    fn lineage_visual_rows_for(
        &self,
        category: HistoryCategory,
        container: usize,
    ) -> Vec<LineageRow> {
        let Some(group) = self.selected_store_history() else {
            return Vec::new();
        };
        lineage_visual_rows_from_rows(lineage_rows_for_selection(group, category, container))
    }

    fn lineage_offset_for(&self, rows: Vec<LineageRow>, fact_index: usize) -> usize {
        let Some(group) = self.selected_store_history() else {
            return 0;
        };
        let mut offset = self
            .lineage_child
            .map(|child| self.lineage_base_prefix_width_for_child(child))
            .unwrap_or_else(|| self.lineage_base_prefix_width());
        let mut node_count = 0;
        for row in rows {
            let Some(fact) = group.facts.get(row.fact_index).copied() else {
                continue;
            };
            let Some(width) = lineage_node_width(fact, row.number) else {
                continue;
            };
            if row.fact_index == fact_index {
                return offset;
            }
            if node_count > 0 {
                offset += 5;
            }
            offset += width;
            node_count += 1;
        }
        0
    }

    pub(crate) fn lineage_visual_rows_active(&self) -> Vec<LineageRow> {
        if self.lineage_child.is_some() {
            self.lineage_child_visual_rows()
        } else {
            self.lineage_visual_rows()
        }
    }

    pub(crate) fn history_category_expanded(&self, category: HistoryCategory) -> bool {
        self.expanded_categories.contains(&category)
    }

    pub(crate) fn actions(&self) -> Vec<Action> {
        let Some(store) = self.selected_store() else {
            return Vec::new();
        };
        match (store.role.as_str(), self.active_lineage_fact()) {
            ("stackstore", Some(Fact::Layer(_))) => vec![Action::PullLayer],
            ("stackstore", Some(Fact::Stack(_))) => vec![Action::PullStack, Action::PushStack],
            ("branchstore", Some(Fact::Branch(_))) => {
                vec![Action::PullBranch, Action::PullCommits, Action::PushBranch]
            }
            _ => Vec::new(),
        }
    }

    pub(crate) fn selected_action(&self) -> Option<Action> {
        self.actions().get(self.selected_action).copied()
    }

    pub(crate) fn selected_action_index(&self) -> usize {
        self.selected_action
    }

    pub(crate) fn focus(&self) -> Focus {
        self.focus
    }

    pub(crate) fn select_store_next(&mut self) {
        let count = self.store_rows().len();
        if count > 0 {
            self.selected_store = (self.selected_store + 1).min(count - 1);
            self.selected_history = 0;
            self.selected_action = 0;
            self.history_scroll = 0;
            self.reset_lineage();
        }
    }

    pub(crate) fn select_store_next_if_possible(&mut self) -> bool {
        let count = self.store_rows().len();
        if self.selected_store + 1 >= count {
            false
        } else {
            self.select_store_next();
            true
        }
    }

    pub(crate) fn select_store_previous(&mut self) {
        self.selected_store = self.selected_store.saturating_sub(1);
        self.selected_history = 0;
        self.selected_action = 0;
        self.history_scroll = 0;
        self.reset_lineage();
    }

    pub(crate) fn select_store_previous_if_possible(&mut self) -> bool {
        if self.selected_store == 0 {
            false
        } else {
            self.select_store_previous();
            true
        }
    }

    pub(crate) fn select_store_first(&mut self) {
        self.select_store_row(0);
    }

    pub(crate) fn select_store_last(&mut self) {
        self.select_store_row(self.store_rows().len().saturating_sub(1));
    }

    pub(crate) fn toggle_store(&mut self) {
        let Some(row) = self.store_rows().get(self.selected_store).copied() else {
            return;
        };
        if let Some(expanded) = self.expanded_stores.get_mut(row.topology_index) {
            *expanded = !*expanded;
            self.selected_store = self
                .store_rows()
                .iter()
                .position(|candidate| candidate.topology_index == row.topology_index)
                .unwrap_or(self.selected_store);
        }
    }

    pub(crate) fn toggle_history_category(&mut self) {
        let Some(category) = self.selected_history_category() else {
            return;
        };
        let expanded = !self.history_category_expanded(category);
        self.set_history_category_expanded(category, expanded);
    }

    fn set_history_category_expanded(&mut self, category: HistoryCategory, expanded: bool) {
        if expanded {
            if !self.expanded_categories.contains(&category) {
                self.expanded_categories.push(category);
            }
        } else {
            self.expanded_categories.retain(|value| *value != category);
        }
        self.selected_history = self
            .history_rows()
            .iter()
            .position(|row| row.category == category && row.fact.is_none())
            .unwrap_or(0);
    }

    pub(crate) fn select_history_next(&mut self) {
        let count = self.history_rows().len();
        if count > 0 {
            self.selected_history = (self.selected_history + 1).min(count - 1);
            self.selected_action = 0;
            self.reset_lineage();
        }
    }

    pub(crate) fn select_history_next_if_possible(&mut self) -> bool {
        let count = self.history_rows().len();
        if self.selected_history + 1 >= count {
            false
        } else {
            self.select_history_next();
            true
        }
    }

    pub(crate) fn select_history_previous(&mut self) {
        self.selected_history = self.selected_history.saturating_sub(1);
        self.selected_action = 0;
        self.reset_lineage();
    }

    pub(crate) fn select_history_previous_if_possible(&mut self) -> bool {
        if self.selected_history == 0 {
            false
        } else {
            self.select_history_previous();
            true
        }
    }

    pub(crate) fn select_history_first(&mut self) {
        self.select_history_row(0);
    }

    pub(crate) fn select_history_last(&mut self) {
        self.select_history_row(self.history_rows().len().saturating_sub(1));
    }

    pub(crate) fn select_action_next(&mut self) {
        let count = self.actions().len();
        if count > 0 {
            self.selected_action = (self.selected_action + 1).min(count - 1);
        }
    }

    pub(crate) fn select_action_next_if_possible(&mut self) -> bool {
        let count = self.actions().len();
        if self.selected_action + 1 >= count {
            false
        } else {
            self.select_action_next();
            true
        }
    }

    pub(crate) fn select_action_previous(&mut self) {
        self.selected_action = self.selected_action.saturating_sub(1);
    }

    pub(crate) fn select_action_previous_if_possible(&mut self) -> bool {
        if self.selected_action == 0 {
            false
        } else {
            self.select_action_previous();
            true
        }
    }

    pub(crate) fn select_action_first(&mut self) {
        self.select_action_row(0);
    }

    pub(crate) fn select_action_last(&mut self) {
        self.select_action_row(self.actions().len().saturating_sub(1));
    }

    pub(crate) fn focus_next(&mut self) {
        self.focus = match self.focus {
            Focus::Stores => Focus::Histories,
            Focus::Histories => Focus::Details,
            Focus::Details => Focus::Lineage,
            Focus::Lineage => Focus::Command,
            Focus::Command => Focus::Stores,
        };
    }

    pub(crate) fn focus_previous(&mut self) {
        self.focus = match self.focus {
            Focus::Stores => Focus::Command,
            Focus::Histories => Focus::Stores,
            Focus::Details => Focus::Histories,
            Focus::Lineage => Focus::Details,
            Focus::Command => Focus::Lineage,
        };
    }

    pub(crate) fn focus_direction(&mut self, direction: Direction) {
        self.focus = match (self.focus, direction) {
            (Focus::Stores, Direction::Down) => Focus::Histories,
            (Focus::Stores, Direction::Right) => Focus::Details,
            (Focus::Histories, Direction::Up) => Focus::Stores,
            (Focus::Histories, Direction::Right) => Focus::Lineage,
            (Focus::Details, Direction::Down) => Focus::Lineage,
            (Focus::Details, Direction::Left) => Focus::Stores,
            (Focus::Lineage, Direction::Up) => Focus::Details,
            (Focus::Lineage, Direction::Down) => Focus::Command,
            (Focus::Lineage, Direction::Left) => Focus::Histories,
            (focus, _) => focus,
        };
    }

    pub(crate) fn focus_stores(&mut self) {
        self.focus = Focus::Stores;
    }

    pub(crate) fn focus_histories(&mut self) {
        self.focus = Focus::Histories;
    }

    pub(crate) fn focus_details(&mut self) {
        self.focus = Focus::Details;
    }

    pub(crate) fn focus_lineage(&mut self) {
        self.focus = Focus::Lineage;
    }

    pub(crate) fn scroll_stores(&mut self, down: bool) {
        self.store_scroll = if down {
            self.store_scroll.saturating_add(1)
        } else {
            self.store_scroll.saturating_sub(1)
        };
        self.clamp_scrolls();
    }

    pub(crate) fn scroll_histories(&mut self, down: bool) {
        self.history_scroll = if down {
            self.history_scroll.saturating_add(1)
        } else {
            self.history_scroll.saturating_sub(1)
        };
        self.clamp_scrolls();
    }

    pub(crate) fn select_lineage_next_if_possible(&mut self) -> bool {
        self.select_lineage_offset(1)
    }

    pub(crate) fn select_lineage_previous_if_possible(&mut self) -> bool {
        self.select_lineage_offset(-1)
    }

    pub(crate) fn select_lineage_first(&mut self) {
        self.select_lineage_position(0);
    }

    pub(crate) fn select_lineage_last(&mut self) {
        self.select_lineage_position(self.lineage_node_fact_indices().len().saturating_sub(1));
    }

    pub(crate) fn select_lineage_page(&mut self, down: bool) {
        for _ in 0..8 {
            if !(if down {
                self.select_lineage_next_if_possible()
            } else {
                self.select_lineage_previous_if_possible()
            }) {
                break;
            }
        }
    }

    pub(crate) fn scroll_details(&mut self, down: bool) {
        self.details_scroll = if down {
            self.details_scroll.saturating_add(1)
        } else {
            self.details_scroll.saturating_sub(1)
        };
        self.clamp_scrolls();
    }

    pub(crate) fn store_scroll(&self) -> u16 {
        self.store_scroll
    }

    pub(crate) fn history_scroll(&self) -> u16 {
        self.history_scroll
    }

    pub(crate) fn details_scroll(&self) -> u16 {
        self.details_scroll
    }

    pub(crate) fn lineage_scroll(&self) -> u16 {
        self.lineage_scroll
    }

    fn lineage_content_width(&self) -> usize {
        let Some(group) = self.selected_store_history() else {
            return 0;
        };
        let rows = self.lineage_visual_rows_active();
        let nodes = rows
            .iter()
            .filter_map(|row| group.facts.get(row.fact_index).copied())
            .filter(|fact| matches!(fact, Fact::Layer(_) | Fact::Stack(_) | Fact::Commit(_)))
            .count();
        rows.into_iter()
            .filter_map(|row| {
                group
                    .facts
                    .get(row.fact_index)
                    .copied()
                    .map(|fact| (fact, row))
            })
            .filter_map(|(fact, row)| lineage_node_width(fact, row.number))
            .sum::<usize>()
            .saturating_add(nodes.saturating_sub(1) * 5)
    }

    fn lineage_node_fact_indices(&self) -> Vec<usize> {
        let Some(group) = self.selected_store_history() else {
            return Vec::new();
        };
        self.lineage_visual_rows_active()
            .into_iter()
            .filter(|row| {
                group.facts.get(row.fact_index).is_some_and(|fact| {
                    matches!(fact, Fact::Layer(_) | Fact::Stack(_) | Fact::Commit(_))
                })
            })
            .map(|row| row.fact_index)
            .collect()
    }

    fn select_lineage_offset(&mut self, offset: isize) -> bool {
        let nodes = self.lineage_node_fact_indices();
        let current = self
            .active_lineage_fact_index()
            .and_then(|fact| nodes.iter().position(|index| *index == fact));
        let target = current
            .and_then(|position| position.checked_add_signed(offset))
            .filter(|position| *position < nodes.len());
        let Some(position) = target else {
            return false;
        };
        self.select_lineage_position(position)
    }

    fn select_lineage_position(&mut self, position: usize) -> bool {
        let nodes = self.lineage_node_fact_indices();
        let Some(fact_index) = nodes.get(position).copied() else {
            return false;
        };
        let offset = self.lineage_offset(fact_index);
        if let Some(child) = self.lineage_child.as_mut() {
            child.selected = fact_index;
            self.lineage_relation_focus = false;
            self.lineage_scroll = offset as u16;
            self.selected_action = 0;
            self.clamp_scrolls();
            return true;
        }
        let Some(history_row) = self
            .history_rows()
            .iter()
            .position(|row| row.fact == Some(fact_index))
        else {
            return false;
        };
        self.selected_history = history_row;
        self.selected_action = 0;
        self.lineage_scroll = offset as u16;
        self.clamp_scrolls();
        true
    }

    fn lineage_offset(&self, fact_index: usize) -> usize {
        let Some(group) = self.selected_store_history() else {
            return 0;
        };
        let mut offset = 0;
        let mut node_count = 0;
        for row in self.lineage_visual_rows_active() {
            let Some(fact) = group.facts.get(row.fact_index).copied() else {
                continue;
            };
            let Some(width) = lineage_node_width(fact, row.number) else {
                continue;
            };
            if row.fact_index == fact_index {
                return offset;
            }
            if node_count > 0 {
                offset += 5;
            }
            offset += width;
            node_count += 1;
        }
        0
    }

    pub(crate) fn on_resize(&mut self) {
        self.clamp_scrolls();
    }

    pub(crate) fn command(&self) -> &str {
        &self.command
    }

    pub(crate) fn command_cursor(&self) -> usize {
        self.command_cursor
    }

    pub(crate) fn command_focused(&self) -> bool {
        self.focus == Focus::Command
    }

    pub(crate) fn command_running(&self) -> bool {
        self.command_running
    }

    pub(crate) fn completions(&self) -> &[Completion] {
        &self.completions
    }

    pub(crate) fn selected_completion(&self) -> usize {
        self.selected_completion
    }

    pub(crate) fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub(crate) fn help_visible(&self) -> bool {
        self.help
    }

    pub(crate) fn toggle_help(&mut self) {
        self.help = !self.help;
    }

    pub(crate) fn focus_command(&mut self) {
        if self.command_running {
            return;
        }
        self.focus = Focus::Command;
        if self.command.is_empty() {
            self.command.push('/');
            self.command_cursor = 1;
        }
    }

    pub(crate) fn prepare_command(&mut self, command: String) {
        self.command = command;
        self.command_cursor = self.command.len();
        self.message = None;
        self.focus = Focus::Command;
    }

    pub(crate) fn blur_command(&mut self) {
        self.focus = Focus::Stores;
        self.completions.clear();
    }

    pub(crate) fn type_command(&mut self, character: char) {
        self.message = None;
        self.command.insert(self.command_cursor, character);
        self.command_cursor += character.len_utf8();
    }

    pub(crate) fn erase_command(&mut self) {
        self.message = None;
        let Some(previous) = self.command[..self.command_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
        else {
            return;
        };
        self.command.drain(previous..self.command_cursor);
        self.command_cursor = previous;
    }

    pub(crate) fn delete_command(&mut self) {
        self.message = None;
        let Some(character) = self.command[self.command_cursor..].chars().next() else {
            return;
        };
        self.command
            .drain(self.command_cursor..self.command_cursor + character.len_utf8());
    }

    pub(crate) fn move_command_left(&mut self) {
        if let Some((index, _)) = self.command[..self.command_cursor]
            .char_indices()
            .next_back()
        {
            self.command_cursor = index;
        }
    }

    pub(crate) fn move_command_right(&mut self) {
        if let Some(character) = self.command[self.command_cursor..].chars().next() {
            self.command_cursor += character.len_utf8();
        }
    }

    pub(crate) fn move_command_home(&mut self) {
        self.command_cursor = usize::from(self.command.starts_with('/'));
    }

    pub(crate) fn move_command_end(&mut self) {
        self.command_cursor = self.command.len();
    }

    pub(crate) fn set_completions(&mut self, completions: Vec<Completion>) {
        self.completions = completions;
        self.selected_completion = 0;
    }

    pub(crate) fn previous_completion(&mut self) {
        if !self.completions.is_empty() {
            self.selected_completion = self
                .selected_completion
                .checked_sub(1)
                .unwrap_or(self.completions.len() - 1);
        }
    }

    pub(crate) fn next_completion(&mut self) {
        if !self.completions.is_empty() {
            self.selected_completion = (self.selected_completion + 1) % self.completions.len();
        }
    }

    pub(crate) fn apply_completion(&mut self) {
        let Some(value) = self
            .completions
            .get(self.selected_completion)
            .map(|completion| completion.value.clone())
        else {
            return;
        };
        let start = self.command[..self.command_cursor]
            .rfind(char::is_whitespace)
            .map_or(usize::from(self.command.starts_with('/')), |index| {
                index + 1
            });
        let end = self.command_cursor
            + self.command[self.command_cursor..]
                .find(char::is_whitespace)
                .unwrap_or(self.command.len() - self.command_cursor);
        self.command.replace_range(start..end, &value);
        self.command_cursor = start + value.len();
        if self.command_cursor == self.command.len() {
            self.command.push(' ');
            self.command_cursor += 1;
        }
    }

    pub(crate) fn command_started(&mut self) {
        self.command_running = true;
        self.focus = Focus::Stores;
        self.completions.clear();
        self.message = Some(Message::Info("RUNNING".to_owned()));
    }

    pub(crate) fn command_succeeded(&mut self, message: impl Into<String>) {
        self.command.clear();
        self.command_cursor = 0;
        self.command_running = false;
        self.focus = self.focus_after_command.take().unwrap_or({
            if self.topology.is_empty() {
                Focus::Command
            } else {
                Focus::Stores
            }
        });
        self.completions.clear();
        self.message = Some(Message::Info(message.into()));
    }

    pub(crate) fn command_failed(&mut self, error: impl Into<String>) {
        self.command_running = false;
        self.focus_after_command = None;
        self.focus = Focus::Command;
        self.message = Some(Message::Error(error.into()));
    }

    pub(crate) fn info(&mut self, message: impl Into<String>) {
        self.message = Some(Message::Info(message.into()));
    }

    pub(crate) fn focus_after_command(&mut self, focus: Focus) {
        self.focus_after_command = Some(focus);
    }

    pub(crate) fn replace_topology(&mut self, topology: Vec<TopologyEntry>) {
        let selected_location = self.selected_store().map(|store| store.location.clone());
        let old_locations = self
            .topology
            .iter()
            .map(|entry| entry.location.clone())
            .collect::<std::collections::HashSet<_>>();
        let old_expanded = self
            .topology
            .iter()
            .enumerate()
            .filter(|(index, _)| self.expanded_stores.get(*index).copied().unwrap_or(true))
            .map(|(_, entry)| entry.location.clone())
            .collect::<std::collections::HashSet<_>>();
        self.topology = topology;
        self.expanded_stores = self
            .topology
            .iter()
            .map(|entry| {
                !old_locations.contains(&entry.location) || old_expanded.contains(&entry.location)
            })
            .collect();
        self.selected_store = selected_location
            .and_then(|location| {
                store_rows(&self.topology, &self.expanded_stores)
                    .iter()
                    .position(|row| self.topology[row.topology_index].location == location)
            })
            .unwrap_or(0);
        self.selected_history = 0;
        self.selected_action = 0;
        self.reset_lineage();
        self.clamp_scrolls();
    }

    pub(crate) fn set_histories(&mut self, histories: Vec<HistoryGroup>) {
        let selected_id = self.selected_history().and_then(|row| {
            row.fact.and_then(|index| {
                self.selected_store_history()
                    .and_then(|group| group.facts.get(index).map(|fact| fact.id()))
            })
        });
        self.histories = histories;
        self.expanded_categories = self.history_categories();
        self.selected_history = selected_id
            .and_then(|id| {
                let group = self.selected_store_history()?;
                self.history_rows().iter().position(|row| {
                    row.fact
                        .and_then(|index| group.facts.get(index))
                        .is_some_and(|fact| fact.id() == id)
                })
            })
            .unwrap_or_else(|| {
                self.selected_history
                    .min(self.history_rows().len().saturating_sub(1))
            });
        self.selected_action = self
            .selected_action
            .min(self.actions().len().saturating_sub(1));
        self.reset_lineage();
        self.clamp_scrolls();
    }

    pub(crate) fn select_store_row(&mut self, row: usize) {
        if row < self.store_rows().len() {
            self.selected_store = row;
            self.selected_history = 0;
            self.selected_action = 0;
            self.history_scroll = 0;
            self.reset_lineage();
            self.clamp_scrolls();
        }
    }

    pub(crate) fn select_history_row(&mut self, row: usize) {
        if row < self.history_rows().len() {
            self.selected_history = row;
            self.selected_action = 0;
            self.reset_lineage();
            self.clamp_scrolls();
        }
    }

    pub(crate) fn select_action_row(&mut self, row: usize) {
        if row < self.actions().len() {
            self.selected_action = row;
            self.clamp_scrolls();
        }
    }

    fn reset_lineage(&mut self) {
        self.lineage_child = None;
        self.lineage_parent_child = None;
        self.lineage_relation_focus = false;
        self.lineage_relation = 0;
        self.lineage_scroll = 0;
    }

    fn clamp_scrolls(&mut self) {
        self.store_scroll = self
            .store_scroll
            .min(self.store_rows().len().saturating_sub(1) as u16);
        self.history_scroll = self
            .history_scroll
            .min(self.history_rows().len().saturating_sub(1) as u16);
        self.lineage_scroll = self
            .lineage_scroll
            .min(self.lineage_content_width().saturating_sub(1) as u16);
        let details_max = self
            .actions()
            .len()
            .saturating_add(12)
            .min(usize::from(u16::MAX));
        self.details_scroll = self.details_scroll.min(details_max as u16);
    }
}

fn lineage_rows_for_selection(
    group: &HistoryGroup,
    category: HistoryCategory,
    selected_index: usize,
) -> Vec<LineageRow> {
    let rows = category_fact_rows(group, category);
    let Some(container_index) = lineage_container_index(group, category, selected_index, &rows)
    else {
        return rows
            .into_iter()
            .filter(|(index, _, _, _, _)| *index == selected_index)
            .map(|(fact_index, depth, head, tail, number)| LineageRow {
                fact_index,
                depth,
                head,
                tail,
                number,
            })
            .collect();
    };
    let mut lineage = Vec::new();
    let mut active = false;
    for (fact_index, depth, head, tail, number) in rows {
        if fact_index == container_index {
            active = true;
        } else if active
            && group
                .facts
                .get(fact_index)
                .is_some_and(|fact| is_history_container(category, *fact))
        {
            break;
        }
        if active {
            lineage.push(LineageRow {
                fact_index,
                depth,
                head,
                tail,
                number,
            });
        }
    }
    lineage
}

fn lineage_visual_rows_from_rows(rows: Vec<LineageRow>) -> Vec<LineageRow> {
    let Some((first, rest)) = rows.split_first() else {
        return rows;
    };
    if first.depth == 1 {
        let mut visual = Vec::with_capacity(rows.len());
        visual.push(*first);
        visual.extend(rest.iter().rev().copied());
        visual
    } else {
        rows.into_iter().rev().collect()
    }
}

fn lineage_first_node(
    group: &HistoryGroup,
    category: HistoryCategory,
    container: usize,
) -> Option<usize> {
    lineage_visual_rows_from_rows(lineage_rows_for_selection(group, category, container))
        .into_iter()
        .find(|row| {
            group.facts.get(row.fact_index).is_some_and(|fact| {
                matches!(fact, Fact::Layer(_) | Fact::Stack(_) | Fact::Commit(_))
            })
        })
        .map(|row| row.fact_index)
}

fn fact_index_by_id(group: &HistoryGroup, id: &[u8]) -> Option<usize> {
    group
        .facts
        .iter()
        .enumerate()
        .find(|(_, fact)| !matches!(fact, Fact::AddResult(_)) && fact.id().as_slice() == id)
        .map(|(index, _)| index)
}

fn lineage_base_for_container(group: &HistoryGroup, container_index: usize) -> Option<LineageBase> {
    let (base_id, category) = match group.facts.get(container_index).copied()? {
        Fact::StackHistory(history) => (
            history.base_layer_id.to_bytes().to_vec(),
            HistoryCategory::Layers,
        ),
        Fact::Branch(branch) => {
            let category = match group
                .facts
                .iter()
                .find(|fact| fact.id().as_slice() == branch.base_id.as_slice())
            {
                Some(Fact::Layer(_)) => HistoryCategory::Layers,
                Some(Fact::Stack(_)) => HistoryCategory::Stacks,
                _ => return None,
            };
            (branch.base_id.as_slice().to_vec(), category)
        }
        _ => return None,
    };
    let fact_index = fact_index_by_id(group, &base_id)?;
    let number = category_fact_rows(group, category)
        .into_iter()
        .find(|(index, _, _, _, _)| *index == fact_index)
        .map(|(_, _, _, _, number)| number)?;
    Some(LineageBase { fact_index, number })
}

fn lineage_base_prefix_width(group: &HistoryGroup, base: LineageBase) -> usize {
    let Some(fact) = group.facts.get(base.fact_index).copied() else {
        return 0;
    };
    let Some(node) = lineage_node_width(fact, base.number) else {
        return 0;
    };
    node + " ──base──▶ ".chars().count()
}

fn fact_relations(group: &HistoryGroup, index: usize) -> Vec<LineageRelation> {
    match group.facts.get(index).copied() {
        Some(Fact::Layer(layer)) => layer_relations(group, index, layer.id.to_bytes().as_slice()),
        Some(Fact::StackHistory(_)) => stack_history_relations(group, index),
        Some(Fact::Stack(stack)) => stack_relations(group, index, stack.id.to_bytes().as_slice()),
        Some(Fact::Branch(branch)) => branch_relations(group, index, branch.base_id.as_slice()),
        _ => Vec::new(),
    }
}

fn layer_relations(group: &HistoryGroup, source: usize, layer_id: &[u8]) -> Vec<LineageRelation> {
    let mut relations = Vec::new();
    for (index, fact) in group.facts.iter().copied().enumerate() {
        match fact {
            Fact::StackHistory(history)
                if history.base_layer_id.to_bytes().as_slice() == layer_id =>
            {
                relations.push(LineageRelation {
                    source,
                    target: index,
                    kind: RelationKind::Instantiates,
                });
            }
            _ => {}
        }
    }
    if let Some(target) = group.facts.iter().find_map(|fact| match *fact {
        Fact::AddResult(result) if result.result_id.as_slice() == layer_id => {
            fact_index_by_id(group, result.source_id.as_slice())
        }
        _ => None,
    }) {
        relations.push(LineageRelation {
            source,
            target,
            kind: RelationKind::CreatedBy,
        });
    }
    relations
}

fn stack_history_relations(group: &HistoryGroup, source: usize) -> Vec<LineageRelation> {
    let Some(Fact::StackHistory(history)) = group.facts.get(source).copied() else {
        return Vec::new();
    };
    let mut relations = Vec::new();
    if let Some(target) = fact_index_by_id(group, history.base_layer_id.to_bytes().as_slice()) {
        relations.push(LineageRelation {
            source,
            target,
            kind: RelationKind::Base,
        });
    }
    for (index, fact) in group.facts.iter().copied().enumerate() {
        let Fact::Stack(stack) = fact else {
            continue;
        };
        if stack.history_id != history.id {
            continue;
        }
        relations.extend(stack_outputs(group, index, stack.id.to_bytes().as_slice()));
        relations.extend(
            group
                .facts
                .iter()
                .enumerate()
                .filter_map(|(target, fact)| match *fact {
                    Fact::Branch(branch)
                        if branch.base_id.as_slice() == stack.id.to_bytes().as_slice() =>
                    {
                        Some(LineageRelation {
                            source: index,
                            target,
                            kind: RelationKind::Instantiates,
                        })
                    }
                    _ => None,
                }),
        );
    }
    relations
}

fn stack_relations(group: &HistoryGroup, source: usize, stack_id: &[u8]) -> Vec<LineageRelation> {
    let mut relations = Vec::new();
    let history = group.facts.iter().find_map(|fact| match *fact {
        Fact::StackHistory(history) => {
            group
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Stack(stack) if stack.history_id == history.id && stack.id.to_bytes().as_slice() == stack_id))
                .then_some(history)
        }
        _ => None,
    });
    if let Some(history) = history {
        if let Some(target) = fact_index_by_id(group, history.base_layer_id.to_bytes().as_slice()) {
            relations.push(LineageRelation {
                source,
                target,
                kind: RelationKind::Base,
            });
        }
    }
    relations.extend(stack_outputs(group, source, stack_id));
    relations.extend(
        group
            .facts
            .iter()
            .enumerate()
            .filter_map(|(target, fact)| match *fact {
                Fact::Branch(branch) if branch.base_id.as_slice() == stack_id => {
                    Some(LineageRelation {
                        source,
                        target,
                        kind: RelationKind::Instantiates,
                    })
                }
                _ => None,
            }),
    );
    relations
}

fn stack_outputs(group: &HistoryGroup, source: usize, stack_id: &[u8]) -> Vec<LineageRelation> {
    group
        .facts
        .iter()
        .filter_map(|fact| match *fact {
            Fact::AddResult(result) if result.source_id.as_slice() == stack_id => {
                fact_index_by_id(group, result.result_id.as_slice()).map(|target| LineageRelation {
                    source,
                    target,
                    kind: RelationKind::Produces,
                })
            }
            _ => None,
        })
        .collect()
}

fn branch_relations(group: &HistoryGroup, source: usize, base_id: &[u8]) -> Vec<LineageRelation> {
    fact_index_by_id(group, base_id)
        .map(|target| {
            vec![LineageRelation {
                source,
                target,
                kind: RelationKind::Base,
            }]
        })
        .unwrap_or_default()
}

struct ChainContainer {
    index: usize,
    key: Vec<u8>,
    head: Option<Vec<u8>>,
}

struct ChainRecord {
    index: usize,
    parent: Option<Vec<u8>>,
    container: Vec<u8>,
}

fn category_fact_rows(
    group: &HistoryGroup,
    category: HistoryCategory,
) -> Vec<(usize, usize, bool, bool, usize)> {
    let mut containers = Vec::new();
    let mut records = std::collections::BTreeMap::<Vec<u8>, ChainRecord>::new();
    let mut ungrouped = Vec::new();

    for (index, fact) in group.facts.iter().copied().enumerate() {
        match (category, fact) {
            (HistoryCategory::Layers, Fact::LayerHistory(value)) => {
                containers.push(ChainContainer {
                    index,
                    key: value.id.to_bytes().to_vec(),
                    head: Some(value.head_layer_id.to_bytes().to_vec()),
                })
            }
            (HistoryCategory::Layers, Fact::Layer(value)) => {
                let id = value.id.to_bytes().to_vec();
                records.insert(
                    id.clone(),
                    ChainRecord {
                        index,
                        parent: value.parent_id.map(|id| id.to_bytes().to_vec()),
                        container: value.history_id.to_bytes().to_vec(),
                    },
                );
            }
            (HistoryCategory::Stacks, Fact::StackHistory(value)) => {
                containers.push(ChainContainer {
                    index,
                    key: value.id.to_bytes().to_vec(),
                    head: Some(value.head_stack_id.to_bytes().to_vec()),
                })
            }
            (HistoryCategory::Stacks, Fact::Stack(value)) => {
                let id = value.id.to_bytes().to_vec();
                records.insert(
                    id.clone(),
                    ChainRecord {
                        index,
                        parent: value.parent_id.map(|id| id.to_bytes().to_vec()),
                        container: value.history_id.to_bytes().to_vec(),
                    },
                );
            }
            (HistoryCategory::Branches, Fact::Branch(value)) => containers.push(ChainContainer {
                index,
                key: value.id.to_bytes().to_vec(),
                head: Some(value.head_commit_id.to_bytes().to_vec()),
            }),
            (HistoryCategory::Branches, Fact::Commit(value)) => {
                let id = value.id.to_bytes().to_vec();
                records.insert(
                    id.clone(),
                    ChainRecord {
                        index,
                        parent: value.parent_id.map(|id| id.to_bytes().to_vec()),
                        container: Vec::new(),
                    },
                );
                ungrouped.push(index);
            }
            _ => {}
        }
    }

    let mut rows = Vec::new();
    let mut displayed = std::collections::BTreeSet::new();
    for container in containers {
        rows.push((container.index, 1, false, true, 0));
        let Some(mut current) = container.head else {
            continue;
        };
        let mut depth = 2;
        let mut seen = std::collections::BTreeSet::new();
        let mut chain = Vec::new();
        while let Some(record) = records.get(&current) {
            if (!record.container.is_empty() && record.container != container.key)
                || !seen.insert(record.index)
            {
                break;
            }
            chain.push((record.index, depth, depth == 2));
            displayed.insert(record.index);
            depth += 1;
            let Some(parent) = record.parent.clone() else {
                break;
            };
            current = parent;
        }
        let chain_len = chain.len();
        rows.extend(
            chain
                .into_iter()
                .enumerate()
                .map(|(position, (index, depth, head))| {
                    (
                        index,
                        depth,
                        head,
                        position + 1 == chain_len,
                        chain_len.saturating_sub(position),
                    )
                }),
        );
        if category != HistoryCategory::Branches {
            for record in records.values() {
                if record.container == container.key && !displayed.contains(&record.index) {
                    rows.push((record.index, 2, false, true, 1));
                    displayed.insert(record.index);
                }
            }
        }
    }
    if category == HistoryCategory::Branches {
        rows.extend(
            ungrouped
                .into_iter()
                .filter(|index| !displayed.contains(index))
                .map(|index| (index, 2, false, true, 1)),
        );
    }
    rows
}

fn lineage_container_index(
    group: &HistoryGroup,
    category: HistoryCategory,
    selected_index: usize,
    rows: &[(usize, usize, bool, bool, usize)],
) -> Option<usize> {
    let selected = group.facts.get(selected_index).copied()?;
    match (category, selected) {
        (HistoryCategory::Layers, Fact::LayerHistory(_))
        | (HistoryCategory::Stacks, Fact::StackHistory(_))
        | (HistoryCategory::Branches, Fact::Branch(_)) => Some(selected_index),
        (HistoryCategory::Layers, Fact::Layer(layer)) => group.facts.iter().position(
            |fact| matches!(fact, Fact::LayerHistory(history) if history.id == layer.history_id),
        ),
        (HistoryCategory::Stacks, Fact::Stack(stack)) => group.facts.iter().position(
            |fact| matches!(fact, Fact::StackHistory(history) if history.id == stack.history_id),
        ),
        (HistoryCategory::Branches, Fact::Commit(_)) => {
            let mut container = None;
            for (fact_index, _, _, _, _) in rows {
                if *fact_index == selected_index {
                    return container;
                }
                if group
                    .facts
                    .get(*fact_index)
                    .is_some_and(|fact| is_history_container(category, *fact))
                {
                    container = Some(*fact_index);
                }
            }
            container
        }
        _ => None,
    }
}

fn is_history_container(category: HistoryCategory, fact: Fact) -> bool {
    match category {
        HistoryCategory::Layers => matches!(fact, Fact::LayerHistory(_)),
        HistoryCategory::Stacks => matches!(fact, Fact::StackHistory(_)),
        HistoryCategory::Branches => matches!(fact, Fact::Branch(_)),
    }
}

fn lineage_node_width(fact: Fact, number: usize) -> Option<usize> {
    let label = match fact {
        Fact::Layer(_) => "layer",
        Fact::Stack(_) => "stack",
        Fact::Commit(_) => "commit",
        _ => return None,
    };
    Some(label.len() + number.to_string().len() + 2)
}

fn store_rows(entries: &[TopologyEntry], expanded: &[bool]) -> Vec<StoreRow> {
    let Some(root) = entries
        .iter()
        .position(|entry| entry.role == "layerstore" && entry.parent.is_none())
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    append_store_rows(entries, expanded, root, 0, true, &mut rows);
    rows
}

fn append_store_rows(
    entries: &[TopologyEntry],
    expanded: &[bool],
    index: usize,
    depth: usize,
    last: bool,
    rows: &mut Vec<StoreRow>,
) {
    rows.push(StoreRow {
        topology_index: index,
        depth,
        last,
    });
    if !expanded.get(index).copied().unwrap_or(true) {
        return;
    }
    let location = &entries[index].location;
    let children = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.parent.as_deref() == Some(location.as_str()))
        .map(|(child, _)| child)
        .collect::<Vec<_>>();
    let child_count = children.len();
    for (position, child) in children.into_iter().enumerate() {
        append_store_rows(
            entries,
            expanded,
            child,
            depth + 1,
            position + 1 == child_count,
            rows,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{App, Direction, Focus};

    #[test]
    fn arrows_follow_the_panel_grid() {
        let mut app = App::default();
        assert_eq!(app.focus(), Focus::Stores);
        app.focus_direction(Direction::Down);
        assert_eq!(app.focus(), Focus::Histories);
        app.focus_direction(Direction::Right);
        assert_eq!(app.focus(), Focus::Lineage);
        app.focus_direction(Direction::Up);
        assert_eq!(app.focus(), Focus::Details);
        app.focus_direction(Direction::Left);
        assert_eq!(app.focus(), Focus::Stores);
    }
}
