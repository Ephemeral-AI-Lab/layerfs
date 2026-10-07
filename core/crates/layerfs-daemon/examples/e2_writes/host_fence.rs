//! Source-scoped joined evidence; originals stay in the caller through refusal.
use super::super::json::Json;
use layerfs_bridge::native::{ChannelError, FramingError};
use layerfs_sdk::runtime::{
    service::{InputFailure, Response, ServiceOutcome},
    supervisor::{AttachFailure, AttachmentFence, AttachmentId, Delivery, SupervisorFailure},
    Binding,
};
use std::io;

pub fn final_binding_matches(
    delivery: &Delivery,
    expected: &Binding,
    correlation: u64,
    bytes: &[u8],
) -> bool {
    delivery.header.operation == layerfs_sdk::client::Operation::Binding
        && delivery.refused.is_none()
        && delivery.output.complete
        && delivery.output.completed_bytes == delivery.output.packet.bytes.len() as u64
        && delivery.output.packet.correlation == correlation
        && delivery.output.packet.bytes.as_slice() == bytes
        && delivery.completion.as_ref().is_some_and(|completion| {
            matches!(completion.outcome(), ServiceOutcome::Dispatched(Ok(Response::Binding(actual)))
                if actual.as_ref() == expected)
        })
}

/// Explicit application fencing has its own original result predicate. The
/// historical peer-EOF predicate below is deliberately unchanged.
pub fn record_explicit(
    out: &mut Json,
    fence: &AttachmentFence,
    expected: AttachmentId,
) -> io::Result<bool> {
    let mut valid = fence.attachment == expected
        && fence.failure.is_none()
        && fence.request.is_none()
        && fence.input.events.is_empty()
        && fence.output.receipts.is_empty()
        && fence.service.cancelled == 0
        && fence.service.completed == 0
        && matches!(fence.input.close, Some(Ok(())))
        && matches!(fence.output.close, Some(Err(ChannelError::Quarantined)));
    out.raw(",")?;
    out.field("attachment_matches", fence.attachment == expected)?;
    out.raw(",")?;
    out.field("inflight_request", fence.request.is_some())?;
    out.raw(",")?;
    out.field("input_events", fence.input.events.len())?;
    out.raw(",")?;
    out.field("output_receipts", fence.output.receipts.len())?;
    out.raw(",")?;
    out.field("service_cancelled", fence.service.cancelled)?;
    out.raw(",")?;
    out.field("service_completed", fence.service.completed)?;
    out.raw(",")?;
    out.text("input_close", &format!("{:?}", fence.input.close))?;
    out.raw(",")?;
    out.text("output_close", &format!("{:?}", fence.output.close))?;
    out.raw(",\"supervisor_failure\":")?;
    match &fence.failure {
        Some(value) => out.string(&cause(value))?,
        None => out.raw("null")?,
    }
    out.raw(",\"input_worker\":{")?;
    match &fence.input.worker {
        Err(_) => {
            valid = false;
            out.raw("\"status\":\"PANIC-original-payload-retained\"")?;
        }
        Ok(report) => {
            let native = report.native;
            let eof = matches!(&report.failure,
                InputFailure::Native(FramingError::Native(ChannelError::Io(error)))
                    if error.kind() == io::ErrorKind::UnexpectedEof);
            let quarantined = matches!(
                &report.failure,
                InputFailure::Native(FramingError::Native(ChannelError::Quarantined))
            );
            let complete_wire = native
                .records
                .checked_mul(18)
                .and_then(|overhead| native.plaintext_bytes.checked_add(overhead))
                == Some(native.wire_bytes);
            let exact_attempts = (eof
                && native.records.checked_add(1) == Some(native.record_io_attempts))
                || (quarantined && native.record_io_attempts == native.records);
            let clean = complete_wire && exact_attempts;
            valid &= clean
                && report.partial.is_empty()
                && report.undelivered.is_none()
                && report.close_failure.is_none()
                && report
                    .reassembly
                    .as_ref()
                    .is_ok_and(|value| value.live_messages == 0 && value.credited_bytes == 0);
            out.raw("\"status\":\"JOINED\",")?;
            out.text("original_failure", &input_cause(&report.failure))?;
            out.raw(",")?;
            out.text(
                "stop_kind",
                if eof {
                    "interrupted-idle-read-eof"
                } else if quarantined {
                    "pre-io-quarantined"
                } else {
                    "UNEXPECTED"
                },
            )?;
            out.raw(",")?;
            out.field("complete_record_accounting", clean)?;
            out.raw(",")?;
            out.field("partial_messages", report.partial.len())?;
            out.raw(",")?;
            out.field("undelivered", report.undelivered.is_some())?;
            out.raw(",")?;
            out.text("close_failure", &format!("{:?}", report.close_failure))?;
            out.raw(",")?;
            out.text("native_debug", &format!("{native:?}"))?;
            out.raw(",")?;
            out.text("reassembly_debug", &format!("{:?}", report.reassembly))?;
            for (name, value) in [
                ("records", native.records),
                ("record_io_attempts", native.record_io_attempts),
                ("plaintext_bytes", native.plaintext_bytes),
                ("wire_bytes", native.wire_bytes),
            ] {
                out.raw(",")?;
                out.field(name, value)?;
            }
            out.raw(",\"live_messages\":")?;
            match &report.reassembly {
                Ok(value) => out.raw(&value.live_messages.to_string())?,
                Err(_) => out.raw("null")?,
            }
            out.raw(",\"credited_bytes\":")?;
            match &report.reassembly {
                Ok(value) => out.raw(&value.credited_bytes.to_string())?,
                Err(_) => out.raw("null")?,
            }
        }
    }
    out.raw("},\"output_worker\":{")?;
    match &fence.output.worker {
        Err(_) => {
            valid = false;
            out.raw("\"status\":\"PANIC-original-payload-retained\"")?;
        }
        Ok(report) => {
            valid &= report.failure.is_none() && report.retained.is_empty();
            out.raw("\"status\":\"JOINED\",")?;
            out.text("original_failure", &format!("{:?}", report.failure))?;
            out.raw(",")?;
            out.field("retained_receipts", report.retained.len())?;
            out.raw(",")?;
            out.text("framing_debug", &format!("{:?}", report.framing))?;
            out.raw(",")?;
            out.text("native_debug", &format!("{:?}", report.native))?;
        }
    }
    out.raw("},")?;
    out.field("expected_explicit_fence", valid)?;
    out.raw(",\"scope\":\"original-explicit-joined-input-output-service-custody-no-product-publication-inference\"}")?;
    Ok(valid)
}

