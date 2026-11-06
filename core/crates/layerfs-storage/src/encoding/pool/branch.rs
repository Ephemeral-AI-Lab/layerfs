//! Tree pages stored against their predecessor: inode-table branches (#360
//! item 3), directory leaves and branches, and extent leaves and branches
//! (layerfs-project #92).
//!
//! A program page lives in the ordinary lane. Tag zero is the canonical page as
//! it always was. Tag one is the pooled COPY/INSERT grammar ([`super::delta`])
//! against one direct base page of the same role: `1 || base || output length ||
//! count || program`, the canonical page being the output. The locator's role
//! decides the interpretation, exactly as it does for a pooled leaf, so a Store
//! written before this form reads unchanged and a mixed chain of whole and delta
//! pages is ordinary. Every ordinary pack is now written at
//! [`crate::pack::layout::VERSION_ORDINARY_PAGES`], so a 2.0 reader, which
//! accepts tag one for inode-table branches only, refuses a pack that may hold a
//! directory or extent program by its version instead of failing inside a decode.
//!
//! A page chain is read like a pooled leaf chain: walked root to base through
//! the caller's caches, bounded by the metadata depth cap and charged record
//! bytes against the metadata chain budgets, then decoded base first, every
//! element authenticated against its own locator. Every element has the root's
//! role.

use layerfs_content::{ObjectId, ObjectRole};

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::pool::{delta, leaf, PoolReader};
use crate::error::{StorageError, StorageResult};
use crate::location::ObjectLocation;
use crate::policy::StorageCapacities;
use crate::source::Source;

use super::read::PooledCaches;

/// True for the roles whose records may be a COPY/INSERT program against a
/// direct base page of the same role.
pub const fn is_program_role(role: ObjectRole) -> bool {
    matches!(
        role,
        ObjectRole::InodeBranch
            | ObjectRole::DirectoryLeaf
            | ObjectRole::DirectoryBranch
            | ObjectRole::ExtentLeaf
            | ObjectRole::ExtentBranch
    )
}

/// Largest stored record of a program page: the whole tag and a canonical page
/// at the metadata record limit. A program is kept only when it is shorter
/// than the whole record, so it is never wider.
pub const PAGE_RECORD_LIMIT: usize = crate::policy::METADATA_RECORD_LIMIT + 1;

/// Record tag of a branch stored whole.
pub const BRANCH_FULL_TAG: u8 = 0;
/// Record tag of a branch stored against one direct base branch.
pub const BRANCH_DELTA_TAG: u8 = 1;

/// One parsed branch record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchRecord<'a> {
    /// The canonical page.
    Full(&'a [u8]),
    /// Instructions against one direct base branch.
    Delta {
        /// Direct base.
        base: ObjectId,
        /// Canonical length the program produces.
        output_length: usize,
        /// Instruction count.
        count: usize,
        /// Instruction program.
        instructions: &'a [u8],
    },
}

/// Parses one stored program-page record.
pub fn parse(record: &[u8]) -> StorageResult<BranchRecord<'_>> {
    match record.first() {
        Some(&BRANCH_FULL_TAG) => {
            let page = &record[1..];
            if page.is_empty() || record.len() > PAGE_RECORD_LIMIT {
                return Err(StorageError::Integrity("branch record width"));
            }
            Ok(BranchRecord::Full(page))
        }
        // The delta header and program are the pooled grammar; its parser
        // validates the program before anything is applied.
        Some(&BRANCH_DELTA_TAG) => match leaf::parse(record)? {
            leaf::PooledRecord::Delta {
                base,
                output_length,
                count,
                instructions,
            } => Ok(BranchRecord::Delta {
                base,
                output_length,
                count,
                instructions,
            }),
            leaf::PooledRecord::Full(_) => Err(StorageError::Integrity("branch record tag")),
        },
        _ => Err(StorageError::Integrity("branch record tag")),
    }
}

/// The direct base one stored branch record names, or `None` when whole.
pub fn stored_base(record: &[u8]) -> StorageResult<Option<ObjectId>> {
    Ok(match parse(record)? {
        BranchRecord::Full(_) => None,
        BranchRecord::Delta { base, .. } => Some(base),
    })
}

/// Work one page reconstruction charged: depth and the chain budgets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BranchChain {
    /// Dependency edges below the requested branch.
    pub depth: u8,
    /// Canonical bytes of every element, the requested one included.
    pub canonical: u64,
    /// Record bytes of every element, the requested one included.
    pub encoded: u64,
}

