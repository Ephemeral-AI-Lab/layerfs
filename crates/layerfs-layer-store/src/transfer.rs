use crate::LayerStore;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use layerfs_core::ObjectId;
use layerfs_storage_core::{
    closest_common_layer, closest_common_stack, AddResultRecord, BaseId, BaseSnapshot, BranchId,
    BranchRecord, CanonicalObject, CommitId, Fact, FactKind, LayerHistoryId, LayerHistoryRecord,
    LayerId, LayerRecord, MissingBitmap, ObjectSource, Result, StackAttestation, StackHistoryId,
    StackHistoryRecord, StackId, StackPush, StackRecord, StorageError, StoreEndpoint,
    TransferExchange, TransferIntent, TransferOutcome, TransferTarget, FACT_BATCH_BYTES,
    FACT_BATCH_COUNT,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

type Membership<'a> =
    dyn FnMut(FactKind, &[Vec<u8>]) -> Result<layerfs_storage_core::MissingBitmap> + 'a;

impl ObjectSource for LayerStore {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.db.read_object_row(id)
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        self.db.read_object_rows(ids)
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        self.db.visit_object_rows(ids, visitor)
    }
}

impl StoreEndpoint for LayerStore {
    fn begin_transfer(&self) -> Result<Box<dyn TransferTarget + '_>> {
        Ok(Box::new(LayerTransfer {
            store: self,
            _permit: self.db.enter_operation()?,
            attestation: StackAttestation::default(),
            publication: None,
            publication_page: None,
        }))
    }

    fn transfer_exchange_unlocked(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange> {
        self.db
            .transfer_exchange(objects, facts, object_ids, fact_ids, false)
    }

    fn base_snapshot(&self, base_id: BaseId) -> Result<BaseSnapshot> {
        match base_id {
            BaseId::Layer(id) => {
                let layer = self.db.layer(id)?.ok_or(StorageError::NotFound("Layer"))?;
                Ok(BaseSnapshot {
                    base_id,
                    layer_history_id: layer.history_id,
                    root_id: layer.root_id,
                })
            }
            BaseId::Stack(id) => {
                let stack = self.db.stack(id)?.ok_or(StorageError::NotFound("Stack"))?;
                let history = self
                    .db
                    .stack_history(stack.history_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                let base = self
                    .db
                    .layer(history.base_layer_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                Ok(BaseSnapshot {
                    base_id,
                    layer_history_id: base.history_id,
                    root_id: stack.root_id,
                })
            }
        }
    }

    fn common_base(&self, left: BaseId, right: BaseId) -> Result<BaseSnapshot> {
        let left_snapshot = self.base_snapshot(left)?;
        let right_snapshot = self.base_snapshot(right)?;
        if left_snapshot.layer_history_id != right_snapshot.layer_history_id {
            return Err(StorageError::NoCommonBase);
        }
        if let (BaseId::Stack(left_id), BaseId::Stack(right_id)) = (left, right) {
            let left_stack = self
                .db
                .stack(left_id)?
                .ok_or(StorageError::MissingBaseData)?;
            let right_stack = self
                .db
                .stack(right_id)?
                .ok_or(StorageError::MissingBaseData)?;
            if left_stack.history_id == right_stack.history_id {
                if let Some(id) = closest_common_stack(&self.db, left_id, right_id)? {
                    let stack = self.db.stack(id)?.ok_or(StorageError::MissingBaseData)?;
                    return Ok(BaseSnapshot {
                        base_id: BaseId::Stack(id),
                        layer_history_id: left_snapshot.layer_history_id,
                        root_id: stack.root_id,
                    });
                }
            }
        }
        let left_layer = base_layer_id(&self.db, left)?;
        let right_layer = base_layer_id(&self.db, right)?;
        let id = closest_common_layer(&self.db, left_layer, right_layer)?
            .ok_or(StorageError::NoCommonBase)?;
        let layer = self.db.layer(id)?.ok_or(StorageError::MissingBaseData)?;
        Ok(BaseSnapshot {
            base_id: BaseId::Layer(id),
            layer_history_id: layer.history_id,
            root_id: layer.root_id,
        })
    }

    fn branch_record(&self, branch_id: BranchId) -> Result<BranchRecord> {
        self.db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))
    }

    fn visit_commits(
        &self,
        branch_id: BranchId,
        membership: &mut Membership<'_>,
        visitor: &mut dyn FnMut(&[layerfs_storage_core::CommitRecord]) -> Result<()>,
    ) -> Result<()> {
        let branch = self.branch_record(branch_id)?;
        self.db
            .visit_commit_ancestry(branch.head_commit_id, None, &mut |_, page| {
                missing_page(
                    FactKind::Commit,
                    page,
                    |row| row.id.to_bytes(),
                    membership,
                    visitor,
                )
            })
    }

    fn layer_history_record(&self, history_id: LayerHistoryId) -> Result<LayerHistoryRecord> {
        self.db
            .layer_history(history_id)?
            .ok_or(StorageError::NotFound("LayerHistory"))
    }

    fn visit_layers(
        &self,
        history_id: LayerHistoryId,
        through: LayerId,
        membership: &mut dyn FnMut(
            FactKind,
            &[Vec<u8>],
        ) -> Result<layerfs_storage_core::MissingBitmap>,
        visitor: &mut dyn FnMut(&[LayerRecord]) -> Result<()>,
    ) -> Result<()> {
        self.db.visit_layers(history_id, through, &mut |page| {
            missing_page(
                FactKind::Layer,
                page,
                |row| row.id.to_bytes(),
                membership,
                visitor,
            )
        })
    }

    fn stack_history_record(&self, history_id: StackHistoryId) -> Result<StackHistoryRecord> {
        self.db
            .stack_history(history_id)?
            .ok_or(StorageError::NotFound("StackHistory"))
    }

    fn visit_stacks(
        &self,
        history_id: StackHistoryId,
        through: StackId,
        membership: &mut dyn FnMut(
            FactKind,
            &[Vec<u8>],
        ) -> Result<layerfs_storage_core::MissingBitmap>,
        visitor: &mut dyn FnMut(&[StackRecord]) -> Result<()>,
    ) -> Result<()> {
        self.db.visit_stacks(history_id, through, &mut |page| {
            missing_page(
                FactKind::Stack,
                page,
                |row| row.id.to_bytes(),
                membership,
                visitor,
            )
        })
    }

    fn add_layer(
        &self,
        layer_history_id: LayerHistoryId,
        source: layerfs_storage_core::AddLayerSource,
    ) -> Result<layerfs_storage_core::AddResult<LayerId>> {
        LayerStore::add_layer(self, layer_history_id, source)
    }
}

