use crate::{
    BaseId, BranchId, BranchRecord, CanonicalObject, CommitId, Fact, FactKind, LayerHistoryId,
    LayerId, ObjectSource, Result, StorageError, StoreEndpoint, TransferExchange, TransferPipeline,
};
use layerfs_content::object::references::referenced_objects;
use layerfs_content::{
    encode_bytes_object, encode_object, CanonicalName, DirectoryEntry, Object, ObjectId,
    ObjectKind, ObjectReference,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn stack_attestation_is_independent_of_transport_page_boundaries() {
    let ids = (0..1_025)
        .map(|index| (index as u64).to_be_bytes().to_vec())
        .collect::<Vec<_>>();
    let mut whole = crate::StackAttestation::default();
    whole.observe(FactKind::Stack, &ids);
    let mut paged = crate::StackAttestation::default();
    for page in ids.chunks(crate::ID_BATCH_COUNT) {
        paged.observe(FactKind::Stack, page);
    }
    assert_eq!(whole.finish(), paged.finish());
}

struct Source {
    objects: BTreeMap<ObjectId, Vec<u8>>,
    reads: RefCell<BTreeMap<ObjectId, usize>>,
}

struct StreamingSource {
    objects: BTreeMap<ObjectId, Vec<u8>>,
    pages: RefCell<Vec<usize>>,
}

impl ObjectSource for StreamingSource {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.objects
            .get(&id)
            .cloned()
            .ok_or(StorageError::MissingBaseData)
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        assert_eq!(ids.len(), 1, "bulk reads must use visit_objects");
        Ok(vec![CanonicalObject {
            id: ids[0],
            bytes: self.read_object(ids[0])?,
        }])
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        self.pages.borrow_mut().push(ids.len());
        for id in ids {
            visitor(CanonicalObject {
                id: *id,
                bytes: self.read_object(*id)?,
            })?;
        }
        Ok(())
    }
}

impl ObjectSource for Source {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        *self.reads.borrow_mut().entry(id).or_default() += 1;
        self.objects
            .get(&id)
            .cloned()
            .ok_or(StorageError::MissingBaseData)
    }
}

struct Destination {
    known: std::sync::Mutex<BTreeSet<ObjectId>>,
    max_objects: std::sync::atomic::AtomicUsize,
}

impl ObjectSource for Destination {
    fn read_object(&self, _id: ObjectId) -> Result<Vec<u8>> {
        Err(StorageError::WrongSourceRoute)
    }
}

impl StoreEndpoint for Destination {
    fn begin_transfer(&self) -> Result<Box<dyn crate::TransferTarget + '_>> {
        Ok(Box::new(DestinationTarget(self)))
    }

    fn transfer_exchange_unlocked(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange> {
        self.max_objects
            .fetch_max(objects.len(), std::sync::atomic::Ordering::SeqCst);
        let mut known = self
            .known
            .lock()
            .map_err(|_| StorageError::Integrity("test endpoint"))?;
        let mut admission = crate::AdmissionStats::default();
        for object in objects {
            known.insert(object.id);
            admission.objects.inserted_ids += 1;
            admission.objects.inserted_bytes += object.bytes.len() as u64;
        }
        if let Some(kind) = facts.first().map(|fact| fact.kind()) {
            let receipt = admission.facts.entry(kind).or_default();
            receipt.inserted_ids += facts.len() as u64;
            receipt.inserted_bytes += facts
                .iter()
                .map(|fact| fact.encoded_size() as u64)
                .sum::<u64>();
        }
        admission.database.object_admission_transactions =
            crate::object_batches(objects)?.len() as u64;
        admission.database.fact_admission_transactions = crate::fact_batches(facts)?.len() as u64;
        admission.database.write_transactions = admission.database.object_admission_transactions
            + admission.database.fact_admission_transactions;
        let object_missing = crate::MissingBitmap::from_missing(object_ids.len(), |index| {
            !known.contains(&object_ids[index])
        })?;
        let fact_missing =
            crate::MissingBitmap::from_missing(fact_ids.map_or(0, |(_, ids)| ids.len()), |_| true)?;
        Ok(TransferExchange::new(
            admission,
            object_missing,
            fact_missing,
        ))
    }
}