impl PoolReader {
    /// The authenticated canonical page of one stored program page.
    pub(crate) fn branch_canonical_with_caches(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
        caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<(Vec<u8>, BranchChain)> {
        let result =
            self.reconstruct_branch(connection, capacities, ceiling, workspace, root, caches);
        let counters = &mut self.counters;
        let inode = root.role == ObjectRole::InodeBranch;
        let (requests, edges) = match inode {
            true => (&mut counters.branch_requests, &mut counters.branch_chain_edges),
            false => (&mut counters.page_requests, &mut counters.page_chain_edges),
        };
        *requests = requests.saturating_add(1);
        if let Ok((_, chain)) = &result {
            *edges = edges.saturating_add(u64::from(chain.depth));
            counters.program_chain_canonical = counters
                .program_chain_canonical
                .saturating_add(chain.canonical);
            counters.program_chain_encoded =
                counters.program_chain_encoded.saturating_add(chain.encoded);
            counters.program_max_depth = counters.program_max_depth.max(u64::from(chain.depth));
        }
        result
    }

    fn reconstruct_branch(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
        mut caches: Option<PooledCaches<'_>>,
    ) -> StorageResult<(Vec<u8>, BranchChain)> {
        if !is_program_role(root.role) {
            return Err(StorageError::Integrity("branch record role"));
        }
        let cap = usize::from(capacities.metadata_delta_max_depth);
        let mut chain: Vec<(ObjectLocation, Vec<u8>)> = Vec::with_capacity(cap + 1);
        let mut current = root;
        loop {
            if current.pack_id > ceiling {
                return Err(StorageError::VisibilityCeiling {
                    pack_id: current.pack_id,
                    ceiling,
                });
            }
            if chain
                .iter()
                .any(|(row, _)| row.object_id == current.object_id)
            {
                return Err(StorageError::Integrity("branch dependency cycle"));
            }
            let record = self.record(connection, workspace, &current, caches.as_mut())?;
            let base = stored_base(&record)?;
            chain.push((current, record));
            let Some(base) = base else {
                break;
            };
            if chain.len() > cap {
                return Err(StorageError::Integrity("branch chain depth"));
            }
            // A base a stored record names is re-located, never answered by a
            // stale per-wave "absent" (F-036): a concurrent winner may have
            // published it after this wave first asked.
            let location = connection
                .base_location(base, ceiling)?
                .ok_or(StorageError::ObjectMissing(base))?;
            if location.role != root.role {
                return Err(StorageError::Integrity("branch chain role"));
            }
            if connection.ordered_dependencies()
                && (
                    location.pack_id,
                    location.group_number,
                    location.record_number,
                ) >= (current.pack_id, current.group_number, current.record_number)
            {
                return Err(StorageError::Integrity("branch chain chronology"));
            }
            current = location;
        }
        let mut work = BranchChain {
            depth: u8::try_from(chain.len() - 1)
                .map_err(|_| StorageError::Integrity("branch chain depth"))?,
            ..BranchChain::default()
        };
        let mut page: Option<(ObjectId, Vec<u8>)> = None;
        let mut applies = 0_u64;
        for (location, record) in chain.iter().rev() {
            work.canonical = work
                .canonical
                .saturating_add(location.canonical_length as u64);
            work.encoded = work.encoded.saturating_add(record.len() as u64);
            if work.canonical > capacities.metadata_chain_canonical_limit
                || work.encoded > capacities.metadata_chain_encoded_limit
            {
                return Err(StorageError::Integrity("branch chain work"));
            }
            let decoded = match parse(record)? {
                BranchRecord::Full(stored) => stored.to_vec(),
                BranchRecord::Delta {
                    base,
                    output_length,
                    count,
                    instructions,
                } => {
                    let (base_id, base_page) = page
                        .as_ref()
                        .ok_or(StorageError::Integrity("branch delta without base"))?;
                    if base != *base_id {
                        return Err(StorageError::Integrity("branch delta base identity"));
                    }
                    if output_length != location.canonical_length {
                        return Err(StorageError::Integrity("branch delta length"));
                    }
                    applies += 1;
                    delta::apply(base_page, instructions, count, output_length)?
                }
            };
            if decoded.len() != location.canonical_length {
                return Err(StorageError::Integrity("branch canonical length"));
            }
            if ObjectId::for_bytes(&decoded) != location.object_id {
                return Err(StorageError::Integrity("branch canonical identity"));
            }
            page = Some((location.object_id, decoded));
        }
        let applied = match root.role {
            ObjectRole::InodeBranch => &mut self.counters.branch_delta_applies,
            _ => &mut self.counters.page_delta_applies,
        };
        *applied = applied.saturating_add(applies);
        let (id, canonical) = page.ok_or(StorageError::Integrity("branch chain empty"))?;
        if id != root.object_id {
            return Err(StorageError::Integrity("branch canonical root"));
        }
        Ok((canonical, work))
    }
}