const PUBLICATION_MEMORY_BYTES: usize = 8 * 1024 * 1024;
static PUBLICATION_SPOOL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct PublicationSpool {
    memory: Vec<(Fact, bool)>,
    memory_bytes: usize,
    spill: Option<(PathBuf, File)>,
    count: u64,
    last_rank: Option<u8>,
    last_id: Vec<u8>,
    #[cfg(test)]
    peak_batch_bytes: usize,
}

pub(crate) struct PublicationPage {
    kind: FactKind,
    ids: Vec<Vec<u8>>,
    missing: BTreeSet<Vec<u8>>,
    received: BTreeMap<Vec<u8>, Fact>,
}

impl PublicationPage {
    pub(crate) fn begin(
        store: &LayerStore,
        kind: FactKind,
        ids: &[Vec<u8>],
        missing: MissingBitmap,
        publication: &mut Option<PublicationSpool>,
    ) -> Result<Option<Self>> {
        missing.validate_tail(ids.len())?;
        let mut missing_ids = BTreeSet::new();
        for (index, id) in ids.iter().enumerate() {
            if missing.is_missing(index)? {
                missing_ids.insert(id.clone());
            }
        }
        if missing_ids.is_empty() {
            append_publication_page(store, kind, ids, BTreeMap::new(), publication)?;
            Ok(None)
        } else {
            Ok(Some(Self {
                kind,
                ids: ids.to_vec(),
                missing: missing_ids,
                received: BTreeMap::new(),
            }))
        }
    }

