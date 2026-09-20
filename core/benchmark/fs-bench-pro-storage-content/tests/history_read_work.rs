//! The read-work observer must preserve demand, method dispatch and typed errors.
use std::cell::Cell;

use fs_bench_storage_content::ops::history::reads::{ReadCounters, ReadWork};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_telemetry::timer::{Timing, TimingScope};

struct Provider {
    result: ContentResult<Vec<Vec<u8>>>,
    ordinary: Cell<u64>,
    scoped: Cell<u64>,
}

impl Provider {
    fn new(result: ContentResult<Vec<Vec<u8>>>) -> Self {
        Self {
            result,
            ordinary: Cell::new(0),
            scoped: Cell::new(0),
        }
    }
}

impl AuthenticatedObjects for Provider {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        assert_eq!(
            ids,
            &[ObjectId::for_bytes(b"abc"), ObjectId::for_bytes(b"de")]
        );
        self.ordinary.set(self.ordinary.get() + 1);
        self.result.clone()
    }

    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        _scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        assert_eq!(
            ids,
            &[ObjectId::for_bytes(b"abc"), ObjectId::for_bytes(b"de")]
        );
        self.scoped.set(self.scoped.get() + 1);
        self.result.clone()
    }
}

#[test]
fn read_observation_preserves_results_errors_dispatch_and_disabled_behavior() {
    let ids = [ObjectId::for_bytes(b"abc"), ObjectId::for_bytes(b"de")];
    let expected = Ok(vec![b"abc".to_vec(), b"de".to_vec()]);
    let observed = ReadWork::new(Provider::new(expected.clone()), true);
    assert_eq!(observed.read_canonical_batch(&ids), expected);
    let (scoped, _) = Timing::disabled("test", |scope| {
        observed.read_canonical_batch_scoped(&ids, scope.child("read"))
    });
    assert_eq!(scoped, expected);
    assert_eq!(observed.inner.ordinary.get(), 1);
    assert_eq!(observed.inner.scoped.get(), 1);
    let counts = observed.counters();
    assert_eq!(counts.waves, 2);
    assert_eq!(counts.requested_objects, 4);
    assert_eq!(counts.returned_objects, 4);
    assert_eq!(counts.returned_bytes, 10);
    assert_eq!(counts.failed_waves, 0);
    // Elapsed time is observational; there is no machine-dependent time threshold.
    for error in [
        ContentError::MissingObject,
        ContentError::ProviderFailure {
            what: "corrupt pack",
        },
        ContentError::IdentityMismatch,
    ] {
        let observed = ReadWork::new(Provider::new(Err(error.clone())), true);
        assert_eq!(observed.read_canonical_batch(&ids), Err(error.clone()));
        let (scoped, _) = Timing::disabled("test", |scope| {
            observed.read_canonical_batch_scoped(&ids, scope.child("read"))
        });
        assert_eq!(scoped, Err(error));
        assert_eq!(observed.counters().failed_waves, 2);
        assert_eq!(observed.counters().returned_objects, 0);
        assert_eq!(observed.counters().returned_bytes, 0);
    }
    let disabled = ReadWork::new(Provider::new(expected.clone()), false);
    assert_eq!(disabled.read_canonical_batch(&ids), expected);
    assert_eq!(disabled.counters(), ReadCounters::default());
    assert_eq!(disabled.inner.ordinary.get(), 1);
}
