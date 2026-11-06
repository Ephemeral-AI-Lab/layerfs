//! What one running call can reach (moved from layerfs-api `calls/dispatch.rs`).
use layerfs_bridge::control::SdkValue;
use std::path::PathBuf;

/// What one running call can reach: its home and its event stream.
pub struct Context<'a> {
    /// The home every record lives in.
    pub home: PathBuf,
    events: &'a mut dyn FnMut(SdkValue),
}

impl<'a> Context<'a> {
    /// A call's context: its home and its one event stream.
    pub fn new(home: PathBuf, events: &'a mut dyn FnMut(SdkValue)) -> Self {
        Context { home, events }
    }
}

impl Context<'_> {
    /// Sends one progress update, `{ progress: value }`.
    pub fn progress(&mut self, value: SdkValue) {
        (self.events)(SdkValue::map([("progress", value)]));
    }

    /// Passes on one event a worker sent, as it came.
    pub fn event(&mut self, event: SdkValue) {
        (self.events)(event);
    }

    /// Sends one piece of a command's output, `{ stream, bytes }`.
    pub fn output(&mut self, stream: &str, bytes: &[u8]) {
        (self.events)(SdkValue::map([
            ("stream", SdkValue::text(stream)),
            ("bytes", SdkValue::Bytes(bytes.to_vec())),
        ]));
    }
}
