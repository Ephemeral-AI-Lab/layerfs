//! Shared bounded traversal laws for canonical typed-object graphs.

use crate::format::{MAX_PATH_DEPTH, MAX_TREE_PAGE_DEPTH};
use crate::{CoreError, CoreResult};

use super::model::{StrongEdgeV1, StrongEdgeVisitorV1, TypedPhysicalObjectIdV1};

/// A canonical path may contain at most 256 child components. At every path
/// level, traversal may retain one directory frame, up to two index-page
/// frames, and one leaf/child frame. The extra root level covers the root
/// directory and its page chain without permitting a 257th path component.
pub(crate) const MAX_CANONICAL_TRAVERSAL_FRAMES_V1: usize =
    (MAX_PATH_DEPTH + 1) * (MAX_TREE_PAGE_DEPTH as usize + 2);

pub(crate) fn require_canonical_traversal_depth_v1(depth: usize) -> CoreResult<()> {
    if depth < MAX_CANONICAL_TRAVERSAL_FRAMES_V1 {
        Ok(())
    } else {
        Err(CoreError::CountCap)
    }
}

/// Operation-local depth accounting for an authenticated canonical object
/// walk. The object layer owns the cap and its checked push/pop law; a reader
/// may store any frame representation it needs without reimplementing the
/// bounded traversal policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalTraversalBudgetV1 {
    len: usize,
}

impl CanonicalTraversalBudgetV1 {
    pub(crate) const fn new() -> Self {
        Self { len: 0 }
    }

    pub(crate) const fn len(self) -> usize {
        self.len
    }

    pub(crate) fn push(&mut self) -> CoreResult<()> {
        require_canonical_traversal_depth_v1(self.len)?;
        self.len = self.len.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn pop(&mut self) -> Option<()> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(())
    }
}

/// Narrow operation-owned queue port for iterative strong-edge traversal.
///
/// The object layer owns the traversal loop and edge delivery. The caller
/// owns queue persistence, de-duplication, occupied-object decoding, and
/// typed failure retention behind this port.
pub(crate) trait StrongEdgeTraversalQueueV1 {
    fn enqueue_if_new_v1(&mut self, id: TypedPhysicalObjectIdV1) -> CoreResult<()>;
    fn pending_count_v1(&mut self) -> CoreResult<u32>;
    fn pending_id_v1(&mut self, ordinal: u32) -> CoreResult<TypedPhysicalObjectIdV1>;
    fn complete_pending_v1(&mut self, ordinal: u32, complete_len: u64) -> CoreResult<()>;
}

/// Adapter that gives the canonical decoder a transactional edge sink while
/// keeping the queue implementation outside the object module.
pub(crate) struct StrongEdgeCollectorV1<'queue, Q> {
    queue: &'queue mut Q,
}

impl<Q> StrongEdgeCollectorV1<'_, Q>
where
    Q: StrongEdgeTraversalQueueV1,
{
    pub(crate) fn new(queue: &mut Q) -> StrongEdgeCollectorV1<'_, Q> {
        StrongEdgeCollectorV1 { queue }
    }
}

impl<Q> StrongEdgeVisitorV1 for StrongEdgeCollectorV1<'_, Q>
where
    Q: StrongEdgeTraversalQueueV1,
{
    fn begin_object(&mut self) {}

    fn visit_edge(&mut self, edge: StrongEdgeV1) -> CoreResult<()> {
        self.queue.enqueue_if_new_v1(edge.typed_id())
    }

    fn commit_object(&mut self) {}

    fn abort_object(&mut self) {}
}