    pub(crate) fn receive(
        &mut self,
        store: &LayerStore,
        facts: &[Fact],
        publication: &mut Option<PublicationSpool>,
    ) -> Result<bool> {
        for fact in facts {
            let id = fact.id();
            if fact.kind() != self.kind
                || !self.missing.contains(&id)
                || self.received.insert(id, *fact).is_some()
            {
                return Err(StorageError::Integrity("publication missing set"));
            }
        }
        if self.received.len() == self.missing.len() {
            append_publication_page(
                store,
                self.kind,
                &self.ids,
                std::mem::take(&mut self.received),
                publication,
            )?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn append_publication_page(
    store: &LayerStore,
    kind: FactKind,
    ids: &[Vec<u8>],
    received: BTreeMap<Vec<u8>, Fact>,
    publication: &mut Option<PublicationSpool>,
) -> Result<()> {
    let received_ids = received.keys().cloned().collect::<BTreeSet<_>>();
    let mut rows = store.db.publication_facts(kind, ids)?;
    for (id, fact) in received {
        if let Some(known) = rows.insert(id, fact) {
            if known != fact {
                return Err(StorageError::Integrity("publication race"));
            }
        }
    }
    let facts = ids
        .iter()
        .map(|id| rows.remove(id).ok_or(StorageError::MissingBaseData))
        .collect::<Result<Vec<_>>>()?;
    if !rows.is_empty() {
        return Err(StorageError::Integrity("publication page"));
    }
    let spool = publication.get_or_insert(PublicationSpool::new()?);
    for fact in facts {
        spool.push(&[fact], received_ids.contains(&fact.id()))?;
    }
    Ok(())
}

impl PublicationSpool {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            memory: Vec::new(),
            memory_bytes: 0,
            spill: None,
            count: 0,
            last_rank: None,
            last_id: Vec::new(),
            #[cfg(test)]
            peak_batch_bytes: 0,
        })
    }

    pub(crate) fn push(&mut self, facts: &[Fact], admit: bool) -> Result<()> {
        let bytes = facts.iter().map(|fact| fact.encoded_size()).sum::<usize>();
        if facts.is_empty() || facts.len() > FACT_BATCH_COUNT || bytes > FACT_BATCH_BYTES {
            return Err(StorageError::InvalidInput("publication batch"));
        }
        #[cfg(test)]
        {
            self.peak_batch_bytes = self.peak_batch_bytes.max(bytes);
        }
        for fact in facts {
            let rank = match fact {
                Fact::Branch(_) => 0,
                Fact::AddResult(_) => 1,
                _ => return Err(StorageError::Integrity("Stack publication fact")),
            };
            let id = fact.id();
            if self.last_rank.is_some_and(|last| last > rank)
                || self.last_rank == Some(rank) && self.last_id >= id
            {
                return Err(StorageError::Integrity("Stack publication ordering"));
            }
            let charge = fact.signing_bytes().len() + 5;
            if self.spill.is_none() && self.memory_bytes + charge <= PUBLICATION_MEMORY_BYTES {
                self.memory.push((*fact, admit));
                self.memory_bytes += charge;
            } else {
                self.ensure_spill()?;
                write_publication_fact(&mut self.spill.as_mut().unwrap().1, *fact, admit)?;
            }
            self.last_rank = Some(rank);
            self.last_id = id;
            self.count += 1;
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.count == 0
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
            "layerfs-publication-{}-{nonce}-{}",
            std::process::id(),
            PUBLICATION_SPOOL_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut writer = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        for (fact, admit) in self.memory.drain(..) {
            write_publication_fact(&mut writer, fact, admit)?;
        }
        self.memory_bytes = 0;
        self.spill = Some((path, writer));
        Ok(())
    }

    fn prepare_read(&mut self) -> Result<()> {
        if let Some((_, writer)) = &mut self.spill {
            writer.flush()?;
        }
        Ok(())
    }

    fn reader(&self) -> Result<PublicationReader<'_>> {
        if let Some((path, _)) = &self.spill {
            Ok(PublicationReader::Spill(File::open(path)?))
        } else {
            Ok(PublicationReader::Memory(self.memory.iter()))
        }
    }

    fn verify(&mut self, push: &StackPush) -> Result<()> {
        self.prepare_read()?;
        let mut input = self.reader()?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/stack-publication/v1\0");
        let mut count = 0_u64;
        while let Some((fact, _)) = input.next()? {
            let bytes = fact.signing_bytes();
            hasher.update(&(bytes.len() as u64).to_be_bytes());
            hasher.update(&bytes);
            count += 1;
        }
        if count == push.publication_count
            && *hasher.finalize().as_bytes() == push.publication_digest
        {
            Ok(())
        } else {
            Err(StorageError::Integrity("Stack publication digest"))
        }
    }

