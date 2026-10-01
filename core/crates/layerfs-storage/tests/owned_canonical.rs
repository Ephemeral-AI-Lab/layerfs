//! Returned-data ownership only: real System requests and actual C2 lifetime.
//! Native/decode/cache/process qualification remains outside these proofs.
#[path = "support/allocation_observer.rs"]
mod allocation_observer;
mod support;
use layerfs_content::object::{
    CanonicalBudget, CanonicalBuffer, CanonicalOwnership, CanonicalReadKind, CanonicalReadPermit,
    OwnedCanonicalBatch, CANONICAL_COMPATIBILITY_BYTES,
};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{cas::OwnedReadProfile, StoreProvider};
use std::cell::Cell;
static CAPTURE_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
use support::{assembled_small_object, create_store, disabled, save_one, TempDir};
#[global_allocator]
static ALLOCATOR: allocation_observer::ObservedSystem = allocation_observer::ObservedSystem;

struct RawSupplier {
    calls: Cell<usize>,
}
impl AuthenticatedObjects for RawSupplier {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.calls.set(self.calls.get() + 1);
        Ok(ids.iter().map(|_| vec![1; 16]).collect())
    }
}

#[test]
fn output_permits_moves_copies_and_actual_data_release() {
    let _serial = CAPTURE_SERIAL.lock().unwrap();
    assert_eq!(CanonicalBudget::compatibility().limit(), 32 * 1024 * 1024);
    assert_eq!(CANONICAL_COMPATIBILITY_BYTES, 32 * 1024 * 1024);
    assert!(matches!(
        CanonicalBudget::new(CANONICAL_COMPATIBILITY_BYTES + 1),
        Err(ContentError::BoundedCapacityExceeded {
            what: "canonical.budget_limit",
            ..
        })
    ));
    let budget = CanonicalBudget::new(80).unwrap();
    let permit = CanonicalReadPermit::new(&budget, 1, 64, CanonicalReadKind::Any).unwrap();
    let observation =
        allocation_observer::Observation::start([64, 16], [usize::MAX, usize::MAX - 1]);
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&[7; 16]);
    let actual = bytes.capacity();
    let batch =
        OwnedCanonicalBatch::from_vectors(permit, vec![bytes], CanonicalOwnership::AdmittedOutput)
            .unwrap();
    assert_eq!(budget.used_bytes(), actual);
    let copy = batch.buffers()[0].try_clone().unwrap();
    assert_eq!(copy.as_slice(), &[7; 16]);
    assert_eq!(budget.used_bytes(), actual + copy.capacity());
    assert!(batch.buffers()[0].try_clone().is_err());
    let held = observation.stop();
    assert!(!held.overflow);
    assert_eq!(held.live[0], budget.used_bytes());
    let mut iterator = batch.into_iter();
    let retained = iterator.next().unwrap();
    drop(iterator);
    drop(copy);
    assert_eq!(budget.used_bytes(), retained.capacity());
    assert_eq!(observation.stop().live[0], retained.capacity());
    drop(retained);
    assert_eq!(budget.used_bytes(), 0);
    assert_eq!(
        observation.released().live_total(),
        0,
        "all actual data/descriptors freed"
    );

    let budget = CanonicalBudget::new(8 * 1024 * 1024).unwrap();
    let held = budget.reserve(budget.limit()).unwrap();
    let raw = RawSupplier {
        calls: Cell::new(0),
    };
    let refused = disabled(|scope| {
        let permit = CanonicalReadPermit::new(&budget, 1, 16, CanonicalReadKind::Any)?;
        raw.read_canonical_owned(&[ObjectId::for_bytes(b"one")], permit, scope.child("read"))
    });
    assert!(matches!(
        refused,
        Err(ContentError::ResourceUnavailable {
            what: "canonical.data_capacity"
        })
    ));
    assert_eq!(raw.calls.get(), 0, "refusal precedes supplier effects");
    drop(held);
    let batch = disabled(|scope| {
        raw.read_canonical_owned(
            &[ObjectId::for_bytes(b"one")],
            CanonicalReadPermit::new(&budget, 1, 16, CanonicalReadKind::Any)?,
            scope.child("read"),
        )
    })
    .unwrap();
    assert_eq!(batch.ownership(), CanonicalOwnership::Compatibility);
    assert_eq!(budget.used_bytes(), batch.buffers()[0].capacity());
    drop(batch);
    assert_eq!(budget.used_bytes(), 0);

    let budget = CanonicalBudget::new(64).unwrap();
    let permit = CanonicalReadPermit::new(&budget, 1, 8, CanonicalReadKind::Any).unwrap();
    assert!(OwnedCanonicalBatch::from_vectors(
        permit,
        vec![Vec::with_capacity(64)],
        CanonicalOwnership::AdmittedOutput
    )
    .is_err());
    assert_eq!(
        budget.used_bytes(),
        0,
        "capacity refusal refunds after data drop"
    );
    let foreign = CanonicalBudget::new(64).unwrap();
    assert!(!budget.same_authority(&foreign));
    let adopted = CanonicalBuffer::compatibility(vec![1; 16], &budget).unwrap();
    assert_eq!(adopted.ownership(), CanonicalOwnership::Compatibility);
    drop(adopted);
    assert_eq!(budget.used_bytes(), 0);
}

