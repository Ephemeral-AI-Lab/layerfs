//! Service boundaries; declarations and reexports only.
mod captured_runs;
mod files;
pub use captured_runs::OverlayCapturedRuns;
mod lengths;
mod overlay;
mod scratch;
pub use files::OverlayFileRead;
pub use lengths::FileLengths;
pub use overlay::{OverlayJobs, OverlayRead};
pub use scratch::{
    OverlayScratch, ScratchApply, ScratchCopies, ScratchInputRefusal, ScratchRefusal, ScratchReply,
};