    fn verify_relations(
        &mut self,
        store: &LayerStore,
        push: &StackPush,
        positions: &layerfs_storage_core::StackPositions,
    ) -> Result<()> {
        self.prepare_read()?;
        let mut branches = self.reader()?;
        let mut results = self.reader()?;
        let mut pairs = Vec::with_capacity(64);
        loop {
            let branch = next_branch(&mut branches)?;
            let result = next_result(&mut results)?;
            let (Some(branch), Some(result)) = (branch, result) else {
                if branch.is_some() || result.is_some() {
                    return Err(StorageError::Integrity("AddResult relationship"));
                }
                if !pairs.is_empty() {
                    store
                        .db
                        .validate_stack_publication(push, &pairs, positions)?;
                }
                return Ok(());
            };
            pairs.push((branch, result));
            if pairs.len() == 64 {
                store
                    .db
                    .validate_stack_publication(push, &pairs, positions)?;
                pairs.clear();
            }
        }
    }

    fn visit_batches(
        &mut self,
        visitor: &mut dyn FnMut(&[Fact], bool) -> Result<()>,
    ) -> Result<()> {
        self.prepare_read()?;
        let mut input = self.reader()?;
        let mut batch: Vec<Fact> = Vec::with_capacity(FACT_BATCH_COUNT);
        let mut bytes = 0;
        while let Some((fact, admit)) = input.next()? {
            if !admit {
                continue;
            }
            if !batch.is_empty()
                && (batch.len() == FACT_BATCH_COUNT
                    || batch[0].kind() != fact.kind()
                    || bytes + fact.encoded_size() > FACT_BATCH_BYTES)
            {
                visitor(&batch, false)?;
                batch.clear();
                bytes = 0;
            }
            bytes += fact.encoded_size();
            batch.push(fact);
        }
        if !batch.is_empty() {
            visitor(&batch, true)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn spilled(&self) -> bool {
        self.spill.is_some()
    }
}

impl Drop for PublicationSpool {
    fn drop(&mut self) {
        if let Some((path, _)) = &self.spill {
            let _ = std::fs::remove_file(path);
        }
    }
}

enum PublicationReader<'a> {
    Memory(std::slice::Iter<'a, (Fact, bool)>),
    Spill(File),
}

impl PublicationReader<'_> {
    fn next(&mut self) -> Result<Option<(Fact, bool)>> {
        match self {
            Self::Memory(values) => Ok(values.next().copied()),
            Self::Spill(input) => read_publication_fact(input),
        }
    }
}

fn write_publication_fact(output: &mut File, fact: Fact, admit: bool) -> Result<()> {
    let bytes = fact.signing_bytes();
    output.write_all(&[u8::from(admit)])?;
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(&bytes)?;
    Ok(())
}

fn read_publication_fact(input: &mut File) -> Result<Option<(Fact, bool)>> {
    let mut admit = [0];
    if input.read(&mut admit)? == 0 {
        return Ok(None);
    }
    if admit[0] > 1 {
        return Err(StorageError::Integrity("Stack publication spool"));
    }
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 1024 {
        return Err(StorageError::Integrity("Stack publication spool length"));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    let fact = layerfs_storage_core::decode_fact(&bytes)?;
    if !matches!(fact, Fact::Branch(_) | Fact::AddResult(_)) {
        return Err(StorageError::Integrity("Stack publication fact"));
    }
    Ok(Some((fact, admit[0] != 0)))
}

fn next_branch(input: &mut PublicationReader<'_>) -> Result<Option<BranchRecord>> {
    match input.next()? {
        Some((Fact::Branch(value), _)) => Ok(Some(value)),
        Some((Fact::AddResult(_), _)) | None => Ok(None),
        _ => unreachable!(),
    }
}

fn next_result(input: &mut PublicationReader<'_>) -> Result<Option<AddResultRecord>> {
    loop {
        match input.next()? {
            Some((Fact::Branch(_), _)) => {}
            Some((Fact::AddResult(value), _)) => return Ok(Some(value)),
            None => return Ok(None),
            _ => unreachable!(),
        }
    }
}

fn missing_page<T: Copy, const N: usize>(
    kind: FactKind,
    page: &[T],
    id: impl Fn(T) -> [u8; N],
    membership: &mut Membership<'_>,
    visitor: &mut dyn FnMut(&[T]) -> Result<()>,
) -> Result<()> {
    let mut ids = page
        .iter()
        .copied()
        .map(|row| id(row).to_vec())
        .collect::<Vec<_>>();
    ids.sort();
    let missing = membership(kind, &ids)?;
    missing.validate_tail(ids.len())?;
    let mut selected_ids = std::collections::BTreeSet::new();
    for (index, value) in ids.into_iter().enumerate() {
        if missing.is_missing(index)? {
            selected_ids.insert(value);
        }
    }
    let selected = page
        .iter()
        .copied()
        .filter(|row| selected_ids.contains(id(*row).as_slice()))
        .collect::<Vec<_>>();
    visitor(&selected)
}

struct LayerTransfer<'a> {
    store: &'a LayerStore,
    _permit: layerfs_storage_core::OperationPermit<'a>,
    attestation: StackAttestation,
    publication: Option<PublicationSpool>,
    publication_page: Option<PublicationPage>,
}

impl TransferTarget for LayerTransfer<'_> {
    fn preflight_branch(
        &mut self,
        branch: BranchRecord,
        root: ObjectId,
    ) -> Result<(Option<CommitId>, bool, layerfs_storage_core::MissingBitmap)> {
        let (current, up_to_date) = self.store.db.preflight_branch_push(branch)?;
        Ok((current, up_to_date, self.store.db.missing_objects(&[root])?))
    }

