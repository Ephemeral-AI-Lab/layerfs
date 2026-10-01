//! Exact scoped canonical overlaps, separate from native/global/provider memory.
use super::{count_state, release_state};
use layerfs_content::filesystem::{references, state, update};
use state::{
    BaseFact, CountLedger, CountPage, CountRecord, GraphMemory, StatePage, StateRecord, ZeroLedger,
    ZeroPage,
};

pub(crate) const COUNT_PERSISTENT: usize =
    std::mem::size_of::<count_state::Counts>() + std::mem::size_of::<count_state::CountsOwner>();
pub(crate) const COUNT_RETIRE_PROOF: usize = std::mem::size_of::<count_state::RetireProof>()
    + std::mem::size_of::<count_state::RetireOwner>();
const RELEASE_FOLD: usize =
    std::mem::size_of::<release_state::FoldOwner>() + std::mem::size_of::<ZeroLedger>();
pub(crate) const RELEASE_PERSISTENT: usize = std::mem::size_of::<release_state::Release>()
    + std::mem::size_of::<release_state::ReleaseOwner>()
    + RELEASE_FOLD;
const COUNT_FIXED_ATTEMPT: usize = std::mem::size_of::<count_state::AttemptData>()
    + std::mem::size_of::<count_state::CountAttempt>();
const RELEASE_FIXED_ATTEMPT: usize = std::mem::size_of::<release_state::AttemptData>()
    + std::mem::size_of::<release_state::ReleaseAttempt>();
// These borrowed SQL verification buffers are leased one component at a time.
const VERIFY: usize = 228 + 2 * 294 + 579 + 12 * std::mem::size_of::<u64>();
const BASE: usize = GraphMemory::control_bytes()
    + std::mem::size_of::<super::graph_state::Graph>()
    + super::graph_layout::ALIAS_PERSISTENT
    + super::graph_layout::FACT_PERSISTENT;
const CONTROL: usize = update::canonical_reduction_working_bytes();
const ROOTS_OPEN: usize = state::directory_roots_working_bytes(128);
const ROOTS_SEALED: usize = state::directory_roots_working_bytes(0);
const COUNT64: usize = std::mem::size_of::<CountPage>() + 64 * std::mem::size_of::<CountRecord>();
const ZERO64: usize = std::mem::size_of::<ZeroPage>() + 64 * std::mem::size_of::<BaseFact>();
const ZERO128: usize = std::mem::size_of::<ZeroPage>() + 128 * std::mem::size_of::<BaseFact>();
const ANSWERS64: usize =
    std::mem::size_of::<Vec<Option<layerfs_content::object::inode_leaf::InodeValue>>>()
        + 64 * std::mem::size_of::<Option<layerfs_content::object::inode_leaf::InodeValue>>();
const PARENTS64: usize = std::mem::size_of::<Vec<u64>>() + 64 * std::mem::size_of::<u64>();
const RETAINED: usize = ANSWERS64 + PARENTS64;
const FACT64: usize = std::mem::size_of::<super::fact_state::FactAttemptData>()
    + std::mem::size_of::<super::fact_state::FactAttempt>()
    + 64 * std::mem::size_of::<BaseFact>();
const COUNT_POINT: usize =
    COUNT_FIXED_ATTEMPT + std::mem::size_of::<(Option<CountRecord>, Option<CountRecord>)>();
const COUNT_ZERO64: usize =
    COUNT_FIXED_ATTEMPT + 64 * std::mem::size_of::<(Option<BaseFact>, Option<BaseFact>)>();
const COUNT_RETIRE128: usize =
    COUNT_FIXED_ATTEMPT + COUNT_RETIRE_PROOF + 128 * std::mem::size_of::<CountRecord>();
const RELEASE128: usize = RELEASE_FIXED_ATTEMPT + 128 * std::mem::size_of::<state::ReleaseJob>();
const RELEASE64: usize = RELEASE_FIXED_ATTEMPT + 64 * std::mem::size_of::<state::ReleaseJob>();
const fn maximum(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}

