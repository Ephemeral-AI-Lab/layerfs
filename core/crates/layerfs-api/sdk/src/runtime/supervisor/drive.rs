//! Bounded fair turns, pre-body grants and one-attempt provider/output transfers.
use super::{
    attachment::{Attachment, Exchange, Phase},
    *,
};
use crate::{
    client::Operation,
    runtime::{self, service::*},
};
use layerfs_bridge::{
    codec::Message,
    contract::{FrameError, MessageClass},
};
use std::sync::mpsc::TryRecvError;

impl Supervisor<'_, '_> {
    /// Performs one rotating nonblocking attachment turn and at most one provider
    /// job. No provider lock spans socket I/O. Caller-owned events keep all credits
    /// until released. `None` means no completed event, not no outstanding work.
    /// Drive this owner independently of consumer calls and explicit fence waits.
    pub fn step(&mut self) -> Option<SupervisorEvent> {
        self.work.turns = self.work.turns.saturating_add(1);
        let slot = self.next;
        self.next = (self.next + 1) % self.attachments.len();
        let mut event = None;
        if let Some(mut attachment) = self.attachments[slot].take() {
            if attachment.service_fence.is_none() {
                match turn(&mut self.service, &mut attachment, &mut self.work) {
                    Ok(Some(delivery)) => {
                        self.work.delivered = self.work.delivered.saturating_add(1);
                        event = Some(SupervisorEvent::Delivered(delivery));
                    }
                    Ok(None) => (),
                    Err(error) => self.begin_fence(&mut attachment, Some(error)),
                }
            }
            if let Some(fence) = Self::join(&mut attachment) {
                self.work.attachments -= 1;
                self.work.fenced = self.work.fenced.saturating_add(1);
                event = Some(SupervisorEvent::Fenced(fence));
            } else {
                self.attachments[slot] = Some(attachment);
            }
        }
        if let Some(ticket) = self.service.step() {
            let completion = self
                .service
                .take_completion(ticket)
                .expect("original dispatched ticket")
                .expect("dispatched result");
            let attachment = self
                .attachments
                .iter_mut()
                .flatten()
                .find(|attachment| {
                    attachment
                        .exchange
                        .as_ref()
                        .is_some_and(|exchange| exchange.ticket == Some(ticket))
                })
                .expect("admitted supervisor request owner");
            let exchange = attachment.exchange.as_mut().expect("owned exchange");
            exchange.ticket = None;
            exchange.custody.completion = Some(completion);
            exchange.phase = Phase::Result;
        }
        event
    }
}