    fn preflight_stack(
        &mut self,
        history_id: StackHistoryId,
        base_layer_id: LayerId,
        incoming: StackId,
        root: ObjectId,
    ) -> Result<(Option<StackId>, bool, layerfs_storage_core::MissingBitmap)> {
        let (current, up_to_date) =
            self.store
                .db
                .preflight_stack_push(history_id, base_layer_id, incoming)?;
        Ok((current, up_to_date, self.store.db.missing_objects(&[root])?))
    }

    fn exchange(
        &mut self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange> {
        if self.publication_page.is_some() {
            return Err(StorageError::Integrity("incomplete publication page"));
        }
        if let Some((kind, ids)) = fact_ids {
            self.attestation.observe(kind, ids);
        }
        let exchange = self
            .store
            .transfer_exchange_unlocked(objects, facts, object_ids, fact_ids)?;
        if let Some((kind, ids)) =
            fact_ids.filter(|(kind, _)| matches!(kind, FactKind::Branch | FactKind::AddResult))
        {
            self.publication_page = PublicationPage::begin(
                self.store,
                kind,
                ids,
                exchange.missing().1,
                &mut self.publication,
            )?;
        }
        Ok(exchange)
    }

    fn defer_publication(&mut self, facts: &[Fact]) -> Result<()> {
        let page = self
            .publication_page
            .as_mut()
            .ok_or(StorageError::Integrity("publication announcement"))?;
        if page.receive(self.store, facts, &mut self.publication)? {
            self.publication_page = None;
        }
        Ok(())
    }

    fn finish(
        self: Box<Self>,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
    ) -> Result<(TransferExchange, TransferOutcome)> {
        let LayerTransfer {
            store,
            _permit,
            attestation,
            publication,
            publication_page,
        } = *self;
        if publication_page.is_some() {
            return Err(StorageError::Integrity("incomplete publication page"));
        }
        store.finish_local_transfer(objects, facts, intent, attestation, publication)
    }
}

impl LayerStore {
    fn verify_stack_push(&self, push: &StackPush) -> Result<()> {
        if push.history_id.verification_key_digest() != *blake3::hash(&push.public_key).as_bytes() {
            return Err(StorageError::Integrity("Stack writer key"));
        }
        let key = VerifyingKey::from_bytes(&push.public_key)
            .map_err(|_| StorageError::Integrity("Stack writer key"))?;
        key.verify(
            &push.signing_bytes(),
            &Signature::from_bytes(&push.signature),
        )
        .map_err(|_| StorageError::Integrity("Stack writer signature"))
    }

    pub(crate) fn finish_received_transfer(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
        attestation: StackAttestation,
        publication: Option<PublicationSpool>,
    ) -> Result<(TransferExchange, TransferOutcome)> {
        self.finish_transfer_with_authentication(
            objects,
            facts,
            intent,
            attestation,
            publication,
            true,
        )
    }

    fn finish_local_transfer(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
        attestation: StackAttestation,
        publication: Option<PublicationSpool>,
    ) -> Result<(TransferExchange, TransferOutcome)> {
        self.finish_transfer_with_authentication(
            objects,
            facts,
            intent,
            attestation,
            publication,
            false,
        )
    }

