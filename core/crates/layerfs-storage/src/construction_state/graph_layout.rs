//! Compiled graph working owner layouts and the normal maximum overlap.

use layerfs_content::filesystem::state::{
    AliasFact, BaseFact, FactLedger, FactPage, GraphBuildAck, GraphEdgeChange, GraphMemory,
    GraphMutationAck, GraphNode, GraphNodeChange, GraphPopAck, ParentFact, StateRecord,
    GRAPH_WORKING_BYTES,
};

use super::alias_state::{AliasAttempt, AliasAttemptData, Aliases, AliasesOwner};
pub(crate) use super::canonical_layout::{
    COUNT_PERSISTENT, COUNT_RETIRE_PROOF, RELEASE_PERSISTENT,
};
use super::fact_state::{FactAttempt, FactAttemptData, Facts, FactsOwner};
use super::graph_state::{Graph, GraphAttempt, GraphAttemptData, GraphAttemptKind, GraphOwner};
use super::root_retire::{RootAttemptData, RootRetirement};
use super::session::{Resource, ScratchSession};

/// Actual compiled layouts used by scoped graph admission; no RSS/native claim.
#[derive(Clone, Debug)]
pub struct GraphWorkingLayout {
    memory: GraphMemory,
    /// Fixed logical state/attempt/returned-result class.
    pub limit_bytes: usize,
    /// Fixed shared control allocation.
    pub control_bytes: usize,
    /// Boxed persistent graph layout, including its small attempt owner slot.
    pub graph_bytes: usize,
    /// Boxed full expected/proposed attempt payload.
    pub attempt_bytes: usize,
    /// Largest closed-kind metadata/wrapper/requested actual Vec capacities.
    pub maximum_attempt_bytes: usize,
    /// Actual full128 Seed class, excluding any inactive edge/solver windows.
    pub seed_attempt_bytes: usize,
    /// Actual128 closed Mutation class, including exact selected solver reads.
    pub mutation_attempt_bytes: usize,
    /// Small inline Box/lease owner wrapper.
    pub attempt_owner_bytes: usize,
    /// Small persistent Box/lease owner wrapper.
    pub graph_owner_bytes: usize,
    /// Full inline Resource layout, with no worst-case attempt arrays.
    pub resource_bytes: usize,
    /// Full caller session layout, including its inline Resource option.
    pub session_bytes: usize,
    /// Largest normal 128-record acknowledgement layout and vector allowance.
    pub maximum_ack_bytes: usize,
    /// Maximum successful root retirement metadata/transcript/row overlap.
    pub maximum_root_retirement_bytes: usize,
    /// Profile5 persistent alias payload plus actually charged owner wrapper.
    pub alias_persistent_bytes: usize,
    /// Profile5 boxed attempt and bounded actual128 change slots.
    pub alias_attempt_bytes: usize,
    /// Deferred graph plus persistent/attempt aliases and shared control.
    pub maximum_alias_phase_bytes: usize,
    /// Graph maximum attempt/ACK plus retained retired aliases and control.
    pub maximum_graph_phase_bytes: usize,
    /// Profile7 captured fixed fact/parent payload and charged owner wrapper.
    pub fact_persistent_bytes: usize,
    /// Actual C1 hot8 payload/control plus all its charged fixed slots.
    pub fact_hot_bytes: usize,
    /// Largest bounded Fact/Parent attempt payload, wrapper and actual128 slots.
    pub fact_attempt_bytes: usize,
    /// Maximum actual owned128-page/Vec allocation held by a consumer.
    pub maximum_fact_page_bytes: usize,
    /// Actual C1 prefetch64 arrays/results plus its simultaneous native attempt.
    pub maximum_fact_prefetch_bytes: usize,
    /// Complete prefetch phase, including the producer and simultaneously retained hot facts.
    pub maximum_namespace_prefetch_bytes: usize,
    /// Complete alias phase with the authenticated fact owner and producer.
    pub maximum_namespace_alias_bytes: usize,
    /// Maximum7 normalGraph attempt/ACK with persistent facts and hot8 owner.
    pub maximum_namespace_graph_bytes: usize,
    /// Actual hot/attempt/heldpage/fold overlap during fact/parent retirement.
    pub maximum_namespace_fact_bytes: usize,
    /// Parent result page/fold while canonical Roots already exist.
    pub maximum_namespace_roots_bytes: usize,
    /// Profile8 directory producer, roots append and immutable base wave.
    pub maximum_canonical_directory_bytes: usize,
    /// Profile8 nonborrowing Roots page while count/base answers coexist.
    pub maximum_canonical_root_scan_bytes: usize,
    /// Profile8 exact zero scan, held pages and producer attempts.
    pub maximum_canonical_zero_bytes: usize,
    /// Profile8 descendant cursor and same-fact base wave with seed custody.
    pub maximum_canonical_release_bytes: usize,
    /// Profile8 final inode consumer and its held count page.
    pub maximum_canonical_final_bytes: usize,
    /// Profile8 fact transcript and retirement while canonical owners coexist.
    pub maximum_canonical_fact_retirement_bytes: usize,
    /// Profile8 known/proposed count+zero retirement transcript and full128 rows.
    pub maximum_canonical_count_retirement_bytes: usize,
    /// Profile8 root retirement with all ended fixed canonical owner payloads.
    pub maximum_canonical_root_retirement_bytes: usize,
    /// Actual native128 release request with funded known/proposed seed folds.
    pub maximum_canonical_native_release_bytes: usize,
}

