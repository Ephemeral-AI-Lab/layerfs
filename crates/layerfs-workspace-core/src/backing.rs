use std::any::Any;
use std::sync::Arc;

/// A segment identity is scoped to one backing owner / mount lifetime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackingId(pub u64);

struct Backing {
    id: BackingId,
    resource: Box<dyn Any + Send + Sync>,
}

/// One-word ownership retained by live pieces, frozen inputs and old read plans.
/// The adapter supplies physical storage or charged pending bytes; the core
/// neither reads the resource nor drops it before its last owner releases it.
#[derive(Clone)]
pub struct BackingRef(Arc<Backing>);

impl BackingRef {
    pub fn new(id: BackingId, resource: impl Any + Send + Sync) -> Self {
        Self(Arc::new(Backing {
            id,
            resource: Box::new(resource),
        }))
    }

    pub fn id(&self) -> BackingId {
        self.0.id
    }

    /// Adapter-only access; a mismatched placement cannot yield a resource.
    pub fn resource<T: Any>(&self) -> Option<&T> {
        self.0.resource.downcast_ref()
    }

    /// Retirement is valid only under the adapter's exclusive registry access.
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }

    /// Adapter retirement accounting: how many owners currently retain this
    /// segment (registry, live pieces, frozen frontiers, read plans). The
    /// core never interprets the count; adapters use it to decide when a
    /// failed reservation's segment is physically unreachable.
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl PartialEq for BackingRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for BackingRef {}

impl std::fmt::Debug for BackingRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BackingRef").field(&self.id()).finish()
    }
}

#[cfg(test)]
pub(crate) fn test_backing() -> BackingRef {
    static BACKING: std::sync::OnceLock<BackingRef> = std::sync::OnceLock::new();
    BACKING
        .get_or_init(|| BackingRef::new(BackingId(1), ()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_edit::{Piece, PieceTree};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn old_range_retains_resource_after_live_tree_and_registry_drop() {
        struct Resource(Arc<AtomicUsize>);
        impl Drop for Resource {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicUsize::new(0));
        let backing = BackingRef::new(BackingId(1), Resource(dropped.clone()));
        assert_eq!(
            std::mem::size_of::<BackingRef>(),
            std::mem::size_of::<usize>()
        );
        assert!(backing.is_unique());
        assert_ne!(backing, BackingRef::new(BackingId(1), ()));
        assert!(backing.resource::<Vec<u8>>().is_none());
        let tree = PieceTree::empty()
            .replace(
                0,
                0,
                [Piece::Spool {
                    segment: backing.clone(),
                    offset: 0,
                    len: 8,
                }],
            )
            .unwrap();
        let held = tree.range(2, 6).unwrap();
        assert!(!backing.is_unique());
        drop(tree);
        drop(backing);
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(held);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}