impl Destination {
    fn new(known: BTreeSet<ObjectId>) -> Self {
        Self {
            known: std::sync::Mutex::new(known),
            max_objects: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

struct DestinationTarget<'a>(&'a Destination);

impl crate::TransferTarget for DestinationTarget<'_> {
    fn preflight_branch(
        &mut self,
        _branch: BranchRecord,
        root: ObjectId,
    ) -> Result<(Option<CommitId>, bool, crate::MissingBitmap)> {
        let known = self
            .0
            .known
            .lock()
            .map_err(|_| StorageError::Integrity("test endpoint"))?;
        Ok((
            None,
            false,
            crate::MissingBitmap::from_missing(1, |_| !known.contains(&root))?,
        ))
    }

    fn exchange(
        &mut self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange> {
        self.0
            .transfer_exchange_unlocked(objects, facts, object_ids, fact_ids)
    }

    fn finish(
        self: Box<Self>,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: crate::TransferIntent,
    ) -> Result<(TransferExchange, crate::TransferOutcome)> {
        if intent != crate::TransferIntent::None {
            return Err(StorageError::WrongSourceRoute);
        }
        Ok((
            self.0
                .transfer_exchange_unlocked(objects, facts, &[], None)?,
            crate::TransferOutcome::Unit,
        ))
    }
}

#[test]
fn known_descendant_prunes_before_source_read_and_turns_are_p_plus_one() {
    let built = crate::empty_root([42; 32]).unwrap();
    let root = built.root_id;
    let mut objects = BTreeMap::new();
    built
        .objects
        .visit_batches(&mut |batch, _| {
            for object in batch {
                objects.insert(object.id, object.bytes.clone());
            }
            Ok(())
        })
        .unwrap();
    let child = referenced_objects(&objects[&root]).unwrap()[0];
    let source = Source {
        objects,
        reads: RefCell::new(BTreeMap::new()),
    };
    let destination = Destination::new(BTreeSet::from([child]));
    let mut transfer = TransferPipeline::new(&destination).unwrap();
    transfer.snapshot(&source, root).unwrap();
    let receipt = transfer.test_receipt().unwrap();
    assert_eq!(source.reads.borrow().get(&root), Some(&1));
    assert_eq!(source.reads.borrow().get(&child), None);
    assert!(
        receipt.transport.request_reply_turns
            <= receipt.transport.object_membership_pages
                + receipt.transport.typed_membership_pages
                + 1
    );
}

#[test]
fn deep_closure_drains_bounded_object_pages() {
    let leaf = encode_bytes_object(b"leaf").unwrap();
    let mut id = ObjectId::for_bytes(&leaf);
    let mut kind = ObjectKind::Bytes;
    let mut objects = BTreeMap::from([(id, leaf)]);
    for _ in 0..300 {
        let object = Object::directory(vec![DirectoryEntry::new(
            CanonicalName::new("child").unwrap(),
            ObjectReference::new(kind, id),
        )])
        .unwrap();
        let canonical = encode_object(&object).unwrap();
        id = ObjectId::for_bytes(&canonical);
        kind = ObjectKind::Directory;
        objects.insert(id, canonical);
    }
    let source = Source {
        objects,
        reads: RefCell::new(BTreeMap::new()),
    };
    let destination = Destination::new(BTreeSet::new());
    let mut transfer = TransferPipeline::new(&destination).unwrap();
    transfer.snapshot(&source, id).unwrap();
    let receipt = transfer.test_receipt().unwrap();
    assert!(
        destination
            .max_objects
            .load(std::sync::atomic::Ordering::SeqCst)
            <= crate::OBJECT_BATCH_COUNT
    );
    assert!(receipt.transport.one_way_payload_batches > 1);
    assert_eq!(
        receipt.transport.payload_frames,
        receipt.objects.set.sent_ids
    );
    assert_eq!(
        receipt.database.write_transactions,
        receipt.transport.one_way_payload_batches
    );
    assert!(receipt.transport.peak_buffer_bytes <= crate::OBJECT_BATCH_BYTES as u64);
    assert_eq!(
        receipt.transport.request_reply_turns,
        receipt.transport.object_membership_pages + 1
    );
}

#[test]
fn wide_frontier_streams_source_bodies_instead_of_materializing_a_page() {
    let mut objects = BTreeMap::new();
    let mut entries = Vec::new();
    for index in 0..crate::OBJECT_BATCH_COUNT {
        let bytes = encode_bytes_object(&(index as u64).to_be_bytes()).unwrap();
        let id = ObjectId::for_bytes(&bytes);
        objects.insert(id, bytes);
        entries.push(DirectoryEntry::new(
            CanonicalName::new(&format!("child-{index:03}")).unwrap(),
            ObjectReference::new(ObjectKind::Bytes, id),
        ));
    }
    let root = encode_object(&Object::directory(entries).unwrap()).unwrap();
    let root_id = ObjectId::for_bytes(&root);
    objects.insert(root_id, root);
    let source = StreamingSource {
        objects,
        pages: RefCell::new(Vec::new()),
    };
    let destination = Destination::new(BTreeSet::new());
    let mut transfer = TransferPipeline::new(&destination).unwrap();
    transfer.snapshot(&source, root_id).unwrap();
    transfer.finish().unwrap();
    assert_eq!(*source.pages.borrow(), [1, crate::OBJECT_BATCH_COUNT]);
}

#[test]
fn root_membership_is_deduplicated_in_512_id_pages() {
    let mut objects = BTreeMap::new();
    let mut roots = Vec::new();
    for index in 0..(crate::ID_BATCH_COUNT * 2 + 1) {
        let bytes = encode_bytes_object(&(index as u64).to_be_bytes()).unwrap();
        let id = ObjectId::for_bytes(&bytes);
        objects.insert(id, bytes);
        roots.push(id);
    }
    let source = StreamingSource {
        objects,
        pages: RefCell::new(Vec::new()),
    };
    let destination = Destination::new(BTreeSet::new());
    let mut transfer = TransferPipeline::new(&destination).unwrap();
    transfer.snapshots(&source, &roots).unwrap();
    let receipt = transfer.test_receipt().unwrap();
    assert_eq!(receipt.transport.object_membership_pages, 3);
    assert_eq!(
        *source.pages.borrow(),
        [crate::ID_BATCH_COUNT, crate::ID_BATCH_COUNT, 1]
    );

    let root = roots[0];
    let source = StreamingSource {
        objects: BTreeMap::from([(root, source.objects[&root].clone())]),
        pages: RefCell::new(Vec::new()),
    };
    let destination = Destination::new(BTreeSet::new());
    let mut transfer = TransferPipeline::new(&destination).unwrap();
    transfer
        .snapshots(&source, &vec![root; crate::ID_BATCH_COUNT * 2 + 1])
        .unwrap();
    let receipt = transfer.test_receipt().unwrap();
    assert_eq!(receipt.transport.object_membership_pages, 1);
    assert_eq!(*source.pages.borrow(), [1]);
}

#[test]
fn receipt_separates_fact_membership_from_objects_while_admission_stays_128_wide() {
    let facts = (0..(crate::ID_BATCH_COUNT * 2 + 1))
        .map(|index| {
            let root = ObjectId::for_bytes(&(index as u64).to_be_bytes());
            Fact::Commit(crate::CommitRecord {
                id: CommitId::derive(root, None, None),
                root_id: root,
                parent_id: None,
                merge_parent_id: None,
            })
        })
        .collect::<Vec<_>>();
    let destination = Destination::new(BTreeSet::new());
    let mut transfer = TransferPipeline::new(&destination).unwrap();
    transfer.facts(&facts).unwrap();
    let receipt = transfer.test_receipt().unwrap();
    assert_eq!(receipt.transport.typed_membership_pages, 3);
    assert_eq!(receipt.database.write_transactions, 9);
    assert_eq!(receipt.database.fact_admission_transactions, 9);
    assert_eq!(receipt.transport.request_reply_turns, 4);
    assert_eq!(receipt.objects.set, crate::TransferSetReceipt::default());
    let facts = receipt.facts[&FactKind::Commit];
    assert_eq!(facts.announced_ids, 1025);
    assert_eq!(facts.missing_ids, 1025);
    assert_eq!(facts.sent_ids, 1025);
    assert_eq!(facts.inserted_ids, 1025);
    assert_eq!(facts.raced_existing_ids, 0);
}

#[test]
fn branch_preflight_reuses_first_root_announcement_inside_p_plus_one() {
    let built = crate::empty_root([17; 32]).unwrap();
    let root = built.root_id;
    let mut objects = BTreeMap::new();
    built
        .objects
        .visit_batches(&mut |batch, _| {
            for object in batch {
                objects.insert(object.id, object.bytes.clone());
            }
            Ok(())
        })
        .unwrap();
    let source = Source {
        objects,
        reads: RefCell::new(BTreeMap::new()),
    };
    let destination = Destination::new(BTreeSet::new());
    let commit = CommitId::derive(root, None, None);
    let branch = BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit,
        base_id: BaseId::Layer(LayerId::derive(LayerHistoryId::new(), None, root)),
    };
    let mut transfer = TransferPipeline::new(&destination).unwrap();
    assert_eq!(
        transfer.preflight_branch(branch, root).unwrap(),
        (None, false)
    );
    transfer.snapshot(&source, root).unwrap();
    let receipt = transfer.test_receipt().unwrap();
    assert_eq!(source.reads.borrow().get(&root), Some(&1));
    assert!(
        receipt.transport.request_reply_turns
            <= receipt.transport.object_membership_pages
                + receipt.transport.typed_membership_pages
                + 1
    );
}
