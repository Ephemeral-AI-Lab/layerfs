//! Original Docker stage failures retained across the provider's common error port.
use super::{
    ExecCreateFailure, ExecHandle, ListenerFailure, OutputFailure, OutputProgress,
    SandboxCreateFailure, SplitFailure, StartFailure, UploadFailure,
};
use crate::{ExecInspection, WireFailure};
/// No failed creation/Start/stream owner is reconstructed, replayed or silently cleaned up.
#[derive(Debug)]
pub enum ProviderFailure {
    Create(Box<SandboxCreateFailure>),
    Upload(Box<UploadFailure>),
    Start(Box<WireFailure>),
    Listener(Box<ListenerFailure>),
    ExecCreate(Box<ExecCreateFailure>),
    ExecStart(Box<StartFailure>),
    ExecSplit(Box<SplitFailure>),
    ExecOutput {
        failure: Box<OutputFailure>,
        handle: ExecHandle,
    },
    ExecInspect {
        failure: Box<WireFailure>,
        handle: ExecHandle,
        progress: OutputProgress,
    },
    /// EOF is not proof of process exit. The original handle permits explicit observation.
    ExecExitUnknown {
        inspection: ExecInspection,
        handle: ExecHandle,
        progress: OutputProgress,
    },
}
