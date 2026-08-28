use layerfs_core::object::access::ObjectStore;
use layerfs_core::{encode_bytes_object, ObjectId};
use layerfs_durable_store::DurableStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::BranchId;
use layerfs_sync::{abort_fetch_transfer, fetch_objects, push_objects, LocalDurable};
use layerfs_sync::{DurableEndpoint, ResumeToken, SyncError, TransferResult, MAX_BATCH_BYTES};
use layerfs_working_store::WorkingStore;
use std::cell::Cell;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn push_fetch_are_bounded_known_present_and_visibility_free() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sync-transfer-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let working =
        WorkingStore::open(&base.join("working-a"), IntegrityMode::TrustedLocalDev).unwrap();
    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let endpoint = LocalDurable::new(&durable);
    let first = encode_bytes_object(b"first transfer object").unwrap();
    let second = encode_bytes_object(b"second transfer object").unwrap();
    let first_id = ObjectId::for_bytes(&first);
    let second_id = ObjectId::for_bytes(&second);
    let mut writer = working.begin_candidate_write().unwrap();
    assert_eq!(writer.put(&first).unwrap(), first_id);
    assert_eq!(writer.put(&second).unwrap(), second_id);
    writer.commit_objects().unwrap();
    let absent_branch = BranchId::from_bytes([0x71; 32]);

    let pushed = push_objects(
        &working,
        &endpoint,
        [0x72; 32],
        [first_id, second_id],
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(pushed.result, TransferResult::TransferredNoVisibility);
    assert_eq!(pushed.transferred_objects, 2);
    assert_eq!(pushed.known_present_objects, 0);
    assert_eq!(
        pushed.objects_examined,
        pushed.known_present_objects + pushed.missing_objects
    );
    assert_eq!(pushed.retransmitted_bytes, 0);
    assert!(pushed.largest_batch_bytes <= MAX_BATCH_BYTES as u64);
    assert_eq!(pushed.terminal_buffer_bytes, 0);
    assert_eq!(pushed.terminal_queued_batches, 0);
    assert!(pushed.complete_wall_ns >= pushed.receiver_admission_ns);
    assert_eq!(durable.branch_head(absent_branch).unwrap(), None);
    assert!(
        durable
            .sync_custody_rows(layerfs_storage::RequestId::from_bytes([0x72; 32]), "push",)
            .unwrap()
            > 0
    );
    assert!(matches!(
        durable.reap_one_abandoned_sync(i64::MAX).unwrap(),
        Some((owner, direction, rows))
            if owner == layerfs_storage::RequestId::from_bytes([0x72; 32])
                && direction == "push" && rows > 0
    ));

    let repeated = push_objects(
        &working,
        &endpoint,
        [0x73; 32],
        [first_id, second_id],
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(repeated.transferred_objects, 0);
    assert_eq!(repeated.known_present_objects, 2);
    assert_eq!(repeated.missing_objects, 0);
    assert_eq!(repeated.unique_bytes, 0);

    let working_b = WorkingStore::open(&base.join("working-b"), IntegrityMode::Verified).unwrap();
    let fetch_request = [0x74; 32];
    let first_fetch = fetch_objects(
        &endpoint,
        &working_b,
        fetch_request,
        [first_id],
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(first_fetch.transferred_objects, 1);
    assert!(matches!(
        fetch_objects(
            &endpoint,
            &working_b,
            [0x75; 32],
            [first_id, second_id],
            first_fetch.resume,
        ),
        Err(layerfs_sync::SyncError::InvalidResume)
    ));
    drop(working_b);
    let working_b = WorkingStore::open(&base.join("working-b"), IntegrityMode::Verified).unwrap();
    let resumed = fetch_objects(
        &endpoint,
        &working_b,
        fetch_request,
        [first_id, second_id],
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(resumed.transferred_objects, 1);
    assert_eq!(
        resumed.unique_bytes,
        first_fetch.unique_bytes + resumed.resumed_bytes
    );
    assert_eq!(working_b.branch_head(absent_branch).unwrap(), None);
    assert!(working_b.sync_has_object(first_id).unwrap());
    assert!(working_b.sync_has_object(second_id).unwrap());
    assert!(abort_fetch_transfer(&working_b, fetch_request).unwrap() > 0);
    assert_eq!(
        working_b
            .sync_custody_rows(
                layerfs_storage::RequestId::from_bytes(fetch_request),
                "fetch",
            )
            .unwrap(),
        0
    );

    drop(working_b);
    drop(durable);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn rejected_batch_resumes_from_disk_and_charges_retransmission() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sync-retransmit-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let working_path = base.join("working");
    let working = WorkingStore::open(&working_path, IntegrityMode::TrustedLocalDev).unwrap();
    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let canonical = encode_bytes_object(b"received once, rejected once").unwrap();
    let id = ObjectId::for_bytes(&canonical);
    let mut writer = working.begin_candidate_write().unwrap();
    writer.put(&canonical).unwrap();
    writer.commit_objects().unwrap();
    let endpoint = RejectFirstBatch {
        durable: &durable,
        reject: Cell::new(true),
    };
    let request_id = [0x61; 32];
    assert!(matches!(
        push_objects(
            &working,
            &endpoint,
            request_id,
            [id],
            ResumeToken::default(),
        ),
        Err(SyncError::Destination(_))
    ));
    drop(working);

    let working = WorkingStore::open(&working_path, IntegrityMode::TrustedLocalDev).unwrap();
    let resumed = push_objects(
        &working,
        &endpoint,
        request_id,
        [id],
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(resumed.unique_bytes, canonical.len() as u64);
    assert_eq!(resumed.retransmitted_bytes, canonical.len() as u64);
    assert_eq!(resumed.resumed_bytes, canonical.len() as u64);
    assert!(durable.sync_has_object(id).unwrap());

    drop(working);
    drop(durable);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn transferred_new_and_incumbent_corruption_fail_closed() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sync-corruption-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let canonical = encode_bytes_object(b"authenticated transfer object").unwrap();
    let id = ObjectId::for_bytes(&canonical);
    durable
        .sync_accept_objects(
            layerfs_storage::RequestId::from_bytes([0x80; 32]),
            layerfs_storage::RequestId::from_bytes([0x80; 32]),
            "fetch",
            &[(id, canonical.clone())],
        )
        .unwrap();
    let corrupt_source = CorruptRead { durable: &durable };
    let new_destination = WorkingStore::open(&base.join("new"), IntegrityMode::Verified).unwrap();
    assert!(matches!(
        fetch_objects(
            &corrupt_source,
            &new_destination,
            [0x81; 32],
            [id],
            ResumeToken::default(),
        ),
        Err(SyncError::Destination(_))
    ));
    assert!(!new_destination.sync_has_object(id).unwrap());

    let incumbent = WorkingStore::open(&base.join("incumbent"), IntegrityMode::Verified).unwrap();
    fetch_objects(
        &LocalDurable::new(&durable),
        &incumbent,
        [0x82; 32],
        [id],
        ResumeToken::default(),
    )
    .unwrap();
    let mut corrupted = canonical;
    *corrupted.last_mut().unwrap() ^= 1;
    incumbent.corrupt_object_for_test(id, &corrupted).unwrap();
    match fetch_objects(
        &LocalDurable::new(&durable),
        &incumbent,
        [0x83; 32],
        [id],
        ResumeToken::default(),
    ) {
        Err(SyncError::Destination(_)) => {}
        other => panic!("corrupt incumbent did not fail as Destination: {other:?}"),
    }

    drop(incumbent);
    drop(new_destination);
    drop(durable);
    fs::remove_dir_all(base).unwrap();
}

struct CorruptRead<'a> {
    durable: &'a DurableStore,
}

impl DurableEndpoint for CorruptRead<'_> {
    fn durable_storage_id(&self) -> [u8; 32] {
        self.durable.storage_id()
    }

    fn read_object(&self, id: ObjectId, maximum: usize) -> layerfs_sync::Result<Vec<u8>> {
        let mut bytes = self
            .durable
            .sync_read_object(id, maximum)
            .map_err(|error| SyncError::Source(error.to_string()))?;
        *bytes.last_mut().unwrap() ^= 1;
        Ok(bytes)
    }

    fn contains_object(&self, id: ObjectId) -> layerfs_sync::Result<bool> {
        self.durable
            .sync_has_object(id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_objects(
        &self,
        owner_request_id: layerfs_storage::RequestId,
        request_id: layerfs_storage::RequestId,
        direction: layerfs_sync::Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> layerfs_sync::Result<()> {
        self.durable
            .sync_accept_objects(
                owner_request_id,
                request_id,
                direction_name(direction),
                objects,
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn abort_transfer(
        &self,
        owner_request_id: layerfs_storage::RequestId,
        direction: layerfs_sync::Direction,
    ) -> layerfs_sync::Result<u64> {
        self.durable
            .abort_sync_transfer(owner_request_id, direction_name(direction))
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

struct RejectFirstBatch<'a> {
    durable: &'a DurableStore,
    reject: Cell<bool>,
}

impl DurableEndpoint for RejectFirstBatch<'_> {
    fn durable_storage_id(&self) -> [u8; 32] {
        self.durable.storage_id()
    }

    fn read_object(&self, id: ObjectId, maximum: usize) -> layerfs_sync::Result<Vec<u8>> {
        self.durable
            .sync_read_object(id, maximum)
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn contains_object(&self, id: ObjectId) -> layerfs_sync::Result<bool> {
        self.durable
            .sync_has_object(id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_objects(
        &self,
        owner_request_id: layerfs_storage::RequestId,
        request_id: layerfs_storage::RequestId,
        direction: layerfs_sync::Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> layerfs_sync::Result<()> {
        if self.reject.replace(false) {
            return Err(SyncError::Destination(
                "injected post-receive rejection".into(),
            ));
        }
        self.durable
            .sync_accept_objects(
                owner_request_id,
                request_id,
                direction_name(direction),
                objects,
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn abort_transfer(
        &self,
        owner_request_id: layerfs_storage::RequestId,
        direction: layerfs_sync::Direction,
    ) -> layerfs_sync::Result<u64> {
        self.durable
            .abort_sync_transfer(owner_request_id, direction_name(direction))
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

fn direction_name(direction: layerfs_sync::Direction) -> &'static str {
    match direction {
        layerfs_sync::Direction::Fetch => "fetch",
        layerfs_sync::Direction::Push => "push",
    }
}
