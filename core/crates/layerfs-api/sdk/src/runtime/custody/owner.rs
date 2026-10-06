//! Consuming local completion fence; original receipts move without provider replay.
use super::{
    failure_knowledge, CustodyDisposition, FailureKnowledge, ScopeFenceError, ScopeFenceRefusal,
    ServingCustody, SlotCustody,
};
use crate::runtime::{SaveId, Sessions};

impl<'a> Sessions<'a> {
    /// Ends this local serving scope once and moves its original fixed-slot custody.
    ///
    /// Service, queued/dispatched jobs and caller-held ServiceCompletions must
    /// first release their exact service ownership. Refusal returns this Sessions
    /// unchanged before any producer/receipt effect. Native attachment fence/join
    /// remains the separate R1 boundary; this method does not claim worker exit.
    ///
    /// Active Save producers are dropped explicitly, preserving acknowledged
    /// waves through their existing owning Drop contract. No Finish/Abort receipt
    /// is synthesized, and no stage/transition/discard/delete or provider refresh
    /// is invoked. Known stages, cleanup failure and unknown outcomes move intact
    /// to application custody. That transfer supplies no automatic resolver,
    /// fresh authority, crash recovery or permission to start another Commit.
    #[allow(clippy::result_large_err)]
    pub fn fence(mut self) -> Result<ServingCustody, ScopeFenceRefusal<'a>> {
        let owners = self.service_owners.get();
        if owners != 0 {
            return Err(ScopeFenceRefusal {
                error: ScopeFenceError::RetainedCustody { owners },
                sessions: self,
            });
        }
        let mut slots = Vec::new();
        if let Err(error) = slots.try_reserve_exact(self.slots.len()) {
            return Err(ScopeFenceRefusal {
                error: ScopeFenceError::Allocation {
                    slots: self.slots.len(),
                    error,
                },
                sessions: self,
            });
        }
        let incarnation = self.owner;
        for (index, mut slot) in self.slots.drain(..).enumerate() {
            let Some(binding) = slot.binding.take() else {
                slots.push(None);
                continue;
            };
            let producer_ended = slot.save.is_some();
            // All provider dispatch returned before owner admission reached zero.
            // Existing Save Drop only ends producer/cache ownership; no Finish or
            // guessed publication/discard/cleanup operation is introduced here.
            drop(slot.save.take());
            let completion = slot.completion.take();
            let save_knowledge = completion
                .as_ref()
                .and_then(|c| c.outcome().as_ref().err())
                .map_or(FailureKnowledge::Known, failure_knowledge);
            let retains_custody = completion.as_ref().is_some_and(|c| c.retains_custody())
                || slot.history.retains_custody();
            let disposition = if save_knowledge == FailureKnowledge::TerminalUnknown
                || slot.history.terminal_unknown()
            {
                CustodyDisposition::TerminalUnknown
            } else if save_knowledge == FailureKnowledge::RetainedFailure {
                CustodyDisposition::RetainedFailure
            } else if completion.is_some() {
                CustodyDisposition::KnownTerminal
            } else {
                CustodyDisposition::ActiveProducerEnded
            };
            slots.push(Some(SlotCustody {
                binding,
                save: SaveId {
                    owner: incarnation,
                    slot: index,
                    serial: slot.serial,
                },
                completion,
                history: slot.history,
                producer_ended,
                disposition,
                save_knowledge,
                retains_custody,
            }));
        }
        Ok(ServingCustody { incarnation, slots })
    }
}
