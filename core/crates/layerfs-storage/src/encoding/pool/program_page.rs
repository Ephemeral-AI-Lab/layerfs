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
//! role: an authoritative locator's, or, for an element a chain hint located
//! (#758), the role its authenticated page header declares.

use layerfs_content::{ObjectId, ObjectRole};

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::pool::{delta, leaf, PoolReader};
use crate::error::{StorageError, StorageResult};
use crate::location::ObjectLocation;
use crate::policy::StorageCapacities;
use crate::source::Source;

use super::memo::Known;
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

/// The program role a canonical page's own header declares: the role of a
/// chain element whose locator came from a chain hint, which carries no role
/// (#758), checked once its bytes are authenticated.
fn declared_role(canonical: &[u8]) -> Option<ObjectRole> {
    use layerfs_content::filesystem::sorted::format as sorted;
    let value = layerfs_content::object::decode_bytes_object(canonical).ok()?;
    let (magic, role) = (value.get(..8)?, *value.get(10)?);
    if magic == sorted::DIRECTORY_MAGIC {
        return match role {
            sorted::DIRECTORY_LEAF_ROLE => Some(ObjectRole::DirectoryLeaf),
            sorted::DIRECTORY_BRANCH_ROLE => Some(ObjectRole::DirectoryBranch),
            _ => None,
        };
    }
    if magic == sorted::INODE_MAGIC {
        return (role == sorted::INODE_BRANCH_ROLE).then_some(ObjectRole::InodeBranch);
    }
    match layerfs_content::file::mapping::decode_node(canonical).ok()? {
        layerfs_content::file::mapping::ExtentNode::Leaf { .. } => Some(ObjectRole::ExtentLeaf),
        layerfs_content::file::mapping::ExtentNode::Branch { .. } => Some(ObjectRole::ExtentBranch),
    }
}