fn turn(
    service: &mut Service<'_, '_>,
    attachment: &mut Attachment,
    work: &mut SupervisorWork,
) -> Result<Option<Delivery>, SupervisorFailure> {
    if attachment.exchange.is_none() {
        match attachment.input.try_event() {
            Ok(InputEvent::Admission { envelope, header }) => {
                work.headers = work.headers.saturating_add(1);
                attachment.exchange = Some(Exchange {
                    custody: RequestCustody {
                        envelope,
                        header,
                        input: None,
                        rejected: None,
                        completion: None,
                        refused: None,
                        admission_error: service.authorize_header(attachment.id.0, &header).err(),
                        unsent: None,
                        reservation: None,
                    },
                    phase: Phase::Header,
                    ticket: None,
                });
            }
            Ok(event) => {
                attachment.input_events.push(event);
                return Err(SupervisorFailure::Frame(FrameError::Invalid(
                    "input without admission",
                )));
            }
            Err(TryRecvError::Empty) => return Ok(None),
            Err(TryRecvError::Disconnected) => return Err(SupervisorFailure::InputStopped),
        }
    }
    let exchange = attachment
        .exchange
        .as_mut()
        .expect("current original exchange");
    match exchange.phase {
        Phase::Header => {
            let capacity = if exchange.custody.admission_error.is_some() {
                64 << 10
            } else {
                8
            };
            let Some(permit) = attachment
                .output
                .reserve(
                    exchange.custody.envelope.correlation,
                    MessageClass::Control,
                    capacity,
                )
                .map_err(SupervisorFailure::Frame)?
            else {
                work.output_waits = work.output_waits.saturating_add(1);
                return Ok(None);
            };
            exchange.custody.reservation = Some(permit);
            let reply = match &exchange.custody.admission_error {
                Some(error) => {
                    runtime::encode_refusal(error, capacity).map_err(SupervisorFailure::Frame)?
                }
                None => runtime::encode_grant(),
            };
            transfer(
                &attachment.output,
                exchange,
                reply.bytes,
                MessageClass::Control,
            )?;
            exchange.phase = if exchange.custody.admission_error.is_some() {
                Phase::Refusal
            } else {
                Phase::Grant
            };
        }
        Phase::Grant | Phase::Refusal | Phase::Reply => {
            match attachment.output.try_receipt() {
                Ok(receipt) => {
                    if receipt.packet.correlation != exchange.custody.envelope.correlation
                        || !receipt.complete
                    {
                        attachment.output_receipts.push(receipt);
                        return Err(SupervisorFailure::Frame(FrameError::Invalid(
                            "output receipt identity/state",
                        )));
                    }
                    match exchange.phase {
                        Phase::Grant => {
                            drop(receipt);
                            attachment
                                .input
                                .decide(exchange.custody.envelope, true)
                                .map_err(SupervisorFailure::Frame)?;
                            exchange.phase = Phase::Body;
                        }
                        Phase::Refusal => {
                            // NativeInput closes a refused header. Complete the original
                            // refusal send first so its own shutdown cannot erase it.
                            attachment.output_receipts.push(receipt);
                            attachment
                                .input
                                .decide(exchange.custody.envelope, false)
                                .map_err(SupervisorFailure::Frame)?;
                            return Err(SupervisorFailure::InputStopped);
                        }
                        Phase::Reply => {
                            let exchange = attachment.exchange.take().expect("completed exchange");
                            return Ok(Some(Delivery {
                                attachment: attachment.id,
                                header: exchange.custody.header,
                                completion: exchange.custody.completion,
                                refused: exchange.custody.refused,
                                output: receipt,
                            }));
                        }
                        _ => unreachable!(),
                    }
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Disconnected) => return Err(SupervisorFailure::OutputStopped),
            }
        }
        Phase::Body => match attachment.input.try_event() {
            Ok(InputEvent::Ready(message)) => receive(exchange, message, work)?,
            Ok(event) => {
                attachment.input_events.push(event);
                return Err(SupervisorFailure::Frame(FrameError::Invalid(
                    "input body event order",
                )));
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => return Err(SupervisorFailure::InputStopped),
        },
        Phase::Ready => {
            let capacity = reply_capacity(exchange.custody.header);
            let Some(permit) = attachment
                .output
                .reserve(
                    exchange.custody.envelope.correlation,
                    exchange.custody.header.operation.class(),
                    capacity,
                )
                .map_err(SupervisorFailure::Frame)?
            else {
                work.output_waits = work.output_waits.saturating_add(1);
                return Ok(None);
            };
            exchange.custody.reservation = Some(permit);
            let input = exchange
                .custody
                .input
                .take()
                .expect("ready original request");
            match service.try_submit(attachment.id.0, input.request) {
                Ok(ticket) => {
                    exchange.ticket = Some(ticket);
                    exchange.phase = Phase::Queued;
                }
                Err((error, request)) => {
                    exchange.custody.refused = Some(RefusedInput {
                        error,
                        request,
                        lease: input.lease,
                    });
                    exchange.phase = Phase::Result;
                }
            }
        }
        Phase::Result => {
            let capacity = reply_capacity(exchange.custody.header);
            let reply = match &exchange.custody.completion {
                Some(completion) => runtime::encode_reply(service, completion, capacity),
                None => runtime::encode_refusal(
                    &exchange
                        .custody
                        .refused
                        .as_ref()
                        .expect("original refusal")
                        .error,
                    capacity,
                ),
            }
            .map_err(SupervisorFailure::Frame)?;
            work.reply_copied_bytes = work
                .reply_copied_bytes
                .saturating_add(reply.canonical_copied_bytes);
            transfer(
                &attachment.output,
                exchange,
                reply.bytes,
                exchange.custody.header.operation.class(),
            )?;
            exchange.phase = Phase::Reply;
        }
        Phase::Queued => (),
    }
    Ok(None)
}
fn receive(
    exchange: &mut Exchange,
    message: Message,
    work: &mut SupervisorWork,
) -> Result<(), SupervisorFailure> {
    if message.envelope() != exchange.custody.envelope {
        exchange.custody.rejected = Some(message);
        return Err(SupervisorFailure::Frame(FrameError::Invalid(
            "original input identity",
        )));
    }
    match runtime::decode_request(message) {
        Ok(input) => {
            work.received = work.received.saturating_add(1);
            work.input_copied_bytes = work.input_copied_bytes.saturating_add(input.copied_bytes);
            exchange.custody.input = Some(input);
            exchange.phase = Phase::Ready;
            Ok(())
        }
        Err((error, message)) => {
            exchange.custody.rejected = Some(message);
            Err(SupervisorFailure::Frame(error))
        }
    }
}
fn transfer(
    output: &NativeOutput,
    exchange: &mut Exchange,
    bytes: Vec<u8>,
    class: MessageClass,
) -> Result<(), SupervisorFailure> {
    let packet = OutputPacket {
        correlation: exchange.custody.envelope.correlation,
        class,
        bytes,
    };
    let permit = exchange
        .custody
        .reservation
        .take()
        .expect("pre-encoding reply reservation");
    match output.submit_reserved(permit, packet) {
        Ok(()) => Ok(()),
        Err((error, packet, permit)) => {
            exchange.custody.unsent = Some((packet, permit));
            Err(SupervisorFailure::Output(error))
        }
    }
}
fn reply_capacity(header: crate::client::RequestHeader) -> usize {
    match header.operation {
        Operation::Objects => {
            (header.value as usize * (16 << 20)).min(32 << 20)
                + (header.value as usize * 36)
                + (64 << 10)
        }
        Operation::Lengths => (header.value as usize * 40) + (64 << 10),
        _ => 64 << 10,
    }
}
