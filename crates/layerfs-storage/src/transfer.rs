use crate::{
    decode_fact, fact_batches, BranchRecord, CanonicalObject, CommitId, Fact, FactKind,
    MissingBitmap, ObjectSource, Result, StackId, StorageError, StorageReceipt, StoreDb,
    StoreEndpoint, TransferExchange, TransferReceipt, ID_BATCH_COUNT, OBJECT_BATCH_BYTES,
    OBJECT_BATCH_COUNT,
};
use layerfs_content::ObjectId;
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

thread_local! {
    static COMPLETED_RECEIPTS: std::cell::RefCell<Vec<StorageReceipt>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

#[doc(hidden)]
pub fn take_storage_receipts() -> Vec<StorageReceipt> {
    COMPLETED_RECEIPTS.with(|receipts| std::mem::take(&mut *receipts.borrow_mut()))
}

pub(crate) fn record(receipt: StorageReceipt) {
    COMPLETED_RECEIPTS.with(|receipts| {
        let mut receipts = receipts.borrow_mut();
        if receipts.len() == 16 {
            receipts.remove(0);
        }
        receipts.push(receipt);
    });
}

#[doc(hidden)]
pub const DEFERRED_MEMORY_BYTES: usize = 8 * 1024 * 1024;
static DEFERRED_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[doc(hidden)]
pub struct DeferredFactStore {
    memory: Vec<Fact>,
    memory_bytes: usize,
    spill: Option<(PathBuf, File)>,
}

impl DeferredFactStore {
    pub fn new() -> Result<Self> {
        Ok(Self {
            memory: Vec::new(),
            memory_bytes: 0,
            spill: None,
        })
    }

    pub fn stage(&mut self, fact: Fact) -> Result<()> {
        let bytes = fact.signing_bytes();
        let charge = bytes.len() + 4;
        if self.spill.is_none() && self.memory_bytes + charge <= DEFERRED_MEMORY_BYTES {
            self.memory.push(fact);
            self.memory_bytes += charge;
            return Ok(());
        }
        self.ensure_spill()?;
        write_fact(&mut self.spill.as_mut().expect("created spill").1, &bytes)
    }

    pub fn visit_batches(&mut self, visitor: &mut dyn FnMut(&[Fact]) -> Result<()>) -> Result<()> {
        let mut page: Vec<Fact> = Vec::with_capacity(ID_BATCH_COUNT);
        let mut emit = |fact: Fact| -> Result<()> {
            if !page.is_empty() && (page.len() == ID_BATCH_COUNT || page[0].kind() != fact.kind()) {
                visitor(&page)?;
                page.clear();
            }
            page.push(fact);
            Ok(())
        };
        if let Some((path, writer)) = &mut self.spill {
            writer.flush()?;
            let mut input = File::open(path)?;
            while let Some(fact) = read_fact(&mut input)? {
                emit(fact)?;
            }
        } else {
            for fact in self.memory.iter().copied() {
                emit(fact)?;
            }
        }
        if !page.is_empty() {
            visitor(&page)?;
        }
        Ok(())
    }

    fn ensure_spill(&mut self) -> Result<()> {
        if self.spill.is_some() {
            return Ok(());
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "layerfs-facts-{}-{nonce}-{}",
            std::process::id(),
            DEFERRED_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut writer = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        for fact in self.memory.drain(..) {
            write_fact(&mut writer, &fact.signing_bytes())?;
        }
        self.memory_bytes = 0;
        self.spill = Some((path, writer));
        Ok(())
    }

    #[doc(hidden)]
    pub fn spilled(&self) -> bool {
        self.spill.is_some()
    }
}

impl Drop for DeferredFactStore {
    fn drop(&mut self) {
        if let Some((path, _)) = &self.spill {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn write_fact(output: &mut File, bytes: &[u8]) -> Result<()> {
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(bytes)?;
    Ok(())
}

fn read_fact(input: &mut File) -> Result<Option<Fact>> {
    let mut length = [0; 4];
    let read = input.read(&mut length)?;
    if read == 0 {
        return Ok(None);
    }
    input.read_exact(&mut length[read..])?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 1024 {
        return Err(StorageError::Integrity("deferred fact length"));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    decode_fact(&bytes).map(Some)
}

#[doc(hidden)]
pub struct TransferPipeline<'a> {
    target: Box<dyn crate::TransferTarget + 'a>,
    pending_objects: Vec<CanonicalObject>,
    pending_object_bytes: usize,
    pending_facts: Vec<Fact>,
    defer_publication: bool,
    preannounced: Option<(ObjectId, MissingBitmap)>,
    receipt: TransferReceipt,
}

impl<'a> TransferPipeline<'a> {
    pub fn new(destination: &'a dyn StoreEndpoint) -> Result<Self> {
        Ok(Self {
            target: destination.begin_transfer()?,
            pending_objects: Vec::new(),
            pending_object_bytes: 0,
            pending_facts: Vec::new(),
            defer_publication: false,
            preannounced: None,
            receipt: TransferReceipt::default(),
        })
    }

    pub fn preflight_branch(
        &mut self,
        branch: BranchRecord,
        root: ObjectId,
    ) -> Result<(Option<CommitId>, bool)> {
        self.receipt.objects.set.announced_ids += 1;
        self.receipt.transport.object_membership_pages += 1;
        let (current, up_to_date, missing) = self.target.preflight_branch(branch, root)?;
        missing.validate_tail(1)?;
        self.receipt.transport.request_reply_turns += 1;
        self.receipt.transport.command_frames += 1;
        self.receipt.transport.reply_frames += 2;
        if !up_to_date {
            self.preannounced = Some((root, missing));
        } else {
            self.receipt.objects.known_subtrees_pruned += 1;
        }
        Ok((current, up_to_date))
    }

    pub fn preflight_stack(
        &mut self,
        history_id: crate::StackHistoryId,
        base_layer_id: crate::LayerId,
        incoming: StackId,
        root: ObjectId,
    ) -> Result<(Option<StackId>, bool)> {
        self.receipt.objects.set.announced_ids += 1;
        self.receipt.transport.object_membership_pages += 1;
        let (current, up_to_date, missing) =
            self.target
                .preflight_stack(history_id, base_layer_id, incoming, root)?;
        missing.validate_tail(1)?;
        self.receipt.transport.request_reply_turns += 1;
        self.receipt.transport.command_frames += 1;
        self.receipt.transport.reply_frames += 2;
        if !up_to_date {
            self.preannounced = Some((root, missing));
        } else {
            self.receipt.objects.known_subtrees_pruned += 1;
        }
        Ok((current, up_to_date))
    }

    pub fn snapshot(
        &mut self,
        source: &(impl ObjectSource + ?Sized),
        root: ObjectId,
    ) -> Result<()> {
        self.snapshots(source, &[root])
    }

    pub fn snapshots(
        &mut self,
        source: &(impl ObjectSource + ?Sized),
        roots: &[ObjectId],
    ) -> Result<()> {
        let mut roots = roots.iter().copied().collect::<BTreeSet<_>>();
        if let Some((root, missing)) = self.preannounced.take() {
            if roots.remove(&root) {
                if missing.is_missing(0)? {
                    let object = source
                        .read_objects(&[root])?
                        .pop()
                        .ok_or(StorageError::MissingBaseData)?;
                    crate::candidate::transfer_closure(source, object, self)?;
                } else {
                    self.receipt.objects.known_subtrees_pruned += 1;
                }
            } else {
                self.preannounced = Some((root, missing));
            }
        }
        let roots = roots.into_iter().collect::<Vec<_>>();
        for page in roots.chunks(ID_BATCH_COUNT) {
            self.receipt.objects.set.announced_ids += page.len() as u64;
            self.receipt.transport.object_membership_pages += 1;
            let missing = self.exchange(page, None, true)?.0;
            let mut selected = Vec::new();
            for (index, id) in page.iter().enumerate() {
                if missing.is_missing(index)? {
                    selected.push(*id);
                }
            }
            self.receipt.objects.known_subtrees_pruned += (page.len() - selected.len()) as u64;
            if selected.is_empty() {
                continue;
            }
            let mut fetched = crate::DeferredObjectStore::new()?;
            source.visit_objects(&selected, &mut |object| fetched.stage(object))?;
            while let Some(object) = fetched.pop_first()? {
                crate::candidate::transfer_closure(source, object, self)?;
            }
        }
        Ok(())
    }

    pub fn facts(&mut self, facts: &[Fact]) -> Result<()> {
        let mut start = 0;
        while start < facts.len() {
            let kind = facts[start].kind();
            let mut end = start + 1;
            while end < facts.len() && facts[end].kind() == kind && end - start < ID_BATCH_COUNT {
                end += 1;
            }
            let page = &facts[start..end];
            let mut ids = page.iter().copied().map(Fact::id).collect::<Vec<_>>();
            ids.sort();
            if ids.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(StorageError::Integrity("fact announcement ordering"));
            }
            let missing = self.announce_fact_ids(kind, &ids)?;
            let mut selected = BTreeSet::new();
            for (index, id) in ids.iter().enumerate() {
                if missing.is_missing(index)? {
                    selected.insert(id.clone());
                }
            }
            let publication = matches!(kind, FactKind::Branch | FactKind::AddResult);
            if self.defer_publication
                && publication
                && page.windows(2).any(|pair| pair[0].id() >= pair[1].id())
            {
                return Err(StorageError::Integrity("publication ordering"));
            }
            let selected = page
                .iter()
                .copied()
                .filter(|fact| selected.contains(&fact.id()))
                .collect::<Vec<_>>();
            self.stage_fact_page(&selected, publication)?;
            start = end;
        }
        Ok(())
    }

    pub fn announce_fact_ids(&mut self, kind: FactKind, ids: &[Vec<u8>]) -> Result<MissingBitmap> {
        self.receipt.transport.typed_membership_pages += 1;
        self.receipt.facts.entry(kind).or_default().announced_ids += ids.len() as u64;
        let missing = self.exchange(&[], Some((kind, ids)), true)?.1;
        missing.validate_tail(ids.len())?;
        Ok(missing)
    }

    pub fn stage_facts(&mut self, facts: &[Fact]) -> Result<()> {
        if facts.is_empty() {
            return Ok(());
        }
        if facts.iter().any(|fact| fact.kind() != facts[0].kind()) {
            return Err(StorageError::Integrity("fact page kind"));
        }
        self.stage_fact_page(facts, false)?;
        Ok(())
    }

    fn stage_fact_page(&mut self, facts: &[Fact], publication: bool) -> Result<()> {
        if facts.is_empty() {
            return Ok(());
        }
        let bytes = facts
            .iter()
            .map(|fact| fact.encoded_size() as u64)
            .sum::<u64>();
        if self.defer_publication && publication {
            for batch in fact_batches(facts)? {
                self.target.defer_publication(batch)?;
                self.receipt.transport.one_way_payload_batches += 1;
                self.receipt.transport.command_frames += 1;
            }
        } else {
            self.pending_facts = facts.to_vec();
        }
        let set = self.receipt.facts.entry(facts[0].kind()).or_default();
        set.missing_ids += facts.len() as u64;
        set.sent_ids += facts.len() as u64;
        set.sent_bytes += bytes;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        record(StorageReceipt::Transfer(self.finish_receipt()?));
        Ok(())
    }

    fn finish_receipt(self) -> Result<TransferReceipt> {
        let (receipt, outcome) = self.finish_with(crate::TransferIntent::None)?;
        if outcome != crate::TransferOutcome::Unit {
            return Err(StorageError::Integrity("transfer outcome"));
        }
        Ok(receipt)
    }

    #[cfg(test)]
    pub(crate) fn test_receipt(self) -> Result<TransferReceipt> {
        self.finish_receipt()
    }

    pub fn finish_up_to_date(self) -> Result<()> {
        self.receipt.validate()?;
        record(StorageReceipt::Transfer(self.receipt));
        Ok(())
    }

    pub fn finish_branch(
        self,
        branch: BranchRecord,
        expected: Option<CommitId>,
    ) -> Result<crate::RefOutcome<CommitId>> {
        let (receipt, outcome) =
            self.finish_with(crate::TransferIntent::Branch { branch, expected })?;
        record(StorageReceipt::Transfer(receipt));
        match outcome {
            crate::TransferOutcome::Commit(outcome) => Ok(outcome),
            _ => Err(StorageError::Integrity("transfer Branch outcome")),
        }
    }

    pub fn finish_stack(self, push: crate::StackPush) -> Result<crate::RefOutcome<StackId>> {
        let (receipt, outcome) = self.finish_with(crate::TransferIntent::Stack(push))?;
        record(StorageReceipt::Transfer(receipt));
        match outcome {
            crate::TransferOutcome::Stack(outcome) => Ok(outcome),
            _ => Err(StorageError::Integrity("transfer Stack outcome")),
        }
    }

    pub fn defer_stack_publication(&mut self) {
        self.defer_publication = true;
    }

    pub fn finish_layer_history(
        self,
        history: crate::LayerHistoryRecord,
    ) -> Result<crate::RefOutcome<crate::LayerId>> {
        let (receipt, outcome) = self.finish_with(crate::TransferIntent::ObserveLayer(history))?;
        record(StorageReceipt::Transfer(receipt));
        match outcome {
            crate::TransferOutcome::Layer(outcome) => Ok(outcome),
            _ => Err(StorageError::Integrity("transfer Layer outcome")),
        }
    }

    pub fn finish_stack_history(
        self,
        history: crate::StackHistoryRecord,
        expected: Option<StackId>,
    ) -> Result<crate::RefOutcome<StackId>> {
        let (receipt, outcome) =
            self.finish_with(crate::TransferIntent::ObserveStack { history, expected })?;
        record(StorageReceipt::Transfer(receipt));
        match outcome {
            crate::TransferOutcome::Stack(outcome) => Ok(outcome),
            _ => Err(StorageError::Integrity("transfer StackHistory outcome")),
        }
    }

    pub(crate) fn announce_objects(&mut self, ids: &[ObjectId]) -> Result<crate::MissingBitmap> {
        self.receipt.transport.object_membership_pages += 1;
        self.receipt.objects.set.announced_ids += ids.len() as u64;
        let missing = self.exchange(ids, None, true)?.0;
        missing.validate_tail(ids.len())?;
        for index in 0..ids.len() {
            if !missing.is_missing(index)? {
                self.receipt.objects.known_subtrees_pruned += 1;
            }
        }
        Ok(missing)
    }

    pub(crate) fn stage_object(&mut self, object: CanonicalObject) -> Result<()> {
        let bytes = object.bytes.len() as u64;
        if !self.pending_objects.is_empty()
            && (self.pending_objects.len() == OBJECT_BATCH_COUNT
                || self.pending_object_bytes + object.bytes.len() > OBJECT_BATCH_BYTES)
        {
            self.exchange(&[], None, false)?;
        }
        self.receipt.objects.set.missing_ids += 1;
        self.receipt.objects.set.sent_ids += 1;
        self.receipt.objects.set.sent_bytes += bytes;
        self.pending_object_bytes += object.bytes.len();
        self.pending_objects.push(object);
        self.receipt.transport.peak_buffer_bytes = self
            .receipt
            .transport
            .peak_buffer_bytes
            .max(self.pending_object_bytes as u64);
        Ok(())
    }

    fn exchange(
        &mut self,
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
        reply_turn: bool,
    ) -> Result<(crate::MissingBitmap, crate::MissingBitmap)> {
        let payload_frames = self.pending_objects.len() as u64;
        let has_payload = !self.pending_objects.is_empty() || !self.pending_facts.is_empty();
        let reply = self.target.exchange(
            &self.pending_objects,
            &self.pending_facts,
            object_ids,
            fact_ids,
        )?;
        self.pending_object_bytes = 0;
        self.pending_objects.clear();
        self.pending_facts.clear();
        self.receipt.transport.command_frames += 1;
        self.receipt.transport.payload_frames += payload_frames;
        if has_payload {
            self.receipt.transport.one_way_payload_batches += 1;
        }
        if reply_turn {
            self.receipt.transport.request_reply_turns += 1;
            self.receipt.transport.reply_frames += 1;
        }
        let (admission, objects, facts) = reply.into_parts();
        self.receipt.merge_admission(admission);
        Ok((objects, facts))
    }

    fn finish_with(
        mut self,
        intent: crate::TransferIntent,
    ) -> Result<(TransferReceipt, crate::TransferOutcome)> {
        if self.preannounced.is_some() {
            return Err(StorageError::Integrity("unused preflight root"));
        }
        if !self.pending_objects.is_empty() {
            self.receipt.transport.payload_frames += self.pending_objects.len() as u64;
        }
        if !self.pending_objects.is_empty() || !self.pending_facts.is_empty() {
            self.receipt.transport.one_way_payload_batches += 1;
        }
        self.receipt.transport.command_frames += 1;
        let (exchange, outcome) =
            self.target
                .finish(&self.pending_objects, &self.pending_facts, intent)?;
        self.receipt.transport.request_reply_turns += 1;
        self.receipt.transport.reply_frames += 1;
        self.receipt.merge_admission(exchange.into_parts().0);
        self.receipt.validate()?;
        Ok((self.receipt, outcome))
    }
}

impl StoreDb {
    #[doc(hidden)]
    pub fn transfer_exchange(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
        authenticate: bool,
    ) -> Result<TransferExchange> {
        let mut admission = crate::AdmissionStats::default();
        for batch in crate::candidate::object_batches(objects)? {
            admission.merge(if authenticate {
                self.admit_remote(batch)?
            } else {
                self.admit_local(batch)?
            });
        }
        for batch in fact_batches(facts)? {
            admission.merge(self.admit_received_facts(batch)?);
        }
        let objects = if object_ids.is_empty() {
            crate::MissingBitmap::empty()
        } else {
            self.missing_objects(object_ids)?
        };
        let facts = match fact_ids {
            Some((kind, ids)) if !ids.is_empty() => self.missing_facts(kind, ids)?,
            _ => crate::MissingBitmap::empty(),
        };
        Ok(TransferExchange::new(admission, objects, facts))
    }
}
