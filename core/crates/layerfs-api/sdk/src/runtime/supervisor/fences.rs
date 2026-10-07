//! Exact service cancellation and independently joined input/output custody.
use super::{attachment::Attachment, *};
use crate::runtime::RuntimeResult;

impl Supervisor<'_, '_> {
    /// Starts this attachment's explicit service/socket fences once. It does not
    /// abort/release Saves, resolve unknown publication or terminate a Workspace.
    pub fn fence(&mut self, id: AttachmentId) -> RuntimeResult<()> {
        self.invalidate_round();
        let slot = self.slot(id)?;
        let mut attachment = self.attachments[slot].take().expect("checked attachment");
        self.begin_fence(&mut attachment, None);
        self.attachments[slot] = Some(attachment);
        Ok(())
    }
    /// Observes both actual worker exits without waiting, returning original
    /// request/result/partial/error custody once. A close result alone is no fence.
    pub fn try_join(&mut self, id: AttachmentId) -> RuntimeResult<Option<AttachmentFence>> {
        self.invalidate_round();
        let slot = self.slot(id)?;
        let mut attachment = self.attachments[slot].take().expect("checked attachment");
        let fence = Self::join(&mut attachment);
        if fence.is_some() {
            self.work.attachments -= 1;
            self.work.fenced = self.work.fenced.saturating_add(1);
        } else {
            self.attachments[slot] = Some(attachment);
        }
        Ok(fence)
    }
    pub(super) fn begin_fence(
        &mut self,
        attachment: &mut Attachment,
        failure: Option<SupervisorFailure>,
    ) {
        if attachment.service_fence.is_some() {
            return;
        }
        attachment.failure = failure;
        attachment.service_fence = Some(
            self.service
                .disconnect(attachment.id.0)
                .expect("live supervisor attachment"),
        );
        if let Some(exchange) = &mut attachment.exchange {
            if let Some(ticket) = exchange.ticket.take() {
                exchange.custody.completion = self
                    .service
                    .take_completion(ticket)
                    .expect("owned fenced ticket");
            }
        }
        attachment.input.fence();
        attachment.output.fence();
        attachment
            .input_events
            .extend(attachment.input.detach_events());
        attachment
            .output_receipts
            .extend(attachment.output.detach_receipts());
    }
    pub(super) fn join(attachment: &mut Attachment) -> Option<AttachmentFence> {
        attachment.service_fence.as_ref()?;
        if attachment.input_fence.is_none() {
            attachment.input_fence = attachment.input.try_join();
        }
        if attachment.output_fence.is_none() {
            attachment.output_fence = attachment.output.try_join();
        }
        if attachment.input_fence.is_none() || attachment.output_fence.is_none() {
            return None;
        }
        let mut input = attachment.input_fence.take().expect("joined input");
        let mut output = attachment.output_fence.take().expect("joined output");
        input.events.append(&mut attachment.input_events);
        output.receipts.append(&mut attachment.output_receipts);
        Some(AttachmentFence {
            attachment: attachment.id,
            service: attachment.service_fence.take().expect("service fence"),
            input,
            output,
            request: attachment.exchange.take().map(|exchange| exchange.custody),
            failure: attachment.failure.take(),
        })
    }
}
impl Drop for Supervisor<'_, '_> {
    fn drop(&mut self) {
        for slot in 0..self.attachments.len() {
            if let Some(mut attachment) = self.attachments[slot].take() {
                self.begin_fence(&mut attachment, None);
            }
        }
    }
}