const MAXIMUM_ATTEMPT: usize = maximum(
    GraphAttemptKind::Seeds.allocation_bytes(),
    maximum(
        GraphAttemptKind::Append.allocation_bytes(),
        maximum(
            GraphAttemptKind::Mutation.allocation_bytes(),
            maximum(
                GraphAttemptKind::Pop.allocation_bytes(),
                GraphAttemptKind::Retire.allocation_bytes(),
            ),
        ),
    ),
);
const BUILD: usize =
    std::mem::size_of::<GraphBuildAck>() + 128 * std::mem::size_of::<GraphNodeChange>();
const MUTATION: usize =
    std::mem::size_of::<GraphMutationAck>() + 128 * std::mem::size_of::<GraphNodeChange>();
const POP: usize = std::mem::size_of::<GraphPopAck>() + 128 * std::mem::size_of::<GraphNode>();
const MAXIMUM_ACK: usize = maximum(maximum(BUILD, MUTATION), POP);
pub(crate) const ALIAS_PERSISTENT: usize =
    std::mem::size_of::<Aliases>() + std::mem::size_of::<AliasesOwner>();
const ALIAS_ATTEMPT: usize = std::mem::size_of::<AliasAttemptData>()
    + std::mem::size_of::<AliasAttempt>()
    + 128 * std::mem::size_of::<(Option<AliasFact>, Option<AliasFact>)>();
const ALIAS_PHASE: usize =
    GraphMemory::control_bytes() + std::mem::size_of::<Graph>() + ALIAS_PERSISTENT + ALIAS_ATTEMPT;
const GRAPH_PHASE: usize = GraphMemory::control_bytes()
    + std::mem::size_of::<Graph>()
    + ALIAS_PERSISTENT
    + MAXIMUM_ATTEMPT
    + MAXIMUM_ACK;
pub(crate) const FACT_PERSISTENT: usize =
    std::mem::size_of::<Facts>() + std::mem::size_of::<FactsOwner>();
const HOT_FACTS: usize = layerfs_content::filesystem::validate::validation_fact_working_bytes();
const FACT_WIDTH: usize = maximum(
    std::mem::size_of::<BaseFact>(),
    std::mem::size_of::<(Option<ParentFact>, Option<ParentFact>)>(),
);
const FACT_ATTEMPT: usize =
    std::mem::size_of::<FactAttemptData>() + std::mem::size_of::<FactAttempt>() + 128 * FACT_WIDTH;
const FACT_PAGE: usize = maximum(
    std::mem::size_of::<FactPage<BaseFact>>() + 128 * std::mem::size_of::<BaseFact>(),
    std::mem::size_of::<FactPage<ParentFact>>() + 128 * std::mem::size_of::<ParentFact>(),
);
const FACT_PREFETCH: usize =
    layerfs_content::filesystem::validate::validation_fact_prefetch_working_bytes()
        + std::mem::size_of::<FactAttemptData>()
        + std::mem::size_of::<FactAttempt>()
        + 64 * std::mem::size_of::<BaseFact>();
const RAW_PRODUCER: usize =
    layerfs_content::filesystem::validate::validation_fact_producer_working_bytes();
const PARENT_WINDOW: usize =
    layerfs_content::filesystem::validate::validation_parent_working_bytes();
const NAMESPACE_GRAPH: usize = GRAPH_PHASE + FACT_PERSISTENT + HOT_FACTS + RAW_PRODUCER;
const NAMESPACE_FACT: usize = GraphMemory::control_bytes()
    + std::mem::size_of::<Graph>()
    + ALIAS_PERSISTENT
    + FACT_PERSISTENT
    + HOT_FACTS
    + FACT_ATTEMPT
    + PARENT_WINDOW
    + FACT_PAGE
    + std::mem::size_of::<FactLedger>();
const NAMESPACE_PREFETCH: usize = GraphMemory::control_bytes()
    + std::mem::size_of::<Graph>()
    + ALIAS_PERSISTENT
    + FACT_PERSISTENT
    + HOT_FACTS
    + RAW_PRODUCER
    + FACT_PREFETCH;
const NAMESPACE_ALIAS: usize = ALIAS_PHASE + FACT_PERSISTENT + HOT_FACTS + RAW_PRODUCER;
pub(crate) const FACT_RETIRE_WINDOW: usize = FACT_ATTEMPT
    + FACT_PAGE
    + std::mem::size_of::<FactLedger>()
    + std::mem::size_of::<layerfs_content::filesystem::state::FactSeal>();
pub(crate) const ROOT_RETIRE_WINDOW: usize = ROOTS;
const ROOTS: usize = std::mem::size_of::<RootRetirement>()
    + std::mem::size_of::<RootAttemptData>()
    + 128 * std::mem::size_of::<StateRecord>();

