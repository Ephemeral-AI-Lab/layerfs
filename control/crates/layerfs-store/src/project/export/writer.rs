//! The one local writer: applies streamed records strictly into staging,
//! counts them and emits `prepare`, `export` and `finalize` progress.
use crate::project::staged;
use crate::project::tree::{Reader, Record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::outcome::refused;
use std::path::Path;
use std::time::{Duration, Instant};

use super::destination::StagingGuard;
use super::walk::Counts;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The least time between two `export` progress events: at most five a
/// second however fast entries are written, so the event count follows
/// time, not bytes or entries.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// Local writer and throttled progress emitter for `prepare`, `export` and
/// `finalize`.
pub(super) struct LocalWriter {
    guard: StagingGuard,
    files: u64,
    directories: u64,
    symlinks: u64,
    bytes: u64,
    last_emit: Instant,
    emitted_final_counts: bool,
}

impl LocalWriter {
    pub(super) fn start(context: &mut Events<'_>, dest: &Path) -> Result<Self> {
        let guard = StagingGuard::create(dest)?;
        let mut writer = Self {
            guard,
            files: 0,
            directories: 0,
            symlinks: 0,
            bytes: 0,
            last_emit: Instant::now(),
            emitted_final_counts: false,
        };
        writer.emit_phase(context, "prepare", SdkValue::Null);
        writer.emit_export(context);
        Ok(writer)
    }

    fn emit_phase(&self, context: &mut Events<'_>, phase: &str, done: SdkValue) {
        context.progress(self.phase_event(phase, done));
    }

    fn phase_event(&self, phase: &str, done: SdkValue) -> SdkValue {
        SdkValue::map([
            ("phase", SdkValue::text(phase)),
            ("files", SdkValue::Int(self.files as i64)),
            ("directories", SdkValue::Int(self.directories as i64)),
            ("symlinks", SdkValue::Int(self.symlinks as i64)),
            ("bytes", SdkValue::Int(self.bytes as i64)),
            ("done", done),
            ("total", SdkValue::Null),
        ])
    }

    fn emit_export(&mut self, context: &mut Events<'_>) {
        self.emit_phase(context, "export", SdkValue::Int(self.bytes as i64));
        self.last_emit = Instant::now();
        self.emitted_final_counts = true;
    }

    fn maybe_emit_export(&mut self, context: &mut Events<'_>) {
        self.emitted_final_counts = false;
        if self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            self.emit_export(context);
        }
    }

    pub(super) fn write_part(&mut self, context: &mut Events<'_>, part: &[u8]) -> Result<()> {
        let mut reader = Reader::new(part);
        while let Some(record) = reader.next_record()? {
            staged::write_exported(self.guard.path(), record)?;
            match record {
                Record::Directory { .. } => {
                    self.directories += 1;
                    self.maybe_emit_export(context);
                }
                Record::Write { offset, bytes, .. } => {
                    if offset == 0 {
                        self.files += 1;
                    }
                    self.bytes += bytes.len() as u64;
                    self.maybe_emit_export(context);
                }
                Record::Link { .. } => {
                    self.symlinks += 1;
                    self.maybe_emit_export(context);
                }
                Record::Meta { .. } => {}
            }
        }
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        context: &mut Events<'_>,
        dest: &Path,
        expected: Counts,
    ) -> Result<Counts> {
        if self.files != expected.files
            || self.directories != expected.directories
            || self.symlinks != expected.symlinks
            || self.bytes != expected.bytes
        {
            return Err(refused(
                "Failed",
                format!(
                    "exported counts ({} files, {} directories, {} symlinks, {} bytes) do not match snapshot ({} files, {} directories, {} symlinks, {} bytes)",
                    self.files,
                    self.directories,
                    self.symlinks,
                    self.bytes,
                    expected.files,
                    expected.directories,
                    expected.symlinks,
                    expected.bytes,
                ),
            ));
        }
        if !self.emitted_final_counts {
            self.emit_export(context);
        }
        // `finalize` is the last event and means the folder is in place: a
        // publish that fails emits none
        let finalize = self.phase_event("finalize", SdkValue::Int(self.bytes as i64));
        self.guard.publish(dest)?;
        context.progress(finalize);
        Ok(expected)
    }
}
