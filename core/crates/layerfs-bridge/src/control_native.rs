//! Additive native attachment records: Ready receipt, status block and custody.
use crate::{
    control::{ControlError, WorkspaceToken},
    control_history::{boolean, put_token, token},
    wire::{Reader, Writer},
};
/// Longest mount directory a Ready or status record carries.
pub const MOUNT_DIRECTORY_LIMIT: usize = 1024;
/// Native half of one bound Workspace, separate from control activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NativePhase {
    /// Bound, with no kernel mount owned by this incarnation.
    Unattached = 1,
    /// One Attach is admitted; its outcome is not yet known.
    Attaching = 2,
    /// Mounted, handshake complete and every receive loop serving.
    Ready = 3,
    /// The reversible normal-unmount probe is in progress; requests are served.
    Probing = 4,
    /// Detach is known; loop joins and daemon-work drain are in progress.
    Draining = 5,
    /// A detach, join, drain, revocation or Close outcome was not established.
    Retained = 6,
}
/// Kernel connection identity and the profile actually negotiated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReceipt {
    /// Engine mount incarnation, unique within its never-reused namespace.
    pub mount: u64,
    /// Engine root serial presented as kernel inode 1.
    pub root: u64,
    /// Kernel mount identifier observed in the daemon's mount table.
    pub mount_id: u64,
    /// Anonymous device major of this connection.
    pub device_major: u32,
    /// Anonymous device minor; also the connection's control identifier.
    pub device_minor: u32,
    /// Negotiated kernel protocol major.
    pub abi_major: u32,
    /// Negotiated kernel protocol minor.
    pub abi_minor: u32,
    /// Capability bits the kernel offered.
    pub offered: u64,
    /// Capability bits selected; offered bits are never reported as selected.
    pub selected: u64,
    /// Negotiated largest write.
    pub max_write: u32,
    /// Negotiated readahead window.
    pub max_readahead: u32,
    /// Negotiated background request limit.
    pub max_background: u16,
    /// Negotiated congestion threshold.
    pub congestion_threshold: u16,
    /// Kernel page size at negotiation.
    pub page_size: u32,
    /// Receive loops that entered service on the shared descriptor.
    pub loops: u16,
    /// A connection-specific abort control was bound for later forced teardown.
    pub abort_bound: bool,
}
/// Original Ready acknowledgement: where the Workspace is mounted and how.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadyMount {
    /// Exact attached incarnation and namespace.
    pub token: WorkspaceToken,
    /// Absolute mount directory inside the daemon's mount namespace.
    pub directory: String,
    /// Kernel identity and negotiation facts.
    pub receipt: NativeReceipt,
}
/// Maintained native request and loop counters; no scan and no syscall.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeWork {
    /// Receive loops configured for this connection.
    pub loops_configured: u16,
    /// Loops that entered service.
    pub loops_entered: u16,
    /// Loops that have exited.
    pub loops_exited: u16,
    /// Loops known joined.
    pub loops_joined: u16,
    /// Fixed receive units currently held by callbacks.
    pub received: u32,
    /// Requests admitted and not yet disposed.
    pub admitted: u32,
    /// Admitted requests queued for a worker.
    pub queued: u32,
    /// Admitted requests currently executing one step.
    pub running: u32,
    /// Admitted requests parked on an owner or Store completion.
    pub parked: u32,
    /// Requests retained with an original failure or panic.
    pub retained: u32,
    /// Requests that reached a disposition.
    pub completed: u64,
    /// Kernel frames handed to the shared dispatcher.
    pub handoffs: u64,
    /// Kernel frames answered inline with no engine or Store work.
    pub inline: u64,
    /// Kernel frames refused by declared policy.
    pub refused: u64,
    /// Kernel frames answered with a terminal error after admission closed.
    pub terminal: u64,
    /// No-reply ownership units that arrived after admission closed.
    pub unadmitted: u64,
    /// Bounded lookup-decrement ownership units, distinct from frames.
    pub forget_units: u64,
}
/// Bounded native block of a status observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeStatus {
    /// Native lifecycle phase recorded by the control owner.
    pub phase: NativePhase,
    /// Original Ready receipt while a connection is owned.
    pub ready: Option<ReadyMount>,
    /// Kernel detach is known for the owned connection.
    pub detached: bool,
    /// Maintained counters while a connection is owned.
    pub work: Option<NativeWork>,
}
/// First teardown boundary that was not established.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TeardownStage {
    /// The kernel detach outcome is not the reversible Busy answer or success.
    Detach = 1,
    /// Not every created receive loop is known joined.
    Join = 2,
    /// The connection owner or its admission watcher did not return normally.
    Owner = 3,
    /// Received, admitted or retained request ownership remains.
    Requests = 4,
    /// The shared dispatcher did not release this mount's lane.
    Lane = 5,
    /// Native-owner revocation was refused or its outcome is unknown.
    Revoke = 6,
    /// Logical Close was refused or its outcome is unknown.
    Close = 7,
    /// The registry could not record the terminal transition.
    Registry = 8,
}
/// Terminal operation stopped after effects; nothing here is a retry token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeardownCustody {
    /// Exact incarnation whose custody is retained.
    pub token: WorkspaceToken,
    /// First boundary not established.
    pub stage: TeardownStage,
    /// Kernel detach is known.
    pub detached: bool,
    /// Counters observed at the stopping boundary, when available.
    pub work: Option<NativeWork>,
    /// Bounded original cause description; never parsed to decide custody.
    pub detail: String,
}
fn put_receipt(out: &mut Writer, value: &NativeReceipt) -> Result<(), ControlError> {
    out.put(&value.mount.to_be_bytes())?;
    out.put(&value.root.to_be_bytes())?;
    out.put(&value.mount_id.to_be_bytes())?;
    for field in [
        value.device_major,
        value.device_minor,
        value.abi_major,
        value.abi_minor,
    ] {
        out.put(&field.to_be_bytes())?;
    }
    out.put(&value.offered.to_be_bytes())?;
    out.put(&value.selected.to_be_bytes())?;
    out.put(&value.max_write.to_be_bytes())?;
    out.put(&value.max_readahead.to_be_bytes())?;
    out.put(&value.max_background.to_be_bytes())?;
    out.put(&value.congestion_threshold.to_be_bytes())?;
    out.put(&value.page_size.to_be_bytes())?;
    out.put(&value.loops.to_be_bytes())?;
    out.byte(u8::from(value.abort_bound))
}
fn receipt(input: &mut Reader<'_>) -> Result<NativeReceipt, ControlError> {
    Ok(NativeReceipt {
        mount: u64::from_be_bytes(input.array()?),
        root: u64::from_be_bytes(input.array()?),
        mount_id: u64::from_be_bytes(input.array()?),
        device_major: u32::from_be_bytes(input.array()?),
        device_minor: u32::from_be_bytes(input.array()?),
        abi_major: u32::from_be_bytes(input.array()?),
        abi_minor: u32::from_be_bytes(input.array()?),
        offered: u64::from_be_bytes(input.array()?),
        selected: u64::from_be_bytes(input.array()?),
        max_write: u32::from_be_bytes(input.array()?),
        max_readahead: u32::from_be_bytes(input.array()?),
        max_background: u16::from_be_bytes(input.array()?),
        congestion_threshold: u16::from_be_bytes(input.array()?),
        page_size: u32::from_be_bytes(input.array()?),
        loops: u16::from_be_bytes(input.array()?),
        abort_bound: boolean(input)?,
    })
}
pub(crate) fn put_ready(out: &mut Writer, value: &ReadyMount) -> Result<(), ControlError> {
    if value.directory.is_empty()
        || value.directory.len() > MOUNT_DIRECTORY_LIMIT
        || value.receipt.mount == 0
        || value.receipt.root == 0
    {
        return Err(ControlError("native Ready fields"));
    }
    put_token(out, value.token)?;
    out.blob(value.directory.as_bytes())?;
    put_receipt(out, &value.receipt)
}
pub(crate) fn ready(input: &mut Reader<'_>) -> Result<ReadyMount, ControlError> {
    let value = ReadyMount {
        token: token(input)?,
        directory: input.text(MOUNT_DIRECTORY_LIMIT)?,
        receipt: receipt(input)?,
    };
    if value.directory.is_empty() || value.receipt.mount == 0 || value.receipt.root == 0 {
        return Err(ControlError("native Ready fields"));
    }
    Ok(value)
}
fn put_work(out: &mut Writer, value: &NativeWork) -> Result<(), ControlError> {
    for field in [
        value.loops_configured,
        value.loops_entered,
        value.loops_exited,
        value.loops_joined,
    ] {
        out.put(&field.to_be_bytes())?;
    }
    for field in [
        value.received,
        value.admitted,
        value.queued,
        value.running,
        value.parked,
        value.retained,
    ] {
        out.put(&field.to_be_bytes())?;
    }
    for field in [
        value.completed,
        value.handoffs,
        value.inline,
        value.refused,
        value.terminal,
        value.unadmitted,
        value.forget_units,
    ] {
        out.put(&field.to_be_bytes())?;
    }
    Ok(())
}
fn work(input: &mut Reader<'_>) -> Result<NativeWork, ControlError> {
    Ok(NativeWork {
        loops_configured: u16::from_be_bytes(input.array()?),
        loops_entered: u16::from_be_bytes(input.array()?),
        loops_exited: u16::from_be_bytes(input.array()?),
        loops_joined: u16::from_be_bytes(input.array()?),
        received: u32::from_be_bytes(input.array()?),
        admitted: u32::from_be_bytes(input.array()?),
        queued: u32::from_be_bytes(input.array()?),
        running: u32::from_be_bytes(input.array()?),
        parked: u32::from_be_bytes(input.array()?),
        retained: u32::from_be_bytes(input.array()?),
        completed: u64::from_be_bytes(input.array()?),
        handoffs: u64::from_be_bytes(input.array()?),
        inline: u64::from_be_bytes(input.array()?),
        refused: u64::from_be_bytes(input.array()?),
        terminal: u64::from_be_bytes(input.array()?),
        unadmitted: u64::from_be_bytes(input.array()?),
        forget_units: u64::from_be_bytes(input.array()?),
    })
}
fn put_optional_work(out: &mut Writer, value: &Option<NativeWork>) -> Result<(), ControlError> {
    out.byte(u8::from(value.is_some()))?;
    match value {
        Some(value) => put_work(out, value),
        None => Ok(()),
    }
}
fn optional_work(input: &mut Reader<'_>) -> Result<Option<NativeWork>, ControlError> {
    if boolean(input)? {
        Ok(Some(work(input)?))
    } else {
        Ok(None)
    }
}
pub(crate) fn put_native(out: &mut Writer, value: &NativeStatus) -> Result<(), ControlError> {
    out.byte(value.phase as u8)?;
    out.byte(u8::from(value.ready.is_some()))?;
    if let Some(ready) = &value.ready {
        put_ready(out, ready)?;
    }
    out.byte(u8::from(value.detached))?;
    put_optional_work(out, &value.work)
}
pub(crate) fn native(input: &mut Reader<'_>) -> Result<NativeStatus, ControlError> {
    let phase = match input.byte()? {
        1 => NativePhase::Unattached,
        2 => NativePhase::Attaching,
        3 => NativePhase::Ready,
        4 => NativePhase::Probing,
        5 => NativePhase::Draining,
        6 => NativePhase::Retained,
        _ => return Err(ControlError("native phase")),
    };
    let ready = if boolean(input)? {
        Some(ready(input)?)
    } else {
        None
    };
    Ok(NativeStatus {
        phase,
        ready,
        detached: boolean(input)?,
        work: optional_work(input)?,
    })
}
pub(crate) fn put_custody(out: &mut Writer, value: &TeardownCustody) -> Result<(), ControlError> {
    if value.detail.len() > 2048 {
        return Err(ControlError("teardown detail limit"));
    }
    put_token(out, value.token)?;
    out.byte(value.stage as u8)?;
    out.byte(u8::from(value.detached))?;
    put_optional_work(out, &value.work)?;
    out.blob(value.detail.as_bytes())
}
pub(crate) fn custody(input: &mut Reader<'_>) -> Result<TeardownCustody, ControlError> {
    let token = token(input)?;
    let stage = match input.byte()? {
        1 => TeardownStage::Detach,
        2 => TeardownStage::Join,
        3 => TeardownStage::Owner,
        4 => TeardownStage::Requests,
        5 => TeardownStage::Lane,
        6 => TeardownStage::Revoke,
        7 => TeardownStage::Close,
        8 => TeardownStage::Registry,
        _ => return Err(ControlError("teardown stage")),
    };
    Ok(TeardownCustody {
        token,
        stage,
        detached: boolean(input)?,
        work: optional_work(input)?,
        detail: input.text(2048)?,
    })
}