impl PoolReader {
    /// Whether the last program-page reconstruction failed at an element
    /// whose locator came from a chain hint, or in a walk such an element
    /// may have redirected (#758): only such a failure may be a stale hint's.
    pub(crate) fn hint_blamed(&self) -> bool {
        self.hint_blamed
    }

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
        let mut suspect = false;
        let result = self.reconstruct_branch(
            connection,
            capacities,
            ceiling,
            workspace,
            root,
            caches,
            &mut suspect,
        );
        self.hint_blamed = result.is_err() && suspect;
        let counters = &mut self.counters;
        let inode = root.role == ObjectRole::InodeBranch;
        let (requests, edges) = match inode {
            true => (
                &mut counters.branch_requests,
                &mut counters.branch_chain_edges,
            ),
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

    /// `suspect` is left true when a failure is attributable to an element
    /// a chain hint located, or to an element reached through one: every
    /// element below the first hinted one was named by an unauthenticated
    /// record (the hinted element's own, or one below it), so a failure while
    /// reading, binding or decoding any of them, or of a walk past the depth
    /// cap, may be the hint's. A failure above the first hinted element is
    /// never a hint's, except the chain work check: its totals include the
    /// elements below, so it is suspect whenever any element was hinted.
    #[allow(clippy::too_many_arguments)]
    fn reconstruct_branch(
        &mut self,
        connection: &dyn Source,
        capacities: &StorageCapacities,
        ceiling: i64,
        workspace: &mut DecompressionWorkspace,
        root: ObjectLocation,
        mut caches: Option<PooledCaches<'_>>,
        suspect: &mut bool,
    ) -> StorageResult<(Vec<u8>, BranchChain)> {
        if !is_program_role(root.role) {
            return Err(StorageError::Integrity("branch record role"));
        }
        let cap = usize::from(capacities.metadata_delta_max_depth);
        let mut chain: Vec<(ObjectLocation, Vec<u8>)> = Vec::with_capacity(cap + 1);
        // Whether each element of `chain` was located from a chain hint
        // (#758); the root never is.
        let mut hinted: Vec<bool> = Vec::with_capacity(cap + 1);
        let mut current_hinted = false;
        let mut current = root;
        // The element the walk stopped at because this reader already rebuilt
        // and authenticated it: the chain below it is not read again.
        let mut known: Option<(ObjectId, Known)> = None;
        loop {
            *suspect = current_hinted || hinted.contains(&true);
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
            let held = self
                .memo
                .as_mut()
                .and_then(|memo| memo.get(&current.object_id, root.role, ceiling));
            if let Some(held) = held {
                // The depth a full walk would have reached, under the same cap.
                if chain.len() + usize::from(held.depth) > cap {
                    *suspect = hinted.contains(&true);
                    return Err(StorageError::Integrity("branch chain depth"));
                }
                known = Some((current.object_id, held));
                break;
            }
            let record = self.record(connection, workspace, &current, caches.as_mut())?;
            let base = stored_base(&record)?;
            if let Some(base) = base {
                // Unless this walk binds members (a Save's check of its
                // bases), the deeper bases join this level's locate (#768).
                self.locate_ahead(connection, workspace, &current, base, caches.as_ref())?;
            }
            // Only a Save's check of its bases binds a program page's chain
            // member here (#758, `Fetch::bind`); every element below is still
            // hashed to the id its child's record names.
            connection.bind_base(&current, base)?;
            chain.push((current, record));
            hinted.push(current_hinted);
            let Some(base) = base else {
                break;
            };
            if chain.len() > cap {
                // Any hinted element's record may have named a wrong base.
                *suspect = hinted.contains(&true);
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
            // A hinted locator carries the root's role, not a checked one:
            // its element's role is checked from its bytes below.
            current_hinted = connection.hinted(location.object_id);
            current = location;
        }
        // A known element brings the work its own chain charged, so the
        // totals and the allowance checks below are those of a full walk.
        let below = known
            .as_ref()
            .map_or(0, |(_, held)| usize::from(held.depth) + 1);
        let mut work = BranchChain {
            depth: u8::try_from(chain.len() + below - 1)
                .map_err(|_| StorageError::Integrity("branch chain depth"))?,
            canonical: known.as_ref().map_or(0, |(_, held)| held.canonical),
            encoded: known.as_ref().map_or(0, |(_, held)| held.encoded),
        };
        let mut highest_pack = known.as_ref().map_or(0, |(_, held)| held.highest_pack);
        let mut page: Option<(ObjectId, Vec<u8>)> = known.map(|(id, held)| (id, held.bytes));
        let mut applies = 0_u64;
        // The first hinted element, root first: it and every element below
        // it were located through a record not yet authenticated.
        let first_hinted = hinted.iter().position(|from_hint| *from_hint);
        for (position, ((location, record), from_hint)) in
            chain.iter().zip(&hinted).rev().enumerate()
        {
            // Elements below this one are authenticated: a failure from here
            // to the next element is this one's, and a hint's when this one
            // is at or below the first hinted element.
            *suspect = first_hinted.is_some_and(|first| chain.len() - 1 - position >= first);
            work.canonical = work
                .canonical
                .saturating_add(location.canonical_length as u64);
            work.encoded = work.encoded.saturating_add(record.len() as u64);
            if work.canonical > capacities.metadata_chain_canonical_limit
                || work.encoded > capacities.metadata_chain_encoded_limit
            {
                // The totals include every element below this one, so a
                // hinted element's record or canonical length may be what
                // crossed the limit, even at an authoritative element above
                // it (fix 3b).
                *suspect = first_hinted.is_some();
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
            if *from_hint && declared_role(&decoded) != Some(root.role) {
                return Err(StorageError::Integrity("branch chain role"));
            }
            highest_pack = highest_pack.max(location.pack_id);
            if let Some(memo) = self.memo.as_mut() {
                // Authenticated just above, with the chain facts up to here.
                memo.insert(
                    location.object_id,
                    Known {
                        role: root.role,
                        bytes: decoded.clone(),
                        depth: u8::try_from(below + position)
                            .map_err(|_| StorageError::Integrity("branch chain depth"))?,
                        canonical: work.canonical,
                        encoded: work.encoded,
                        highest_pack,
                    },
                );
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