pub(crate) const DIRECTORY: usize = BASE
    + COUNT_PERSISTENT
    + CONTROL
    + ROOTS_OPEN
    + RETAINED
    + update::canonical_directory_working_bytes(64)
    + PARENTS64
    + ANSWERS64
    + maximum(
        references::canonical_base_working_bytes() + FACT64,
        COUNT_POINT,
    )
    + VERIFY;
pub(crate) const ROOT_SCAN: usize = BASE
    + COUNT_PERSISTENT
    + CONTROL
    + ROOTS_SEALED
    + RETAINED
    + state::directory_root_cursor_working_bytes()
    + std::mem::size_of::<StatePage>()
    + 64 * std::mem::size_of::<StateRecord>()
    + PARENTS64
    + ANSWERS64
    + maximum(
        references::canonical_base_working_bytes() + FACT64,
        COUNT_POINT,
    )
    + VERIFY;
pub(crate) const ZERO_SCAN: usize = BASE
    + COUNT_PERSISTENT
    + CONTROL
    + ROOTS_SEALED
    + references::canonical_zero_working_bytes()
    + maximum(
        COUNT64 + maximum(FACT64, COUNT_ZERO64),
        maximum(
            std::mem::size_of::<ZeroLedger>() + ZERO128,
            std::mem::size_of::<ZeroLedger>()
                + 128 * std::mem::size_of::<BaseFact>()
                + COUNT_FIXED_ATTEMPT,
        ),
    )
    + VERIFY;
pub(crate) const RELEASE: usize = BASE
    + COUNT_PERSISTENT
    + RELEASE_PERSISTENT
    + CONTROL
    + ROOTS_SEALED
    + references::canonical_release_working_bytes()
    + maximum(
        ZERO64 + RELEASE64 + RELEASE_FOLD,
        maximum(FACT64, maximum(RELEASE64, COUNT_POINT)),
    )
    + VERIFY;
pub(crate) const FINAL: usize = BASE
    + COUNT_PERSISTENT
    + RELEASE_PERSISTENT
    + CONTROL
    + ROOTS_SEALED
    + maximum(
        std::mem::size_of::<CountLedger>()
            + 128 * std::mem::size_of::<CountRecord>()
            + COUNT_FIXED_ATTEMPT,
        references::canonical_final_working_bytes() + COUNT64 + FACT64,
    )
    + VERIFY;
pub(crate) const FACT_RETIRE: usize = BASE
    + COUNT_PERSISTENT
    + RELEASE_PERSISTENT
    + CONTROL
    + ROOTS_SEALED
    + super::graph_layout::FACT_RETIRE_WINDOW
    + VERIFY;
pub(crate) const COUNT_RETIRE: usize = BASE
    + COUNT_PERSISTENT
    + COUNT_RETIRE_PROOF
    + RELEASE_PERSISTENT
    + CONTROL
    + ROOTS_SEALED
    + COUNT_RETIRE128
    + VERIFY;
pub(crate) const ROOT_RETIRE: usize = BASE
    + COUNT_PERSISTENT
    + COUNT_RETIRE_PROOF
    + RELEASE_PERSISTENT
    + super::graph_layout::ROOT_RETIRE_WINDOW
    + VERIFY;
pub(crate) const NATIVE_RELEASE: usize =
    BASE + COUNT_PERSISTENT + RELEASE_PERSISTENT + RELEASE128 + RELEASE_FOLD + VERIFY;
const _: () = {
    assert!(std::mem::size_of::<CountRecord>() >= std::mem::size_of::<BaseFact>());
    assert!(DIRECTORY <= state::GRAPH_WORKING_BYTES);
    assert!(ROOT_SCAN <= state::GRAPH_WORKING_BYTES);
    assert!(ZERO_SCAN <= state::GRAPH_WORKING_BYTES);
    assert!(RELEASE <= state::GRAPH_WORKING_BYTES);
    assert!(FINAL <= state::GRAPH_WORKING_BYTES);
    assert!(FACT_RETIRE <= state::GRAPH_WORKING_BYTES);
    assert!(COUNT_RETIRE <= state::GRAPH_WORKING_BYTES);
    assert!(ROOT_RETIRE <= state::GRAPH_WORKING_BYTES);
    assert!(NATIVE_RELEASE <= state::GRAPH_WORKING_BYTES);
};