    fn finish_transfer_with_authentication(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
        attestation: StackAttestation,
        publication: Option<PublicationSpool>,
        authenticate: bool,
    ) -> Result<(TransferExchange, TransferOutcome)> {
        let TransferIntent::Stack(push) = intent else {
            return if authenticate {
                self.db.finish_transfer(objects, facts, intent)
            } else {
                self.db.finish_transfer_local(objects, facts, intent)
            };
        };
        if facts
            .iter()
            .any(|fact| matches!(fact, Fact::Branch(_) | Fact::AddResult(_)))
        {
            return Err(StorageError::Integrity("Stack publication ordering"));
        }
        self.verify_stack_push(&push)?;
        attestation.verify(&push)?;
        let Some(mut publication) = publication else {
            verify_empty_publication(&push)?;
            return if authenticate {
                self.db
                    .finish_transfer(objects, facts, TransferIntent::Stack(push))
            } else {
                self.db
                    .finish_transfer_local(objects, facts, TransferIntent::Stack(push))
            };
        };
        publication.verify(&push)?;
        if publication.is_empty() {
            let (exchange, outcome) = if authenticate {
                self.db
                    .finish_transfer(objects, facts, TransferIntent::Stack(push))?
            } else {
                self.db
                    .finish_transfer_local(objects, facts, TransferIntent::Stack(push))?
            };
            return Ok((exchange, outcome));
        }
        let mut exchange = if authenticate {
            self.db.transfer_exchange(objects, facts, &[], None, true)?
        } else {
            self.db
                .transfer_exchange(objects, facts, &[], None, false)?
        };
        let positions = self
            .db
            .stack_positions(push.history_id, push.incoming_head)?;
        self.verify_stack_foundation(&push, &positions)?;
        publication.verify_relations(self, &push, &positions)?;
        let mut outcome = None;
        publication.visit_batches(&mut |page, last| {
            if last {
                let (final_exchange, final_outcome) = self.db.finish_transfer_local(
                    &[],
                    page,
                    TransferIntent::Stack(push.clone()),
                )?;
                exchange.absorb(final_exchange)?;
                outcome = Some(final_outcome);
            } else {
                exchange.absorb(self.transfer_exchange_unlocked(&[], page, &[], None)?)?;
            }
            Ok(())
        })?;
        if outcome.is_none() {
            let (final_exchange, final_outcome) =
                self.db
                    .finish_transfer_local(&[], &[], TransferIntent::Stack(push.clone()))?;
            exchange.absorb(final_exchange)?;
            outcome = Some(final_outcome);
        }
        Ok((
            exchange,
            outcome.ok_or(StorageError::Integrity("Stack publication spool"))?,
        ))
    }

    fn verify_stack_foundation(
        &self,
        push: &StackPush,
        positions: &layerfs_storage_core::StackPositions,
    ) -> Result<()> {
        let incoming = self
            .db
            .stack(push.incoming_head)?
            .ok_or(StorageError::MissingBaseData)?;
        if incoming.history_id != push.history_id {
            return Err(StorageError::Integrity("Stack history"));
        }
        if let Some(expected) = push.expected_head {
            let prior = self
                .db
                .stack(expected)?
                .ok_or(StorageError::MissingBaseData)?;
            if prior.history_id != push.history_id || positions.position(expected)?.is_none() {
                return Err(StorageError::Integrity("Stack suffix predecessor"));
            }
        }
        let base = self
            .db
            .layer(push.base_layer_id)?
            .ok_or(StorageError::MissingBaseData)?;
        if self.db.has_object(base.root_id)? {
            Ok(())
        } else {
            Err(StorageError::MissingBaseData)
        }
    }
}

fn verify_empty_publication(push: &StackPush) -> Result<()> {
    let digest = *blake3::hash(b"layerfs/stack-publication/v1\0").as_bytes();
    if push.publication_count == 0 && push.publication_digest == digest {
        Ok(())
    } else {
        Err(StorageError::Integrity("Stack publication digest"))
    }
}

fn base_layer_id(db: &layerfs_storage_core::StoreDb, base: BaseId) -> Result<LayerId> {
    match base {
        BaseId::Layer(id) => Ok(id),
        BaseId::Stack(id) => {
            let stack = db.stack(id)?.ok_or(StorageError::MissingBaseData)?;
            Ok(db
                .stack_history(stack.history_id)?
                .ok_or(StorageError::MissingBaseData)?
                .base_layer_id)
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/transfer_unit.rs"]
mod tests;