const fn maximum(left: usize, right: usize) -> usize {
    if left > right {
        left
    } else {
        right
    }
}

const _: () = {
    // Mixed Build windows have combined capacity <=128; a NodeChange is widest.
    assert!(std::mem::size_of::<GraphNodeChange>() >= std::mem::size_of::<GraphEdgeChange>());
    assert!(
        GraphMemory::control_bytes() + std::mem::size_of::<Graph>() + MAXIMUM_ATTEMPT + MAXIMUM_ACK
            <= GRAPH_WORKING_BYTES
    );
    assert!(NAMESPACE_GRAPH <= GRAPH_WORKING_BYTES);
    assert!(NAMESPACE_FACT <= GRAPH_WORKING_BYTES);
    assert!(NAMESPACE_PREFETCH <= GRAPH_WORKING_BYTES);
    assert!(NAMESPACE_ALIAS <= GRAPH_WORKING_BYTES);
    assert!(
        GraphMemory::control_bytes()
            + std::mem::size_of::<Graph>()
            + ALIAS_PERSISTENT
            + FACT_PERSISTENT
            + ROOTS
            + PARENT_WINDOW
            + FACT_PAGE
            + std::mem::size_of::<FactLedger>()
            <= GRAPH_WORKING_BYTES
    );
    assert!(ALIAS_PHASE <= GRAPH_WORKING_BYTES);
    assert!(GRAPH_PHASE <= GRAPH_WORKING_BYTES);
    assert!(
        GraphMemory::control_bytes() + std::mem::size_of::<Graph>() + ALIAS_PERSISTENT + ROOTS
            <= GRAPH_WORKING_BYTES
    );
};

impl GraphWorkingLayout {
    pub(crate) fn observe(memory: &GraphMemory) -> Self {
        Self {
            memory: memory.clone(),
            limit_bytes: memory.limit(),
            control_bytes: GraphMemory::control_bytes(),
            graph_bytes: std::mem::size_of::<Graph>(),
            attempt_bytes: std::mem::size_of::<GraphAttemptData>(),
            maximum_attempt_bytes: MAXIMUM_ATTEMPT,
            seed_attempt_bytes: GraphAttemptKind::Seeds.allocation_bytes(),
            mutation_attempt_bytes: GraphAttemptKind::Mutation.allocation_bytes(),
            attempt_owner_bytes: std::mem::size_of::<GraphAttempt>(),
            graph_owner_bytes: std::mem::size_of::<GraphOwner>(),
            resource_bytes: std::mem::size_of::<Resource>(),
            session_bytes: std::mem::size_of::<ScratchSession>(),
            maximum_ack_bytes: MAXIMUM_ACK,
            maximum_root_retirement_bytes: ROOTS,
            alias_persistent_bytes: ALIAS_PERSISTENT,
            alias_attempt_bytes: ALIAS_ATTEMPT,
            maximum_alias_phase_bytes: ALIAS_PHASE,
            maximum_graph_phase_bytes: GRAPH_PHASE,
            fact_persistent_bytes: FACT_PERSISTENT,
            fact_hot_bytes: HOT_FACTS,
            fact_attempt_bytes: FACT_ATTEMPT,
            maximum_fact_page_bytes: FACT_PAGE,
            maximum_fact_prefetch_bytes: FACT_PREFETCH,
            maximum_namespace_prefetch_bytes: NAMESPACE_PREFETCH,
            maximum_namespace_alias_bytes: NAMESPACE_ALIAS,
            maximum_namespace_graph_bytes: NAMESPACE_GRAPH,
            maximum_namespace_fact_bytes: NAMESPACE_FACT,
            maximum_canonical_directory_bytes: super::canonical_layout::DIRECTORY,
            maximum_canonical_root_scan_bytes: super::canonical_layout::ROOT_SCAN,
            maximum_canonical_zero_bytes: super::canonical_layout::ZERO_SCAN,
            maximum_canonical_release_bytes: super::canonical_layout::RELEASE,
            maximum_canonical_final_bytes: super::canonical_layout::FINAL,
            maximum_canonical_fact_retirement_bytes: super::canonical_layout::FACT_RETIRE,
            maximum_canonical_count_retirement_bytes: super::canonical_layout::COUNT_RETIRE,
            maximum_canonical_root_retirement_bytes: super::canonical_layout::ROOT_RETIRE,
            maximum_canonical_native_release_bytes: super::canonical_layout::NATIVE_RELEASE,
            maximum_namespace_roots_bytes: GraphMemory::control_bytes()
                + std::mem::size_of::<Graph>()
                + ALIAS_PERSISTENT
                + FACT_PERSISTENT
                + ROOTS
                + PARENT_WINDOW
                + FACT_PAGE
                + std::mem::size_of::<FactLedger>(),
        }
    }

    /// Read current credit, including results held after the status was captured.
    pub fn reserved_bytes(&self) -> usize {
        self.memory.reserved_bytes()
    }
}