/// Iteratively decode and complete a bounded strong-edge closure.
///
/// The queue's count validation is performed by `enqueue_if_new_v1` before a
/// newly discovered object can reach the decode callback. Polling occurs once
/// per queued object, before any occupied-object length or payload read.
pub(crate) fn traverse_strong_edges_v1<Q, D, O, P, T>(
    queue: &mut Q,
    root: TypedPhysicalObjectIdV1,
    mut decode: D,
    mut observe: O,
    mut poll: P,
) -> CoreResult<()>
where
    Q: StrongEdgeTraversalQueueV1,
    D: FnMut(TypedPhysicalObjectIdV1, &mut StrongEdgeCollectorV1<'_, Q>) -> CoreResult<(u64, T)>,
    O: FnMut(u32, TypedPhysicalObjectIdV1, u64, T) -> CoreResult<()>,
    P: FnMut() -> CoreResult<()>,
{
    queue.enqueue_if_new_v1(root)?;
    let mut ordinal = 0_u32;
    while ordinal < queue.pending_count_v1()? {
        poll()?;
        let id = queue.pending_id_v1(ordinal)?;
        let (complete_len, decoded) = {
            let mut collector = StrongEdgeCollectorV1::new(queue);
            decode(id, &mut collector)?
        };
        queue.complete_pending_v1(ordinal, complete_len)?;
        observe(ordinal, id, complete_len, decoded)?;
        ordinal = ordinal.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{traverse_strong_edges_v1, StrongEdgeTraversalQueueV1};
    use crate::identity::PhysicalTreeIdV1;
    use crate::object::model::{StrongEdgeV1, StrongEdgeVisitorV1, TypedPhysicalObjectIdV1};
    use crate::{CoreError, CoreResult};

    #[derive(Default)]
    struct Queue {
        entries: Vec<(TypedPhysicalObjectIdV1, bool)>,
        capacity: usize,
    }

    impl Queue {
        fn with_capacity(capacity: usize) -> Self {
            Self {
                entries: Vec::new(),
                capacity,
            }
        }
    }

    impl StrongEdgeTraversalQueueV1 for Queue {
        fn enqueue_if_new_v1(&mut self, id: TypedPhysicalObjectIdV1) -> CoreResult<()> {
            if self.entries.iter().any(|(existing, _)| *existing == id) {
                return Ok(());
            }
            if self.entries.len() >= self.capacity {
                return Err(CoreError::CountCap);
            }
            self.entries.push((id, true));
            Ok(())
        }

        fn pending_count_v1(&mut self) -> CoreResult<u32> {
            u32::try_from(self.entries.len()).map_err(|_| CoreError::IntegerOverflow)
        }

        fn pending_id_v1(&mut self, ordinal: u32) -> CoreResult<TypedPhysicalObjectIdV1> {
            self.entries
                .get(usize::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?)
                .map(|(id, _)| *id)
                .ok_or(CoreError::RangeResyncFailed)
        }

        fn complete_pending_v1(&mut self, ordinal: u32, _complete_len: u64) -> CoreResult<()> {
            let entry = self
                .entries
                .get_mut(usize::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?)
                .ok_or(CoreError::RangeResyncFailed)?;
            if !entry.1 {
                return Err(CoreError::RangeResyncFailed);
            }
            entry.1 = false;
            Ok(())
        }
    }

    fn tree_id(byte: u8) -> PhysicalTreeIdV1 {
        PhysicalTreeIdV1::from_digest([byte; 32])
    }

    #[test]
    fn traversal_polls_and_completes_each_queued_object() {
        let root = TypedPhysicalObjectIdV1::Tree(tree_id(1));
        let child = tree_id(2);
        let mut queue = Queue::with_capacity(4);
        let mut decoded = 0_u8;
        let mut polled = 0_u8;
        let mut observed = Vec::new();

        traverse_strong_edges_v1(
            &mut queue,
            root,
            |id, visitor| {
                decoded += 1;
                if id == root {
                    visitor.visit_edge(StrongEdgeV1::Tree(child))?;
                }
                Ok((64, id))
            },
            |ordinal, id, complete_len, decoded_id| {
                observed.push((ordinal, id, complete_len, decoded_id));
                Ok(())
            },
            || {
                polled += 1;
                Ok(())
            },
        )
        .expect("the bounded iterative walk should finish");

        assert_eq!(decoded, 2);
        assert_eq!(polled, 2);
        assert_eq!(observed.len(), 2);
        assert_eq!(
            queue.entries.iter().filter(|(_, pending)| *pending).count(),
            0
        );
    }

    #[test]
    fn traversal_rejects_a_new_edge_before_decoding_it_when_queue_is_full() {
        let root = TypedPhysicalObjectIdV1::Tree(tree_id(1));
        let child = tree_id(2);
        let mut queue = Queue::with_capacity(1);
        let mut decoded = 0_u8;

        let error = traverse_strong_edges_v1(
            &mut queue,
            root,
            |id, visitor| {
                decoded += 1;
                if id == root {
                    visitor.visit_edge(StrongEdgeV1::Tree(child))?;
                }
                Ok((64, id))
            },
            |_ordinal, _id, _complete_len, _decoded| Ok(()),
            || Ok(()),
        )
        .expect_err("a depth/queue cap must win before child decode");

        assert_eq!(error, CoreError::CountCap);
        assert_eq!(decoded, 1);
    }

    #[test]
    fn traversal_polls_cancellation_before_the_next_object_read() {
        let root = TypedPhysicalObjectIdV1::Tree(tree_id(1));
        let child = tree_id(2);
        let mut queue = Queue::with_capacity(4);
        let mut decoded = 0_u8;
        let mut polls = 0_u8;

        let error = traverse_strong_edges_v1(
            &mut queue,
            root,
            |id, visitor| {
                decoded += 1;
                if id == root {
                    visitor.visit_edge(StrongEdgeV1::Tree(child))?;
                }
                Ok((64, id))
            },
            |_ordinal, _id, _complete_len, _decoded| Ok(()),
            || {
                polls += 1;
                if polls == 2 {
                    Err(CoreError::Cancelled)
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("cancellation must stop before the child decode");

        assert_eq!(error, CoreError::Cancelled);
        assert_eq!(decoded, 1);
        assert_eq!(polls, 2);
    }
}
