//! Ordered-start multiplexed fragments with admission before body allocation.
use super::{credits::Credit, Message, ReassemblyConfig, ReassemblyWork, ReceiveBudget};
use crate::contract::{Fragment, FrameError, FrameResult};
use std::collections::BTreeMap;
/// Direction-owned checked partial bodies. SDK validates operation/capability and
/// class before submitting an envelope here; framing alone does not grant authority.
pub struct Reassembly {
    config: ReassemblyConfig,
    last_started: u64,
    last_request: u64,
    partial: BTreeMap<u64, Message>,
    owner: u64,
    budget: ReceiveBudget,
}
impl Reassembly {
    /// Chooses bounded live receive admission once, before reading body fragments.
    pub fn new(config: ReassemblyConfig) -> FrameResult<Self> {
        Self::with_budget(ReceiveBudget::new(config)?)
    }
    /// Uses the host/client's shared aggregate across channel owners.
    pub fn with_budget(budget: ReceiveBudget) -> FrameResult<Self> {
        let owner = budget.owner()?;
        Ok(Self {
            config: budget.config(),
            last_started: 0,
            last_request: 0,
            partial: BTreeMap::new(),
            owner,
            budget,
        })
    }
    /// Copies one authenticated checked fragment once. Errors retain earlier
    /// partial input; fence/drain it explicitly rather than retrying/replaying.
    pub fn push(&mut self, fragment: Fragment<'_>) -> FrameResult<Option<Message>> {
        fragment.validate()?;
        let envelope = fragment.envelope;
        if envelope.kind != self.config.kind {
            return Err(FrameError::Invalid("receive direction"));
        }
        if fragment.offset == 0 {
            if envelope.message <= self.last_started {
                return Err(FrameError::Invalid("message ID reused/out of start order"));
            }
            if envelope.kind == crate::contract::MessageKind::Request
                && envelope.correlation <= self.last_request
            {
                return Err(FrameError::Invalid(
                    "request correlation reused/out of order",
                ));
            }
            let size = usize::try_from(envelope.total_bytes)
                .map_err(|_| FrameError::Invalid("message platform size"))?;
            if size > self.config.message_bytes {
                return Err(FrameError::Invalid("adapter message window"));
            }
            let charge = size
                .checked_add(std::mem::size_of::<Message>())
                .ok_or(FrameError::Invalid("message charge"))?;
            let mut credit = Credit::acquire(
                &self.budget.ledger,
                (self.owner, envelope.message),
                envelope.class,
                charge,
            )?;
            // Once allocation begins this message ID is burned, including failure.
            self.last_started = envelope.message;
            if envelope.kind == crate::contract::MessageKind::Request {
                self.last_request = envelope.correlation;
            }
            {
                let mut ledger = self
                    .budget
                    .ledger
                    .lock()
                    .map_err(|_| FrameError::Poisoned)?;
                ledger.work.allocation_attempts = ledger.work.allocation_attempts.saturating_add(1);
                ledger.work.requested_body_bytes =
                    ledger.work.requested_body_bytes.saturating_add(size as u64);
            }
            let mut body = Vec::new();
            body.try_reserve_exact(size)
                .map_err(|error| FrameError::Allocation {
                    requested_bytes: size,
                    error,
                })?;
            credit.add_capacity(body.capacity() - size)?;
            self.partial.insert(
                envelope.message,
                Message {
                    envelope,
                    body,
                    credit,
                },
            );
        }
        let message = self
            .partial
            .get_mut(&envelope.message)
            .ok_or(FrameError::Invalid("fragment before start"))?;
        if message.envelope != envelope || message.body.len() as u64 != fragment.offset {
            return Err(FrameError::Invalid("fragment gap/overlap/envelope"));
        }
        message.body.extend_from_slice(fragment.bytes);
        let mut ledger = message
            .credit
            .ledger
            .lock()
            .map_err(|_| FrameError::Poisoned)?;
        ledger.work.fragments = ledger.work.fragments.saturating_add(1);
        ledger.work.copied_bytes = ledger
            .work
            .copied_bytes
            .saturating_add(fragment.bytes.len() as u64);
        drop(ledger);
        if fragment.is_end() {
            Ok(self.partial.remove(&envelope.message))
        } else {
            Ok(None)
        }
    }
    /// Fences this collector's partial inputs after its I/O owner has stopped.
    /// Original bodies and credits move to caller custody; no adapter is invoked.
    pub fn drain_partial(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.partial).into_values().collect()
    }
    /// Actual cumulative copies and live body/receipt credit; poison is unavailable.
    pub fn work(&self) -> FrameResult<ReassemblyWork> {
        self.budget.work()
    }
    /// Immutable selected processing windows.
    pub const fn config(&self) -> ReassemblyConfig {
        self.config
    }
}