fn read_owned(
    provider: &StoreProvider<'_>,
    id: ObjectId,
    budget: &CanonicalBudget,
    maximum: usize,
    kind: CanonicalReadKind,
) -> ContentResult<OwnedCanonicalBatch> {
    disabled(|scope| {
        provider.read_canonical_owned(
            &[id],
            CanonicalReadPermit::new(budget, 1, maximum, kind)?,
            scope.child("read"),
        )
    })
}

#[test]
fn actual_store_result_outlives_provider_and_strict8_refuses_before_open() {
    let _serial = CAPTURE_SERIAL.lock().unwrap();
    let dir = TempDir::new("owned_canonical");
    let store = create_store(&dir.store_path("objects"));
    let canonical = assembled_small_object(b"independent finite whole-file bytes");
    let object = FinalizedObject::new(ObjectRole::WholeFile, canonical.clone()).unwrap();
    let id = object.id();
    save_one(&store, object).unwrap();
    let budget = CanonicalBudget::new(2 * canonical.len()).unwrap();
    let provider = StoreProvider::new(&store);
    let observation = allocation_observer::Observation::start(
        [canonical.len(), usize::MAX],
        [usize::MAX - 1, usize::MAX - 2],
    );
    let batch = read_owned(
        &provider,
        id,
        &budget,
        canonical.len(),
        CanonicalReadKind::Any,
    )
    .unwrap();
    assert_eq!(batch.ownership(), CanonicalOwnership::AdmittedOutput);
    assert_eq!(batch.buffers()[0].as_slice(), canonical);
    assert_eq!(provider.connection_opens(), 1);
    let during = observation.stop();
    assert!(!during.overflow);
    let mut iterator = batch.into_iter();
    let retained = iterator.next().unwrap();
    drop(iterator);
    drop(provider);
    assert_eq!(
        budget.used_bytes(),
        retained.capacity(),
        "slow consumer remains funded after provider drop"
    );
    let held = observation.stop();
    assert_eq!(
        held.live[0],
        retained.capacity(),
        "real observed System data allocation survives provider"
    );
    let copied = retained.try_clone().unwrap();
    assert_eq!(copied.as_slice(), canonical);
    assert!(
        retained.try_clone().is_err(),
        "held result and copy exhaust exact caller budget"
    );
    drop(retained);
    assert_eq!(budget.used_bytes(), copied.capacity());
    drop(copied);
    assert_eq!(budget.used_bytes(), 0);

    let released = observation.released();
    assert_eq!(
        released.live_total(),
        0,
        "real provider and retained captured Rust owners dropped"
    );
    eprintln!("R1b diagnostic owned C2 result: during={during:?}; provider-dropped={held:?}; released={released:?}; exact requested layouts only; native/cache-work/process unqualified");

    let strict = StoreProvider::with_owned_profile(&store, OwnedReadProfile::Strict8);
    assert!(matches!(
        read_owned(
            &strict,
            id,
            &budget,
            canonical.len(),
            CanonicalReadKind::Any
        ),
        Err(ContentError::ProviderFailure {
            what: "strict8 C2 read working profile"
        })
    ));
    assert_eq!(strict.connection_opens(), 0);
    assert_eq!(budget.used_bytes(), 0);
    let provider = StoreProvider::new(&store);
    assert!(matches!(
        read_owned(
            &provider,
            id,
            &budget,
            canonical.len(),
            CanonicalReadKind::MappingNodes
        ),
        Err(ContentError::WrongLogicalRole)
    ));
    assert_eq!(budget.used_bytes(), 0);
    assert!(matches!(
        read_owned(
            &provider,
            id,
            &budget,
            canonical.len() - 1,
            CanonicalReadKind::Any
        ),
        Err(ContentError::ProviderFailure {
            what: "canonical owned output window"
        })
    ));
    assert_eq!(
        provider.group_decodes(),
        0,
        "locator window refusal precedes payload decode"
    );
    assert_eq!(budget.used_bytes(), 0);
}
