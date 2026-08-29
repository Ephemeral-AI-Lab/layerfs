use crate::StackStore;
use layerfs_storage_core::{
    decode_fact, BaseId, Fact, LayerHistoryId, LayerHistoryRecord, LayerId, RefOutcome, Result,
    StackHistoryId, StackHistoryRecord, StackId, StorageError, TransferPipeline, ID_BATCH_COUNT,
};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFERRED_MEMORY_BYTES: usize = 8 * 1024 * 1024;
static DEFERRED_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct DeferredFactStore {
    memory: Vec<Fact>,
    memory_bytes: usize,
    spill: Option<(PathBuf, File)>,
}

impl DeferredFactStore {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            memory: Vec::new(),
            memory_bytes: 0,
            spill: None,
        })
    }

    pub(crate) fn stage(&mut self, fact: Fact) -> Result<()> {
        let bytes = fact.signing_bytes();
        let charge = bytes.len() + 4;
        if self.spill.is_none() && self.memory_bytes + charge <= DEFERRED_MEMORY_BYTES {
            self.memory.push(fact);
            self.memory_bytes += charge;
            return Ok(());
        }
        self.ensure_spill()?;
        write_fact(&mut self.spill.as_mut().unwrap().1, &bytes)
    }

    pub(crate) fn visit_batches(
        &mut self,
        visitor: &mut dyn FnMut(&[Fact]) -> Result<()>,
    ) -> Result<()> {
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

    #[cfg(test)]
    fn spilled(&self) -> bool {
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

impl StackStore {
    pub fn pull_layer_history(
        &self,
        history_id: LayerHistoryId,
        through: LayerId,
    ) -> Result<RefOutcome<LayerId>> {
        let mut transfer = TransferPipeline::new(self)?;
        self.pull_layers(history_id, through, &mut transfer)?;
        transfer.finish_layer_history(LayerHistoryRecord {
            id: history_id,
            head_layer_id: through,
        })
    }

    fn pull_layers(
        &self,
        history_id: LayerHistoryId,
        through: LayerId,
        transfer: &mut TransferPipeline<'_>,
    ) -> Result<()> {
        self.parent.layer_history_record(history_id)?;
        let mut spool = DeferredFactStore::new()?;
        self.parent.visit_layers(
            history_id,
            through,
            &mut |kind, ids| transfer.announce_fact_ids(kind, ids),
            &mut |layers| {
                for layer in layers {
                    spool.stage(Fact::Layer(*layer))?;
                }
                Ok(())
            },
        )?;
        spool.visit_batches(&mut |facts| {
            let roots = facts
                .iter()
                .map(|fact| match fact {
                    Fact::Layer(layer) => Ok(layer.root_id),
                    _ => Err(StorageError::Integrity("Layer pull spool")),
                })
                .collect::<Result<Vec<_>>>()?;
            transfer.snapshots(self.parent.as_ref(), &roots)?;
            transfer.stage_facts(facts)
        })
    }

    pub fn pull_stack_history(
        &self,
        history_id: StackHistoryId,
        through: StackId,
    ) -> Result<RefOutcome<StackId>> {
        let mut transfer = TransferPipeline::new(self)?;
        let history = self.parent.stack_history_record(history_id)?;
        let base = self
            .parent
            .base_snapshot(BaseId::Layer(history.base_layer_id))?;
        self.pull_layers(base.layer_history_id, history.base_layer_id, &mut transfer)?;
        let mut spool = DeferredFactStore::new()?;
        self.parent.visit_stacks(
            history_id,
            through,
            &mut |kind, ids| transfer.announce_fact_ids(kind, ids),
            &mut |stacks| {
                for stack in stacks {
                    spool.stage(Fact::Stack(*stack))?;
                }
                Ok(())
            },
        )?;
        spool.visit_batches(&mut |facts| {
            let roots = facts
                .iter()
                .map(|fact| match fact {
                    Fact::Stack(stack) => Ok(stack.root_id),
                    _ => Err(StorageError::Integrity("Stack pull spool")),
                })
                .collect::<Result<Vec<_>>>()?;
            transfer.snapshots(self.parent.as_ref(), &roots)?;
            transfer.stage_facts(facts)
        })?;
        let expected = self
            .db
            .stack_history(history_id)?
            .map(|value| value.head_stack_id);
        transfer.finish_stack_history(
            StackHistoryRecord {
                head_stack_id: through,
                ..history
            },
            expected,
        )
    }
}

#[cfg(test)]
#[path = "../tests/support/history_pull_unit.rs"]
mod tests;