pub fn input_cause(value: &InputFailure) -> String {
    match value {
        InputFailure::Native(error) => format!("Native({error:?})"),
        InputFailure::Frame(error) => format!("Frame({error:?})"),
        InputFailure::Refused(envelope) => format!("Refused({envelope:?})"),
        InputFailure::Detached => "Detached".into(),
        InputFailure::Thread(error) => format!("Thread({error:?})"),
    }
}
pub fn attach_cause(value: &AttachFailure) -> String {
    match value {
        AttachFailure::Runtime { error, .. } => format!("Runtime({error:?})"),
        AttachFailure::Input { error, .. } => input_cause(error),
        AttachFailure::Output { error, .. } => format!("Output({error:?})"),
    }
}
fn cause(value: &SupervisorFailure) -> String {
    match value {
        SupervisorFailure::Frame(error) => format!("Frame({error:?})"),
        SupervisorFailure::Runtime(error) => format!("Runtime({error:?})"),
        SupervisorFailure::Output(error) => format!("Output({error:?})"),
        SupervisorFailure::InputStopped => "InputStopped".into(),
        SupervisorFailure::OutputStopped => "OutputStopped".into(),
    }
}
fn close_ok(value: &Option<Result<(), ChannelError>>) -> bool {
    // EOF quarantines the shared native channel before the supervisor fences it.
    // Quarantined is the original pre-I/O close refusal, not a new successful close.
    matches!(value, Some(Ok(())) | Some(Err(ChannelError::Quarantined)))
}
pub fn record(
    out: &mut Json,
    fence: &AttachmentFence,
    expected: AttachmentId,
    binding: &Binding,
) -> io::Result<bool> {
    let known_final = fence.request.as_ref().is_some_and(|request| {
        request.header.operation == layerfs_sdk::client::Operation::Binding
            && request.input.is_none() && request.rejected.is_none()
            && request.refused.is_none() && request.admission_error.is_none()
            && request.unsent.is_none() && request.reservation.is_none()
            && request.completion.as_ref().is_some_and(|completion| {
                matches!(completion.outcome(),ServiceOutcome::Dispatched(Ok(Response::Binding(actual))) if actual.as_ref()==binding)
            })
    });
    let receipt_count = fence.output.receipts.len()
        + fence
            .output
            .worker
            .as_ref()
            .map_or(0, |report| report.retained.len());
    let receipt_matches = |receipt: &layerfs_sdk::runtime::service::OutputReceipt| {
        known_final
            && receipt.complete
            && receipt.completed_bytes == receipt.packet.bytes.len() as u64
            && fence.request.as_ref().is_some_and(|request| {
                receipt.packet.correlation == request.envelope.correlation
                    && receipt.packet.class == request.header.operation.class()
            })
    };
    let original_receipts_match = fence.output.receipts.iter().all(receipt_matches)
        && fence
            .output
            .worker
            .as_ref()
            .is_ok_and(|report| report.retained.iter().all(receipt_matches));
    let expected_tail = if fence.request.is_none() {
        receipt_count == 0
    } else {
        known_final && receipt_count == 1 && original_receipts_match
    };
    let mut valid = fence.attachment == expected
        && expected_tail
        && fence.input.events.is_empty()
        && fence.service.cancelled == 0
        && fence.service.completed == 0
        && close_ok(&fence.input.close)
        && close_ok(&fence.output.close)
        && matches!(fence.failure, Some(SupervisorFailure::InputStopped));
    out.raw(",")?;
    out.field("attachment_matches", fence.attachment == expected)?;
    out.raw(",")?;
    out.field("inflight_request", fence.request.is_some())?;
    out.raw(",")?;
    out.field("known_final_binding_completion", known_final)?;
    out.raw(",")?;
    out.field("tail_receipt_count", receipt_count)?;
    out.raw(",")?;
    out.field("tail_receipts_match", original_receipts_match)?;
    out.raw(",")?;
    out.field("input_events", fence.input.events.len())?;
    out.raw(",")?;
    out.field("output_receipts", fence.output.receipts.len())?;
    out.raw(",")?;
    out.field("service_cancelled", fence.service.cancelled)?;
    out.raw(",")?;
    out.field("service_completed", fence.service.completed)?;
    out.raw(",")?;
    out.text("input_close", &format!("{:?}", fence.input.close))?;
    out.raw(",")?;
    out.text("output_close", &format!("{:?}", fence.output.close))?;
    out.raw(",\"supervisor_failure\":")?;
    match &fence.failure {
        Some(value) => out.string(&cause(value))?,
        None => out.raw("null")?,
    }
    out.raw(",\"input_worker\":{")?;
    match &fence.input.worker {
        Err(_) => {
            valid = false;
            out.raw("\"status\":\"PANIC-original-payload-retained\"")?;
        }
        Ok(report) => {
            let native = report.native;
            let clean_wire = native
                .records
                .checked_mul(18)
                .and_then(|overhead| native.plaintext_bytes.checked_add(overhead))
                == Some(native.wire_bytes)
                && native.records.checked_add(1) == Some(native.record_io_attempts);
            let eof = matches!(&report.failure, InputFailure::Native(FramingError::Native(ChannelError::Io(error))) if error.kind() == io::ErrorKind::UnexpectedEof);
            let reassembly_empty = report
                .reassembly
                .as_ref()
                .is_ok_and(|value| value.live_messages == 0 && value.credited_bytes == 0);
            valid &= eof
                && clean_wire
                && reassembly_empty
                && report.partial.is_empty()
                && report.undelivered.is_none()
                && report.close_failure.is_none();
            out.raw("\"status\":\"JOINED\",")?;
            out.text("original_failure", &input_cause(&report.failure))?;
            out.raw(",")?;
            out.field("expected_eof", eof)?;
            out.raw(",")?;
            out.field("complete_record_accounting", clean_wire)?;
            out.raw(",")?;
            out.field("partial_messages", report.partial.len())?;
            out.raw(",")?;
            out.field("undelivered", report.undelivered.is_some())?;
            out.raw(",")?;
            out.text("close_failure", &format!("{:?}", report.close_failure))?;
            out.raw(",")?;
            out.text("native_debug", &format!("{native:?}"))?;
            out.raw(",")?;
            out.text("reassembly_debug", &format!("{:?}", report.reassembly))?;
        }
    }
    out.raw("},\"output_worker\":{")?;
    match &fence.output.worker {
        Err(_) => {
            valid = false;
            out.raw("\"status\":\"PANIC-original-payload-retained\"")?;
        }
        Ok(report) => {
            valid &= report.failure.is_none() && report.retained.iter().all(receipt_matches);
            out.raw("\"status\":\"JOINED\",")?;
            out.text("original_failure", &format!("{:?}", report.failure))?;
            out.raw(",")?;
            out.field("retained_receipts", report.retained.len())?;
            out.raw(",")?;
            out.text("framing_debug", &format!("{:?}", report.framing))?;
            out.raw(",")?;
            out.text("native_debug", &format!("{:?}", report.native))?;
        }
    }
    out.raw("},")?;
    out.field("expected_clean_eof_fence", valid)?;
    out.raw(",\"scope\":\"original-joined-input-output-service-custody-no-product-publication-inference\"}")?;
    Ok(valid)
}
